import AAWCore
import AppKit
import SwiftUI

/// The browser's Generate tab: a sound, a loop or a song made by ElevenLabs
/// from a description, as `daw generate` makes one at the person's request,
/// and below it what the song and the generated folder hold, played on a
/// click and dragged onto the timeline as the samples are. With no key, a
/// line and a way to Settings › ElevenLabs.
struct GenerateView: View {
    let model: SongModel
    private var browser: Browser { model.browser }
    private var generator: Generator { model.generator }
    private var account: ElevenLabsAccount { .shared }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                form
                Divider()
                results
            }
        }
        .onAppear {
            generator.refresh(song: model.url)
            if account.status == nil { account.refresh() }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            // A sound generated from the terminal, or a key saved in Settings.
            generator.refresh(song: model.url)
        }
    }

    @ViewBuilder private var form: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let status = account.status, status.kept != true {
                Text(status.kept == false
                    ? "Generating needs an ElevenLabs key of your own, kept in the Keychain."
                    : status.problem ?? "The key could not be read.")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Button("Open Settings…") { Self.openSettings() }
                    .controlSize(.small)
            } else {
                fields
            }
        }
        .padding(10)
    }

    @ViewBuilder private var fields: some View {
        let busy = generator.running != nil
        Picker("", selection: Binding(get: { generator.request.kind }, set: { generator.request.kind = $0 })) {
            ForEach(GenerateRequest.Kind.allCases, id: \.self) { Text($0.name).tag($0) }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .controlSize(.small)
        .disabled(busy)
        TextField(generator.request.kind == .song ? "Describe the song" : "Describe the sound",
                  text: Binding(get: { generator.request.text }, set: { generator.request.text = $0 }), axis: .vertical)
            .lineLimit(3...6)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .disabled(busy)
        length.disabled(busy)
        if let running = generator.running, let started = generator.started {
            HStack(spacing: 6) {
                ProgressView().controlSize(.small)
                TimelineView(.periodic(from: started, by: 1)) { context in
                    Text(Generator.progress(running, since: started, now: context.date))
                        .font(.system(size: 11))
                        .monospacedDigit()
                }
                Spacer(minLength: 0)
                Button("Cancel") { generator.cancel() }
                    .controlSize(.small)
            }
        } else {
            HStack {
                Spacer(minLength: 0)
                Button("Generate") { generator.generate(song: model.url) }
                    .controlSize(.small)
                    .keyboardShortcut(.return, modifiers: .command)
                    .disabled(generator.request.problem != nil)
                    .help(generator.request.problem ?? "Ask ElevenLabs, at your key's cost; the song keeps playing")
            }
        }
        if let error = generator.error { line(error) }
        if let made = generator.made { line(made) }
        ForEach(generator.notes, id: \.self) { line($0) }
    }

    /// The length, as the kind takes it: a sound's seconds or the model's
    /// choice, a loop's bars of the song's tempo, a song's seconds and
    /// whether it has a voice.
    @ViewBuilder private var length: some View {
        HStack(spacing: 6) {
            switch generator.request.kind {
            case .sound:
                Text("Length").font(.system(size: 11))
                TextField("Auto", value: Binding(get: { generator.request.soundSeconds }, set: { generator.request.soundSeconds = $0 }),
                          format: .number)
                    .frame(width: 48)
                Text("s").font(.system(size: 11)).foregroundStyle(.secondary)
            case .loop:
                Text("Length").font(.system(size: 11))
                TextField("", value: Binding(get: { generator.request.bars }, set: { generator.request.bars = $0 }), format: .number)
                    .frame(width: 48)
                Stepper("", value: Binding(get: { generator.request.bars }, set: { generator.request.bars = $0 }), in: 1...64)
                    .labelsHidden()
                Text(generator.request.bars == 1 ? "bar" : "bars").font(.system(size: 11)).foregroundStyle(.secondary)
            case .song:
                Text("Length").font(.system(size: 11))
                TextField("", value: Binding(get: { generator.request.songSeconds }, set: { generator.request.songSeconds = $0 }), format: .number)
                    .frame(width: 48)
                Text("s").font(.system(size: 11)).foregroundStyle(.secondary)
                Spacer(minLength: 0)
                Toggle("Instrumental", isOn: Binding(get: { generator.request.instrumental }, set: { generator.request.instrumental = $0 }))
                    .toggleStyle(.checkbox)
                    .font(.system(size: 11))
            }
        }
        .textFieldStyle(.roundedBorder)
        .controlSize(.small)
    }

    private func line(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10))
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
    }

    @ViewBuilder private var results: some View {
        let shown = generator.results.filter { browser.query.isEmpty || Generator.matches($0, browser.query) }
        let song = shown.filter(\.inProject)
        let folder = shown.filter { !$0.inProject }
        if !song.isEmpty {
            heading("This Song")
            ForEach(song, id: \.path) { row($0) }
        }
        if !folder.isEmpty {
            heading("Generated Folder")
            ForEach(folder, id: \.path) { row($0) }
        }
        if generator.results.isEmpty {
            Text("What you and the agent generate is listed here, this song's first, and kept in the generated folder, which the samples search too.")
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(10)
        } else if shown.isEmpty {
            Text("Nothing generated matches.")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .padding(10)
        } else {
            Text("\(model.sampleLanding) Drag onto the timeline for an audio clip, or onto a MIDI track's name for a Sampler.")
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
        }
    }

    private func heading(_ title: String) -> some View {
        Text(title)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.top, 8)
            .padding(.bottom, 2)
    }

    /// A result: its name and what it is, played on a click, + or a
    /// double-click to add it where the samples' + does, dragged as a
    /// sample, with Generate Again and Show Recipe in its menu.
    private func row(_ result: GeneratedInfo) -> some View {
        let sample = Generator.sample(result)
        let selected = browser.selected == sample.id
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(sample.name)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text(Generator.detail(result))
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                model.addSample(path: sample.path, name: Browser.padName(of: sample), note: nil)
            } label: {
                Image(systemName: "plus").frame(width: 18, height: 18)
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .help(model.sampleLanding)
        }
        .padding(.leading, 10)
        .padding(.trailing, 6)
        .frame(height: 34)
        .background(selected ? Color(nsColor: .unemphasizedSelectedContentBackgroundColor) : Color.clear)
        .contentShape(Rectangle())
        .help(Generator.help(result))
        .contextMenu {
            Button("Generate Again") { generator.again(result, song: model.url) }
                .disabled(result.fromPlan || generator.running != nil || account.status?.kept != true)
            Button("Show Recipe in Finder") {
                NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: result.recipe)])
            }
        }
        .onTapGesture(count: 2) { model.addSample(path: sample.path, name: Browser.padName(of: sample), note: nil) }
        .onTapGesture { browser.audition(sample) }
        .onDrag {
            browser.dragged = sample
            return NSItemProvider(object: sample.path as NSString)
        }
    }

    /// Settings, at its ElevenLabs pane.
    static func openSettings() {
        NSApp.sendAction(#selector(AppDelegate.openSettings(_:)), to: nil, from: SettingsWindowController.Pane.elevenLabs)
    }
}
