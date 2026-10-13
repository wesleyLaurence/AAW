import AAWCore
import AppKit
import SwiftUI

/// The agent panel: the project's threads in a menu at the top, the
/// conversation shown as it streams, each tool the agent runs with its
/// result, each request waiting for the person, and the field the person
/// types in at the foot.
struct AgentPanelView: View {
    let model: SongModel

    var body: some View {
        let agent = model.agent
        VStack(alignment: .leading, spacing: 0) {
            header(agent)
            Divider()
            if agent.entries.isEmpty {
                Text(hint(agent))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Spacer(minLength: 0)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(agent.entries) { entry in
                                EntryRow(agent: agent, entry: entry)
                                    .id(entry.id)
                            }
                        }
                        .padding(.vertical, 4)
                    }
                    .onChange(of: agent.entries.last) {
                        if let last = agent.entries.last { proxy.scrollTo(last.id, anchor: .bottom) }
                    }
                    .onAppear {
                        if let last = agent.entries.last { proxy.scrollTo(last.id, anchor: .bottom) }
                    }
                }
            }
            if let trouble = trouble(agent) {
                Divider()
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                    Text(trouble).font(.system(size: 11)).foregroundStyle(.secondary)
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
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Agent")
    }

    private func header(_ agent: AgentPanel) -> some View {
        HStack(spacing: 8) {
            Menu {
                Button("New Conversation") { agent.newThread() }
                if !agent.threads.isEmpty { Divider() }
                ForEach(agent.threads) { thread in
                    Toggle(isOn: Binding(get: { agent.currentID == thread.id }, set: { if $0 { agent.open(thread.id) } })) {
                        Text(thread.title.isEmpty ? "Untitled conversation" : thread.title)
                    }
                }
            } label: {
                HStack(spacing: 4) {
                    Text(agent.current.map { $0.title.isEmpty ? "Untitled conversation" : $0.title } ?? "New conversation")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 8, weight: .semibold)).foregroundStyle(.secondary)
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize(horizontal: false, vertical: true)
            .help("This project's conversations with the agent. Choose one to go on with it, or start a new one.")
            .accessibilityLabel("Conversation")
            Spacer(minLength: 0)
            if agent.working {
                ProgressView().controlSize(.mini)
                Button {
                    agent.stop()
                } label: {
                    Image(systemName: "stop.circle").font(.system(size: 13))
                }
                .buttonStyle(.borderless)
                .help("Stop the agent's turn")
                .accessibilityLabel("Stop")
            }
        }
        .padding(.horizontal, 12)
        .frame(height: TimelineLayout.rulerHeight - 1)
    }

    private func hint(_ agent: AgentPanel) -> String {
        switch agent.status {
        case .missing:
            "Claude Code was not found on this Mac. Install it, sign in by running `claude` in a terminal, and choose View › Agent again."
        case .signedOut:
            "Claude Code is signed out. Run `claude` in a terminal and sign in, then ask again."
        default:
            "Ask the agent about this song, or for a change: it works in this project's folder with your instructions, taste and skills, and its daw commands land in the window. Each conversation is kept with the project."
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
}

/// The field the person types in, with Send. Return sends; Option-Return
/// starts a new line.
private struct Composer: View {
    let agent: AgentPanel
    let model: SongModel
    @FocusState private var focused: Bool

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField("Ask the agent…", text: Binding(get: { agent.draft }, set: { agent.draft = $0 }), axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...6)
                .font(.system(size: 12))
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background(Color(nsColor: Theme.control), in: RoundedRectangle(cornerRadius: 5))
                .focused($focused)
                .onSubmit { agent.send() }
                .onExitCommand { focused = false; model.onFocus?() }
                .accessibilityLabel("Message to the agent")
                .help("What to ask or change. Return sends; Option-Return starts a new line.")
            Button {
                agent.send()
            } label: {
                Image(systemName: "arrow.up.circle.fill").font(.system(size: 20))
            }
            .buttonStyle(.borderless)
            .disabled(agent.working || agent.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            .help("Send (Return)")
            .accessibilityLabel("Send")
        }
        .padding(10)
    }
}

/// One row of the conversation.
private struct EntryRow: View {
    let agent: AgentPanel
    let entry: AgentPanel.Entry
    @State private var expanded = false

    var body: some View {
        switch entry {
        case .person(_, let text):
            VStack(alignment: .leading, spacing: 4) {
                chip("you", Who.user.color)
                Text(text)
                    .font(.system(size: 12))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
        case .agent(_, let text):
            VStack(alignment: .leading, spacing: 4) {
                chip("agent", Who.agent.color)
                Text(Self.markdown(text))
                    .font(.system(size: 12))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
        case .tool(_, _, let name, let summary, let result, let failed):
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: failed ? "xmark.circle" : result == nil ? "circle.dotted" : "checkmark.circle")
                        .font(.system(size: 10))
                        .foregroundStyle(failed ? Color.red : Color.secondary)
                    Text(name == "Bash" ? summary : "\(name) \(summary)")
                        .font(.system(size: 11, design: .monospaced))
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
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture { expanded.toggle() }
            .help(result == nil ? "Running" : "Click to show or hide what it returned")
        case .approval(_, let request, let summary, let allowed):
            VStack(alignment: .leading, spacing: 6) {
                Text(allowed == nil ? "The agent asks to run this:" : allowed == true ? "Allowed:" : "Refused:")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(allowed == nil ? Color.orange : Color.secondary)
                Text(summary)
                    .font(.system(size: 11, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                if allowed == nil {
                    HStack(spacing: 8) {
                        Button("Allow") { agent.answer(request, allow: true) }
                            .keyboardShortcut(.defaultAction)
                        Button("Refuse") { agent.answer(request, allow: false) }
                    }
                    .controlSize(.small)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.orange.opacity(allowed == nil ? 0.12 : 0.04), in: RoundedRectangle(cornerRadius: 6))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Approval")
        case .note(_, let text):
            Text(text)
                .font(.system(size: 11))
                .italic()
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func chip(_ name: String, _ color: Color) -> some View {
        Text(name)
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(Color.black.opacity(0.8))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(color, in: RoundedRectangle(cornerRadius: 3))
    }

    /// The agent's text with its inline markdown read, each paragraph kept.
    static func markdown(_ text: String) -> AttributedString {
        (try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(text)
    }
}
