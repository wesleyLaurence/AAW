import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// The agent panel: the conversation's title and the project's threads at
/// the top, the conversation shown as it streams, the person's words in a
/// bubble and the agent's in blocks, the tools it runs folded into a row,
/// each request waiting for the person, and the field at the foot.
struct AgentPanelView: View {
    let model: SongModel

    /// The conversation at 13 points, the tools and notes at 11.
    static let textSize: CGFloat = 13
    static let smallSize: CGFloat = 11
    /// The fill of a code block, and of a command on a card.
    static let codeFill = Color(nsColor: Theme.gray(0.09))

    /// Whether a drag is over the panel, which outlines it.
    @State private var dropping = false

    /// What a drag may bring: files, a sample's path from the browser, a
    /// patch or a rack.
    static let dropTypes: [UTType] = [.fileURL, .utf8PlainText, .plainText, UTType(exportedAs: Browser.patchType), UTType(exportedAs: Browser.rackType)]

    var body: some View {
        let agent = model.agent
        VStack(alignment: .leading, spacing: 0) {
            Header(agent: agent)
            Divider()
            ZStack(alignment: .bottom) {
                if agent.entries.isEmpty {
                    // An empty conversation is blank (D133), except for
                    // the two refusals that say what to do.
                    VStack(spacing: 0) {
                        if let refusal = refusal(agent) {
                            Text(refusal)
                                .font(.system(size: Self.textSize))
                                .foregroundStyle(.secondary)
                                .padding(14)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        Spacer(minLength: 0)
                    }
                } else {
                    Conversation(agent: agent)
                }
                // The skills over the conversation's foot while `/` is
                // typed, or the song's things while `@` is, just above
                // the field.
                if agent.commandQuery != nil {
                    CommandList(agent: agent)
                        .padding(.horizontal, 10)
                        .padding(.bottom, 6)
                } else if agent.mentionQuery != nil {
                    MentionList(agent: agent)
                        .padding(.horizontal, 10)
                        .padding(.bottom, 6)
                }
            }
            .frame(maxHeight: .infinity)
            if let trouble = trouble(agent) {
                Divider()
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                    Text(trouble).font(.system(size: Self.smallSize)).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
            }
            Divider()
            Composer(agent: agent, model: model)
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.14)))
        // A drag from the Finder, the browser or elsewhere let go anywhere
        // over the panel goes with the next message.
        .onDrop(of: Self.dropTypes, isTargeted: $dropping) { providers in
            Self.take(providers, into: agent, browser: model.browser)
        }
        .overlay {
            if dropping {
                ZStack {
                    RoundedRectangle(cornerRadius: 8).fill(Who.user.color.opacity(0.1))
                    RoundedRectangle(cornerRadius: 8).stroke(Who.user.color, lineWidth: 2)
                    Text("Add to the message")
                        .font(.system(size: Self.textSize, weight: .semibold))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(Color(nsColor: Theme.header), in: RoundedRectangle(cornerRadius: 6))
                }
                .padding(4)
                .allowsHitTesting(false)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Agent")
    }

    /// Attaches what was dropped: each file URL; a path from the browser,
    /// with what the browser knows of the sample it dragged; a patch or a
    /// rack by name. True when a provider is read.
    static func take(_ providers: [NSItemProvider], into agent: AgentPanel, browser: Browser) -> Bool {
        var taken = false
        for provider in providers {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                taken = true
                provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier) { item, _ in
                    let url = (item as? URL) ?? (item as? Data).flatMap { URL(dataRepresentation: $0, relativeTo: nil) }
                    guard let url else { return }
                    DispatchQueue.main.async { MainActor.assumeIsolated { agent.attach(file: url) } }
                }
            } else if let type = [Browser.patchType, Browser.rackType].first(where: provider.hasItemConformingToTypeIdentifier) {
                taken = true
                let kind: AgentAttachment.Kind = type == Browser.patchType ? .patch : .rack
                provider.loadDataRepresentation(forTypeIdentifier: type) { data, _ in
                    guard let data, let name = String(data: data, encoding: .utf8) else { return }
                    DispatchQueue.main.async {
                        MainActor.assumeIsolated {
                            let file = kind == .patch ? browser.patches.first { $0.name == name }?.file : browser.racks.first { $0.name == name }?.file
                            agent.attach(AgentAttachments.make(kind: kind, name: name, file: file))
                        }
                    }
                }
            } else if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) {
                taken = true
                provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) { item, _ in
                    let text = (item as? String) ?? (item as? Data).flatMap { String(data: $0, encoding: .utf8) }
                    guard let text, text.hasPrefix("/"), FileManager.default.fileExists(atPath: text) else { return }
                    DispatchQueue.main.async {
                        MainActor.assumeIsolated {
                            // The browser's sample, when the path is the one it dragged.
                            let sample = browser.dragged?.path == text ? browser.dragged : nil
                            agent.attach(file: URL(fileURLWithPath: text), sample: sample)
                        }
                    }
                }
            }
        }
        return taken
    }

    /// What an empty conversation says when the agent cannot answer; nil
    /// when it can, and the conversation is blank.
    private func refusal(_ agent: AgentPanel) -> String? {
        switch agent.status {
        case .missing:
            "Claude Code was not found on this Mac. Install it, sign in by running `claude` in a terminal, and choose View › Agent again."
        case .signedOut:
            "Claude Code is signed out. Run `claude` in a terminal and sign in, then ask again."
        default:
            nil
        }
    }

    private func trouble(_ agent: AgentPanel) -> String? {
        switch agent.status {
        case .missing: agent.entries.isEmpty ? nil : "Claude Code was not found. Install it and sign in, then try again."
        case .signedOut: agent.entries.isEmpty ? nil : "Claude Code is signed out: run `claude` in a terminal and sign in."
        case .failed(let reason): "Claude Code failed: \(reason.prefix(200))"
        case .unknown, .ready: nil
        }
    }

    /// The agent's inline markdown read, each line kept.
    static func inline(_ text: String) -> AttributedString {
        (try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(text)
    }
}

// MARK: The header

/// The conversation's title, Stop while the agent works, `+` for a new
/// conversation and the clock that lists the project's conversations.
private struct Header: View {
    let agent: AgentPanel
    @State private var showsThreads = false

    var body: some View {
        HStack(spacing: 6) {
            Text(agent.current.map { $0.title.isEmpty ? "Untitled conversation" : $0.title } ?? "New conversation")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .accessibilityLabel("Conversation")
            Spacer(minLength: 0)
            if agent.working {
                ProgressView().controlSize(.mini)
                Button {
                    agent.stop()
                } label: {
                    Image(systemName: "stop.circle").font(.system(size: 14))
                }
                .buttonStyle(.borderless)
                .help("Stop the agent's turn")
                .accessibilityLabel("Stop")
            }
            Button {
                agent.newThread()
            } label: {
                Image(systemName: "plus").font(.system(size: 13, weight: .medium))
            }
            .buttonStyle(.borderless)
            .help("New conversation")
            .accessibilityLabel("New conversation")
            Button {
                showsThreads.toggle()
            } label: {
                Image(systemName: "clock").font(.system(size: 13, weight: .medium))
            }
            .buttonStyle(.borderless)
            .help("This project's conversations")
            .accessibilityLabel("Conversations")
            .popover(isPresented: $showsThreads, arrowEdge: .bottom) {
                ThreadsList(agent: agent, dismiss: { showsThreads = false })
            }
        }
        .padding(.horizontal, 12)
        .frame(height: TimelineLayout.rulerHeight - 1)
    }
}

/// The project's conversations, most recent first, each with when it was
/// last used, and Rename and Delete in its menu.
private struct ThreadsList: View {
    let agent: AgentPanel
    let dismiss: () -> Void
    @State private var renaming: String?
    @State private var name = ""

    private static let when: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if agent.threads.isEmpty {
                Text("No conversations yet.")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .padding(12)
            }
            ForEach(agent.threads) { thread in
                row(thread)
            }
        }
        .frame(width: 280)
        .padding(.vertical, 6)
    }

    private func row(_ thread: ThreadList.Entry) -> some View {
        let current = agent.currentID == thread.id
        return HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                if renaming == thread.id {
                    TextField("Title", text: $name)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 12))
                        .onSubmit {
                            agent.rename(thread.id, name)
                            renaming = nil
                        }
                        .onExitCommand { renaming = nil }
                } else {
                    Text(thread.title.isEmpty ? "Untitled conversation" : thread.title)
                        .font(.system(size: 12, weight: current ? .semibold : .regular))
                        .lineLimit(1)
                }
                Text(Self.when.localizedString(for: Date(timeIntervalSince1970: thread.last), relativeTo: Date()))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            Menu {
                Button("Rename") {
                    name = thread.title
                    renaming = thread.id
                }
                Button("Delete") {
                    agent.delete(thread.id)
                }
            } label: {
                Image(systemName: "ellipsis").font(.system(size: 12))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .accessibilityLabel("Conversation menu")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .background(current ? Color(nsColor: Theme.selectedRow) : Color.clear)
        .onTapGesture {
            guard renaming == nil else { return }
            agent.open(thread.id)
            dismiss()
        }
    }
}

// MARK: The conversation

/// The rows as they stream. The view follows the stream while the person is
/// at the bottom and stops once they scroll up, with a pill to go back.
private struct Conversation: View {
    let agent: AgentPanel
    @State private var following = true
    /// When the view last scrolled by itself, so that the jump is not read
    /// as the person scrolling up.
    @State private var autoScrolled = Date.distantPast
    @State private var viewport: CGFloat = 0

    /// How near the bottom still counts as at it.
    private static let nearBottom: CGFloat = 60
    private static let bottom = "bottom"

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(AgentPanel.fold(agent.entries)) { row in
                        RowView(agent: agent, row: row, streaming: agent.streamingID).equatable()
                    }
                    Color.clear.frame(height: 1).id(Self.bottom)
                }
                .padding(.vertical, 10)
                .background(GeometryReader { g in
                    let maxY = g.frame(in: .named("conversation")).maxY
                    Color.clear.onChange(of: maxY, initial: true) { _, maxY in saw(bottom: maxY) }
                })
            }
            .coordinateSpace(name: "conversation")
            .background(GeometryReader { g in
                Color.clear.onChange(of: g.size.height, initial: true) { _, height in viewport = height }
            })
            .onChange(of: agent.entries) {
                guard following else { return }
                autoScrolled = Date()
                proxy.scrollTo(Self.bottom, anchor: .bottom)
            }
            .onAppear {
                autoScrolled = Date()
                proxy.scrollTo(Self.bottom, anchor: .bottom)
            }
            .overlay(alignment: .bottom) {
                if !following {
                    Button {
                        following = true
                        autoScrolled = Date()
                        withAnimation { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                    } label: {
                        Image(systemName: "arrow.down")
                            .font(.system(size: 12, weight: .semibold))
                            .frame(width: 28, height: 28)
                            .background(Color(nsColor: Theme.header), in: Circle())
                            .overlay(Circle().stroke(Color.white.opacity(0.12)))
                    }
                    .buttonStyle(.plain)
                    .padding(.bottom, 10)
                    .help("Back to the latest")
                    .accessibilityLabel("Scroll to the end")
                }
            }
        }
    }

    /// The content's foot moved: at the bottom, or near it, the view
    /// follows the stream; further up, unless the view itself just
    /// scrolled, the person scrolled up and it stops.
    private func saw(bottom maxY: CGFloat) {
        let distance = maxY - viewport
        if distance <= Self.nearBottom {
            following = true
        } else if Date().timeIntervalSince(autoScrolled) > 0.5 {
            following = false
        }
    }
}

/// One row of the conversation.
private struct RowView: View, Equatable {
    let agent: AgentPanel
    let row: AgentPanel.Row
    let streaming: Int?

    nonisolated static func == (a: RowView, b: RowView) -> Bool { a.row == b.row && a.streaming == b.streaming }

    var body: some View {
        switch row {
        case .tools(let tools):
            ToolsRow(tools: tools)
        case .entry(let entry):
            switch entry {
            case .person(_, let text, let attachments):
                PersonRow(text: text, attachments: attachments)
            case .agent(let id, let text):
                AgentRow(text: text, streaming: streaming == id)
            case .tool:
                ToolLine(entry: entry)
                    .padding(.horizontal, 12)
            case .approval(_, let request, let summary, let allowed):
                ApprovalCard(agent: agent, request: request, summary: summary, allowed: allowed)
            case .note(_, let text):
                Text(text)
                    .font(.system(size: AgentPanelView.smallSize))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 12)
                    .frame(maxWidth: .infinity, alignment: .center)
            }
        }
    }
}

/// The person's words on a rounded fill in their color, set in from the
/// left; a skill they ran is its chip, then the words after it; a thing of
/// the song they named with `@` is on the color within the words; what went
/// with the message is a row of chips under the words.
private struct PersonRow: View {
    let text: String
    var attachments: [AgentAttachment] = []

    var body: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 36)
            VStack(alignment: .leading, spacing: 6) {
                if let (name, rest) = AgentCommands.split(text) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        CommandChip(name: name)
                        if !rest.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            words(rest.trimmingCharacters(in: .whitespacesAndNewlines))
                        }
                    }
                } else {
                    words(text)
                }
                if !attachments.isEmpty {
                    AttachmentChips(attachments: attachments, remove: nil)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Who.user.color.opacity(0.18), in: RoundedRectangle(cornerRadius: 12))
        }
        .padding(.horizontal, 12)
        .accessibilityLabel("You")
    }

    private func words(_ text: String) -> some View {
        Text(Self.styled(text, mentions: attachments.map(\.name)))
            .font(.system(size: AgentPanelView.textSize))
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
    }

    /// The words with each `@name` of something attached in white on the
    /// person's color, as the field drew it.
    static func styled(_ text: String, mentions names: [String]) -> AttributedString {
        var out = AttributedString(text)
        for (range, _) in AgentMentions.found(in: text, among: Set(names)) {
            guard let r = Range(range, in: out) else { continue }
            out[r].foregroundColor = .white
            out[r].backgroundColor = Who.user.color
        }
        return out
    }
}

/// The attachments as chips that wrap: a thumbnail or a symbol by kind,
/// the name, and `×` where they can still be taken off. A click shows what
/// the chip stands for, and whether its file is still there.
private struct AttachmentChips: View {
    let attachments: [AgentAttachment]
    let remove: ((Int) -> Void)?

    var body: some View {
        FlowLayout(spacing: 6) {
            ForEach(attachments) { attachment in
                AttachmentChip(attachment: attachment, remove: remove.map { remove in { remove(attachment.id) } })
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Attachments")
    }
}

private struct AttachmentChip: View {
    let attachment: AgentAttachment
    let remove: (() -> Void)?
    @State private var showsDetail = false

    var body: some View {
        HStack(spacing: 4) {
            if let picture = attachment.picture {
                Image(nsImage: picture.image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: 18, height: 18)
                    .clipShape(RoundedRectangle(cornerRadius: 3))
            } else {
                Image(systemName: attachment.kind.symbol)
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
                    .frame(width: 14)
            }
            Text(attachment.name)
                .font(.system(size: AgentPanelView.smallSize))
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: 160)
            if let remove {
                Button(action: remove) {
                    Image(systemName: "xmark")
                        .font(.system(size: 8, weight: .bold))
                        .foregroundStyle(.secondary)
                        .frame(width: 14, height: 14)
                }
                .buttonStyle(.plain)
                .help("Take it off the message")
                .accessibilityLabel("Remove \(attachment.name)")
            }
        }
        .padding(.leading, 5)
        .padding(.trailing, remove == nil ? 7 : 3)
        .padding(.vertical, 3)
        .background(Color(nsColor: Theme.header), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.1)))
        .contentShape(Rectangle())
        .onTapGesture { showsDetail.toggle() }
        .help(attachment.path ?? attachment.detail)
        .popover(isPresented: $showsDetail, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 4) {
                Text(attachment.name).font(.system(size: 12, weight: .semibold))
                if let path = attachment.path {
                    Text(path).font(.system(size: 11)).foregroundStyle(.secondary).textSelection(.enabled)
                }
                if let reference = attachment.reference {
                    Text(reference).font(.system(size: 11, design: .monospaced)).foregroundStyle(.secondary).textSelection(.enabled)
                }
                if !attachment.detail.isEmpty {
                    Text(attachment.detail).font(.system(size: 11)).foregroundStyle(.secondary)
                }
                if attachment.missing {
                    Text("This file is no longer there.").font(.system(size: 11)).foregroundStyle(.orange)
                }
            }
            .padding(10)
            .frame(maxWidth: 360, alignment: .leading)
        }
        .accessibilityLabel("\(attachment.kind.rawValue) \(attachment.name)")
    }
}

/// Rows of views that wrap at the width, left to right.
private struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        return place(width: width, subviews: subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let laid = place(width: bounds.width, subviews: subviews)
        for (subview, origin) in zip(subviews, laid.origins) {
            subview.place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), proposal: .unspecified)
        }
    }

    private func place(width: CGFloat, subviews: Subviews) -> (size: CGSize, origins: [CGPoint]) {
        var origins: [CGPoint] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        var widest: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            origins.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
            widest = max(widest, x - spacing)
        }
        return (CGSize(width: widest, height: y + rowHeight), origins)
    }
}

/// A skill's `/name` on a rounded fill in the person's color, as the field
/// draws it.
private struct CommandChip: View {
    let name: String

    var body: some View {
        Text("/" + name)
            .font(.system(size: 12, design: .monospaced))
            .foregroundStyle(.white)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(Who.user.color, in: RoundedRectangle(cornerRadius: 4))
            .accessibilityLabel("Skill \(name)")
    }
}

/// The agent's text in blocks, with a cursor at the end while it streams.
private struct AgentRow: View {
    let text: String
    let streaming: Bool
    @State private var blink = false

    var body: some View {
        let blocks = MarkdownBlocks.parse(text)
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                BlockView(block: block)
            }
            if streaming {
                RoundedRectangle(cornerRadius: 1)
                    .fill(Who.agent.color)
                    .frame(width: 2, height: 14)
                    .opacity(blink ? 0.2 : 1)
                    .onAppear { withAnimation(.easeInOut(duration: 0.6).repeatForever()) { blink = true } }
            }
        }
        .padding(.horizontal, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityLabel("Agent")
    }
}

/// One block of the agent's text.
private struct BlockView: View {
    let block: MarkdownBlock

    var body: some View {
        switch block {
        case .paragraph(let text):
            Text(AgentPanelView.inline(text))
                .font(.system(size: AgentPanelView.textSize))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        case .heading(let level, let text):
            Text(AgentPanelView.inline(text))
                .font(.system(size: level == 1 ? 17 : level == 2 ? 15 : AgentPanelView.textSize, weight: .semibold))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 4)
        case .list(let items):
            VStack(alignment: .leading, spacing: 3) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text(item.ordered ? "\(item.number)." : item.depth == 0 ? "•" : "◦")
                            .font(.system(size: AgentPanelView.textSize))
                            .foregroundStyle(.secondary)
                            .frame(minWidth: 14, alignment: .trailing)
                        Text(AgentPanelView.inline(item.text))
                            .font(.system(size: AgentPanelView.textSize))
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.leading, CGFloat(item.depth) * 16)
                }
            }
        case .code(_, let text, _):
            CodeBlock(text: text)
        case .quote(let text):
            HStack(alignment: .top, spacing: 8) {
                RoundedRectangle(cornerRadius: 1).fill(Color.secondary.opacity(0.5)).frame(width: 2)
                Text(AgentPanelView.inline(text))
                    .font(.system(size: AgentPanelView.textSize))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
        case .rule:
            Divider().padding(.vertical, 2)
        case .table(let text):
            ScrollView(.horizontal, showsIndicators: false) {
                Text(text)
                    .font(.system(size: AgentPanelView.smallSize, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize()
            }
        }
    }
}

/// Fenced code on a darker fill, in monospace, with Copy at its corner.
private struct CodeBlock: View {
    let text: String
    @State private var copied = false

    var body: some View {
        ZStack(alignment: .topTrailing) {
            ScrollView(.horizontal, showsIndicators: false) {
                Text(text)
                    .font(.system(size: 12, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize()
                    .padding(10)
                    .padding(.trailing, 20)
            }
            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(text, forType: .string)
                copied = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
            } label: {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .padding(4)
            .help("Copy")
            .accessibilityLabel("Copy")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AgentPanelView.codeFill, in: RoundedRectangle(cornerRadius: 6))
    }
}

/// Two or more tools in a row, folded into one line that opens.
private struct ToolsRow: View {
    let tools: [AgentPanel.Entry]
    @State private var open = false

    var body: some View {
        let results = tools.compactMap { if case .tool(_, _, _, _, let result, _) = $0 { result } else { nil } }
        let running = results.count < tools.count
        let failed = tools.contains { if case .tool(_, _, _, _, _, true) = $0 { true } else { false } }
        let commands = tools.allSatisfy { if case .tool(_, _, "Bash", _, _, _) = $0 { true } else { false } }
        let last = tools.last.map { if case .tool(_, _, let name, let summary, _, _) = $0 { name == "Bash" ? summary : "\(name) \(summary)" } else { "" } } ?? ""
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                ToolMark(running: running, failed: failed)
                Text(running ? "\(commands ? "Running" : "Using") \(tools.count) \(commands ? "commands" : "tools")" : "\(commands ? "Ran" : "Used") \(tools.count) \(commands ? "commands" : "tools")")
                    .font(.system(size: AgentPanelView.smallSize))
                    .foregroundStyle(.secondary)
                if !open {
                    Text(last)
                        .font(.system(size: AgentPanelView.smallSize, design: .monospaced))
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                Spacer(minLength: 0)
                Image(systemName: open ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
            .contentShape(Rectangle())
            .onTapGesture { open.toggle() }
            .help(open ? "Hide the tools" : "Show each tool")
            if open {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(tools) { tool in
                        ToolLine(entry: tool)
                    }
                }
                .padding(.leading, 16)
            }
        }
        .padding(.horizontal, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Tools")
    }
}

private struct ToolMark: View {
    let running: Bool
    let failed: Bool

    var body: some View {
        Image(systemName: failed ? "xmark.circle" : running ? "circle.dotted" : "checkmark.circle")
            .font(.system(size: 10))
            .foregroundStyle(failed ? Color.red : Color.secondary)
    }
}

/// One tool: gray while it runs, a tick or a cross after, and a click shows
/// what it returned.
private struct ToolLine: View {
    let entry: AgentPanel.Entry
    @State private var expanded = false

    var body: some View {
        if case .tool(_, _, let name, let summary, let result, let failed) = entry {
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    ToolMark(running: result == nil, failed: failed)
                    Text(name == "Bash" ? summary : "\(name) \(summary)")
                        .font(.system(size: AgentPanelView.smallSize, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(expanded ? nil : 2)
                        .textSelection(.enabled)
                    Spacer(minLength: 0)
                }
                if expanded, let result, !result.isEmpty {
                    Text(result.count > 4000 ? String(result.prefix(4000)) + "…" : result)
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(failed ? Color.red.opacity(0.9) : Color.secondary)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.leading, 16)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture { expanded.toggle() }
            .help(result == nil ? "Running" : "Click to show or hide what it returned")
        }
    }
}

/// A request the person answers: the command on the code block's fill,
/// Allow in the accent color.
private struct ApprovalCard: View {
    let agent: AgentPanel
    let request: String
    let summary: String
    let allowed: Bool?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(allowed == nil ? "The agent asks to run this:" : allowed == true ? "Allowed:" : "Refused:")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(allowed == nil ? Color.orange : Color.secondary)
            Text(summary)
                .font(.system(size: 12, design: .monospaced))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(AgentPanelView.codeFill, in: RoundedRectangle(cornerRadius: 6))
            if allowed == nil {
                HStack(spacing: 8) {
                    Button("Allow") { agent.answer(request, allow: true) }
                        .buttonStyle(.borderedProminent)
                        .tint(Color(nsColor: Theme.insertion))
                        .keyboardShortcut(.defaultAction)
                    Button("Refuse") { agent.answer(request, allow: false) }
                }
                .controlSize(.small)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.orange.opacity(allowed == nil ? 0.12 : 0.04), in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 12)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Approval")
    }
}

// MARK: The field

/// The field the person types in, in a rounded well with what is attached
/// as chips above it, `+` for a file or the selected clips at its left,
/// centered on the field's height, and the microphone and Send at its
/// right. Return sends; Shift-Return starts a new line.
private struct Composer: View {
    let agent: AgentPanel
    let model: SongModel
    @State private var height: CGFloat = ComposerTextView.fontSize + 4 + ComposerTextView.inset.height * 2

    var body: some View {
        let empty = agent.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && agent.attachments.isEmpty
        VStack(alignment: .leading, spacing: 0) {
            if agent.recording {
                RecordingStrip(agent: agent)
                    .padding(.horizontal, 8)
                    .padding(.top, 8)
                    .padding(.bottom, 2)
            }
            if !agent.attachments.isEmpty {
                AttachmentChips(attachments: agent.attachments, remove: { agent.removeAttachment($0) })
                    .padding(.horizontal, 8)
                    .padding(.top, 8)
                    .padding(.bottom, 2)
            }
            HStack(alignment: .bottom, spacing: 2) {
                // The `+` at the left, centered on the field's height (D132).
                Menu {
                    Button("Attach File…") { Self.chooseFiles(for: agent) }
                    Button("Attach Selected Clips") { agent.attachSelectedClips() }
                        .disabled(!agent.canAttachClips)
                    Divider()
                    Button(agent.recording ? "Stop Recording" : "Record Audio") { agent.toggleRecording() }
                } label: {
                    Image(systemName: "plus.circle").font(.system(size: 17))
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .frame(height: height)
                .help("Attach a file or the selected clips to the message, or record a sound for it through the input chosen in Settings › Audio; a file dropped or pasted here attaches too")
                .accessibilityLabel("Attach")
                ComposerField(agent: agent, model: model, height: $height)
                    .frame(height: height)
                    .accessibilityLabel("Message to the agent")
                    .help("What to ask or change. Return sends; Shift-Return starts a new line.")
                MicrophoneButton(agent: agent)
                Button {
                    agent.send()
                } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 20))
                        .foregroundStyle(agent.working || empty ? Color.secondary : Who.user.color)
                }
                .buttonStyle(.borderless)
                .disabled(agent.working || empty)
                .help("Send (Return)")
                .accessibilityLabel("Send")
                .padding(.trailing, 2)
            }
            .padding(.horizontal, 4)
            .padding(.vertical, 3)
        }
        .background(Color(nsColor: Theme.control), in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.white.opacity(0.08)))
        .padding(10)
    }

    /// The file panel, as a sheet of the window; what is chosen attaches.
    static func chooseFiles(for agent: AgentPanel) {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = true
        panel.canChooseFiles = true
        panel.message = "Add to the message"
        panel.prompt = "Attach"
        let handler: (NSApplication.ModalResponse) -> Void = { response in
            guard response == .OK else { return }
            agent.attach(files: panel.urls)
        }
        if let window = NSApp.keyWindow ?? NSApp.mainWindow {
            panel.beginSheetModal(for: window, completionHandler: handler)
        } else {
            panel.begin(completionHandler: handler)
        }
    }
}

/// While a sound is recorded for the message: a red dot that follows the
/// input's level, the seconds, Stop, and × to drop the sound.
private struct RecordingStrip: View {
    let agent: AgentPanel

    var body: some View {
        let level = CGFloat(min(1, agent.recordingLevel * 2))
        HStack(spacing: 8) {
            Circle()
                .fill(Color.red.opacity(0.55 + 0.45 * level))
                .frame(width: 10, height: 10)
                .scaleEffect(1 + level * 0.4)
                .animation(.easeOut(duration: 0.08), value: agent.recordingLevel)
                .frame(width: 14, height: 14)
            Text("Recording")
                .font(.system(size: AgentPanelView.smallSize, weight: .semibold))
            Text(Self.clock(agent.recordingSeconds))
                .font(.system(size: AgentPanelView.smallSize, design: .monospaced))
                .foregroundStyle(.secondary)
            Spacer(minLength: 4)
            Button("Stop") { agent.stopRecording() }
                .buttonStyle(.borderedProminent)
                .tint(.red)
                .controlSize(.small)
                .help("Stop recording: the sound attaches to the message (Return)")
            Button { agent.cancelRecording() } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.borderless)
            .help("Drop the recording (Escape)")
            .accessibilityLabel("Discard recording")
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Recording")
    }

    /// Seconds as m:ss.
    static func clock(_ seconds: Double) -> String {
        let whole = Int(max(0, seconds).rounded(.down))
        return String(format: "%d:%02d", whole / 60, whole % 60)
    }
}

/// The microphone: a click listens and the words go into the field; while
/// it listens the symbol is filled in the person's color with a ring that
/// follows the level, and a click stops. Where the Mac cannot listen, the
/// help says why and a click says so in the transport bar.
private struct MicrophoneButton: View {
    let agent: AgentPanel

    var body: some View {
        let listening = agent.listening
        let trouble = agent.speechTrouble
        Button {
            agent.toggleListening()
        } label: {
            ZStack {
                if listening {
                    Circle()
                        .stroke(Who.user.color.opacity(0.5), lineWidth: 2)
                        .frame(width: 20, height: 20)
                        .scaleEffect(1 + CGFloat(agent.inputLevel) * 0.6)
                        .animation(.easeOut(duration: 0.08), value: agent.inputLevel)
                }
                Image(systemName: listening ? "mic.fill" : "mic")
                    .font(.system(size: 15))
                    .foregroundStyle(listening ? Who.user.color : trouble == nil ? Color.primary : Color.secondary)
            }
            .frame(width: 24, height: 24)
        }
        .buttonStyle(.borderless)
        .help(listening ? "Listening: click, Return or a pause stops; Escape takes the words out"
              : trouble ?? "Speak to the agent: the words appear in the field, recognized on this Mac, for you to read and send")
        .accessibilityLabel(listening ? "Stop listening" : "Microphone")
    }
}

/// A row of a list over the field: what the keys choose.
private struct PickRow: Identifiable, Equatable {
    var id: String
    /// The name in monospace: `/roar-edit`, `@drums`.
    var lead: String
    /// After the name in the faint color: a skill's hint, a mention's reference.
    var hint: String
    /// The gray line under them.
    var description: String
}

/// The skills the agent runs by name, in their groups, filtered by what
/// was typed after `/`: each a name, its hint and a line of description.
private struct CommandList: View {
    let agent: AgentPanel

    var body: some View {
        let sections = agent.listed
        PickList(sections: sections.map { section in
            (section.group.title, section.commands.map { PickRow(id: $0.name, lead: "/" + $0.name, hint: $0.hint, description: $0.description) })
        }, selection: agent.commandSelection, empty: empty, label: "Skills") { row in
            if let command = sections.flatMap(\.commands).first(where: { $0.name == row.id }) { agent.choose(command) }
        }
    }

    /// What an empty list says: the handshake not yet answered, no engine,
    /// or nothing matching.
    private var empty: String {
        switch agent.status {
        case .missing: "Claude Code was not found, so no skills are listed."
        case .signedOut: "Claude Code is signed out, so no skills are listed."
        default: agent.commands.isEmpty && !agent.handshook ? "Loading skills…" : "No skill matches."
        }
    }
}

/// The song's things under `@`, in their groups, filtered by what was
/// typed: each `@name`, the reference `daw` takes after it and a line of
/// what the app knows.
private struct MentionList: View {
    let agent: AgentPanel

    var body: some View {
        let sections = agent.listedMentions
        PickList(sections: sections.map { section in
            (section.group.title, section.mentions.map { PickRow(id: $0.id, lead: "@" + $0.name, hint: $0.reference ?? $0.kind.rawValue, description: $0.detail) })
        }, selection: agent.mentionSelection, empty: agent.mentionables.isEmpty ? "Nothing in the song to name yet." : "Nothing in the song matches.", label: "Mentions") { row in
            if let mention = sections.flatMap(\.mentions).first(where: { $0.id == row.id }) { agent.choose(mention) }
        }
    }
}

/// The list over the conversation's foot while `/` or `@` is typed: rows
/// in groups under small headings, the keys' row on a fill, twelve rows
/// tall at the most and scrolling.
private struct PickList: View {
    let sections: [(title: String, rows: [PickRow])]
    let selection: Int
    let empty: String
    let label: String
    let choose: (PickRow) -> Void

    static let rowHeight: CGFloat = 38
    static let headingHeight: CGFloat = 22
    static let mostRows = 12

    var body: some View {
        let rows = sections.flatMap(\.rows)
        let selected = rows.indices.contains(selection) ? rows[selection] : nil
        let height = CGFloat(min(rows.count, Self.mostRows)) * Self.rowHeight + CGFloat(sections.count) * Self.headingHeight
        VStack(alignment: .leading, spacing: 0) {
            if rows.isEmpty {
                Text(empty)
                    .font(.system(size: AgentPanelView.smallSize))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 12)
                    .frame(height: Self.rowHeight)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(sections, id: \.title) { section in
                                Text(section.title)
                                    .font(.system(size: 10, weight: .semibold))
                                    .foregroundStyle(.secondary)
                                    .padding(.horizontal, 12)
                                    .frame(height: Self.headingHeight, alignment: .bottomLeading)
                                    .padding(.bottom, 2)
                                ForEach(section.rows) { row in
                                    self.row(row, selected: row == selected)
                                        .id(row.id)
                                }
                            }
                        }
                        .padding(.vertical, 4)
                    }
                    // No taller than its rows, twelve at the most, nor
                    // than the room over the field, which a fixed height
                    // would push the window out for.
                    .frame(maxHeight: min(height + 8, CGFloat(Self.mostRows) * Self.rowHeight))
                    .onChange(of: selection, initial: true) {
                        if let selected { proxy.scrollTo(selected.id) }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: Theme.header), in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.white.opacity(0.1)))
        .shadow(color: .black.opacity(0.35), radius: 8, y: 2)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(label)
    }

    private func row(_ row: PickRow, selected: Bool) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(row.lead)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(selected ? Color.primary : Color(nsColor: Theme.text))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if !row.hint.isEmpty {
                    Text(row.hint)
                        .font(.system(size: AgentPanelView.smallSize))
                        .foregroundStyle(Color(nsColor: Theme.faintText))
                        .lineLimit(1)
                }
            }
            if !row.description.isEmpty {
                Text(row.description)
                    .font(.system(size: AgentPanelView.smallSize))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
        .padding(.horizontal, 12)
        .frame(maxWidth: .infinity, minHeight: Self.rowHeight, maxHeight: Self.rowHeight, alignment: .leading)
        .background(selected ? Color(nsColor: Theme.selectedRow) : Color.clear)
        .contentShape(Rectangle())
        .onTapGesture { choose(row) }
        .accessibilityLabel(row.lead)
    }
}
