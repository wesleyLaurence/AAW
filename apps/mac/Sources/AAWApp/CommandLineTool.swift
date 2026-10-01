import AppKit

/// The `daw` inside the app's bundle and its link on the PATH. With the link
/// a terminal or an agent runs `daw` from any folder, and its commands for a
/// song open here reach this app, as those of a checkout's `daw` do.
struct CommandLineTool: Equatable {
    /// The bundle's `daw`.
    var binary: URL
    /// The link to it: `daw` in a folder on the PATH.
    var link: URL

    /// Where the link goes. /usr/local/bin is on the PATH of every shell
    /// (`/etc/paths`), and belongs to the system, so an administrator makes it.
    static let folder = URL(fileURLWithPath: "/usr/local/bin")

    /// What is at the link now.
    enum State: Equatable {
        case absent
        /// A link to this app's `daw`.
        case installed
        /// A link to something else, such as the `daw` of a copy of the app
        /// that has since moved.
        case elsewhere(String)
        /// A file or folder that is no link, which this app did not put there.
        case occupied
    }

    enum Failure: LocalizedError, Equatable {
        case occupied
        /// The person closed the request for a password.
        case cancelled
        case failed(String)

        var errorDescription: String? {
            switch self {
            case .occupied: "A file that is not a link is in the way."
            case .cancelled: "No password was given."
            case .failed(let reason): reason
            }
        }
    }

    /// The tool of the app whose executable this is, `Contents/Helpers/daw`
    /// beside its `Contents/MacOS`, or nil where there is none, as in a build
    /// that is not a bundle.
    static func bundled(executable: URL, folder: URL = folder) -> CommandLineTool? {
        let binary = executable.deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Helpers/daw")
        guard FileManager.default.isExecutableFile(atPath: binary.path) else { return nil }
        return CommandLineTool(binary: binary, link: folder.appendingPathComponent("daw"))
    }

    var state: State {
        let files = FileManager.default
        // The link's own attributes: one whose file is gone is still there.
        guard (try? files.attributesOfItem(atPath: link.path)) != nil else { return .absent }
        guard let target = try? files.destinationOfSymbolicLink(atPath: link.path) else { return .occupied }
        let linked = URL(fileURLWithPath: target, relativeTo: link.deletingLastPathComponent())
        let same = linked.resolvingSymlinksInPath().path == binary.resolvingSymlinksInPath().path
        return same ? .installed : .elsewhere(linked.path)
    }

    /// macOS runs an app opened from where it was downloaded out of a
    /// temporary copy, which a link would not outlast.
    var isTranslocated: Bool {
        binary.path.contains("/AppTranslocation/")
    }

    /// Whether the link takes an administrator: its folder, or the nearest
    /// above it that exists, is not the person's to write.
    var needsAdministrator: Bool {
        var folder = link.deletingLastPathComponent()
        while !FileManager.default.fileExists(atPath: folder.path), folder.path != "/" {
            folder.deleteLastPathComponent()
        }
        return !FileManager.default.isWritableFile(atPath: folder.path)
    }

    static func quoted(_ text: String) -> String {
        "'" + text.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// The shell command that makes the link, in place of one already there.
    var installCommand: String {
        let folder = Self.quoted(link.deletingLastPathComponent().path)
        return "/bin/mkdir -p \(folder) && /bin/ln -sfh \(Self.quoted(binary.path)) \(Self.quoted(link.path))"
    }

    var removeCommand: String {
        "/bin/rm -f \(Self.quoted(link.path))"
    }

    /// Makes the link, or points one already there at this app's `daw`. A
    /// file that is no link is left as it is.
    func install() throws {
        guard state != .occupied else { throw Failure.occupied }
        try run(installCommand)
    }

    /// Removes the link, whatever it points at.
    func remove() throws {
        switch state {
        case .absent: return
        case .occupied: throw Failure.occupied
        case .installed, .elsewhere: try run(removeCommand)
        }
    }

    /// Runs a command as the person, or where the folder needs it as an
    /// administrator, for which macOS asks for a password.
    private func run(_ command: String) throws {
        if needsAdministrator {
            let literal = command.replacingOccurrences(of: "\\", with: "\\\\")
                .replacingOccurrences(of: "\"", with: "\\\"")
            let script = NSAppleScript(source: "do shell script \"\(literal)\" with administrator privileges")
            var failure: NSDictionary?
            script?.executeAndReturnError(&failure)
            guard let failure else { return }
            if failure[NSAppleScript.errorNumber] as? Int == -128 { throw Failure.cancelled }
            throw Failure.failed(failure[NSAppleScript.errorMessage] as? String ?? "The command failed")
        }
        let shell = Process()
        let errors = Pipe()
        shell.executableURL = URL(fileURLWithPath: "/bin/sh")
        shell.arguments = ["-c", command]
        shell.standardError = errors
        try shell.run()
        let said = String(decoding: errors.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        shell.waitUntilExit()
        if shell.terminationStatus != 0 {
            throw Failure.failed(said.trimmingCharacters(in: .whitespacesAndNewlines))
        }
    }
}

extension CommandLineTool {
    /// Shows an alert and returns the index of the button chosen.
    @MainActor
    private static func alert(_ title: String, _ text: String, buttons: [String] = ["OK"]) -> Int {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = text
        buttons.forEach { alert.addButton(withTitle: $0) }
        return alert.runModal().rawValue - NSApplication.ModalResponse.alertFirstButtonReturn.rawValue
    }

    /// Install Command Line Tool, from the menu: says what is at the link
    /// now and what would change, and on the person's word changes it.
    @MainActor
    static func offer() {
        guard let executable = Bundle.main.executableURL, let tool = bundled(executable: executable) else {
            _ = alert("This build has no daw inside it", "apps/mac/build.sh makes an app with daw in its bundle.")
            return
        }
        let link = tool.link.path
        if tool.isTranslocated {
            _ = alert("Move AAW to the Applications folder first",
                      "macOS is running this copy from a temporary place, where a link to its daw would stop working. Move AAW, open it again, and install the command line tool from there.")
            return
        }
        let password = tool.needsAdministrator ? " macOS asks for an administrator's password to change that folder." : ""
        var change: (() throws -> Void)?
        var done = ("daw is installed", "A terminal or an agent can now run daw from any folder. Try daw --help.")
        switch tool.state {
        case .occupied:
            _ = alert("daw was not installed",
                      "\(link) is a file and not a link, so this app did not put it there. Move it away and install again.")
        case .installed:
            let chosen = alert("The daw command line tool is installed",
                               "\(link) is a link to the daw inside this app, at \(tool.binary.path).",
                               buttons: ["OK", "Remove"])
            if chosen == 1 {
                change = tool.remove
                done = ("daw is removed", "\(link) is gone. The daw inside the app is as it was.")
            }
        case .absent:
            let chosen = alert("Install the daw command line tool?",
                               "\(link) will be a link to the daw inside this app, so that a terminal or an agent can run daw from any folder.\(password)",
                               buttons: ["Install", "Cancel"])
            if chosen == 0 { change = tool.install }
        case .elsewhere(let other):
            let chosen = alert("Point the daw command line tool at this app?",
                               "\(link) is a link to \(other). It will be a link to the daw inside this app instead.\(password)",
                               buttons: ["Replace", "Cancel"])
            if chosen == 0 { change = tool.install }
        }
        guard let change else { return }
        do {
            try change()
            _ = alert(done.0, done.1)
        } catch Failure.cancelled {
            // The person chose not to; nothing changed.
        } catch {
            _ = alert("\(link) could not be changed", error.localizedDescription)
        }
    }
}
