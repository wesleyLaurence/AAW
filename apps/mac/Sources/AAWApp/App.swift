import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// How the app was started: `AAW [SONG...]`, and for checking the app without
/// anyone at the screen, `--click X,Y`, `--drag X1,Y1,X2,Y2` and `--key
/// space|return|l` in the order to perform them, then `--snapshot PNG [--after
/// SECONDS] [--size WxH]`.
struct Launch {
    /// Input to feed the first window as if a person made it. Points are in
    /// the window's content, from its top left.
    enum Action {
        case click(CGPoint)
        case drag(CGPoint, CGPoint)
        case key(String)
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
            case "--click":
                let n = numbers()
                if n.count == 2 { actions.append(.click(CGPoint(x: n[0], y: n[1]))) }
            case "--drag":
                let n = numbers()
                if n.count == 4 { actions.append(.drag(CGPoint(x: n[0], y: n[1]), CGPoint(x: n[2], y: n[3]))) }
            case "--key":
                let keys = ["space": " ", "return": "\r"]
                if let name = rest.popFirst() { actions.append(.key(keys[name] ?? name)) }
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
        // Scripted input, half a second apart, then the picture.
        var delay = 0.5
        for action in launch.actions {
            later(delay) { $0.perform(action) }
            delay += 0.5
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
        func mouse(_ type: NSEvent.EventType, _ p: CGPoint) {
            let at = CGPoint(x: p.x, y: content.bounds.height - p.y)
            if let event = NSEvent.mouseEvent(
                with: type, location: at, modifierFlags: [], timestamp: now, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0, clickCount: 1, pressure: 1
            ) {
                NSApp.sendEvent(event)
            }
        }
        switch action {
        case .click(let p):
            mouse(.leftMouseDown, p)
            mouse(.leftMouseUp, p)
        case .drag(let from, let to):
            mouse(.leftMouseDown, from)
            mouse(.leftMouseDragged, CGPoint(x: (from.x + to.x) / 2, y: (from.y + to.y) / 2))
            mouse(.leftMouseDragged, to)
            mouse(.leftMouseUp, to)
        case .key(let characters):
            if let event = NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: now, windowNumber: window.windowNumber,
                context: nil, characters: characters, charactersIgnoringModifiers: characters, isARepeat: false,
                keyCode: characters == " " ? 49 : characters == "\r" ? 36 : 37
            ) {
                NSApp.sendEvent(event)
            }
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

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        switch item.action {
        case #selector(toggleLoop(_:)): item.state = model.transport.loopRegion == nil ? .off : .on
        case #selector(toggleActivity(_:)): item.state = model.showsActivity ? .on : .off
        case #selector(returnToStart(_:)): return model.transport.playing
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
