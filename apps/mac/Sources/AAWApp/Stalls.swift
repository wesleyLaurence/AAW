import Darwin
import Foundation
import os

/// Watches the main thread for work that holds it longer than a tenth of a
/// second, which is what the person feels as a window that does not answer
/// and, past a couple of seconds, sees as the spinning wheel.
///
/// The main run loop tells an observer each time it moves on: to timers, to
/// sources, to sleep and back. The time between two of its reports while it
/// is awake is one piece of work. A watchdog thread sleeps while the main
/// thread does; once a piece of work has run past the threshold it reads the
/// main thread's stack, and again every 50 ms until the work ends, so the
/// stall is logged with each thing it was doing. Each stall is logged under
/// `local.aaw.mac`, category `stall`:
///
///     log stream --predicate 'subsystem == "local.aaw.mac" && category == "stall"'
///
/// and kept for `--measure`, which reports them.
final class Stalls: @unchecked Sendable {
    static let shared = Stalls()

    /// A stall: how long the main thread was held, when, and where it was.
    struct Stall {
        var ms: Double
        /// Seconds after the watch started.
        var at: Double
        /// The main thread's frames each time they were read, from when
        /// the stall passed the threshold: each innermost first, as
        /// `image + offset` or a symbol's name.
        var stacks: [[String]]
    }

    /// This process, whose memory the frames are read from.
    private static let task = task_self_trap()

    /// Work longer than this is a stall: 100 ms, or the milliseconds
    /// `AAW_STALL_MS` gives, lower to look into shorter work.
    static let threshold = (ProcessInfo.processInfo.environment["AAW_STALL_MS"].flatMap(Double.init) ?? 100) / 1000
    /// How often a stall's stack is read, and how many times at most.
    static let every = min(0.05, threshold / 2)
    static let most = 100
    /// How many stalls are kept for the report, the latest.
    static let kept = 200

    private let lock = OSAllocatedUnfairLock()
    private let log = Logger(subsystem: "local.aaw.mac", category: "stall")
    private let naming = DispatchQueue(label: "aaw.stalls.naming", qos: .utility)
    /// When the piece of work in progress started, or 0 while the main
    /// thread sleeps.
    private var busySince: Double = 0
    /// The stacks read during the work in progress, once past the threshold.
    private var stacks: [[UInt]] = []
    /// Counts the pieces of work, so a stack read as one ends is not given
    /// to the next.
    private var work = 0
    private var started: Double = 0
    private var stalls: [Stall] = []
    /// Whether the watchdog sleeps until the main thread is busy.
    private var sleeping = false
    private let wake = DispatchSemaphore(value: 0)
    private var observer: CFRunLoopObserver?
    private var main: thread_act_t = 0

    /// Starts watching, on the main thread; once.
    @MainActor
    func start() {
        guard observer == nil else { return }
        main = mach_thread_self()
        started = Self.now()
        let activities = CFRunLoopActivity.allActivities.rawValue
        observer = CFRunLoopObserverCreateWithHandler(nil, activities, true, 0) { [unowned self] _, activity in
            self.moved(activity)
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        let watchdog = Thread { [unowned self] in self.watch() }
        watchdog.name = "aaw.stalls"
        watchdog.qualityOfService = .userInteractive
        watchdog.start()
    }

    /// Seconds since the watch started, for `found(after:)`.
    var elapsed: Double {
        Self.now() - started
    }

    /// The stalls that started after `mark`, a time `elapsed` gave, the
    /// longest first.
    func found(after mark: Double) -> [Stall] {
        found.filter { $0.at >= mark }
    }

    /// The stalls so far, the longest first.
    var found: [Stall] {
        naming.sync {}
        return lock.withLock { stalls }.sorted { $0.ms > $1.ms }
    }

    static func now() -> Double {
        Double(clock_gettime_nsec_np(CLOCK_UPTIME_RAW)) / 1e9
    }

    /// The run loop reports a step: what ran since its last report is done.
    private func moved(_ activity: CFRunLoopActivity) {
        let now = Self.now()
        let (since, read, waking) = lock.withLock { () -> (Double, [[UInt]], Bool) in
            let ended = (busySince, stacks)
            busySince = activity == .beforeWaiting ? 0 : now
            stacks = []
            work += 1
            let waking = sleeping && busySince > 0
            if waking { sleeping = false }
            return (ended.0, ended.1, waking)
        }
        if waking { wake.signal() }
        guard since > 0, now - since > Self.threshold else { return }
        let (ms, at) = (((now - since) * 1000).rounded(), since - started)
        // Naming the frames takes a while: not on the main thread.
        naming.async { [self] in
            let stall = Stall(ms: ms, at: at, stacks: read.map(Self.symbols))
            lock.withLock {
                stalls.append(stall)
                if stalls.count > Self.kept { stalls.removeFirst(stalls.count - Self.kept) }
            }
            let top = (stall.stacks.first ?? []).prefix(12).joined(separator: " < ")
            log.notice("main thread held \(stall.ms, format: .fixed(precision: 0), privacy: .public) ms: \(top, privacy: .public)")
        }
    }

    /// Sleeps while the main thread does, and otherwise until the work in
    /// progress is due its next reading, then reads its stack if it is still
    /// the same work.
    private func watch() {
        while true {
            let due = lock.withLock { () -> (work: Int, at: Double)? in
                guard busySince > 0 else {
                    sleeping = true
                    return nil
                }
                return (work, busySince + Self.threshold + Double(stacks.count) * Self.every)
            }
            guard let due else {
                wake.wait()
                continue
            }
            let wait = due.at - Self.now()
            if wait > 0 { usleep(useconds_t(wait * 1_000_000)) }
            let reading = lock.withLock { work == due.work && stacks.count < Self.most }
            guard reading else { continue }
            let frames = Self.backtrace(of: main)
            lock.withLock { if work == due.work { stacks.append(frames) } }
        }
    }

    /// The return addresses on a thread's stack, innermost first, read by
    /// stopping it and following its frame pointers.
    private static func backtrace(of thread: thread_act_t) -> [UInt] {
        #if arch(arm64)
        guard thread_suspend(thread) == KERN_SUCCESS else { return [] }
        defer { thread_resume(thread) }
        var state = arm_thread_state64_t()
        var count = mach_msg_type_number_t(MemoryLayout<arm_thread_state64_t>.size / MemoryLayout<natural_t>.size)
        let got = withUnsafeMutablePointer(to: &state) {
            $0.withMemoryRebound(to: natural_t.self, capacity: Int(count)) {
                thread_get_state(thread, ARM_THREAD_STATE64, $0, &count)
            }
        }
        guard got == KERN_SUCCESS else { return [] }
        // Addresses carry no pointer signature in user space past 47 bits.
        let mask: UInt64 = 0x0000_7FFF_FFFF_FFFF
        var frames = [UInt(state.__pc & mask), UInt(state.__lr & mask)]
        var fp = state.__fp & mask
        while fp != 0, frames.count < 64 {
            // The frame record: the caller's frame pointer, then the return address.
            var record: (UInt64, UInt64) = (0, 0)
            var size: vm_size_t = 0
            let read = withUnsafeMutablePointer(to: &record) {
                vm_read_overwrite(Self.task, vm_address_t(fp), vm_size_t(16), vm_address_t(UInt(bitPattern: $0)), &size)
            }
            guard read == KERN_SUCCESS, size == 16 else { break }
            let (next, ret) = (record.0 & mask, record.1 & mask)
            if ret == 0 { break }
            frames.append(UInt(ret))
            guard next > fp else { break }
            fp = next
        }
        return frames
        #else
        return []
        #endif
    }

    /// Names for frames: a symbol where the image exports one near, else the
    /// image and the offset into it, for `atos`.
    private static func symbols(_ frames: [UInt]) -> [String] {
        frames.map { address in
            var info = Dl_info()
            guard dladdr(UnsafeRawPointer(bitPattern: address), &info) != 0 else { return String(format: "0x%lx", address) }
            let image = info.dli_fname.map { URL(fileURLWithPath: String(cString: $0)).lastPathComponent } ?? "?"
            let offset = address - UInt(bitPattern: info.dli_fbase)
            // The app's own code is not exported: a name dladdr finds for it
            // is only the nearest one that is.
            if let name = info.dli_sname, image != ProcessInfo.processInfo.processName {
                return "\(image) \(String(cString: name))"
            }
            return String(format: "%@ + 0x%lx", image, offset)
        }
    }

    /// The report `--measure` writes: how many stalls, the longest, and each
    /// with its stack. Frames in the app's own image are named by `atos`
    /// where the developer tools have it.
    var report: [String: Any] {
        let all = found
        let named = Self.symbolicated(all)
        return [
            "count": all.count,
            "max_ms": all.first?.ms ?? 0,
            "total_ms": all.reduce(0) { $0 + $1.ms },
            "stalls": named.map { ["ms": $0.ms, "at_s": ($0.at * 10).rounded() / 10, "stacks": Self.counted($0.stacks)] },
        ]
    }

    /// A stall's stacks, each told once with how many times it was read, in
    /// the order first read: each reading `every` seconds.
    static func counted(_ stacks: [[String]]) -> [[String: Any]] {
        var order: [[String]] = []
        var counts: [[String]: Int] = [:]
        for stack in stacks {
            if counts[stack] == nil { order.append(stack) }
            counts[stack, default: 0] += 1
        }
        return order.map { ["count": counts[$0] ?? 0, "stack": $0] }
    }

    /// The stalls with the app's own frames named by `atos`, when it runs.
    private static func symbolicated(_ stalls: [Stall]) -> [Stall] {
        let image = ProcessInfo.processInfo.processName
        let prefix = "\(image) + 0x"
        let offsets = Set(stalls.flatMap(\.stacks).flatMap { $0 }.filter { $0.hasPrefix(prefix) })
        guard !offsets.isEmpty, let executable = Bundle.main.executableURL,
              let base = _dyld_get_image_header(0).map({ UInt(bitPattern: $0) }) else { return stalls }
        let ordered = Array(offsets)
        let addresses = ordered.compactMap { UInt($0.dropFirst(prefix.count), radix: 16).map { String(format: "0x%lx", $0 + base) } }
        guard addresses.count == ordered.count else { return stalls }
        let atos = Process()
        atos.executableURL = URL(fileURLWithPath: "/usr/bin/atos")
        atos.arguments = ["-o", executable.path, "-l", String(format: "0x%lx", base)] + addresses
        let out = Pipe()
        atos.standardOutput = out
        atos.standardError = FileHandle.nullDevice
        guard (try? atos.run()) != nil else { return stalls }
        let said = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        atos.waitUntilExit()
        let names = said.split(separator: "\n").map(String.init)
        guard atos.terminationStatus == 0, names.count == ordered.count else { return stalls }
        let byOffset = Dictionary(uniqueKeysWithValues: zip(ordered, names))
        return stalls.map { stall in
            var s = stall
            s.stacks = stall.stacks.map { $0.map { byOffset[$0] ?? $0 } }
            return s
        }
    }
}
