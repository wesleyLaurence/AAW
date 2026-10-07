import XCTest
@testable import AAWApp

final class ExportTests: XCTestCase {
    /// The panel's choices as the arguments of the `daw export` that does
    /// what they say: the format by the name's extension and `--bits`, the
    /// level by its one option, the ceiling only when changed, and
    /// `--replace` when the panel asked about a file that is there.
    func testTheChoicesAreTheCommandsArguments() {
        var options = ExportOptions()
        XCTAssertEqual(options.arguments(project: "/p/song.yaml", to: "/p/exports/Song.wav", replace: false),
                       ["export", "/p/song.yaml", "--to", "/p/exports/Song.wav"])
        options.format = .wav16
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

    func testTheFormatNamesTheFile() {
        var options = ExportOptions()
        XCTAssertEqual(options.fileName(title: "My Song"), "My Song.wav")
        options.format = .aac
        XCTAssertEqual(options.fileName(title: "My Song"), "My Song.m4a")
        options.format = .mp3
        XCTAssertEqual(options.fileName(title: "  "), "Untitled.mp3")
        XCTAssertEqual(ExportOptions.Format(fileExtension: "WAV"), .wav24)
        XCTAssertEqual(ExportOptions.Format(fileExtension: "m4a"), .aac)
        XCTAssertEqual(ExportOptions.Format(fileExtension: "mp3"), .mp3)
        XCTAssertNil(ExportOptions.Format(fileExtension: "flac"))
        for format in ExportOptions.Format.allCases {
            XCTAssertEqual(format.contentType.preferredFilenameExtension, format.fileExtension, format.name)
        }
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
        options.remember(in: defaults)
        XCTAssertEqual(ExportOptions(defaults: defaults), options)
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
        XCTAssertEqual(report.measurements, "3:32 · −14.0 LUFS · −1.0 dBTP · +2.3 dB applied")

        // As rendered: the gain is not worth a word. A held-back gain is.
        var plain = report
        plain.policy = "as rendered"
        plain.gainDb = 0
        XCTAssertEqual(plain.measurements, "3:32 · −14.0 LUFS · −1.0 dBTP")
        var held = report
        held.policy = "loudness"
        held.heldBackDb = 1.84
        XCTAssertEqual(held.measurements, "3:32 · −14.0 LUFS · −1.0 dBTP · +2.3 dB applied · 1.8 dB held back by the ceiling")

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
        XCTAssertEqual(report.measurements, "0:08 · −20.5 LUFS · −6.1 dBTP")

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
