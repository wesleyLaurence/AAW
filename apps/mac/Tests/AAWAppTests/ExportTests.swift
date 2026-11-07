import XCTest
@testable import AAWApp

final class ExportTests: XCTestCase {
    /// The sheet's choices as the arguments of the `daw export` that does
    /// what they say: the format by the name's extension, the range, the
    /// depth, bitrate and rate only when not the command's own, the level
    /// by its one option, the ceiling only when changed, and `--replace`
    /// when the person was asked about a file that is there.
    func testTheChoicesAreTheCommandsArguments() {
        var options = ExportOptions()
        XCTAssertEqual(options.arguments(project: "/p/song.yaml", to: "/p/exports/Song.wav", replace: false),
                       ["export", "/p/song.yaml", "--to", "/p/exports/Song.wav"])
        options.bits = 16
        options.level = .peak(-1)
        XCTAssertEqual(options.arguments(project: "/p/song.yaml", to: "/p/exports/Song.wav", replace: true),
                       ["export", "/p/song.yaml", "--to", "/p/exports/Song.wav", "--bits", "16", "--peak", "-1", "--replace"])
        options.format = .aac
        options.level = .lufs(-14)
        options.ceiling = -0.5
        XCTAssertEqual(options.arguments(project: "/p/song.yaml", to: "/p/exports/Song.m4a", replace: false),
                       ["export", "/p/song.yaml", "--to", "/p/exports/Song.m4a", "--lufs", "-14", "--ceiling", "-0.5"])
        options.format = .mp3
        options.level = .gain(-3.25)
        XCTAssertEqual(options.arguments(project: "/p/song.yaml", to: "/p/exports/Song.mp3", replace: false),
                       ["export", "/p/song.yaml", "--to", "/p/exports/Song.mp3", "--gain", "-3.25", "--ceiling", "-0.5"])
        // A ceiling means nothing to a file as rendered, and is not sent.
        options.level = .asRendered
        XCTAssertEqual(options.arguments(project: "/p/song.yaml", to: "/p/exports/Song.mp3", replace: false),
                       ["export", "/p/song.yaml", "--to", "/p/exports/Song.mp3"])
    }

    /// A range, a rate, a depth and a bitrate, each where the format has it.
    func testTheRangeRateAndDepthAreSentWhereTheFormatHasThem() {
        var options = ExportOptions()
        let range = ExportRange(start: 4, end: 140)
        options.sampleRate = 96000
        options.bits = 32
        XCTAssertEqual(options.arguments(project: "/p/s.yaml", to: "/p/exports/S.wav", replace: false, range: range),
                       ["export", "/p/s.yaml", "--to", "/p/exports/S.wav", "--start", "4", "--end", "140", "--bits", "32", "--sample-rate", "96000"])
        // FLAC is at most 24-bit: 32 is sent as the command's 24, which is not sent.
        options.format = .flac
        XCTAssertEqual(options.depth, 24)
        XCTAssertEqual(options.arguments(project: "/p/s.yaml", to: "/p/exports/S.flac", replace: false, range: ExportRange(start: 2.5, end: 3)),
                       ["export", "/p/s.yaml", "--to", "/p/exports/S.flac", "--start", "2.5", "--end", "3", "--sample-rate", "96000"])
        // MP3 has a bitrate and no depth, and is at most 48 kHz.
        options.format = .mp3
        options.bitrate = 192
        XCTAssertNil(options.depth)
        XCTAssertEqual(options.rate, 48000)
        XCTAssertEqual(options.arguments(project: "/p/s.yaml", to: "/p/exports/S.mp3", replace: false),
                       ["export", "/p/s.yaml", "--to", "/p/exports/S.mp3", "--bitrate", "192", "--sample-rate", "48000"])
        // A bitrate means nothing to a WAV.
        options.format = .wav
        options.sampleRate = nil
        XCTAssertEqual(options.arguments(project: "/p/s.yaml", to: "/p/exports/S.wav", replace: false),
                       ["export", "/p/s.yaml", "--to", "/p/exports/S.wav", "--bits", "32"])
    }

    func testTheFormatNamesTheFile() {
        var options = ExportOptions()
        XCTAssertEqual(ExportOptions.name(title: "  "), "Untitled")
        XCTAssertEqual(options.fileName("My Song"), "My Song.wav")
        XCTAssertEqual(options.fileName("My Song.wav"), "My Song.wav", "the extension is not written twice")
        XCTAssertEqual(options.fileName(" a/b:c "), "a-b-c.wav", "a name is a file, not a folder")
        XCTAssertEqual(options.fileName(""), "Untitled.wav")
        options.format = .aac
        XCTAssertEqual(options.fileName("My Song"), "My Song.m4a")
        options.format = .aiff
        XCTAssertEqual(options.fileName("My Song.wav"), "My Song.wav.aiff")
        XCTAssertEqual(ExportOptions.Format(fileExtension: "WAV"), .wav)
        XCTAssertEqual(ExportOptions.Format(fileExtension: "aif"), .aiff)
        XCTAssertEqual(ExportOptions.Format(fileExtension: "flac"), .flac)
        XCTAssertEqual(ExportOptions.Format(fileExtension: "m4a"), .aac)
        XCTAssertEqual(ExportOptions.Format(fileExtension: "mp3"), .mp3)
        XCTAssertNil(ExportOptions.Format(fileExtension: "ogg"))
        for format in ExportOptions.Format.allCases {
            XCTAssertEqual(format.contentType.preferredFilenameExtension, format.fileExtension, format.name)
        }
    }

    /// Positions as bar, beat and sixteenth in the song's meter, and back;
    /// a place off the sixteenths as the nearest one and milliseconds.
    func testARangeIsBarsBeatsAndSixteenthsInTheMeter() {
        // 4/4: a bar is 4 beats, a beat a quarter of four sixteenths.
        XCTAssertTrue(ExportRange.place(0, bar: 4, unit: 1) == (1, 1, 1))
        XCTAssertTrue(ExportRange.place(140, bar: 4, unit: 1) == (36, 1, 1))
        XCTAssertTrue(ExportRange.place(8.5, bar: 4, unit: 1) == (3, 1, 3))
        XCTAssertTrue(ExportRange.place(4.3, bar: 4, unit: 1) == (2, 1, 2.2), "off the grid: a decimal sixteenth")
        XCTAssertTrue(ExportRange.place(3.9999999999, bar: 4, unit: 1) == (2, 1, 1), "a hair under a bar line is the line")
        XCTAssertEqual(ExportRange.beats(bar: 2, beat: 1, sixteenth: 1, barBeats: 4, unit: 1), 4)
        XCTAssertEqual(ExportRange.beats(bar: 2, beat: 1, sixteenth: 2.2, barBeats: 4, unit: 1), 4.3, accuracy: 1e-12)
        XCTAssertEqual(ExportRange.text(4, bar: 4, unit: 1, tempo: 120), "2.1.1")
        XCTAssertEqual(ExportRange.text(4.5, bar: 4, unit: 1, tempo: 120), "2.1.3")
        // 0.3 of a beat is 1.2 sixteenths: the second, and 0.2 of 125 ms after it.
        XCTAssertEqual(ExportRange.text(4.3, bar: 4, unit: 1, tempo: 120), "2.1.2 + 25 ms")
        // A lead of 70 ms before bar 2 at 120: 0.14 beats, nearer 1.4.4 than 2.1.1.
        XCTAssertEqual(ExportRange.text(3.86, bar: 4, unit: 1, tempo: 120), "1.4.4 + 55 ms")
        XCTAssertEqual(ExportRange.text(3.97, bar: 4, unit: 1, tempo: 120), "2.1.1 − 15 ms")
        XCTAssertEqual(ExportRange.text(1 / 3.0, bar: 4, unit: 1, tempo: 120), "1.1.2 + 42 ms")
        // 6/8: a bar is 3 quarter beats, counted in six eighths of two sixteenths.
        XCTAssertTrue(ExportRange.place(4.75, bar: 3, unit: 0.5) == (2, 4, 2))
        XCTAssertEqual(ExportRange.beats(bar: 2, beat: 4, sixteenth: 2, barBeats: 3, unit: 0.5), 4.75)
        XCTAssertEqual(ExportRange.text(4.75, bar: 3, unit: 0.5, tempo: 120), "2.4.2")
        // 7/8: a bar is 3.5.
        XCTAssertEqual(ExportRange.text(7, bar: 3.5, unit: 0.5, tempo: 120), "3.1.1")
    }

    /// The selected clips' span inside the song, else the whole song.
    func testARangeStartsAsTheSelectionOrTheWholeSong() {
        XCTAssertEqual(ExportRange.initial(selection: nil, length: 300), ExportRange(start: 0, end: 300))
        XCTAssertEqual(ExportRange.initial(selection: (4, 140), length: 300), ExportRange(start: 4, end: 140))
        // A clip whose tail rings past the end is cut there.
        XCTAssertEqual(ExportRange.initial(selection: (280, 310), length: 300), ExportRange(start: 280, end: 300))
        // A selection wholly past the end is no range.
        XCTAssertEqual(ExportRange.initial(selection: (300, 310), length: 300), ExportRange(start: 0, end: 300))
        XCTAssertTrue(ExportRange(start: 0, end: 300).isWhole(length: 300))
        XCTAssertFalse(ExportRange(start: 4, end: 300).isWhole(length: 300))
        XCTAssertNil(ExportRange(start: 4, end: 140).problem(length: 300, bar: 4, unit: 1, tempo: 120))
        XCTAssertEqual(ExportRange(start: 4, end: 4).problem(length: 300, bar: 4, unit: 1, tempo: 120), "The end must be after the start.")
        XCTAssertEqual(ExportRange(start: 4, end: 304).problem(length: 300, bar: 4, unit: 1, tempo: 120),
                       "The song ends at 76.1.1; nothing after it is rendered.")
        XCTAssertEqual(Launch.range("4-12.5"), ExportRange(start: 4, end: 12.5))
        XCTAssertNil(Launch.range("4"))
    }

    /// The sheet's fields, what it says of them and what it sends.
    @MainActor
    func testTheSheetShowsTheRangeAsBarsAndSaysWhatIsWrong() {
        let setup = ExportSetup(options: ExportOptions(), title: "Song", range: ExportRange(start: 4, end: 140), fromSelection: true,
                                barBeats: 4, beatUnit: 1, lengthBeats: 300, tempo: 120, timeSignature: "4/4", songRate: 48000, hasFolder: true)
        XCTAssertEqual([setup.startBar, setup.startBeat, setup.endBar, setup.endBeat], [2, 1, 36, 1])
        XCTAssertEqual([setup.startSixteenth, setup.endSixteenth], [1, 1])
        XCTAssertEqual(setup.summary, "Bars 2–35, the selected clips · 1:08")
        XCTAssertEqual(setup.exportedRange, ExportRange(start: 4, end: 140))
        XCTAssertNil(setup.problem)
        setup.endBeat = 5
        XCTAssertEqual(setup.problem, "A bar of 4/4 has beats 1 to 4.")
        setup.endBeat = 2
        setup.endSixteenth = 5
        XCTAssertEqual(setup.problem, "A beat of 4/4 has sixteenths 1 to 4; a decimal is a place between them.")
        setup.endSixteenth = 3
        XCTAssertEqual(setup.summary, "2.1.1 to 36.2.3, the selected clips · 1:08")
        setup.startSixteenth = 2.2
        XCTAssertEqual(setup.range.start, 4.3, accuracy: 1e-12)
        XCTAssertEqual(setup.summary, "2.1.2 + 25 ms to 36.2.3, the selected clips · 1:08")
        setup.endBar = 80
        XCTAssertEqual(setup.problem, "The song ends at 76.1.1; nothing after it is rendered.")
        setup.wholeSong()
        XCTAssertEqual(setup.summary, "The whole song · 2:30")
        XCTAssertNil(setup.exportedRange, "the whole song is sent as no range")
        setup.name = " "
        XCTAssertEqual(setup.problem, "The file needs a name.")
        // The song's rate is the one shown unless another is chosen; MP3
        // offers 48 kHz at most.
        XCTAssertEqual(setup.rate.wrappedValue, 48000)
        setup.rate.wrappedValue = 44100
        XCTAssertEqual(setup.options.sampleRate, 44100)
        setup.rate.wrappedValue = 48000
        XCTAssertNil(setup.options.sampleRate)
        setup.options.format = .mp3
        XCTAssertEqual(setup.rates, [44100, 48000])
        XCTAssertEqual(setup.bitrate.wrappedValue, 320)
        XCTAssertEqual(ExportSetup.duration(1.25), "0:01.3")
        XCTAssertEqual(ExportSetup.kHz(44100), "44.1 kHz")

        // In 6/8 a beat is an eighth of two sixteenths.
        let six = ExportSetup(options: ExportOptions(), title: "Song", range: ExportRange(start: 3, end: 4.75), fromSelection: false,
                              barBeats: 3, beatUnit: 0.5, lengthBeats: 30, tempo: 120, timeSignature: "6/8", songRate: 48000, hasFolder: true)
        XCTAssertEqual([six.endBar, six.endBeat], [2, 4])
        XCTAssertEqual(six.endSixteenth, 2)
        XCTAssertNil(six.problem)
        six.endBeat = 7
        XCTAssertEqual(six.problem, "A bar of 6/8 has beats 1 to 6.")
        six.endBeat = 6
        six.endSixteenth = 3
        XCTAssertEqual(six.problem, "A beat of 6/8 has sixteenths 1 to 2; a decimal is a place between them.")
    }

    /// ⇧⌘R with clips selected: the range is theirs; with none, the whole
    /// song; the name is the song's title, or its folder's while the title
    /// is the one it was made with.
    @MainActor
    func testTheSelectedClipsAreTheRange() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("aaw export \(UUID().uuidString)/Night Drive")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder.deletingLastPathComponent()) }
        let song = folder.appendingPathComponent("song.yaml")
        // kick.wav, half a second.
        _ = try AgentAttachmentsTests.files(in: folder)
        try """
        session: {length_beats: 300, tempo: 120}
        samples: {kick: {path: kick.wav}}
        tracks:
        - id: vox
          pads: {}
          clips: []
          audio:
          - {sample: kick, at: 8, source_start_seconds: 0.2, lead_ms: 70}
        - id: keys
          type: midi
          clips:
          - {at: 4.3, length_beats: 7.7, id: a, notes: [{pitch: 60, duration: 1}]}
          - {at: 132, length_beats: 8, id: b, notes: [{pitch: 64, duration: 1}]}
          - {at: 200, length_beats: 8, id: c, notes: [{pitch: 67, duration: 1}]}

        """.write(to: song, atomically: true, encoding: .utf8)
        let model = try SongModel(url: song)
        defer { model.close() }
        XCTAssertEqual(model.exportRange, ExportRange(start: 0, end: model.arrangement.lengthBeats))
        let clips = model.placedClips().filter { $0.at < 200 && $0.audio == nil }.map(\.key)
        model.select(clips: Set(clips))
        // A clip off the grid starts the range where it starts.
        XCTAssertEqual(model.exportRange.start, 4.3, accuracy: 1e-9)
        XCTAssertEqual(model.exportRange.end, 140)
        let setup = model.exportSetup(options: ExportOptions())
        XCTAssertEqual(setup.name, "Night Drive")
        XCTAssertEqual([setup.startBar, setup.startBeat, setup.endBar, setup.endBeat], [2, 1, 36, 1])
        XCTAssertEqual(setup.startSixteenth, 2.2, accuracy: 1e-9)
        XCTAssertEqual(model.placeText(4.3), "2.1.2 + 25 ms")

        // An audio clip's lead plays before its beat, and is exported: 70 ms
        // at 120 is 0.14 beats before bar 3.
        let audio = try XCTUnwrap(model.placedClips().first { $0.audio != nil })
        model.select(clips: [audio.key])
        XCTAssertEqual(model.exportRange.start, 7.86, accuracy: 1e-9)
        XCTAssertEqual(model.placeText(model.exportRange.start), "2.4.4 + 55 ms")
        XCTAssertEqual(model.selectedSpan?.start, 8, "the loop L sets keeps the clip's beat")
        XCTAssertTrue(setup.fromSelection)
        XCTAssertTrue(setup.hasFolder)
    }

    /// `--export-level` on the command line, and the level as the defaults
    /// keep it, are one spelling.
    func testALevelIsParsedAndSpelled() {
        XCTAssertEqual(ExportOptions.Level.parse("peak=-1"), .peak(-1))
        XCTAssertEqual(ExportOptions.Level.parse("lufs=-14"), .lufs(-14))
        XCTAssertEqual(ExportOptions.Level.parse("gain=-3.5"), .gain(-3.5))
        XCTAssertEqual(ExportOptions.Level.parse("rendered"), .asRendered)
        XCTAssertNil(ExportOptions.Level.parse("peak"))
        XCTAssertNil(ExportOptions.Level.parse("loud=-14"))
        XCTAssertNil(ExportOptions.Level.parse("peak=loud"))
        for level in [ExportOptions.Level.asRendered, .peak(-1), .lufs(-14), .gain(-3.5)] {
            XCTAssertEqual(ExportOptions.Level.parse(level.text), level)
        }
        // A policy chosen in the menu starts at the value usually wanted.
        XCTAssertEqual(ExportOptions.Level.make("peak", nil), .peak(-1))
        XCTAssertEqual(ExportOptions.Level.make("lufs", nil), .lufs(-14))
        XCTAssertEqual(ExportOptions.Level.make("gain", nil), .gain(0))
        XCTAssertEqual(ExportOptions.Level.make("peak", -2), .peak(-2))
        XCTAssertNil(ExportOptions.Level.make("match", nil))
    }

    func testTheLastChoicesAreOfferedNextTime() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "aaw.tests.export.\(UUID().uuidString)"))
        XCTAssertEqual(ExportOptions(defaults: defaults), ExportOptions())
        var options = ExportOptions()
        options.format = .aac
        options.level = .peak(-1)
        options.ceiling = -0.3
        options.sampleRate = 44100
        options.bitrate = 192
        options.bits = 16
        options.remember(in: defaults)
        XCTAssertEqual(ExportOptions(defaults: defaults), options)
        options.sampleRate = nil
        options.bitrate = nil
        options.remember(in: defaults)
        XCTAssertEqual(ExportOptions(defaults: defaults), options)
        // A WAV kept by its depth before the depth was its own.
        defaults.set("wav16", forKey: "export.format")
        defaults.removeObject(forKey: "export.bits")
        XCTAssertEqual(ExportOptions(defaults: defaults).format, .wav)
        XCTAssertEqual(ExportOptions(defaults: defaults).bits, 16)
    }

    /// The command's result as the banner reads it.
    func testTheReportIsReadFromTheCommandsResult() throws {
        let json = """
        {"file": "/p/exports/Song.m4a", "sha256": "abc", "format": {"container": "m4a", "codec": "aac"},
         "sample_rate": 48000, "channels": 2, "duration_seconds": 212.5,
         "level": {"policy": "peak", "target": {"true_peak_dbtp": -1.0}, "gain_db": 2.317, "held_back_db": 0.0,
                   "ceiling_dbfs": -0.1, "integrated_lufs": -14.04, "peak_dbfs": -1.2, "estimated_true_peak_dbtp": -1.0,
                   "rendered": {"integrated_lufs": -16.36, "peak_dbfs": -3.5, "estimated_true_peak_dbtp": -3.3}},
         "render": {"render_id": "r", "directory": "/p/renders/r", "rendered_now": true},
         "warnings": ["True peak -1.00 dBTP before encoding: a decoder can clip what a lossy encoder overshoots; --peak -1 leaves room"]}
        """
        let report = try XCTUnwrap(ExportReport(json: Data(json.utf8)))
        XCTAssertEqual(report.name, "Song.m4a")
        XCTAssertEqual(report.durationSeconds, 212.5)
        XCTAssertEqual(report.policy, "peak")
        XCTAssertEqual(report.gainDb, 2.317)
        XCTAssertEqual(report.integratedLufs, -14.04)
        XCTAssertEqual(report.truePeakDbtp, -1)
        XCTAssertTrue(report.renderedNow)
        XCTAssertEqual(report.warnings.count, 1)
        let place = { (beats: Double) in ExportRange.text(beats, bar: 4, unit: 1, tempo: 120) }
        XCTAssertEqual(report.measurements(place: place), "3:32 · −14.0 LUFS · −1.0 dBTP · +2.3 dB applied")

        // As rendered: the gain is not worth a word. A held-back gain is.
        var plain = report
        plain.policy = "as rendered"
        plain.gainDb = 0
        XCTAssertEqual(plain.measurements(place: place), "3:32 · −14.0 LUFS · −1.0 dBTP")
        var held = report
        held.policy = "loudness"
        held.heldBackDb = 1.84
        XCTAssertEqual(held.measurements(place: place), "3:32 · −14.0 LUFS · −1.0 dBTP · +2.3 dB applied · 1.8 dB held back by the ceiling")

        // A range is said first, its places as the transport writes them.
        let ranged = try XCTUnwrap(ExportReport(json: Data(json.replacingOccurrences(
            of: "\"warnings\"", with: "\"range\": {\"whole_song\": false, \"start_beat\": 3.86, \"end_beat\": 140.0, \"start\": \"1:4.86\", \"end\": \"36:1\"}, \"warnings\"").utf8)))
        XCTAssertEqual(ranged.measurements(place: place), "1.4.4 + 55 ms to 36.1.1 · 3:32 · −14.0 LUFS · −1.0 dBTP · +2.3 dB applied")

        // A result that is not the command's is no report.
        XCTAssertNil(ExportReport(json: Data("{\"ok\": true}".utf8)))
        XCTAssertNil(ExportReport(json: Data()))
    }

    /// The command's reason, from its JSON error or its plain stderr.
    func testTheReasonIsTheCommands() {
        XCTAssertEqual(Exporter.reason(stderr: "{\"error\": \"/p/exports/Song.wav exists; pass --replace to write over it\", \"command\": \"export\"}\n", status: 1),
                       "/p/exports/Song.wav exists; pass --replace to write over it")
        XCTAssertEqual(Exporter.reason(stderr: "This command runs in Python, which was not found at /x/.venv/bin/python\n", status: 1),
                       "This command runs in Python, which was not found at /x/.venv/bin/python")
        XCTAssertEqual(Exporter.reason(stderr: "", status: 2), "daw export failed (status 2)")
    }

    /// A build that is no bundle has no `daw` to run, and says so.
    func testWithoutADawNothingRuns() {
        XCTAssertEqual(Exporter.run(daw: nil, arguments: ["export"], file: "Song.wav"),
                       .failed(file: "Song.wav", reason: "This build has no daw inside it: apps/mac/build.sh makes an app with daw in its bundle."))
    }

    /// A `daw` that fails is read for its reason, and one that answers for
    /// its report; both pipes are read, however much is written.
    func testTheProcessIsReadForItsReportOrItsReason() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("aaw export \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        func daw(_ script: String) throws -> URL {
            let file = folder.appendingPathComponent("daw-\(UUID().uuidString)")
            try "#!/bin/sh\n\(script)\n".write(to: file, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
            return file
        }
        let answers = try daw("""
        echo '{"file": "/p/Song.wav", "duration_seconds": 8, "level": {"policy": "as rendered", "gain_db": 0, "held_back_db": 0, "integrated_lufs": -20.5, "estimated_true_peak_dbtp": -6.1}, "render": {"rendered_now": false}, "warnings": []}'
        """)
        guard case .done(let report) = Exporter.run(daw: answers, arguments: ["export"], file: "Song.wav") else {
            return XCTFail("no report")
        }
        XCTAssertEqual(report.measurements(place: { "\($0)" }), "0:08 · −20.5 LUFS · −6.1 dBTP")

        let refuses = try daw("echo '{\"error\": \"The render is silent\"}' >&2; exit 1")
        XCTAssertEqual(Exporter.run(daw: refuses, arguments: ["export"], file: "Song.wav"),
                       .failed(file: "Song.wav", reason: "The render is silent"))

        // More than a pipe holds on stderr, then the result: neither read waits on the other.
        let chatty = try daw("""
        i=0; while [ $i -lt 2000 ]; do echo 'a line of progress that nobody reads, repeated until the pipe is well past full'; i=$((i+1)); done >&2
        echo '{"file": "/p/Song.wav", "duration_seconds": 1, "level": {"policy": "as rendered"}, "warnings": []}'
        """)
        guard case .done(let late) = Exporter.run(daw: chatty, arguments: ["export"], file: "Song.wav") else {
            return XCTFail("no report after a full stderr")
        }
        XCTAssertEqual(late.durationSeconds, 1)
    }
}
