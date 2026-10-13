import AAWCore
import AppKit
import Observation

/// The conversation with the agent in a project's window: the project's
/// threads, the one shown, its messages as they stream, the tools the agent
/// runs with their results, the requests waiting for the person's answer,
/// and the state of the person's own Claude Code. One runs for each open
/// project, in the project's folder, so two windows have two agents at work.
@MainActor
@Observable
final class AgentPanel {
    /// A row of the conversation.
    enum Entry: Identifiable, Equatable {
        /// The person's words, and what went with them.
        case person(id: Int, text: String, attachments: [AgentAttachment] = [])
        case agent(id: Int, text: String)
        /// A tool the agent used: what it was given, and what came back.
        case tool(id: Int, use: String, name: String, summary: String, result: String?, failed: Bool)
        /// A request the person answers: allowed, refused, or still waiting.
        case approval(id: Int, request: String, summary: String, allowed: Bool?)
        /// A word from the panel: a resumed conversation, a stop, an error.
        case note(id: Int, text: String)

        var id: Int {
            switch self {
            case .person(let id, _, _), .agent(let id, _), .tool(let id, _, _, _, _, _), .approval(let id, _, _, _), .note(let id, _): id
            }
        }

        var isTool: Bool {
            if case .tool = self { return true }
            return false
        }
    }

    /// A row of the panel: an entry, or a run of tools folded into one.
    enum Row: Identifiable, Equatable {
        case entry(Entry)
        /// Two or more tools in a row, with no words between them.
        case tools([Entry])

        var id: Int {
            switch self {
            case .entry(let entry): entry.id
            case .tools(let entries): entries.first?.id ?? 0
            }
        }
    }

    /// The entries as rows: consecutive tools are one row, and one tool
    /// stays one row.
    static func fold(_ entries: [Entry]) -> [Row] {
        var rows: [Row] = []
        var run: [Entry] = []
        func flush() {
            if run.count == 1 { rows.append(.entry(run[0])) } else if run.count > 1 { rows.append(.tools(run)) }
            run = []
        }
        for entry in entries {
            if entry.isTool { run.append(entry) } else { flush(); rows.append(.entry(entry)) }
        }
        flush()
        return rows
    }

    /// Whether the person's Claude Code can be used.
    enum Status: Equatable {
        /// Not looked for yet.
        case unknown
        case ready
        /// Not found by the person's shell.
        case missing
        /// Found, and signed out: what it said.
        case signedOut(String)
        /// Found, and failed to run: what it said.
        case failed(String)
    }

    /// The project's threads, most recent first.
    private(set) var threads: [ThreadList.Entry] = []
    /// The thread shown; nil while a new conversation waits for its first message.
    private(set) var currentID: String?
    private(set) var entries: [Entry] = []
    /// Whether the agent is on a turn.
    private(set) var working = false
    private(set) var status = Status.unknown
    /// The entry whose text is streaming in, for the cursor at its end.
    private(set) var streamingID: Int?
    /// What the person is typing.
    var draft = "" {
        didSet {
            guard draft != oldValue else { return }
            listClosed = false
            draftChanged()
        }
    }

    // The skills under `/`.

    /// The commands the engine listed in its handshake, for `/` in the
    /// field; the last list the project saw until a new process answers.
    private(set) var commands: [AgentCommand] = []
    /// The commands in their groups, as the list shows them unfiltered.
    private(set) var sections: [AgentCommands.Section] = []
    /// Whether the running process has answered the handshake.
    private(set) var handshook = false
    /// The skill chosen from the list, a chip at the field's start while
    /// the draft still begins with its name.
    private(set) var chip: AgentCommand?
    /// What the person typed after `/`, while the list shows; nil when it
    /// does not.
    private(set) var commandQuery: String?
    /// The row the keys are on, over the sections' commands in order.
    private(set) var commandSelection = 0
    /// Escape closed the list for this draft; the next keystroke opens it.
    @ObservationIgnored private var listClosed = false
    /// The last list each project folder saw, for the moment before a new
    /// process answers.
    @ObservationIgnored private static var lastCommands: [String: [AgentCommand]] = [:]

    // What goes with the next message.

    /// The files and clips attached since the last message, as chips above
    /// the field, in the order they were added.
    private(set) var attachments: [AgentAttachment] = []
    /// The paths attached in this conversation, which the agent may read
    /// without a card.
    @ObservationIgnored private(set) var attachedPaths: Set<String> = []

    /// The song whose window this is; set once, after the model is made.
    @ObservationIgnored weak var song: SongModel?

    @ObservationIgnored private var process: AgentProcess?
    /// The thread and the folder the process was started for.
    @ObservationIgnored private var processThread: String?
    @ObservationIgnored private var processFolder: URL?
    @ObservationIgnored private var located: ClaudeCode.Located?
    @ObservationIgnored private var locating: Task<ClaudeCode.Located?, Never>?
    /// The messages the text of which streamed, so that the whole message
    /// that follows is not shown twice.
    @ObservationIgnored private var streamed: Set<String> = []
    /// The entry the current text block streams into.
    @ObservationIgnored private var streaming: Int?
    /// The input of each request waiting for the person, by its request ID.
    @ObservationIgnored private var pending: [String: [String: Any]] = [:] {
        didSet { awaitingAnswer = !pending.isEmpty }
    }
    /// Whether a request waits for the person's Allow or Refuse.
    private(set) var awaitingAnswer = false
    /// The song's revision when the agent's last turn ended: the person's
    /// changes after it go with the next message.
    @ObservationIgnored private var lastTurnRevision: UInt64 = 0
    /// Whether the person pressed Stop during this turn.
    @ObservationIgnored private var stopping = false
    /// The folders the project has had, whose records a thread's earlier
    /// messages may be in.
    @ObservationIgnored private var folders: [URL] = []
    @ObservationIgnored private var ids = 0
    /// The environment of the app, which the agent inherits.
    @ObservationIgnored private let environment: [String: String]

    init(environment: [String: String] = ProcessInfo.processInfo.environment) {
        self.environment = environment
    }

    /// The project's folder, where the agent works.
    private var folder: URL? { song?.url.deletingLastPathComponent() }

    private func next() -> Int {
        ids += 1
        return ids
    }

    // MARK: Threads

    /// Reads the project's threads, and Claude Code's own list for the
    /// folder, and shows the most recent one, if any, where it stopped.
    func start() {
        guard let folder else { return }
        folders = [folder]
        refreshThreads()
        if let first = threads.first { open(first.id) }
        if let last = Self.lastCommands[folder.path] { setCommands(last) }
        locating = Task.detached { [environment] in ClaudeCode.locate(environment: environment) }
        Task { [weak self] in
            guard let self, let task = locating else { return }
            let found = await task.value
            located = found
            if status == .unknown { status = found == nil ? .missing : .ready }
            warm()
        }
    }

    /// Starts the conversation's process before its first message, while
    /// the panel shows, so that the handshake's list of skills is there
    /// when `/` is typed; it waits on its input and costs no turn.
    func warm() {
        guard song?.showsAgent == true, !working, process == nil, folder != nil else { return }
        Task { [weak self] in
            guard let self else { return }
            if located == nil { located = await locating?.value }
            guard let located, !working, process == nil, let folder else { return }
            let id = currentID ?? UUID().uuidString.lowercased()
            _ = start(located, thread: id, folder: folder, resume: currentID != nil && hasRun(id))
        }
    }

    func refreshThreads() {
        guard let folder else { return }
        var list = ThreadList(project: folder)
        list.merge(ClaudeTranscripts.sessions(in: ClaudeTranscripts.folder(for: folder)))
        threads = list.entries
    }

    private func saveThreads(_ change: (inout ThreadList) -> Void) {
        guard let folder else { return }
        var list = ThreadList(project: folder)
        change(&list)
        do {
            try list.save()
        } catch {
            note("The thread list was not saved: \(error.localizedDescription)")
        }
        threads = list.entries
    }

    /// A new conversation: nothing shown until the first message, which
    /// starts it.
    func newThread() {
        guard !working else {
            song?.refuse("The agent is still working; wait for it, or press Stop, before starting another conversation")
            return
        }
        endProcess()
        currentID = nil
        entries = []
        attachedPaths = []
        warm()
    }

    /// Shows a thread where it stopped, drawn from Claude Code's record of
    /// it; the next message resumes it.
    func open(_ id: String) {
        guard !working else {
            song?.refuse("The agent is still working; wait for it, or press Stop, before changing conversations")
            return
        }
        guard id != currentID else { return }
        endProcess()
        currentID = id
        entries = []
        attachedPaths = []
        var found = false
        for folder in folders {
            let file = ClaudeTranscripts.folder(for: folder).appendingPathComponent("\(id).jsonl")
            guard FileManager.default.fileExists(atPath: file.path) else { continue }
            found = true
            for line in ClaudeTranscripts.read(file) {
                switch line {
                case .person(let text, let attached):
                    entries.append(.person(id: next(), text: text, attachments: stamped(attached)))
                case .agent(let text): entries.append(.agent(id: next(), text: text))
                case .tool(let name, let summary): entries.append(.tool(id: next(), use: "", name: name, summary: summary, result: nil, failed: false))
                }
            }
            break
        }
        if !found {
            note("Earlier messages are in Claude Code's own record, which was not found here; the conversation goes on from where it stopped.")
        }
        warm()
    }

    func rename(_ id: String, _ title: String) {
        let title = ThreadList.title(from: title)
        guard !title.isEmpty else { return }
        saveThreads { $0.rename(id, title) }
    }

    /// Takes a thread out of the project's list; Claude Code's own record
    /// of it stays. The panel shows a new conversation if it was the one
    /// shown.
    func delete(_ id: String) {
        if id == currentID {
            guard !working else {
                song?.refuse("The agent is still working; wait for it, or press Stop, before deleting this conversation")
                return
            }
            endProcess()
            currentID = nil
            entries = []
            attachedPaths = []
            warm()
        }
        saveThreads { $0.remove(id) }
    }

    /// The thread shown.
    var current: ThreadList.Entry? {
        currentID.flatMap { id in threads.first { $0.id == id } }
    }

    // MARK: Messages

    /// Sends what the person typed, with the app's context ahead of it and
    /// what they attached, to the thread shown, starting or resuming its
    /// process as needed. Attachments alone send with a word for them.
    func send() {
        var text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty {
            guard !attachments.isEmpty, !working else { return }
            text = "Here is what I attached."
        }
        let attached = attachments
        draft = ""
        chip = nil
        attachments = []
        send(text, attachments: attached)
    }

    func send(_ text: String, attachments attached: [AgentAttachment] = []) {
        guard !working else {
            song?.refuse("The agent is still working; wait for it, or press Stop")
            return
        }
        guard let song, let folder else { return }
        entries.append(.person(id: next(), text: text, attachments: attached))
        for path in attached.compactMap(\.path) { attachedPaths.insert(URL(fileURLWithPath: path).standardizedFileURL.path) }
        working = true
        stopping = false
        // A new conversation takes the ID its warmed process was started under.
        let id = currentID ?? processThread ?? UUID().uuidString.lowercased()
        let isNew = currentID == nil
        currentID = id
        let context = song.agentContext(since: lastTurnRevision)
        let message = AgentAttachments.content(text: AppContext.wrap(context: context, text: text, attachments: attached), attachments: attached)
        Task { [weak self] in
            guard let self else { return }
            if located == nil { located = await locating?.value }
            guard let located else {
                status = .missing
                working = false
                note("Claude Code was not found. Install it, sign in by running `claude` in a terminal, then try again.")
                return
            }
            if isNew {
                saveThreads { $0.note(id, title: ThreadList.title(from: text)) }
            }
            if process == nil || processThread != id || processFolder != folder {
                endProcess()
                guard start(located, thread: id, folder: folder, resume: !isNew && hasRun(id)) else {
                    working = false
                    return
                }
            }
            process?.send(["type": "user", "message": ["role": "user", "content": message]])
        }
    }

    /// Whether a thread has had a turn, so that its process resumes it
    /// rather than starting it under its ID.
    private func hasRun(_ id: String) -> Bool {
        if let entry = threads.first(where: { $0.id == id }), entry.last > entry.started { return true }
        return folders.contains { FileManager.default.fileExists(atPath: ClaudeTranscripts.folder(for: $0).appendingPathComponent("\(id).jsonl").path) }
    }

    /// Starts Claude Code for a thread in the project's folder.
    private func start(_ located: ClaudeCode.Located, thread: String, folder: URL, resume: Bool) -> Bool {
        let workspace = ClaudeCode.workspace(project: folder)
        let under = workspace.map { ClaudeCode.isUnder(folder, workspace: $0) } ?? false
        let daw = Exporter.daw
        let prompt = ClaudeCode.prompt(project: folder, dawOnPath: daw != nil, workspace: workspace,
                                       workspaceText: under ? nil : workspace.map(ClaudeCode.workspaceText))
        let arguments = ClaudeCode.arguments(resume: resume ? thread : nil, sessionID: thread, workspace: workspace, prompt: prompt)
        var environment = self.environment
        var path = located.path
        if let daw { path = daw.deletingLastPathComponent().path + ":" + path }
        environment["PATH"] = path
        do {
            process = try AgentProcess(
                executable: located.executable, arguments: arguments, folder: folder, environment: environment,
                onEvent: { [weak self] event in self?.took(event) },
                onExit: { [weak self] status, said in self?.exited(status: status, said: said) }
            )
        } catch {
            status = .failed(error.localizedDescription)
            note("Claude Code could not be started: \(error.localizedDescription)")
            return false
        }
        processThread = thread
        processFolder = folder
        streamed = []
        handshook = false
        // The handshake the stream-json input expects before its first
        // message, whose answer lists the commands.
        process?.send(["type": "control_request", "request_id": "init", "request": ["subtype": "initialize"]])
        return true
    }

    // MARK: The skills under `/`

    /// The list's rows in order: the filtered sections' commands.
    var listed: [AgentCommands.Section] {
        AgentCommands.filter(sections, query: commandQuery ?? "")
    }

    private var listedCommands: [AgentCommand] { listed.flatMap(\.commands) }

    /// Keeps the handshake's list, grouped by the project's and the
    /// workspace's skill folders, and remembers it for the folder.
    private func setCommands(_ commands: [AgentCommand]) {
        self.commands = commands
        guard let folder else { return }
        Self.lastCommands[folder.path] = commands
        let workspace = ClaudeCode.workspace(project: folder).map(AgentCommands.skillNames) ?? []
        sections = AgentCommands.grouped(commands, project: AgentCommands.skillNames(in: folder), workspace: workspace)
    }

    /// The draft changed: a chip whose name the draft no longer starts
    /// with goes, and `/` with a name typed so far shows the list.
    private func draftChanged() {
        if let chip, AgentCommands.split(draft)?.name != chip.name { self.chip = nil }
        var query: String?
        if chip == nil, draft.hasPrefix("/"), !draft.contains(where: \.isWhitespace), !listClosed {
            query = String(draft.dropFirst())
        }
        if query != commandQuery {
            commandQuery = query
            commandSelection = 0
        }
    }

    /// Moves the list's selection; false when no list shows.
    func moveSelection(by step: Int) -> Bool {
        guard commandQuery != nil else { return false }
        let count = listedCommands.count
        guard count > 0 else { return true }
        commandSelection = (commandSelection + step + count) % count
        return true
    }

    /// Chooses the selected row; false when no list shows.
    func chooseSelected() -> Bool {
        guard commandQuery != nil else { return false }
        let commands = listedCommands
        if commands.indices.contains(commandSelection) { choose(commands[commandSelection]) }
        return true
    }

    /// Puts the command in the field as a chip, with the caret after it.
    func choose(_ command: AgentCommand) {
        chip = command
        draft = "/\(command.name) "
        commandQuery = nil
    }

    /// Closes the list and leaves what was typed; false when no list shows.
    func closeList() -> Bool {
        guard commandQuery != nil else { return false }
        commandQuery = nil
        listClosed = true
        return true
    }

    // MARK: What goes with the next message

    /// Gives attachments their IDs, and remembers their paths as the
    /// conversation's.
    private func stamped(_ list: [AgentAttachment]) -> [AgentAttachment] {
        list.map { attachment in
            var stamped = attachment
            stamped.id = next()
            if let path = attachment.path { attachedPaths.insert(URL(fileURLWithPath: path).standardizedFileURL.path) }
            return stamped
        }
    }

    /// Adds one attachment for the next message; a file already attached
    /// is not added twice, and images stop at ten.
    func attach(_ attachment: AgentAttachment) {
        if let path = attachment.path, attachments.contains(where: { $0.path == path }) { return }
        if attachment.kind == .image, attachments.filter({ $0.kind == .image }).count >= AgentAttachments.mostImages {
            song?.refuse("Ten images go with one message at the most")
            return
        }
        var stamped = attachment
        stamped.id = next()
        attachments.append(stamped)
    }

    /// Attaches a file from the Finder or the browser.
    func attach(file url: URL, sample: SampleInfo? = nil) {
        attach(AgentAttachments.make(file: url, sample: sample))
    }

    func attach(files urls: [URL]) {
        for url in urls { attach(file: url) }
    }

    /// Attaches an image with no file, as one pasted, written under the
    /// project's `.daw/attachments/`.
    func attach(image: NSImage) {
        guard let folder else { return }
        guard let attachment = AgentAttachments.make(image: image, project: folder) else {
            song?.refuse("The image could not be read")
            return
        }
        attach(attachment)
    }

    /// Whether clips are selected in the window to attach.
    var canAttachClips: Bool { song?.selectedClips.isEmpty == false }

    /// Attaches the clips selected in the window, each by its reference.
    func attachSelectedClips() {
        guard let song else { return }
        let a = song.arrangement
        let clips = song.placedClips().filter { song.selectedClips.contains($0.key) }.sorted { ($0.track, $0.at) < ($1.track, $1.at) }
        guard !clips.isEmpty else {
            song.refuse("Select a clip on the timeline first")
            return
        }
        for clip in clips {
            let attachment = AgentAttachments.make(clip: clip, track: a.tracks.indices.contains(clip.track) ? a.tracks[clip.track] : nil) {
                TimelineLayout.position($0, beatsPerBar: a.barBeats, beatUnit: a.beatUnit)
            }
            if attachments.contains(where: { $0.reference == attachment.reference }) { continue }
            attach(attachment)
        }
    }

    /// What was dropped or pasted: files by their URLs, else an image, else
    /// a sample's path from the browser; true when something was taken.
    @discardableResult
    func attach(pasteboard board: NSPasteboard) -> Bool {
        if let urls = board.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL], !urls.isEmpty {
            attach(files: urls)
            return true
        }
        if board.canReadObject(forClasses: [NSImage.self], options: nil), let image = NSImage(pasteboard: board) {
            attach(image: image)
            return true
        }
        return false
    }

    func removeAttachment(_ id: Int) {
        attachments.removeAll { $0.id == id }
    }

    private func endProcess() {
        process?.end()
        process = nil
        processThread = nil
        processFolder = nil
        for (request, _) in pending { decided(request, allowed: false) }
    }

    /// Interrupts the turn.
    func stop() {
        guard working, let process else { return }
        stopping = true
        process.send(["type": "control_request", "request_id": "stop-\(next())", "request": ["subtype": "interrupt"]])
    }

    /// Answers a request the agent made.
    func answer(_ request: String, allow: Bool) {
        guard pending[request] != nil else { return }
        decided(request, allowed: allow)
    }

    private func decided(_ request: String, allowed: Bool) {
        let input = pending.removeValue(forKey: request) ?? [:]
        let response: [String: Any] = allowed
            ? ["behavior": "allow", "updatedInput": input]
            : ["behavior": "deny", "message": "The person refused this in the app's panel."]
        process?.send(["type": "control_response", "response": ["subtype": "success", "request_id": request, "response": response]])
        if let i = entries.firstIndex(where: { if case .approval(_, request, _, _) = $0 { true } else { false } }),
           case .approval(let id, _, let summary, _) = entries[i] {
            entries[i] = .approval(id: id, request: request, summary: summary, allowed: allowed)
        }
    }

    // MARK: What the process says

    private func took(_ event: [String: Any]) {
        switch event["type"] as? String {
        case "stream_event":
            guard let inner = event["event"] as? [String: Any] else { return }
            let message = event["api_message_id"] as? String
            switch inner["type"] as? String {
            case "content_block_start":
                if (inner["content_block"] as? [String: Any])?["type"] as? String == "text" {
                    let id = next()
                    entries.append(.agent(id: id, text: ""))
                    streaming = entries.count - 1
                    streamingID = id
                    if let message { streamed.insert(message) }
                }
            case "content_block_delta":
                if let delta = inner["delta"] as? [String: Any], delta["type"] as? String == "text_delta",
                   let text = delta["text"] as? String, let i = streaming, case .agent(let id, let had) = entries[i] {
                    entries[i] = .agent(id: id, text: had + text)
                }
            case "content_block_stop":
                streaming = nil
                streamingID = nil
            default:
                break
            }
        case "assistant":
            guard let message = event["message"] as? [String: Any], let blocks = message["content"] as? [[String: Any]] else { return }
            let id = message["id"] as? String
            for block in blocks {
                switch block["type"] as? String {
                case "text":
                    // Streamed already, unless the stream gave nothing.
                    if let id, streamed.contains(id) { continue }
                    if let text = block["text"] as? String, !text.isEmpty { entries.append(.agent(id: next(), text: text)) }
                case "tool_use":
                    guard let name = block["name"] as? String else { continue }
                    let input = block["input"] as? [String: Any] ?? [:]
                    let use = block["id"] as? String ?? ""
                    entries.append(.tool(id: next(), use: use, name: name, summary: ClaudeTranscripts.toolSummary(name: name, input: input), result: nil, failed: false))
                default:
                    break
                }
            }
        case "user":
            guard let message = event["message"] as? [String: Any], let blocks = message["content"] as? [[String: Any]] else { return }
            for block in blocks where block["type"] as? String == "tool_result" {
                guard let use = block["tool_use_id"] as? String else { continue }
                let text: String
                if let string = block["content"] as? String {
                    text = string
                } else if let parts = block["content"] as? [[String: Any]] {
                    text = parts.compactMap { $0["text"] as? String }.joined(separator: "\n")
                } else {
                    text = ""
                }
                let failed = block["is_error"] as? Bool ?? false
                if let i = entries.lastIndex(where: { if case .tool(_, use, _, _, _, _) = $0 { true } else { false } }),
                   case .tool(let id, _, let name, let summary, _, _) = entries[i] {
                    entries[i] = .tool(id: id, use: use, name: name, summary: summary, result: text, failed: failed)
                }
            }
        case "control_response":
            // The handshake's answer: the commands the engine runs by name.
            guard let response = event["response"] as? [String: Any], response["request_id"] as? String == "init" else { return }
            handshook = true
            if let inner = response["response"] as? [String: Any], let list = inner["commands"] as? [[String: Any]] {
                setCommands(list.compactMap(AgentCommand.init))
            }
        case "control_request":
            guard let request = event["request"] as? [String: Any], let id = event["request_id"] as? String else { return }
            if request["subtype"] as? String == "can_use_tool", let tool = request["tool_name"] as? String, let folder {
                let input = request["input"] as? [String: Any] ?? [:]
                pending[id] = input
                switch Approval.decide(tool: tool, input: input, project: folder, attached: attachedPaths) {
                case .allow:
                    decided(id, allowed: true)
                case .ask(let summary):
                    entries.append(.approval(id: next(), request: id, summary: summary, allowed: nil))
                    NSSound.beep()
                }
            } else {
                process?.send(["type": "control_response", "response": ["subtype": "error", "request_id": id, "error": "The AAW panel does not answer this request."]])
            }
        case "result":
            working = false
            streaming = nil
            streamingID = nil
            // A request still waiting at the turn's end, after a stop, is refused.
            for (request, _) in pending { decided(request, allowed: false) }
            if let song { lastTurnRevision = song.arrangement.revision }
            if let id = currentID { saveThreads { $0.note(id, title: "") } }
            if event["is_error"] as? Bool == true {
                let said = (event["result"] as? String) ?? (event["subtype"] as? String) ?? "error"
                if stopping {
                    note("Stopped.")
                } else if Self.isSignedOut(said) {
                    status = .signedOut(said)
                    note("Claude Code is signed out: run `claude` in a terminal and sign in, then try again.")
                } else {
                    note("The agent stopped: \(said)")
                }
            }
            stopping = false
        default:
            break
        }
    }

    /// The process ended: by itself after its input closed, or with a
    /// reason, such as being signed out.
    private func exited(status: Int32, said: String) {
        guard process != nil else { return }
        process = nil
        processThread = nil
        processFolder = nil
        let reason = said.trimmingCharacters(in: .whitespacesAndNewlines)
        if status != 0 {
            if Self.isSignedOut(reason) {
                self.status = .signedOut(reason)
                note("Claude Code is signed out: run `claude` in a terminal and sign in, then try again.")
            } else if working || !reason.isEmpty {
                self.status = .failed(reason)
                note("Claude Code ended (status \(status))\(reason.isEmpty ? "." : ": \(reason.suffix(400))")")
            }
        }
        if working {
            working = false
            streaming = nil
            streamingID = nil
            if let song { lastTurnRevision = song.arrangement.revision }
        }
        for (request, _) in pending { decided(request, allowed: false) }
    }

    /// Whether what Claude Code said means it has no login.
    static func isSignedOut(_ text: String) -> Bool {
        let lower = text.lowercased()
        return lower.contains("not logged in") || lower.contains("/login") || lower.contains("invalid api key")
            || lower.contains("please log in") || lower.contains("authentication_error")
    }

    private func note(_ text: String) {
        entries.append(.note(id: next(), text: text))
    }

    // MARK: The project

    /// The project was saved under another name: the process goes on in the
    /// new folder at the next message, and the thread list is read there.
    func projectMoved(from old: URL) {
        guard let folder else { return }
        folders.insert(folder, at: 0)
        if !working {
            endProcess()
            warm()
        }
        refreshThreads()
    }

    /// The window is closing.
    func close() {
        endProcess()
        locating?.cancel()
    }
}

extension SongModel {
    /// What goes ahead of the person's message: the transport, the selection
    /// and what the person changed in the window since the agent's last turn.
    func agentContext(since revision: UInt64) -> String {
        let a = arrangement
        func bar(_ beat: Double) -> String {
            TimelineLayout.position(beat, beatsPerBar: a.barBeats, beatUnit: a.beatUnit)
        }
        var lines: [String] = []
        var transport = "Start position \(bar(self.transport.cue))"
        if let loop = self.transport.loopRegion {
            transport += "; loop from \(bar(loop.start)) to \(bar(loop.start + loop.length))"
        } else {
            transport += "; no loop"
        }
        transport += self.transport.playing ? "; playing." : "; stopped."
        lines.append(transport)
        var selected: [String] = []
        for clip in placedClips() where selectedClips.contains(clip.key) {
            let track = a.tracks.indices.contains(clip.track) ? a.tracks[clip.track].id : "?"
            if let pattern = clip.clip { selected.append("clip of pattern \(pattern.pattern) on \(track) at \(bar(clip.at))") }
            if let audio = clip.audio { selected.append("audio clip \(audio.sample) on \(track) at \(bar(clip.at))") }
            if let notes = clip.notes { selected.append("note clip \(notes.id) on \(track) at \(bar(clip.at))") }
        }
        switch selectedRow {
        case .track(let key): if let t = a.tracks.first(where: { $0.key == key }) { selected.append("track \(t.id)") }
        case .group(let key): if let g = a.groups.first(where: { $0.key == key }) { selected.append("group \(g.id)") }
        case .bus(let key): if let r = a.returns.first(where: { $0.key == key }) { selected.append("return \(r.id)") }
        case .master: selected.append("the master")
        case nil: break
        }
        if let marker = selectedMarker, let m = a.markers.first(where: { $0.key == marker }) {
            selected.append("marker \(m.id) at \(bar(m.at))\(m.text.isEmpty ? "" : " (\(m.text))")")
        }
        if !selectedNotes.isEmpty { selected.append("\(selectedNotes.count) note\(selectedNotes.count == 1 ? "" : "s") in the piano roll") }
        if !selectedEvents.isEmpty { selected.append("\(selectedEvents.count) event\(selectedEvents.count == 1 ? "" : "s") in the pattern") }
        lines.append(selected.isEmpty ? "Nothing is selected." : "Selected: " + selected.joined(separator: "; ") + ".")
        let changes = activity.filter { $0.revision > revision && $0.origin != .agent }.reversed()
        if !changes.isEmpty {
            let shown = changes.prefix(12).map { "- \($0.label) (\($0.origin == .user ? "in the window" : "in the file"))" }
            var text = "Since your last turn the person changed:\n" + shown.joined(separator: "\n")
            if changes.count > 12 { text += "\n- and \(changes.count - 12) more" }
            lines.append(text)
        }
        return lines.joined(separator: "\n")
    }
}
