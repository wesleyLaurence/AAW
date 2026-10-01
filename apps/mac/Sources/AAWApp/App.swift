import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// How the app was started: `AAW [SONG...]`, and for checking the app without
/// anyone at the screen, `--click X,Y`, `--shift-click X,Y`, `--double-click
/// X,Y`, `--drag X1,Y1,X2,Y2`, `--key KEY`, `--type TEXT` and `--wait SECONDS`
/// in the order to perform them, then `--snapshot PNG [--after SECONDS]
/// [--size WxH]`.
struct Launch {
    /// Input to feed the first window as if a person made it. Points are in
    /// the window's content, from its top left.
    enum Action: Equatable {
        case click(CGPoint, shift: Bool = false, count: Int = 1)
        case drag(CGPoint, CGPoint)
        case key(Key)
        /// Time for something else to happen, such as an agent's command.
        case wait(Double)
    }

    /// A key press: a key by name, such as `space`, `delete`, `left` or `z`,
    /// after any of `cmd+`, `shift+` and `opt+`.
    struct Key: Equatable {
        var characters: String
        var code: UInt16
        var modifiers: NSEvent.ModifierFlags = []
        /// One character of typed text, rather than a key of its own.
        var typed = false

        /// Where each letter is on the keyboard, which menus match keys by.
        private static let letters: [Character: UInt16] = [
            "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12,
            "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "o": 31, "u": 32, "i": 34, "p": 35, "l": 37, "j": 38,
            "k": 40, "n": 45, "m": 46,
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
    /// Seconds between the last action and the picture.
    var after: Double = 1
    var size: CGSize?

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
            case "--drag":
                let n = numbers()
                if n.count == 4 { actions.append(.drag(CGPoint(x: n[0], y: n[1]), CGPoint(x: n[2], y: n[3]))) }
            case "--key":
                if let key = rest.popFirst().flatMap(Key.init) { actions.append(.key(key)) }
            case "--type":
                actions += (rest.popFirst() ?? "").map { .key(Key(typing: $0)) }
            case "--wait":
                if let seconds = rest.popFirst().flatMap(Double.init) { actions.append(.wait(seconds)) }
            case "--snapshot":
                snapshot = rest.popFirst().map { URL(fileURLWithPath: $0) }
            case "--after":
                after = rest.popFirst().flatMap(Double.init) ?? after
            case "--size":
                let parts = (rest.popFirst() ?? "").split(separator: "x").compactMap { Double($0) }
                if parts.count == 2 { size = CGSize(width: parts[0], height: parts[1]) }
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
    private var welcome: NSWindow?
    private var recentMenu = NSMenu(title: "Open Recent")
    private var terminating = false

    init(launch: Launch) {
        self.launch = launch
    }

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.mainMenu = mainMenu()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        for url in launch.songs { openSong(url) }
        if songs.isEmpty { showWelcome() }
        if launch.snapshot == nil || !launch.actions.isEmpty {
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
        if let snapshot = launch.snapshot {
            later(delay - 0.5 + launch.after) { $0.snapshot(to: snapshot) }
        }
    }

    private func later(_ seconds: Double, _ body: @escaping @MainActor (AppDelegate) -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) {
            MainActor.assumeIsolated { body(self) }
        }
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        for url in urls { openSong(url) }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        terminating = true
        songs.forEach { $0.model.close() }
    }

    // MARK: Songs

    /// A song's file: `song.yaml` in a folder, or the file itself.
    private static func songFile(_ url: URL) -> URL {
        var isFolder: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: url.path, isDirectory: &isFolder)
        let file = exists && isFolder.boolValue ? url.appendingPathComponent("song.yaml") : url
        return file.standardizedFileURL.resolvingSymlinksInPath()
    }

    func openSong(_ url: URL) {
        let file = Self.songFile(url)
        if let shown = songs.first(where: { $0.model.url == file }) {
            shown.showWindow(nil)
            return
        }
        do {
            let controller = SongWindowController(model: try SongModel(url: file), size: launch.size)
            controller.onClose = { [weak self] closed in self?.closed(closed) }
            songs.append(controller)
            controller.showWindow(nil)
            NSDocumentController.shared.noteNewRecentDocumentURL(file)
            welcome?.close()
            welcome = nil
        } catch {
            let alert = NSAlert()
            alert.messageText = "\(file.deletingLastPathComponent().lastPathComponent)/\(file.lastPathComponent) could not be opened"
            alert.informativeText = error.localizedDescription
            alert.runModal()
        }
    }

    private func closed(_ controller: SongWindowController) {
        songs.removeAll { $0 === controller }
        if songs.isEmpty, !terminating, launch.snapshot == nil { showWelcome() }
    }

    @objc func openDocument(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.message = "Choose a song.yaml, or the folder that holds one"
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.yaml, .folder]
        if panel.runModal() == .OK, let url = panel.url { openSong(url) }
    }

    @objc private func openRecent(_ sender: NSMenuItem) {
        if let url = sender.representedObject as? URL { openSong(url) }
    }

    @objc private func clearRecent(_ sender: Any?) {
        NSDocumentController.shared.clearRecentDocuments(sender)
    }

    private func showWelcome() {
        if welcome == nil {
            let view = WelcomeView(
                recents: NSDocumentController.shared.recentDocumentURLs,
                choose: { [weak self] in self?.openDocument(nil) },
                open: { [weak self] url in self?.openSong(url) }
            )
            let window = NSWindow(
                contentRect: CGRect(x: 0, y: 0, width: 440, height: 320),
                styleMask: [.titled, .closable], backing: .buffered, defer: false
            )
            window.title = "AAW"
            window.isReleasedWhenClosed = false
            window.appearance = NSAppearance(named: .darkAqua)
            window.contentView = NSHostingView(rootView: view)
            window.center()
            welcome = window
        }
        welcome?.makeKeyAndOrderFront(nil)
    }

    // MARK: Scripted input and snapshot

    /// Sends the first window an event as if a person had made it, so that it
    /// takes the path a real click or key press takes.
    private func perform(_ action: Launch.Action) {
        guard let window = songs.first?.window, let content = window.contentView else { return }
        let now = ProcessInfo.processInfo.systemUptime
        func mouse(_ type: NSEvent.EventType, _ p: CGPoint, flags: NSEvent.ModifierFlags = [], count: Int = 1) {
            let at = CGPoint(x: p.x, y: content.bounds.height - p.y)
            if let event = NSEvent.mouseEvent(
                with: type, location: at, modifierFlags: flags, timestamp: now, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0, clickCount: count, pressure: 1
            ) {
                NSApp.sendEvent(event)
            }
        }
        switch action {
        case .click(let p, let shift, let count):
            for n in 1...count {
                mouse(.leftMouseDown, p, flags: shift ? .shift : [], count: n)
                mouse(.leftMouseUp, p, flags: shift ? .shift : [], count: n)
            }
        case .drag(let from, let to):
            mouse(.leftMouseDown, from)
            mouse(.leftMouseDragged, CGPoint(x: (from.x + to.x) / 2, y: (from.y + to.y) / 2))
            mouse(.leftMouseDragged, to)
            mouse(.leftMouseUp, to)
        case .key(let key):
            // With Shift, a letter arrives as its capital, which menus match.
            let characters = key.modifiers.contains(.shift) ? key.characters.uppercased() : key.characters
            guard let event = NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: key.modifiers, timestamp: now,
                windowNumber: window.windowNumber, context: nil, characters: key.characters,
                charactersIgnoringModifiers: characters, isARepeat: false, keyCode: key.code
            ) else { return }
            // A key with Command is a menu's, as it is when a person presses it.
            if key.modifiers.contains(.command), NSApp.mainMenu?.performKeyEquivalent(with: event) == true { return }
            NSApp.sendEvent(event)
        case .wait:
            break
        }
    }

    private func snapshot(to url: URL) {
        guard let view = (songs.first?.window ?? welcome)?.contentView,
              let image = view.bitmapImageRepForCachingDisplay(in: view.bounds) else {
            exit(1)
        }
        view.cacheDisplay(in: view.bounds, to: image)
        do {
            try image.representation(using: .png, properties: [:])?.write(to: url)
        } catch {
            FileHandle.standardError.write(Data("\(error.localizedDescription)\n".utf8))
            exit(1)
        }
        NSApp.terminate(nil)
    }

    // MARK: Menus

    private func item(_ title: String, _ action: Selector?, _ key: String = "", _ modifiers: NSEvent.ModifierFlags = .command) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        return item
    }

    private func mainMenu() -> NSMenu {
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
            item("Hide AAW", #selector(NSApplication.hide(_:)), "h"),
            item("Hide Others", #selector(NSApplication.hideOtherApplications(_:)), "h", [.command, .option]),
            .separator(),
            item("Quit AAW", #selector(NSApplication.terminate(_:)), "q"),
        ])
        let recent = NSMenuItem(title: "Open Recent", action: nil, keyEquivalent: "")
        recentMenu.delegate = self
        recent.submenu = recentMenu
        add("File", [
            item("Open…", #selector(openDocument(_:)), "o"),
            recent,
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
            item("Delete", #selector(SongWindowController.deleteSelection(_:)), "\u{8}", []),
            .separator(),
            item("Select All", #selector(NSResponder.selectAll(_:)), "a"),
        ])
        add("Track", [
            item("Add Track", #selector(SongWindowController.addTrack(_:)), "t"),
            item("Add Return", #selector(SongWindowController.addReturn(_:)), "t", [.command, .option]),
            .separator(),
            item("Rename", #selector(SongWindowController.renameSelection(_:)), "r"),
        ])
        add("Transport", [
            item("Play or Stop", #selector(SongWindowController.togglePlay(_:)), " ", []),
            item("Return to Start Position", #selector(SongWindowController.returnToStart(_:)), "\r", []),
            item("Loop", #selector(SongWindowController.toggleLoop(_:)), "l", []),
        ])
        add("View", [
            item("Zoom In", #selector(SongWindowController.zoomIn(_:)), "="),
            item("Zoom Out", #selector(SongWindowController.zoomOut(_:)), "-"),
            item("Zoom to Fit", #selector(SongWindowController.zoomToFit(_:)), "0"),
            .separator(),
            item("Activity", #selector(SongWindowController.toggleActivity(_:)), "a", [.command, .option]),
        ])
        let window = NSMenu(title: "Window")
        window.addItem(item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"))
        window.addItem(item("Zoom", #selector(NSWindow.performZoom(_:))))
        let holder = NSMenuItem()
        holder.submenu = window
        main.addItem(holder)
        NSApp.windowsMenu = window
        return main
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        guard menu === recentMenu else { return }
        menu.removeAllItems()
        let urls = NSDocumentController.shared.recentDocumentURLs
        for url in urls {
            let entry = item(WelcomeView.name(of: url), #selector(openRecent(_:)))
            entry.target = self
            entry.representedObject = url
            entry.toolTip = url.path
            menu.addItem(entry)
        }
        if !urls.isEmpty { menu.addItem(.separator()) }
        let clear = item("Clear Menu", urls.isEmpty ? nil : #selector(clearRecent(_:)))
        clear.target = self
        menu.addItem(clear)
    }
}

/// A song's window. Menu commands reach it through the responder chain.
final class SongWindowController: NSWindowController, NSWindowDelegate, NSMenuItemValidation {
    let model: SongModel
    var onClose: ((SongWindowController) -> Void)?

    init(model: SongModel, size: CGSize?) {
        self.model = model
        let window = NSWindow(
            contentRect: CGRect(origin: .zero, size: size ?? CGSize(width: 1280, height: 760)),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
        )
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.appearance = NSAppearance(named: .darkAqua)
        window.minSize = CGSize(width: 720, height: 360)
        window.representedURL = model.url
        window.subtitle = model.url.deletingLastPathComponent().lastPathComponent
        window.contentView = NSHostingView(rootView: SongView(model: model))
        window.center()
        if size == nil { window.setFrameAutosaveName("song") }
        super.init(window: window)
        window.delegate = self
        model.onClosed = { [weak self] in self?.close() }
        followTitle()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    /// Keeps the window's title the song's title.
    private func followTitle() {
        withObservationTracking {
            window?.title = model.arrangement.title
        } onChange: { [weak self] in
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.followTitle() }
            }
        }
    }

    func windowWillClose(_ notification: Notification) {
        model.close()
        onClose?(self)
    }

    @objc func togglePlay(_ sender: Any?) { model.togglePlay() }
    @objc func returnToStart(_ sender: Any?) { model.returnToStart() }
    @objc func toggleLoop(_ sender: Any?) { model.toggleLoop() }
    @objc func zoomIn(_ sender: Any?) { model.zoom(.in) }
    @objc func zoomOut(_ sender: Any?) { model.zoom(.out) }
    @objc func zoomToFit(_ sender: Any?) { model.zoom(.fit) }
    @objc func toggleActivity(_ sender: Any?) { model.showsActivity.toggle() }
    @objc func undoEdit(_ sender: Any?) { model.undo() }
    @objc func redoEdit(_ sender: Any?) { model.redo() }
    @objc func duplicateSelection(_ sender: Any?) { model.duplicateSelection() }
    @objc func deleteSelection(_ sender: Any?) { model.deleteSelection() }
    @objc func addTrack(_ sender: Any?) { model.addTrack() }
    @objc func addReturn(_ sender: Any?) { model.addReturn() }
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
        case #selector(toggleActivity(_:)): item.state = model.showsActivity ? .on : .off
        case #selector(returnToStart(_:)): return model.transport.playing && !typing
        case #selector(togglePlay(_:)): return !typing
        case #selector(undoEdit(_:)):
            item.title = Self.title("Undo", model.undoStep)
            return model.undoStep != nil && !typing
        case #selector(redoEdit(_:)):
            item.title = Self.title("Redo", model.redoStep)
            return model.redoStep != nil && !typing
        case #selector(duplicateSelection(_:)): return !model.selectedClips.isEmpty && !typing
        case #selector(deleteSelection(_:)): return model.canDelete && !typing
        case #selector(renameSelection(_:)): return model.canRename && !typing
        case #selector(addTrack(_:)), #selector(addReturn(_:)): return !typing
        default: break
        }
        return true
    }
}

struct WelcomeView: View {
    let recents: [URL]
    let choose: () -> Void
    let open: (URL) -> Void

    /// A song by its folder, which is how songs are told apart.
    static func name(of url: URL) -> String {
        url.deletingLastPathComponent().lastPathComponent
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("AAW").font(.system(size: 28, weight: .bold))
            Text("Open a song to see its arrangement and play it. While it is open, an agent's `daw` commands for the song land here as they are made.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Button("Open Song…", action: choose)
                .keyboardShortcut(.defaultAction)
            if !recents.isEmpty {
                Divider()
                Text("Recent").font(.caption).foregroundStyle(.secondary)
                ForEach(recents.prefix(6), id: \.self) { url in
                    Button {
                        open(url)
                    } label: {
                        Label(Self.name(of: url), systemImage: "music.note.list")
                    }
                    .buttonStyle(.link)
                    .help(url.path)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(24)
        .frame(width: 440, height: 320, alignment: .topLeading)
        .background(Color(nsColor: Theme.gray(0.14)))
        .preferredColorScheme(.dark)
    }
}
