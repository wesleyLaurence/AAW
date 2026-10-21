import AAWCore
import AppKit
import SwiftUI

/// The Settings window, AAW › Settings… (⌘,): the Audio pane, with the
/// output and input device, the song's sample rate, the bit depth of
/// exported files and the output buffer, each a menu of what the Mac
/// offers. A choice is kept across launches and reaches every open song's
/// host at once. A second pane, when one comes, makes the toolbar of tabs.
@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
    /// The song whose window is in front, whose rate the tab shows and sets.
    let frontSong: () -> SongModel?

    init(frontSong: @escaping () -> SongModel?) {
        self.frontSong = frontSong
        let window = NSWindow(
            contentRect: CGRect(x: 0, y: 0, width: 560, height: 440),
            styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false
        )
        window.title = "Settings"
        window.subtitle = "Audio"
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView = NSHostingView(rootView: SettingsView(preferences: .shared, frontSong: frontSong))
        window.setFrameAutosaveName("settings")
        super.init(window: window)
        window.delegate = self
        window.center()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }
}

struct SettingsView: View {
    @Bindable var preferences: AudioPreferences
    let frontSong: () -> SongModel?

    var body: some View {
        AudioSettingsTab(preferences: preferences, frontSong: frontSong)
            .frame(width: 560, height: 440)
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
