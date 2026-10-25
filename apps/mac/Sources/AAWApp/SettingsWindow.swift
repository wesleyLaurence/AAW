import AAWCore
import AppKit
import SwiftUI

/// The Settings window, AAW › Settings… (⌘,), with a pane for each thing
/// set, chosen in its toolbar as in every Mac app's: Audio, with the output
/// and input device, the song's sample rate, the bit depth of exported files
/// and the output buffer, each a menu of what the Mac offers; ElevenLabs,
/// with the key the Generate tab and `daw generate` use and the agent's
/// budget. A choice is kept across launches and reaches every open song at
/// once. The window opens on the pane shown last.
@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
    /// The song whose window is in front, whose rate the tab shows and sets.
    let frontSong: () -> SongModel?
    private let tabs = NSTabViewController()

    enum Pane: String, CaseIterable {
        case audio = "Audio"
        case elevenLabs = "ElevenLabs"

        var symbol: String {
            switch self {
            case .audio: "speaker.wave.2"
            case .elevenLabs: "waveform.badge.plus"
            }
        }
    }

    static let paneKey = "settings.pane"
    static let size = CGSize(width: 560, height: 440)

    init(frontSong: @escaping () -> SongModel?) {
        self.frontSong = frontSong
        tabs.tabStyle = .toolbar
        for pane in Pane.allCases {
            let view: AnyView = switch pane {
            case .audio: AnyView(AudioSettingsTab(preferences: .shared, frontSong: frontSong))
            case .elevenLabs: AnyView(ElevenLabsSettingsTab(account: .shared))
            }
            let content = NSHostingController(rootView: view.frame(width: Self.size.width, height: Self.size.height))
            let item = NSTabViewItem(viewController: content)
            item.label = pane.rawValue
            item.identifier = pane.rawValue
            item.image = NSImage(systemSymbolName: pane.symbol, accessibilityDescription: pane.rawValue)
            tabs.addTabViewItem(item)
        }
        let window = NSWindow(contentViewController: tabs)
        window.styleMask = [.titled, .closable, .miniaturizable]
        window.setContentSize(Self.size)
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.toolbarStyle = .preference
        window.appearance = NSAppearance(named: .darkAqua)
        super.init(window: window)
        window.delegate = self
        show(UserDefaults.standard.string(forKey: Self.paneKey).flatMap(Pane.init(rawValue:)) ?? .audio)
        window.center()
    }

    /// Shows a pane, and keeps it for the next time the window opens.
    func show(_ pane: Pane) {
        tabs.selectedTabViewItemIndex = Pane.allCases.firstIndex(of: pane) ?? 0
        window?.title = pane.rawValue
    }

    func windowWillClose(_ notification: Notification) {
        let shown = Pane.allCases[max(0, min(tabs.selectedTabViewItemIndex, Pane.allCases.count - 1))]
        UserDefaults.standard.set(shown.rawValue, forKey: Self.paneKey)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }
}

/// The Audio tab. The devices are listed again each second while it shows,
/// so a pair of AirPods connecting appears, and what plays through what is
/// read from the front song's host as often.
struct AudioSettingsTab: View {
    @Bindable var preferences: AudioPreferences
    let frontSong: () -> SongModel?
    @State private var status: AudioStatus?
    @State private var songRate: UInt32?
    @State private var exportFormat: ExportOptions.Format = ExportOptions(defaults: .standard).format
    private let tick = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    /// The song's rates the model allows.
    static let songRates: [UInt32] = [44100, 48000]

    private var bitDepth: Binding<ExportOptions.Format> {
        Binding(
            get: { exportFormat == .wav16 ? .wav16 : .wav24 },
            set: { format in
                exportFormat = format
                var options = ExportOptions(defaults: .standard)
                options.format = format
                options.remember(in: .standard)
            }
        )
    }

    private var sampleRate: Binding<UInt32> {
        Binding(
            get: { songRate ?? 48000 },
            set: { rate in
                songRate = rate
                frontSong()?.setSampleRate(rate)
            }
        )
    }

    var body: some View {
        Form {
            Section {
                Picker("Output:", selection: $preferences.output) {
                    ForEach(AudioPreferences.choices(preferences.outputs, chosen: preferences.output), id: \.self) { name in
                        Text(AudioWords.name(name)).tag(name)
                    }
                }
                Picker("Input:", selection: $preferences.input) {
                    ForEach(AudioPreferences.choices(preferences.inputs, chosen: preferences.input), id: \.self) { name in
                        Text(AudioWords.name(name)).tag(name)
                    }
                }
                Text(AudioWords.status(status, songRate: songRate ?? 48000))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Section {
                Picker("Sample Rate:", selection: sampleRate) {
                    ForEach(Self.songRates, id: \.self) { rate in
                        Text(AudioWords.kHz(rate)).tag(rate)
                    }
                }
                .disabled(frontSong() == nil)
                Text("The song's rate, which it is rendered and exported at. A device that does not offer it plays the song resampled to its own.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Picker("Bit Depth:", selection: bitDepth) {
                    Text("24-bit").tag(ExportOptions.Format.wav24)
                    Text("16-bit").tag(ExportOptions.Format.wav16)
                }
                Text("For exported WAV files, as File › Export Audio… offers them. Playback is 32-bit float; the device's own depth is the Mac's to set.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Section {
                Picker("Buffer:", selection: $preferences.buffer) {
                    ForEach(AudioPreferences.buffers, id: \.self) { frames in
                        Text("\(frames) frames · \(AudioWords.latency(buffer: frames, rate: status?.deviceSampleRate ?? songRate ?? 48000))").tag(frames)
                    }
                }
                Text("Smaller for playing the keys or recording, larger for a heavy song.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear(perform: refresh)
        .onReceive(tick) { _ in refresh() }
    }

    private func refresh() {
        preferences.refreshDevices()
        guard let song = frontSong() else {
            status = nil
            songRate = nil
            return
        }
        songRate = song.arrangement.sampleRate
        song.audioStatus { status = $0 }
    }
}

/// The ElevenLabs pane: the key, which the bundle's `daw` keeps in the
/// Keychain and which is never shown again; the plan it is on; and the
/// agent's daily budget. What is shown is `daw keys status`, asked each time
/// the pane appears.
struct ElevenLabsSettingsTab: View {
    @Bindable var account: ElevenLabsAccount
    @State private var key = ""
    @State private var confirmRemove = false

    /// The budgets the menu offers, in seconds a day; one set from the
    /// command line is listed too.
    static let budgets: [Double] = [0, 30, 60, 120, 300, 600, 1800, 3600]

    private var kept: Bool { account.status?.kept == true }

    /// Whether a key is kept, in a line.
    static func keyLine(_ status: KeyStatus?, checking: Bool) -> String {
        guard let status else { return checking ? "Checking…" : "Not checked yet" }
        switch status.kept {
        case true: return "Kept in the Keychain"
        case false: return "None"
        default: return "Could not be read"
        }
    }

    static func budgetName(_ seconds: Double) -> String {
        if seconds == 0 { return "None: ask every time" }
        if seconds >= 60, seconds.truncatingRemainder(dividingBy: 60) == 0 { return "\(Int(seconds / 60)) min a day" }
        return "\(GenerateRequest.length(seconds)) a day"
    }

    private var budget: Binding<Double> {
        Binding(
            get: { account.status?.budget?.perDay ?? 120 },
            set: { account.setBudget($0) }
        )
    }

    var body: some View {
        Form {
            Section {
                LabeledContent("Key:") {
                    HStack(spacing: 6) {
                        if account.busy { ProgressView().controlSize(.small) }
                        Text(Self.keyLine(account.status, checking: account.busy))
                    }
                }
                SecureField("New Key:", text: $key, prompt: Text(kept ? "Paste a key to replace this one" : "Paste your API key"))
                    .onSubmit(save)
                HStack {
                    Link("Make a key at elevenlabs.io", destination: ElevenLabsAccount.keysPage)
                        .font(.callout)
                    Spacer()
                    if kept {
                        Button("Remove…") { confirmRemove = true }
                            .disabled(account.busy)
                    }
                    Button("Save", action: save)
                        .disabled(key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || account.busy)
                }
                if let message = account.message {
                    Text(message)
                        .font(.callout)
                        .foregroundStyle(account.failed ? Color.red : Color.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
                Text("Kept in the macOS Keychain, for daw to send with each request; never shown, written to a file or sent anywhere but ElevenLabs. A request sends only its description and settings: no audio and nothing of the song leaves the Mac.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Section {
                LabeledContent("Plan:") {
                    Text(account.status?.planLine ?? (kept ? "Not known" : "—"))
                }
                if kept, let problem = account.status?.problem {
                    Text(problem)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
            }
            Section {
                Picker("Agent's Budget:", selection: budget) {
                    ForEach(Self.choices(account.status?.budget?.perDay), id: \.self) { seconds in
                        Text(Self.budgetName(seconds)).tag(seconds)
                    }
                }
                .disabled(account.status == nil || account.busy)
                Text(Self.budgetLine(account.status?.budget))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .formStyle(.grouped)
        .onAppear(perform: account.refresh)
        .confirmationDialog("Remove the ElevenLabs key?", isPresented: $confirmRemove) {
            Button("Remove", role: .destructive, action: account.remove)
        } message: {
            Text("Neither you nor the agent can generate until a key is entered again.")
        }
    }

    private func save() {
        guard !key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        account.save(key: key)
        key = ""
    }

    /// The menu's budgets, with the one set if it is not among them.
    static func choices(_ set: Double?) -> [Double] {
        guard let set, !budgets.contains(set) else { return budgets }
        return (budgets + [set]).sorted()
    }

    /// What the budget is for, and today's spend.
    static func budgetLine(_ budget: KeyStatus.Budget?) -> String {
        var text = "How much audio the agent may generate in a day without asking you; past it, it asks. What you generate in the Generate tab does not count."
        if let budget {
            text += " Today the agent has made \(GenerateRequest.length(budget.agentToday)) and you \(GenerateRequest.length(budget.personToday)); the day turns at midnight UTC."
        }
        return text
    }
}
