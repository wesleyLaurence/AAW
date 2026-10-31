import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// How the app was started: `AAW [PROJECT...]`, each a project's `.aaw`, its
/// folder or its song file, and with none a new Untitled project. For checking the app without
/// anyone at the screen, `--click X,Y`, `--shift-click X,Y`, `--double-click
/// X,Y`, `--drag X1,Y1,X2,Y2` (and `--opt-drag`, `--cmd-drag` with a key
/// held), `--key KEY`, `--type TEXT`, `--analyzer-click X,Y`, `--analyzer-drag X1,Y1,X2,Y2`, `--wait SECONDS`,
/// `--ask TEXT`, `--attach FILE`, `--attach-clips`, `--listen` and `--record-audio`
/// in the order to perform them, then `--export PATH [--export-level
/// LEVEL]`, `--measure JSON [--frames N]` and `--snapshot PNG`, with
/// `--after SECONDS` and `--size WxH`.
struct Launch {
    /// Input to feed the first window as if a person made it. Points are in
    /// the window's content, from its top left.
    enum Action: Equatable {
        case click(CGPoint, shift: Bool = false, count: Int = 1)
        case drag(CGPoint, CGPoint, flags: NSEvent.ModifierFlags = [])
        case key(Key)
        /// An audio file let go at a point, as a drag from the Finder ends.
        case drop(URL, CGPoint)
        /// Time for something else to happen, such as an agent's command.
        case wait(Double)
        /// A click, or a drag, in the first analyzer window that is open, in
        /// its content.
        case analyzerClick(CGPoint)
        case analyzerDrag(CGPoint, CGPoint)
        /// A message typed into the agent panel and sent. The picture waits
        /// for the agent's turn to end.
        case ask(String)
        /// A file attached to the agent panel's next message, as a drop on
        /// the panel does it.
        case attach(URL)
        /// The selected clips attached, as `+` › Attach Selected Clips.
        case attachClips
        /// The microphone's button: listening starts, or stops.
        case listen
        /// `+` › Record Audio: a sound starts recording for the agent
        /// panel's next message, or stops and attaches.
        case recordAudio
    }

    /// A key press: a key by name, such as `space`, `delete`, `left` or `z`,
    /// after any of `cmd+`, `shift+` and `opt+`; `up+a` lets the key go.
    struct Key: Equatable {
        var characters: String
        var code: UInt16
        var modifiers: NSEvent.ModifierFlags = []
        /// One character of typed text, rather than a key of its own.
        var typed = false
        /// The key lifted rather than pressed: a note of the typing keyboard let go.
        var up = false

        /// Where each letter is on the keyboard, which menus match keys by.
        private static let letters: [Character: UInt16] = [
            "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12,
            "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "o": 31, "u": 32, "i": 34, "p": 35, "l": 37, "j": 38,
            "k": 40, "n": 45, "m": 46, ";": 41, "'": 39,
        ]
        private static let named: [String: (String, UInt16)] = [
            "space": (" ", 49), "return": ("\r", 36), "delete": ("\u{7f}", 51), "escape": ("\u{1b}", 53),
            "left": ("\u{f702}", 123), "right": ("\u{f703}", 124), "down": ("\u{f701}", 125), "up": ("\u{f700}", 126),
        ]
        private static let modifierNames: [String: NSEvent.ModifierFlags] = [
            "cmd": .command, "shift": .shift, "opt": .option,
        ]

        init?(_ text: String) {
            var parts = text.split(separator: "+").map(String.init)
            guard let name = parts.popLast() else { return nil }
            for part in parts {
                if part == "up" {
                    up = true
                    continue
                }
                guard let flag = Self.modifierNames[part] else { return nil }
                modifiers.insert(flag)
            }
            if let (characters, code) = Self.named[name] {
                (self.characters, self.code) = (characters, code)
                // AppKit marks arrow keys as function keys.
                if (123...126).contains(code) { modifiers.insert([.function, .numericPad]) }
            } else if name.count == 1 {
                (characters, code) = (name, Self.letters[Character(name)] ?? 0)
            } else {
                return nil
            }
        }

        init(typing character: Character) {
            (characters, code) = (String(character), Self.letters[character] ?? 0)
            typed = true
        }
    }

    var songs: [URL] = []
    var actions: [Action] = []
    /// Write a picture of the first window here, then quit.
    var snapshot: URL?
    /// Scroll and zoom the arrangement for `frames` frames, write how long
    /// each took to draw here, then quit.
    var measure: URL?
    var frames = 240
    /// Seconds between the last action and the picture.
    var after: Double = 1
    var size: CGSize?
    /// Write the mix here after the actions, as File › Export Audio… does
    /// when its panel closes, then the picture if one was asked for, and
    /// quit. The format is the extension's.
    var export: URL?
    /// The level of that export: `peak=-1`, `lufs=-14`, `gain=-3` or
    /// `rendered`.
    var exportLevel: ExportOptions.Level?
    /// Open the Settings window, as AAW › Settings… does.
    var settings = false
    /// The pane it opens on, as `--settings-pane NAME` names it.
    var settingsPane: SettingsWindowController.Pane?

    init(arguments: [String]) {
        var rest = arguments[...]
        while let argument = rest.popFirst() {
            func numbers() -> [Double] {
                (rest.popFirst() ?? "").split(separator: ",").compactMap { Double($0) }
            }
            switch argument {
            case "--click", "--shift-click", "--double-click":
                let n = numbers()
                if n.count == 2 {
                    actions.append(.click(CGPoint(x: n[0], y: n[1]), shift: argument == "--shift-click",
                                          count: argument == "--double-click" ? 2 : 1))
                }
            case "--drag", "--opt-drag", "--cmd-drag":
                let n = numbers()
                let flags: NSEvent.ModifierFlags = argument == "--opt-drag" ? .option : argument == "--cmd-drag" ? .command : []
                if n.count == 4 { actions.append(.drag(CGPoint(x: n[0], y: n[1]), CGPoint(x: n[2], y: n[3]), flags: flags)) }
            case "--drop":
                // FILE,X,Y: the file's path may have commas of its own.
                let parts = (rest.popFirst() ?? "").split(separator: ",", omittingEmptySubsequences: false)
                if parts.count >= 3, let x = Double(parts[parts.count - 2]), let y = Double(parts[parts.count - 1]) {
                    let path = parts.dropLast(2).joined(separator: ",")
                    actions.append(.drop(URL(fileURLWithPath: path), CGPoint(x: x, y: y)))
                }
            case "--key":
                if let key = rest.popFirst().flatMap(Key.init) { actions.append(.key(key)) }
            case "--type":
                actions += (rest.popFirst() ?? "").map { .key(Key(typing: $0)) }
            case "--analyzer-click":
                let n = numbers()
                if n.count == 2 { actions.append(.analyzerClick(CGPoint(x: n[0], y: n[1]))) }
            case "--analyzer-drag":
                let n = numbers()
                if n.count == 4 { actions.append(.analyzerDrag(CGPoint(x: n[0], y: n[1]), CGPoint(x: n[2], y: n[3]))) }
            case "--wait":
                if let seconds = rest.popFirst().flatMap(Double.init) { actions.append(.wait(seconds)) }
            case "--ask":
                if let text = rest.popFirst(), !text.isEmpty { actions.append(.ask(text)) }
            case "--attach":
                if let path = rest.popFirst(), !path.isEmpty { actions.append(.attach(URL(fileURLWithPath: path))) }
            case "--attach-clips":
                actions.append(.attachClips)
            case "--listen":
                actions.append(.listen)
            case "--record-audio":
                actions.append(.recordAudio)
            case "--settings":
                settings = true
            case "--settings-pane":
                let name = (rest.popFirst() ?? "").lowercased()
                settingsPane = SettingsWindowController.Pane.allCases.first { $0.rawValue.lowercased() == name }
                settings = true
            case "--snapshot":
                snapshot = rest.popFirst().map { URL(fileURLWithPath: $0) }
            case "--measure":
                measure = rest.popFirst().map { URL(fileURLWithPath: $0) }
            case "--frames":
                frames = rest.popFirst().flatMap(Int.init) ?? frames
            case "--after":
                after = rest.popFirst().flatMap(Double.init) ?? after
            case "--size":
                let parts = (rest.popFirst() ?? "").split(separator: "x").compactMap { Double($0) }
                if parts.count == 2 { size = CGSize(width: parts[0], height: parts[1]) }
            case "--export":
                export = rest.popFirst().map { URL(fileURLWithPath: $0) }
            case "--export-level":
                exportLevel = rest.popFirst().flatMap(ExportOptions.Level.parse)
            case _ where argument.hasPrefix("-"):
                // A default AppKit reads itself, such as -NSQuitAlwaysKeepsWindows NO.
                if argument.hasPrefix("-NS") || argument.hasPrefix("-Apple") { _ = rest.popFirst() }
            default:
                songs.append(URL(fileURLWithPath: argument))
            }
        }
    }
}

@MainActor
public enum AAWMain {
    private static var delegate: AppDelegate?

    public static func run() {
        let app = NSApplication.shared
        let delegate = AppDelegate(launch: Launch(arguments: Array(CommandLine.arguments.dropFirst())))
        Self.delegate = delegate
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        app.run()
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let launch: Launch
    private var songs: [SongWindowController] = []
    /// The projects the app knows, which Open Recent shows.
    private var index = ProjectIndex(file: ProjectIndex.standard)
    /// The Untitled project launch made. A project opened while it holds
    /// nothing takes its place.
    private weak var launched: SongWindowController?
    /// The project whose window is in front, as its host is told.
    private weak var front: SongWindowController?
    /// The Settings window, made when first opened.
    private var settings: SettingsWindowController?
    private var recentMenu = NSMenu(title: "Open Recent")
    /// A scripted run asks nothing: nobody is there to answer.
    private var scripted: Bool { launch.snapshot != nil || launch.measure != nil || launch.export != nil }
    /// A scripted export failed: the run ends with that status after the picture.
    private var exportFailed = false
    /// How long the picture has waited for the agent's turn after `--ask`.
    private var askWaited = 0.0

    /// How many projects Open Recent lists.
    static let recentLimit = 15

    init(launch: Launch) {
        self.launch = launch
    }

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.mainMenu = mainMenu()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        restoreIndex()
        // A device or buffer chosen in Settings reaches every open song.
        AudioPreferences.shared.onChange = { [weak self] settings in
            self?.songs.forEach { $0.model.applyAudio(settings) }
        }
        for url in launch.songs { openSong(url) }
        if songs.isEmpty { launched = newProject() }
        if launch.settings { later(0.3) { $0.openSettings($0.launch.settingsPane) } }
        if !scripted || !launch.actions.isEmpty {
            NSApp.activate(ignoringOtherApps: true)
        }
        // Scripted input, half a second apart, then the picture. Typed
        // characters follow one another closely.
        var delay = 0.5
        var previous: Launch.Action?
        for action in launch.actions {
            if case .wait(let seconds) = action {
                delay += seconds
                continue
            }
            if case .key(let key) = action, case .key(let last)? = previous, key.typed, last.typed {
                delay -= 0.45
            }
            later(delay) { $0.perform(action) }
            delay += 0.5
            previous = action
        }
        let end = delay - 0.5 + launch.after
        if let export = launch.export {
            later(end) { $0.export(to: export) }
        } else if let measure = launch.measure {
            later(end) { $0.measure(to: measure) }
        } else if let snapshot = launch.snapshot {
            later(end) { $0.snapshot(to: snapshot) }
        }
    }

    /// Writes the first song's mix where `--export` said, prints what the
    /// command reported, then takes the picture if one was asked for and
    /// quits; a failed export says why on stderr and ends the run with
    /// status 1.
    private func export(to url: URL) {
        guard let controller = songs.first else { exit(1) }
        var options = ExportOptions()
        options.format = ExportOptions.Format(fileExtension: url.pathExtension) ?? .wav24
        if let level = launch.exportLevel { options.level = level }
        controller.model.export(to: url, options: options, replace: false) { [weak self] outcome in
            switch outcome {
            case .done(let report):
                print("Exported \(report.file): \(report.measurements)")
                for warning in report.warnings { print("Warning: \(warning)") }
            case .failed(_, let reason):
                FileHandle.standardError.write(Data("\(reason)\n".utf8))
                self?.exportFailed = true
            }
            guard let self else { return }
            if let snapshot = launch.snapshot {
                later(launch.after) { $0.snapshot(to: snapshot) }
            } else {
                finishRun()
            }
        }
    }

    /// Ends a scripted run: as a quit, unless its export failed.
    private func finishRun() {
        if exportFailed { exit(1) }
        NSApp.terminate(nil)
    }

    private func later(_ seconds: Double, _ body: @escaping @MainActor (AppDelegate) -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) {
            MainActor.assumeIsolated { body(self) }
        }
    }

    /// The Finder's double-click on a `.aaw`, or `open` from a terminal. With
    /// the app closed this comes before `applicationDidFinishLaunching`, which
    /// then makes no Untitled project; with it open, the song's window comes
    /// forward.
    func application(_ application: NSApplication, open urls: [URL]) {
        for url in urls { openSong(url) }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    /// Quitting asks about each Untitled project that holds something, in
    /// turn. Cancel in any of them keeps the app open.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        for controller in songs where !controller.mayClose() {
            songs.forEach { $0.keep() }
            return .terminateCancel
        }
        return .terminateNow
    }

    func applicationWillTerminate(_ notification: Notification) {
        for controller in songs { closed(controller, deleted: controller.finish()) }
    }

    // MARK: Projects

    /// The app's own file type: a project's `.aaw`, which Info.plist exports.
    static let projectType = UTType(exportedAs: "local.aaw.project")

    /// A project's song file: `song.yaml` in a folder, the file itself, or
    /// the song a `.aaw` names beside itself, by the core's one rule. A
    /// `.aaw` that does not read throws with what is wrong.
    static func songFile(_ url: URL) throws -> URL {
        SongModel.normal(URL(fileURLWithPath: try projectSongFile(path: url.path)))
    }

    /// A project's folder drawn with the app's icon in the Finder: a custom
    /// icon on the folder, which is `Icon\r` inside it, set once. Not for an
    /// Untitled project, which is in the app's own data folder.
    private static func setFolderIcon(of song: URL, untitled: Bool) {
        let folder = song.deletingLastPathComponent()
        guard !untitled, !FileManager.default.fileExists(atPath: folder.appendingPathComponent("Icon\r").path),
              let icon = NSApp.applicationIconImage else { return }
        NSWorkspace.shared.setIcon(icon, forFile: folder.path, options: [])
    }

    private func failed(_ what: String, _ error: Error) {
        let alert = NSAlert()
        alert.messageText = what
        alert.informativeText = SongModel.reason(error)
        alert.runModal()
    }

    private func saveIndex() {
        do {
            try index.save()
        } catch {
            FileHandle.standardError.write(Data("The project index was not saved: \(error.localizedDescription)\n".utf8))
        }
    }

    /// Brings the index up to date with the disk at launch. A project whose
    /// folder moved is found again. An Untitled project a crash left is
    /// deleted if it holds nothing, and otherwise listed in Open Recent.
    private func restoreIndex() {
        if !index.existed {
            // Before there was an index, Open Recent was the system's list.
            for (n, url) in NSDocumentController.shared.recentDocumentURLs.enumerated() {
                guard let song = try? Self.songFile(url) else { continue }
                index.keep(song, title: ProjectIndex.name(of: url), untitled: false, at: Date().addingTimeInterval(-Double(n)))
            }
        }
        let left = projectSweepUntitled().map { SongModel.normal(URL(fileURLWithPath: $0)) }
        index.resolve()
        for entry in index.entries where entry.untitled && entry.missing { index.remove(entry.url) }
        for song in left { index.keep(song, title: ProjectIndex.name(of: song), untitled: true) }
        saveIndex()
    }

    /// File › New: a blank project called Untitled, in a window of its own.
    @objc func newDocument(_ sender: Any?) {
        newProject()
    }

    @discardableResult
    private func newProject() -> SongWindowController? {
        do {
            return openSong(URL(fileURLWithPath: try projectNewUntitled()))
        } catch {
            failed("A new project could not be made", error)
            return nil
        }
    }

    @discardableResult
    func openSong(_ url: URL) -> SongWindowController? {
        let file: URL
        do {
            file = try Self.songFile(url)
        } catch {
            failed("“\(url.lastPathComponent)” could not be opened", error)
            return nil
        }
        if let shown = songs.first(where: { $0.model.url == file }) {
            shown.showWindow(nil)
            return shown
        }
        // A project saved from this one answers `daw` commands for its path
        // until the project itself is opened.
        for other in songs where other.model.formerURLs.contains(file) { other.model.release(file) }
        do {
            let controller = SongWindowController(model: try SongModel(url: file), size: launch.size, asks: !scripted)
            // A folder duplicated in the Finder shares its ID with the
            // project it was copied from: the copy gets one of its own.
            if let id = controller.model.projectID, index.taken(id, byAnotherThan: file) {
                try? controller.model.renewProjectID()
            }
            Self.setFolderIcon(of: file, untitled: controller.model.untitled)
            controller.onClose = { [weak self] closed, deleted in
                self?.songs.removeAll { $0 === closed }
                self?.closed(closed, deleted: deleted)
            }
            controller.onMain = { [weak self] main in self?.cameToFront(main) }
            controller.model.onMoved = { [weak self, weak controller] old in
                if let controller { self?.moved(controller, from: old) }
            }
            songs.append(controller)
            controller.showWindow(nil)
            index.note(file, title: controller.model.arrangement.title, untitled: controller.model.untitled, id: controller.model.projectID)
            saveIndex()
            if let blank = launched, blank.model.untitled, blank.model.untouched { blank.close() }
            launched = nil
            return controller
        } catch {
            failed("“\(ProjectIndex.name(of: file))” could not be opened", error)
            // What Open Recent offers is checked again.
            index.resolve()
            saveIndex()
            return nil
        }
    }

    /// A project is no longer open: deleted, or known as it was left.
    private func closed(_ controller: SongWindowController, deleted: Bool) {
        let model = controller.model
        if deleted {
            index.remove(model.url)
        } else {
            index.note(model.url, title: model.arrangement.title, untitled: model.untitled, id: model.projectID)
        }
        saveIndex()
    }

    /// A project was saved under a name. An Untitled one moved, and is that
    /// project now; one that had a name was copied, and is still known.
    private func moved(_ controller: SongWindowController, from old: URL) {
        if projectIsUntitled(path: old.path) { index.remove(old) }
        let url = controller.model.url
        index.note(url, title: ProjectIndex.name(of: url), untitled: controller.model.untitled, id: controller.model.projectID)
        Self.setFolderIcon(of: url, untitled: controller.model.untitled)
        saveIndex()
    }

    /// The host of the project in front says so to `daw projects`, so that
    /// an agent in a terminal knows which one the person is looking at.
    private func cameToFront(_ controller: SongWindowController) {
        guard front !== controller else { return }
        front?.model.setFront(false)
        controller.model.setFront(true)
        front = controller
    }

    @objc func openDocument(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.message = "Choose a project: its .aaw file, or its folder"
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [Self.projectType, .folder]
        if panel.runModal() == .OK, let url = panel.url { openSong(url) }
    }

    /// Links the bundle's `daw` onto the PATH, or says that it is.
    @objc func installCommandLineTool(_ sender: Any?) {
        CommandLineTool.offer()
    }

    /// Shows the Settings window, made the first time, at the pane the
    /// sender names if it names one, as the Generate tab's Open Settings… does.
    @objc func openSettings(_ sender: Any?) {
        if settings == nil {
            settings = SettingsWindowController { [weak self] in
                guard let self else { return nil }
                return (front ?? songs.first)?.model
            }
        }
        if let pane = sender as? SettingsWindowController.Pane { settings?.show(pane) }
        settings?.showWindow(nil)
        settings?.window?.makeKeyAndOrderFront(nil)
    }

    @objc private func openRecent(_ sender: NSMenuItem) {
        if let url = sender.representedObject as? URL { openSong(url) }
    }

    @objc private func clearRecent(_ sender: Any?) {
        index.clear(keeping: Set(songs.map(\.model.url.path)))
        saveIndex()
    }

    // MARK: Scripted input and snapshot

    /// Gives the first window an event as if a person had made it, so that it
    /// takes the path a real click or key press takes.
    private func perform(_ action: Launch.Action) {
        guard let window = songs.first?.window, let content = window.contentView else { return }
        // The key window, so that a click gives a field the focus and a
        // key reaches it, even while another app is in front.
        if !window.isKeyWindow { window.makeKeyAndOrderFront(nil) }
        let now = ProcessInfo.processInfo.systemUptime
        func mouse(_ type: NSEvent.EventType, _ p: CGPoint, flags: NSEvent.ModifierFlags = [], count: Int = 1) {
            let at = CGPoint(x: p.x, y: content.bounds.height - p.y)
            if let event = NSEvent.mouseEvent(
                with: type, location: at, modifierFlags: flags, timestamp: now, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0, clickCount: count, pressure: 1
            ) {
                // Queued, as a person's are: a button follows a press by
                // waiting for the release in the queue.
                NSApp.postEvent(event, atStart: false)
            }
        }
        switch action {
        case .click(let p, let shift, let count):
            for n in 1...count {
                mouse(.leftMouseDown, p, flags: shift ? .shift : [], count: n)
                mouse(.leftMouseUp, p, flags: shift ? .shift : [], count: n)
            }
        case .drag(let from, let to, let flags):
            mouse(.leftMouseDown, from, flags: flags)
            mouse(.leftMouseDragged, CGPoint(x: (from.x + to.x) / 2, y: (from.y + to.y) / 2), flags: flags)
            mouse(.leftMouseDragged, to, flags: flags)
            mouse(.leftMouseUp, to, flags: flags)
        case .key(let key):
            // With Shift, a letter arrives as its capital, which menus match.
            let characters = key.modifiers.contains(.shift) ? key.characters.uppercased() : key.characters
            guard let event = NSEvent.keyEvent(
                with: key.up ? .keyUp : .keyDown, location: .zero, modifierFlags: key.modifiers, timestamp: now,
                windowNumber: window.windowNumber, context: nil, characters: key.characters,
                charactersIgnoringModifiers: characters, isARepeat: false, keyCode: key.code
            ) else { return }
            // A key pressed goes to the window's key equivalents first, as a
            // person's does: the typing keyboard's notes are taken there.
            if !key.up, window.performKeyEquivalent(with: event) { return }
            // A key with Command is a menu's, as it is when a person presses it.
            if !key.up, key.modifiers.contains(.command), NSApp.mainMenu?.performKeyEquivalent(with: event) == true { return }
            // To the window itself, as a click is: the app has no key
            // window while another app, or another AAW, is in front.
            window.sendEvent(event)
        case .drop(let file, let p):
            // No drag can be made up, so the file is handed to where a drag
            // that ended at the point would have left it: in the detail
            // panel, the Sampler it shows.
            if let model = songs.first?.model, p.y >= content.bounds.height - model.detailHeight, model.dropOnDevices(file: file) {
                return
            }
            func arrangement(in view: NSView) -> ArrangementView? {
                (view as? ArrangementView) ?? view.subviews.lazy.compactMap(arrangement).first
            }
            if let view = arrangement(in: content) {
                view.drop(file: file, at: view.convert(CGPoint(x: p.x, y: content.bounds.height - p.y), from: nil))
            }
        case .analyzerClick, .analyzerDrag:
            // The analyzer's window, whose events the song's would not take.
            guard let analyzer = songs.first?.model.openAnalyzerWindows.first?.window, let content = analyzer.contentView else { return }
            func post(_ type: NSEvent.EventType, _ p: CGPoint) {
                let at = CGPoint(x: p.x, y: content.bounds.height - p.y)
                if let event = NSEvent.mouseEvent(with: type, location: at, modifierFlags: [], timestamp: now, windowNumber: analyzer.windowNumber,
                                                  context: nil, eventNumber: 0, clickCount: 1, pressure: 1) {
                    NSApp.postEvent(event, atStart: false)
                }
            }
            if case .analyzerClick(let p) = action {
                post(.leftMouseDown, p)
                post(.leftMouseUp, p)
            } else if case .analyzerDrag(let from, let to) = action {
                post(.leftMouseDown, from)
                post(.leftMouseDragged, CGPoint(x: (from.x + to.x) / 2, y: (from.y + to.y) / 2))
                post(.leftMouseDragged, to)
                post(.leftMouseUp, to)
            }
        case .ask(let text):
            // As typed and sent, so that what `--attach` added goes with it.
            songs.first?.model.showsAgent = true
            songs.first?.model.agent.draft = text
            songs.first?.model.agent.send()
        case .attach(let file):
            songs.first?.model.showsAgent = true
            songs.first?.model.agent.attach(file: file)
        case .attachClips:
            songs.first?.model.showsAgent = true
            songs.first?.model.agent.attachSelectedClips()
        case .listen:
            songs.first?.model.showsAgent = true
            songs.first?.model.agent.toggleListening()
        case .recordAudio:
            songs.first?.model.showsAgent = true
            songs.first?.model.agent.toggleRecording()
        case .wait:
            break
        }
    }

    /// Writes how long the first window's arrangement takes to draw while it
    /// scrolls and zooms, then the picture if one was asked for, and quits.
    private func measure(to url: URL) {
        guard let controller = songs.first, let size = controller.window?.contentView?.bounds.size else { exit(1) }
        let (frames, snapshot) = (launch.frames, launch.snapshot)
        controller.model.measure(frames: frames) { [weak self] times in
            let a = controller.model.arrangement
            var report = times.report
            report["window"] = [size.width, size.height]
            report["tracks"] = a.tracks.count
            report["clips"] = a.tracks.reduce(0) { $0 + $1.clips.count }
            // How long waveforms trailed the song's opening and its last
            // change of audio.
            let round = { (x: Double) in (x * 10).rounded() / 10 }
            var waveforms: [String: Double] = [:]
            if let open = controller.model.waveformDelay.open { waveforms["after_open"] = round(open) }
            if let change = controller.model.waveformDelay.change { waveforms["after_change"] = round(change) }
            report["waveform_ms"] = waveforms
            // An analyzer window open through the run: how its drawing kept up.
            if let analyzer = controller.model.openAnalyzerWindows.first {
                var times = analyzer.view.drawTimes.report
                times["read_ms"] = controller.model.analysisReadTimes.report["draw_ms"]
                times["window"] = analyzer.window.map { [$0.contentView?.bounds.width ?? 0, $0.contentView?.bounds.height ?? 0] } ?? []
                report["analyzer"] = times
            }
            do {
                try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]).write(to: url)
            } catch {
                FileHandle.standardError.write(Data("\(error.localizedDescription)\n".utf8))
                exit(1)
            }
            // The view is back at the zoom that shows the whole song.
            if let snapshot {
                self?.later(0.3) { $0.snapshot(to: snapshot) }
            } else {
                NSApp.terminate(nil)
            }
        }
    }

    /// Writes the first window's picture where asked, and beside it one of
    /// each analyzer window that is open, named `NAME-analyzer.png`, then
    /// `NAME-analyzer-2.png` and so on.
    private func snapshot(to url: URL) {
        guard let controller = songs.first, let view = controller.window?.contentView else { exit(1) }
        // After `--ask`, the picture is of the agent's answer: it waits for
        // the turn, up to ten minutes, unless a request waits for a person,
        // which the picture then shows.
        if controller.model.agent.working, !controller.model.agent.awaitingAnswer,
           launch.actions.contains(where: { if case .ask = $0 { true } else { false } }), askWaited < 600 {
            askWaited += 0.5
            later(0.5) { $0.snapshot(to: url) }
            return
        }
        var pictures = [(view, url)]
        // The Settings window, when open, as `NAME-settings.png`.
        if let window = settings?.window, window.isVisible, let content = window.contentView {
            let name = url.deletingPathExtension().lastPathComponent + "-settings"
            pictures.append((content, url.deletingLastPathComponent().appendingPathComponent(name).appendingPathExtension(url.pathExtension)))
        }
        for (i, analyzer) in controller.model.openAnalyzerWindows.enumerated() {
            guard let content = analyzer.window?.contentView else { continue }
            let name = url.deletingPathExtension().lastPathComponent + "-analyzer" + (i == 0 ? "" : "-\(i + 1)")
            pictures.append((content, url.deletingLastPathComponent().appendingPathComponent(name).appendingPathExtension(url.pathExtension)))
        }
        for (view, url) in pictures {
            guard let image = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { exit(1) }
            view.cacheDisplay(in: view.bounds, to: image)
            do {
                try image.representation(using: .png, properties: [:])?.write(to: url)
            } catch {
                FileHandle.standardError.write(Data("\(error.localizedDescription)\n".utf8))
                exit(1)
            }
        }
        finishRun()
    }

    // MARK: Menus

    private func item(_ title: String, _ action: Selector?, _ key: String = "", _ modifiers: NSEvent.ModifierFlags = .command) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        return item
    }

    func mainMenu() -> NSMenu {
        let main = NSMenu()
        func add(_ title: String, _ items: [NSMenuItem]) {
            let menu = NSMenu(title: title)
            items.forEach(menu.addItem)
            let holder = NSMenuItem()
            holder.submenu = menu
            main.addItem(holder)
        }
        add("AAW", [
            item("About AAW", #selector(NSApplication.orderFrontStandardAboutPanel(_:))),
            .separator(),
            item("Settings…", #selector(openSettings(_:)), ","),
            item("Install Command Line Tool…", #selector(installCommandLineTool(_:))),
            .separator(),
            item("Hide AAW", #selector(NSApplication.hide(_:)), "h"),
            item("Hide Others", #selector(NSApplication.hideOtherApplications(_:)), "h", [.command, .option]),
            .separator(),
            item("Quit AAW", #selector(NSApplication.terminate(_:)), "q"),
        ])
        let recent = NSMenuItem(title: "Open Recent", action: nil, keyEquivalent: "")
        recentMenu.delegate = self
        recent.submenu = recentMenu
        add("File", [
            item("New", #selector(newDocument(_:)), "n"),
            item("Open…", #selector(openDocument(_:)), "o"),
            recent,
            .separator(),
            item("Save As…", #selector(SongWindowController.saveDocumentAs(_:)), "S"),
            .separator(),
            item("Export Audio…", #selector(SongWindowController.exportAudio(_:)), "R"),
            item("Export MIDI Clip…", #selector(SongWindowController.exportMIDIClip(_:)), "E"),
            .separator(),
            item("Close", #selector(NSWindow.performClose(_:)), "w"),
        ])
        add("Edit", [
            item("Undo", #selector(SongWindowController.undoEdit(_:)), "z"),
            item("Redo", #selector(SongWindowController.redoEdit(_:)), "Z"),
            .separator(),
            item("Cut", #selector(NSText.cut(_:)), "x"),
            item("Copy", #selector(NSText.copy(_:)), "c"),
            item("Paste", #selector(NSText.paste(_:)), "v"),
            item("Duplicate", #selector(SongWindowController.duplicateSelection(_:)), "d"),
            item("Split", #selector(SongWindowController.splitSelection(_:)), "e"),
            item("Join", #selector(SongWindowController.joinSelection(_:)), "j"),
            item("Delete", #selector(SongWindowController.deleteSelection(_:)), "\u{8}", []),
            .separator(),
            item("Select All", #selector(NSResponder.selectAll(_:)), "a"),
        ])
        add("Track", [
            item("Add Track", #selector(SongWindowController.addTrack(_:)), "t"),
            item("Add MIDI Track", #selector(SongWindowController.addMIDITrack(_:)), "T"),
            item("Add Return", #selector(SongWindowController.addReturn(_:)), "t", [.command, .option]),
            .separator(),
            item("Group Tracks", #selector(SongWindowController.groupSelection(_:)), "g"),
            item("Ungroup", #selector(SongWindowController.ungroupSelection(_:)), "G"),
            .separator(),
            item("Freeze Track", #selector(SongWindowController.freezeSelection(_:)), "F"),
            .separator(),
            item("Rename", #selector(SongWindowController.renameSelection(_:)), "r"),
        ])
        // The count-in Record leads with, kept across launches.
        let countIn = NSMenu(title: "Count-In")
        for bars in RecordPreferences.countIns {
            let entry = item(RecordPreferences.name(bars), #selector(SongWindowController.chooseCountIn(_:)))
            entry.representedObject = Int(bars)
            countIn.addItem(entry)
        }
        let countInHolder = NSMenuItem(title: "Count-In", action: nil, keyEquivalent: "")
        countInHolder.submenu = countIn
        add("Transport", [
            item("Play or Stop", #selector(SongWindowController.togglePlay(_:)), " ", []),
            item("Return to Start Position", #selector(SongWindowController.returnToStart(_:)), "\r", []),
            item("Go to Beginning", #selector(SongWindowController.goToBeginning(_:)), String(UnicodeScalar(UInt16(NSHomeFunctionKey))!), []),
            item("Loop", #selector(SongWindowController.toggleLoop(_:)), "l", []),
            .separator(),
            item("Record", #selector(SongWindowController.toggleRecord(_:)), "r", []),
            countInHolder,
            // The Mac's keys as a piano, through the armed or selected MIDI track.
            item("Typing Keyboard", #selector(SongWindowController.toggleTypingKeyboard(_:)), "K"),
            .separator(),
            // A note left where the song is playing, for the agent to read.
            item("Add Marker", #selector(SongWindowController.addMarker(_:)), "m", []),
            item("Add Marker and Name It", #selector(SongWindowController.addNamedMarker(_:)), "M", []),
            item("Delete All Markers", #selector(SongWindowController.deleteAllMarkers(_:))),
        ])
        // The grid of the editor that has the keys, as Ableton's ⌘1 to ⌘4.
        let grid = NSMenu(title: "Grid")
        grid.addItem(item("Follow Zoom", #selector(SongWindowController.gridFollowsZoom(_:))))
        grid.addItem(.separator())
        for size in Grid.sizes() {
            let entry = item(Grid.name(size), #selector(SongWindowController.chooseGrid(_:)))
            // The bar is the song's, whatever its time signature.
            entry.representedObject = size == "4" ? "bar" : size
            grid.addItem(entry)
        }
        grid.addItem(.separator())
        grid.addItem(item("Finer", #selector(SongWindowController.gridFiner(_:)), "1"))
        grid.addItem(item("Coarser", #selector(SongWindowController.gridCoarser(_:)), "2"))
        grid.addItem(item("Triplets", #selector(SongWindowController.gridTriplets(_:)), "3"))
        grid.addItem(.separator())
        grid.addItem(item("Snap to Grid", #selector(SongWindowController.toggleSnap(_:)), "4"))
        let gridHolder = NSMenuItem(title: "Grid", action: nil, keyEquivalent: "")
        gridHolder.submenu = grid
        add("View", [
            item("Zoom In", #selector(SongWindowController.zoomIn(_:)), "="),
            item("Zoom Out", #selector(SongWindowController.zoomOut(_:)), "-"),
            item("Zoom to Fit", #selector(SongWindowController.zoomToFit(_:)), "0"),
            .separator(),
            gridHolder,
            .separator(),
            // Every row's automation lanes, as Ableton's A.
            item("Automation", #selector(SongWindowController.toggleAutomation(_:)), "a", []),
            .separator(),
            item("Browser", #selector(SongWindowController.toggleBrowser(_:)), "b", [.command, .option]),
            item("Devices", #selector(SongWindowController.toggleDevices(_:)), "d", [.command, .option]),
            item("Pattern", #selector(SongWindowController.togglePattern(_:)), "p", [.command, .option]),
            item("Agent", #selector(SongWindowController.toggleAgent(_:)), "c", [.command, .option]),
            .separator(),
            item("Analyzer Window", #selector(SongWindowController.openAnalyzer(_:)), "l", [.command, .option]),
        ])
        let window = NSMenu(title: "Window")
        window.addItem(item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"))
        window.addItem(item("Zoom", #selector(NSWindow.performZoom(_:))))
        window.addItem(.separator())
        // The changes with who made them: a record, not a view of the song,
        // so it is here and not in View or the transport bar.
        window.addItem(item("Activity", #selector(SongWindowController.toggleActivity(_:)), "a", [.command, .option]))
        let holder = NSMenuItem()
        holder.submenu = window
        main.addItem(holder)
        NSApp.windowsMenu = window
        return main
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        guard menu === recentMenu else { return }
        menu.removeAllItems()
        let entries = index.entries.prefix(Self.recentLimit)
        for project in entries {
            // A project that is not found is listed and cannot be chosen.
            let entry = item(project.name, project.missing ? nil : #selector(openRecent(_:)))
            entry.target = self
            entry.representedObject = project.url
            let folder = project.url.deletingLastPathComponent().path
            entry.toolTip = project.missing ? "Not found: \(folder)" : project.untitled ? "Not saved under a name yet" : folder
            menu.addItem(entry)
        }
        if !entries.isEmpty { menu.addItem(.separator()) }
        let clear = item("Clear Menu", entries.isEmpty ? nil : #selector(clearRecent(_:)))
        clear.target = self
        menu.addItem(clear)
    }
}

/// A project's window. Menu commands reach it through the responder chain.
/// A song's window. While the typing keyboard is on, its note keys are
/// taken here, before the menus' key equivalents and the views' keys see
/// them, and let go as they are lifted, which only the window sees.
final class SongWindow: NSWindow {
    var typing: ((NSEvent) -> Bool)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if typing?(event) == true { return true }
        return super.performKeyEquivalent(with: event)
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown || event.type == .keyUp, typing?(event) == true { return }
        super.sendEvent(event)
    }
}

final class SongWindowController: NSWindowController, NSWindowDelegate, NSMenuItemValidation {
    let model: SongModel
    /// Called when the window closed, with whether the project was deleted.
    var onClose: ((SongWindowController, Bool) -> Void)?
    /// Called when the window became the one in front.
    var onMain: ((SongWindowController) -> Void)?
    /// Whether closing asks about an Untitled project that holds something.
    private let asks: Bool
    /// The person chose Delete for an Untitled project.
    private var discard = false
    /// Whether the project was deleted when it closed; nil while it is open.
    private var deleted: Bool?

    init(model: SongModel, size: CGSize?, asks: Bool = true) {
        self.model = model
        self.asks = asks
        let window = SongWindow(
            contentRect: CGRect(origin: .zero, size: size ?? CGSize(width: 1280, height: 760)),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
        )
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.appearance = NSAppearance(named: .darkAqua)
        window.minSize = CGSize(width: 720, height: 480)
        window.contentView = NSHostingView(rootView: SongView(model: model))
        window.center()
        if size == nil { window.setFrameAutosaveName("song") }
        super.init(window: window)
        window.delegate = self
        window.typing = { [weak self] event in self?.typingKey(event) ?? false }
        model.onClosed = { [weak self] in self?.close() }
        followTitle()
    }

    /// A key of the typing keyboard, while it is on and no text is being
    /// typed: pressed, a note held through the MIDI tracks it plays, or the
    /// octave or velocity moved; lifted, the note let go. True when the key
    /// was taken, so neither a menu nor a view sees it.
    func typingKey(_ event: NSEvent) -> Bool {
        guard event.type == .keyDown || event.type == .keyUp, model.typingKeyboard else { return false }
        guard !(window?.firstResponder is NSText) else { return false }
        guard event.modifierFlags.intersection([.command, .control, .option]).isEmpty else { return false }
        let characters = event.charactersIgnoringModifiers ?? ""
        if event.type == .keyDown {
            return model.typingKeyDown(code: event.keyCode, characters: characters, isRepeat: event.isARepeat)
        }
        return model.typingKeyUp(code: event.keyCode, characters: characters)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    /// Keeps the window's title the song's title, with the project's folder
    /// beside it when that has another name. A project without a name shows
    /// no folder: its own is the app's.
    private func followTitle() {
        withObservationTracking {
            let (title, folder) = (model.arrangement.title, ProjectIndex.name(of: model.url))
            window?.title = title
            window?.subtitle = model.untitled || folder == title ? "" : folder
            window?.representedURL = model.untitled ? nil : model.url
        } onChange: { [weak self] in
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.followTitle() }
            }
        }
    }

    func windowDidBecomeMain(_ notification: Notification) {
        onMain?(self)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        mayClose()
    }

    /// Whether the project may close. An Untitled project that holds
    /// something is asked about: Save… gives it a name and a place, Delete
    /// lets it close and be deleted, and Cancel keeps it open.
    func mayClose() -> Bool {
        guard asks, !discard, deleted == nil,
              Closing.of(untitled: model.untitled, untouched: model.untouched) == .ask else { return true }
        showWindow(nil)
        switch Closing.ask(model.arrangement.title) {
        case .save: return saveAs()
        case .delete:
            discard = true
            return true
        case .cancel: return false
        }
    }

    /// Takes back a Delete answered while quitting, when the quit was cancelled.
    func keep() {
        discard = false
    }

    /// Stops hosting the project, and deletes an Untitled one that holds
    /// nothing or that the person chose to delete. Returns whether it was
    /// deleted.
    @discardableResult
    func finish() -> Bool {
        if let deleted { return deleted }
        let delete = model.untitled && (discard || model.untouched)
        let url = model.url
        model.close()
        if delete { try? projectDeleteUntitled(path: url.path) }
        deleted = delete
        return delete
    }

    func windowWillClose(_ notification: Notification) {
        onClose?(self, finish())
    }

    /// File › Export Audio…: asks for a name and a place, with the format
    /// and the level beside them, and writes the mix there with `daw
    /// export`. The panel opens in the project's `exports` folder, made if
    /// it is not there, with the song's title as the name, and asks about a
    /// file that is there already, as every Mac app does. The format and
    /// level chosen are offered next time.
    @objc func exportAudio(_ sender: Any?) {
        let panel = NSSavePanel()
        let choice = ExportChoice(options: ExportOptions(defaults: .standard))
        panel.title = "Export Audio"
        panel.prompt = "Export"
        panel.nameFieldLabel = "Name:"
        panel.canCreateDirectories = true
        panel.message = "The mix as one file at the level chosen below, with a record of what was measured beside it."
        panel.allowedContentTypes = [choice.options.format.contentType]
        panel.nameFieldStringValue = choice.options.fileName(title: model.arrangement.title)
        if !model.untitled {
            let folder = model.exportsFolder
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            panel.directoryURL = folder
        }
        // The format chooses the extension: the name follows it.
        choice.onFormat = { [weak panel] format in
            guard let panel else { return }
            panel.allowedContentTypes = [format.contentType]
            let name = (panel.nameFieldStringValue as NSString).deletingPathExtension
            panel.nameFieldStringValue = "\(name).\(format.fileExtension)"
        }
        panel.accessoryView = NSHostingView(rootView: ExportAccessory(choice: choice))
        guard panel.runModal() == .OK, let url = panel.url else { return }
        choice.options.remember(in: .standard)
        // The panel asked about a file that is there.
        let replace = FileManager.default.fileExists(atPath: url.path)
        model.export(to: url, options: choice.options, replace: replace)
    }

    /// File › Export MIDI Clip…: writes the selected note clip's notes as a
    /// MIDI file where the person says.
    @objc func exportMIDIClip(_ sender: Any?) {
        guard let (track, clip) = model.exportableClip else { return }
        let panel = NSSavePanel()
        panel.title = "Export MIDI Clip"
        panel.prompt = "Export"
        panel.nameFieldLabel = "Name:"
        panel.allowedContentTypes = [.midi]
        panel.canCreateDirectories = true
        panel.nameFieldStringValue = "\(track.id) \(clip.id).mid"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try model.exportMIDI(clip: clip.key, to: url)
        } catch {
            let alert = NSAlert()
            alert.messageText = "The clip could not be exported as “\(url.lastPathComponent)”"
            alert.informativeText = SongModel.reason(error)
            alert.runModal()
        }
    }

    @objc func saveDocumentAs(_ sender: Any?) {
        saveAs()
    }

    /// File › Save As…: asks for a name and a place, and saves the project
    /// there. An Untitled project moves; one that has a name is copied, and
    /// the window carries on in the copy. False when the person cancelled or
    /// the project could not be saved there.
    @discardableResult
    func saveAs() -> Bool {
        let panel = NSSavePanel()
        let title = model.arrangement.title
        panel.title = "Save As"
        panel.prompt = "Save"
        panel.nameFieldLabel = "Name:"
        panel.canCreateDirectories = true
        if model.untitled {
            panel.message = "Choose a name and a place for the project, which is kept as a folder."
            panel.nameFieldStringValue = title
        } else {
            panel.message = "Choose a name and a place for a copy of the project. This window carries on in the copy."
            panel.nameFieldStringValue = "\(title) copy"
            panel.directoryURL = model.url.deletingLastPathComponent().deletingLastPathComponent()
        }
        guard panel.runModal() == .OK, let folder = panel.url else { return false }
        do {
            try model.saveAs(folder)
            return true
        } catch {
            let alert = NSAlert()
            alert.messageText = "The project could not be saved as “\(folder.lastPathComponent)”"
            alert.informativeText = SongModel.reason(error)
            alert.runModal()
            return false
        }
    }

    @objc func togglePlay(_ sender: Any?) { model.togglePlay() }
    @objc func returnToStart(_ sender: Any?) { model.returnToStart() }
    @objc func goToBeginning(_ sender: Any?) { model.goToBeginning() }
    @objc func toggleLoop(_ sender: Any?) { model.toggleLoop() }
    @objc func toggleTypingKeyboard(_ sender: Any?) { model.toggleTypingKeyboard() }
    @objc func toggleRecord(_ sender: Any?) { model.toggleRecord() }
    @objc func chooseCountIn(_ sender: NSMenuItem) {
        if let bars = sender.representedObject as? Int { RecordPreferences.shared.countIn = UInt32(bars) }
    }
    @objc func addMarker(_ sender: Any?) { model.addMarker() }
    @objc func addNamedMarker(_ sender: Any?) { model.addMarker(ask: true) }
    @objc func deleteAllMarkers(_ sender: Any?) { model.removeAllMarkers() }
    @objc func zoomIn(_ sender: Any?) { model.zoom(.in) }
    @objc func zoomOut(_ sender: Any?) { model.zoom(.out) }
    @objc func zoomToFit(_ sender: Any?) { model.zoom(.fit) }
    @objc func toggleActivity(_ sender: Any?) { model.showsActivity.toggle() }
    @objc func toggleAutomation(_ sender: Any?) { model.toggleAutomation() }
    @objc func toggleAgent(_ sender: Any?) { model.showsAgent.toggle() }
    @objc func toggleBrowser(_ sender: Any?) { model.showsBrowser.toggle() }

    /// Shows the devices or the pattern in the detail panel, or hides the
    /// panel when it shows them already.
    private func toggle(_ detail: Detail) {
        if model.showsDetail, model.detail == detail {
            model.showsDetail = false
        } else {
            model.showsDetail = true
            model.detail = detail
        }
    }

    @objc func toggleDevices(_ sender: Any?) { toggle(.devices) }
    @objc func togglePattern(_ sender: Any?) { toggle(.pattern) }
    /// Opens the window of the selected analyzer, or of the first one on
    /// the row whose devices show.
    @objc func openAnalyzer(_ sender: Any?) { model.openShownAnalyzer() }

    /// The editor a Grid menu command is for: the piano roll or the pattern
    /// editor while it has the keys, else the timeline.
    private var gridPlace: GridPlace {
        switch window?.firstResponder {
        case is NoteEditor: .notes
        case is PatternEditor: .pattern
        default: .timeline
        }
    }

    @objc func gridFollowsZoom(_ sender: Any?) { model.timelineGrid = nil }
    @objc func chooseGrid(_ sender: NSMenuItem) {
        if let size = sender.representedObject as? String { model.chooseGrid(size: gridSize(size), in: gridPlace) }
    }

    /// A Grid menu item's size as a value: `bar` is the song's bar.
    private func gridSize(_ item: String) -> String { item == "bar" ? Grid.text(model.barBeats) : item }
    @objc func gridFiner(_ sender: Any?) { model.stepGrid(.finer, in: gridPlace) }
    @objc func gridCoarser(_ sender: Any?) { model.stepGrid(.coarser, in: gridPlace) }
    @objc func gridTriplets(_ sender: Any?) { model.stepGrid(.triplets, in: gridPlace) }
    @objc func toggleSnap(_ sender: Any?) { model.snapsToGrid.toggle() }
    @objc func undoEdit(_ sender: Any?) { model.undo() }
    @objc func redoEdit(_ sender: Any?) { model.redo() }
    @objc func duplicateSelection(_ sender: Any?) { model.duplicateSelection() }
    @objc func splitSelection(_ sender: Any?) { model.splitSelection() }
    @objc func joinSelection(_ sender: Any?) { model.joinSelection() }
    /// Delete, for what has the keys: in the pattern editor the selected
    /// events and nothing else, so that it never takes the clip being edited.
    @objc func deleteSelection(_ sender: Any?) {
        if window?.firstResponder is PatternEditor, model.selectedEvents.isEmpty {
            NSSound.beep()
        } else if window?.firstResponder is NoteEditor, model.selectedNotes.isEmpty {
            NSSound.beep()
        } else {
            model.deleteSelection()
        }
    }
    /// Where Copy and Paste act: in the piano roll, notes, and in the pattern
    /// editor, events, pasted where the clip or pattern was last clicked;
    /// elsewhere, clips, pasted at the start position.
    private var editPlace: EditPlace {
        switch window?.firstResponder {
        case is NoteEditor: .notes
        case is PatternEditor: .events
        default: .clips
        }
    }

    @objc func copy(_ sender: Any?) { model.copySelection(in: editPlace) }
    @objc func cut(_ sender: Any?) { model.cutSelection(in: editPlace) }
    @objc func paste(_ sender: Any?) { model.paste(in: editPlace) }
    @objc func addTrack(_ sender: Any?) { model.addTrack() }
    @objc func addMIDITrack(_ sender: Any?) { model.addTrack(midi: true) }
    @objc func addReturn(_ sender: Any?) { model.addReturn() }
    @objc func groupSelection(_ sender: Any?) { model.groupSelection() }
    @objc func ungroupSelection(_ sender: Any?) { model.ungroupSelection() }
    @objc func freezeSelection(_ sender: Any?) { model.freezeSelection() }
    @objc func renameSelection(_ sender: Any?) { model.renameSelection() }

    /// "Undo" with the step it would undo, and whose it is when not the
    /// person's own: "Undo Agent: Move clip beat at 16".
    static func title(_ verb: String, _ step: HistoryStep?) -> String {
        guard let step else { return verb }
        var label = step.label
        if label.count > 60 { label = label.prefix(59) + "…" }
        switch step.origin {
        case .user: return "\(verb) \(label)"
        case .agent: return "\(verb) Agent: \(label)"
        case .external: return "\(verb) File Edit: \(label)"
        }
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        // While a name is typed, the keys are the text's.
        let typing = window?.firstResponder is NSText
        switch item.action {
        case #selector(toggleLoop(_:)):
            item.state = model.transport.loopRegion == nil ? .off : .on
            return !typing
        case #selector(toggleTypingKeyboard(_:)): item.state = model.typingKeyboard ? .on : .off
        case #selector(toggleActivity(_:)): item.state = model.showsActivity ? .on : .off
        case #selector(toggleAutomation(_:)):
            item.state = model.showsAutomation ? .on : .off
            return !typing
        case #selector(toggleAgent(_:)): item.state = model.showsAgent ? .on : .off
        case #selector(toggleDevices(_:)): item.state = model.showsDetail && model.detail == .devices ? .on : .off
        case #selector(togglePattern(_:)):
            // The clip last selected: a pattern, or an audio clip.
            item.title = DetailView.clipTitle(model)
            item.state = model.showsDetail && model.detail == .pattern ? .on : .off
        case #selector(toggleBrowser(_:)): item.state = model.showsBrowser ? .on : .off
        case #selector(openAnalyzer(_:)): return model.shownAnalyzer != nil && !typing
        case #selector(gridFollowsZoom(_:)):
            item.state = gridPlace == .timeline && model.timelineGrid == nil ? .on : .off
            return gridPlace == .timeline && !typing
        case #selector(chooseGrid(_:)):
            guard let size = (item.representedObject as? String).map(gridSize) else { return false }
            item.state = Grid.size(model.grid(in: gridPlace), bar: model.barBeats) == size ? .on : .off
            return model.gridValues(in: gridPlace).contains(size) && !typing
        case #selector(gridTriplets(_:)):
            item.state = Grid.isTriplet(model.grid(in: gridPlace)) ? .on : .off
            return model.gridStep(.triplets, in: gridPlace) != nil && !typing
        case #selector(gridFiner(_:)): return model.gridStep(.finer, in: gridPlace) != nil && !typing
        case #selector(gridCoarser(_:)): return model.gridStep(.coarser, in: gridPlace) != nil && !typing
        case #selector(toggleSnap(_:)):
            item.state = model.snapsToGrid ? .on : .off
            return !typing
        case #selector(addMarker(_:)), #selector(addNamedMarker(_:)): return !typing
        case #selector(deleteAllMarkers(_:)): return !model.arrangement.markers.isEmpty && !typing
        case #selector(returnToStart(_:)): return model.transport.playing && !typing
        case #selector(togglePlay(_:)), #selector(goToBeginning(_:)): return !typing
        case #selector(toggleRecord(_:)):
            item.title = model.transport.recording ? "Stop Recording" : "Record"
            return (model.transport.recording || model.canRecord) && !typing
        case #selector(chooseCountIn(_:)):
            item.state = (item.representedObject as? Int).map(UInt32.init) == RecordPreferences.shared.countIn ? .on : .off
            return !typing
        case #selector(undoEdit(_:)):
            item.title = Self.title("Undo", model.undoStep)
            return model.undoStep != nil && !typing
        case #selector(redoEdit(_:)):
            item.title = Self.title("Redo", model.redoStep)
            return model.redoStep != nil && !typing
        case #selector(duplicateSelection(_:)): return model.canDuplicate && !typing
        case #selector(copy(_:)), #selector(cut(_:)):
            return !typing && model.canCopy(in: editPlace)
        case #selector(paste(_:)): return !typing && model.canPaste(in: editPlace)
        case #selector(splitSelection(_:)): return model.canSplit && !typing
        case #selector(joinSelection(_:)): return model.canJoin && !typing
        case #selector(deleteSelection(_:)): return model.canDelete && !typing
        case #selector(renameSelection(_:)): return model.canRename && !typing
        case #selector(groupSelection(_:)): return model.canGroup && !typing
        case #selector(ungroupSelection(_:)): return model.canUngroup && !typing
        case #selector(freezeSelection(_:)):
            item.title = model.selectedTrackFrozen == true ? "Unfreeze Track" : "Freeze Track"
            return model.selectedTrackFrozen != nil && !typing
        case #selector(addTrack(_:)), #selector(addMIDITrack(_:)), #selector(addReturn(_:)), #selector(saveDocumentAs(_:)): return !typing
        case #selector(exportMIDIClip(_:)): return model.exportableClip != nil && !typing
        case #selector(exportAudio(_:)): return model.exporting == nil && !typing
        default: break
        }
        return true
    }
}
