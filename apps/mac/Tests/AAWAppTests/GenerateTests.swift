import AAWCore
import XCTest
@testable import AAWApp

@MainActor
final class GenerateTests: XCTestCase {
    private nonisolated let scratch = FileManager.default.temporaryDirectory.appendingPathComponent("aaw generate's \(UUID().uuidString)")
    /// The workspace the other tests set, put back after.
    private nonisolated(unsafe) var workspace: String?

    override func setUpWithError() throws {
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
        workspace = ProcessInfo.processInfo.environment["AAW_WORKSPACE"]
    }

    override func tearDownWithError() throws {
        if let workspace { setenv("AAW_WORKSPACE", workspace, 1) } else { unsetenv("AAW_WORKSPACE") }
        try FileManager.default.removeItem(at: scratch)
    }

    /// A stand-in `daw`: a shell script that runs `body` with the arguments.
    private func daw(_ body: String) throws -> URL {
        let file = scratch.appendingPathComponent("daw")
        try "#!/bin/sh\n\(body)\n".write(to: file, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
        return file
    }

    private func run(_ daw: URL?, _ arguments: [String], input: Data? = nil, cancelAfter: Double? = nil) async -> DawProcess.Outcome {
        var outcome: DawProcess.Outcome?
        let done = expectation(description: "daw ended")
        let process = DawProcess.run(daw, arguments, input: input) {
            outcome = $0
            done.fulfill()
        }
        if let cancelAfter {
            try? await Task.sleep(for: .seconds(cancelAfter))
            process.cancel()
        }
        await fulfillment(of: [done], timeout: 10)
        return outcome ?? .failed("never ended")
    }

    private func result(kind: String, prompt: String = "vinyl crackle", asked: Double? = nil, bars: Double? = nil,
                        instrumental: Bool = false, fromPlan: Bool = false) -> GeneratedInfo {
        GeneratedInfo(path: "/g/2026-10-14-120000-crackle.wav", recipe: "/g/2026-10-14-120000-crackle.generated.json",
                      inProject: true, sampleId: "crackle", kind: kind, prompt: prompt, title: nil, name: "crackle",
                      seconds: 4.2, created: "2026-10-14T12:00:00Z", askedSeconds: asked, bars: bars,
                      instrumental: instrumental, fromPlan: fromPlan)
    }

    /// Each kind is the command that makes it, as the person's request,
    /// which does not count against the agent's budget.
    func testEachKindIsTheCommandThatMakesItAsThePersons() {
        var request = GenerateRequest()
        request.text = "  vinyl crackle  "
        XCTAssertEqual(request.arguments(song: "/s/song.yaml"),
                       ["--origin", "user", "generate", "sound", "/s/song.yaml", "vinyl crackle"])
        request.soundSeconds = 4.5
        XCTAssertEqual(request.arguments(song: "/s/song.yaml").suffix(2), ["--seconds", "4.5"])
        request.kind = .loop
        request.bars = 4
        XCTAssertEqual(request.arguments(song: "/s/song.yaml"),
                       ["--origin", "user", "generate", "sound", "/s/song.yaml", "vinyl crackle", "--loop", "--bars", "4"])
        request.kind = .song
        request.songSeconds = 90
        request.instrumental = true
        XCTAssertEqual(request.arguments(song: "/s/song.yaml"),
                       ["--origin", "user", "generate", "music", "/s/song.yaml", "vinyl crackle", "--seconds", "90", "--instrumental"])
    }

    /// What the command would refuse is said before anything is sent.
    func testWhatTheCommandRefusesIsSaidFirst() {
        var request = GenerateRequest()
        XCTAssertEqual(request.problem, "Describe what to make.")
        request.text = "a kick"
        XCTAssertNil(request.problem)
        request.soundSeconds = 31
        XCTAssertEqual(request.problem, "A sound is 0.5 to 30 seconds.")
        request.soundSeconds = nil
        request.kind = .song
        request.songSeconds = 2
        XCTAssertEqual(request.problem, "A song is 3 to 300 seconds.")
        request.songSeconds = 300
        XCTAssertNil(request.problem)
        request.kind = .loop
        request.bars = 0
        XCTAssertNotNil(request.problem)
    }

    /// Generate Again asks for what the result was asked for.
    func testAgainAsksForWhatTheResultWasAskedFor() {
        let sound = GenerateRequest(again: result(kind: "sound", asked: 4))
        XCTAssertEqual(sound.kind, .sound)
        XCTAssertEqual(sound.text, "vinyl crackle")
        XCTAssertEqual(sound.soundSeconds, 4)
        let picked = GenerateRequest(again: result(kind: "sound"))
        XCTAssertNil(picked.soundSeconds)
        let loop = GenerateRequest(again: result(kind: "loop", bars: 4))
        XCTAssertEqual(loop.kind, .loop)
        XCTAssertEqual(loop.bars, 4)
        let song = GenerateRequest(again: result(kind: "music", prompt: "lofi", asked: 60, instrumental: true))
        XCTAssertEqual(song.kind, .song)
        XCTAssertEqual(song.songSeconds, 60)
        XCTAssertTrue(song.instrumental)
    }

    /// `daw keys status` with no key, with one and its plan, and with one
    /// the service would not take; `daw keys set`'s reply reads the same.
    func testTheKeysStatusIsRead() throws {
        let none = try XCTUnwrap(KeyStatus(json: Data("""
        {"service": "elevenlabs", "key": false, "hint": "No ElevenLabs key is set", "tier": null,
         "budget": {"agent_seconds_per_day": 120.0, "spent_today": {"agent": 12.5, "user": 30.0}, "left_today": 107.5, "day": "2026-10-25"}}
        """.utf8)))
        XCTAssertEqual(none.kept, false)
        XCTAssertNil(none.planLine)
        XCTAssertEqual(none.budget, KeyStatus.Budget(perDay: 120, agentToday: 12.5, personToday: 30, left: 107.5))
        let kept = try XCTUnwrap(KeyStatus(json: Data("""
        {"key": true, "tier": "creator", "subscription": {"tier": "creator", "credits_used": 1000, "credits_limit": 100000,
         "credits_left": 99000, "credits_reset": "2026-11-01T00:00:00Z", "status": "active"}}
        """.utf8)))
        XCTAssertEqual(kept.kept, true)
        XCTAssertEqual(kept.planLine, "Creator plan · 99,000 of 100,000 credits left · renews \(KeyStatus.day("2026-11-01T00:00:00Z"))")
        XCTAssertNotEqual(KeyStatus.day("2026-11-01T00:00:00Z"), "2026-11-01T00:00:00Z")
        let refused = try XCTUnwrap(KeyStatus(json: Data(#"{"service": "elevenlabs", "stored": true, "problem": "The key was refused"}"#.utf8)))
        XCTAssertEqual(refused.kept, true)
        XCTAssertEqual(refused.problem, "The key was refused")
        XCTAssertEqual(KeyStatus.plan("starter"), "Starter")
        XCTAssertEqual(KeyStatus.plan("pro_plus"), "Pro Plus")
    }

    /// The command's JSON comes back; a failure gives the command's reason;
    /// what is written in goes to its standard input; a stopped one is
    /// cancelled; a build with no `daw` says so.
    func testDawRunsOffTheMainThreadAndStops() async throws {
        let echo = try daw(#"read line; printf '{"args": "%s", "read": %d}' "$*" "${#line}""#)
        guard case .done(let data) = await run(echo, ["keys", "set", "elevenlabs"], input: Data("sk-secret\n".utf8)) else {
            return XCTFail("no result")
        }
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(object["args"] as? String, "keys set elevenlabs")
        XCTAssertEqual(object["read"] as? Int, 9)

        let refuses = try daw(#"echo '{"error": "No ElevenLabs key is set"}' >&2; exit 1"#)
        let failed = await run(refuses, ["generate"])
        XCTAssertEqual(failed, .failed("No ElevenLabs key is set"))

        let slow = try daw("sleep 30")
        let started = Date()
        let stopped = await run(slow, ["generate"], cancelAfter: 0.3)
        XCTAssertEqual(stopped, .cancelled)
        XCTAssertLessThan(Date().timeIntervalSince(started), 5)

        guard case .failed(let reason) = await run(nil, ["keys", "status"]) else { return XCTFail("ran without a daw") }
        XCTAssertTrue(reason.contains("no daw"), reason)
    }

    /// A request runs off the main thread with its time shown, and what it
    /// made, what it was refused or that it was stopped is said after.
    func testARequestSaysWhatItMadeOrWhyNot() async throws {
        let song = scratch.appendingPathComponent("song/song.yaml")
        let made = try daw(#"""
        echo "$*" > "$(dirname "$0")/asked"
        echo '{"sample_id": "vinyl-crackle", "seconds": 4.2, "notes": ["The folder could not be scanned"]}'
        """#)
        let generator = Generator(daw: made)
        generator.generate(song: song)
        XCTAssertNil(generator.running, "nothing is sent without a description")
        XCTAssertEqual(generator.error, "Describe what to make.")
        generator.request.text = "vinyl crackle"
        generator.generate(song: song)
        XCTAssertEqual(generator.running?.text, "vinyl crackle")
        let started = try XCTUnwrap(generator.started)
        XCTAssertEqual(Generator.progress(generator.request, since: started, now: started.addingTimeInterval(72)), "Making a sound… 1:12")
        while generator.running != nil { try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertNil(generator.error)
        XCTAssertEqual(generator.made, "It is in the song as vinyl-crackle of 4.2 s. Nothing is placed: drag it onto the timeline or a MIDI track's name.")
        XCTAssertEqual(generator.notes, ["The folder could not be scanned"])
        let asked = try String(contentsOf: scratch.appendingPathComponent("asked"), encoding: .utf8)
        XCTAssertEqual(asked.trimmingCharacters(in: .newlines), "--origin user generate sound \(song.path) vinyl crackle")

        let slow = Generator(daw: try daw("sleep 30"))
        slow.request.text = "a long song"
        slow.request.kind = .song
        slow.generate(song: song)
        try await Task.sleep(for: .milliseconds(200))
        slow.cancel()
        while slow.running != nil { try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertEqual(slow.error, "Stopped. Nothing was added to the song.")
        XCTAssertNil(slow.made)
    }

    /// The song's results come first, played from the generated folder's
    /// file while it is there; then the folder's others, newest first.
    func testTheSongsResultsComeFirstThenTheFolders() throws {
        let workspace = scratch.appendingPathComponent("workspace")
        let folder = workspace.appendingPathComponent("library/generated")
        let samples = scratch.appendingPathComponent("song/samples")
        for dir in [folder, samples] { try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true) }
        setenv("AAW_WORKSPACE", workspace.path, 1)
        func write(_ dir: URL, _ stem: String, _ recipe: [String: Any], audio: Bool = true) throws {
            if audio { try Data().write(to: dir.appendingPathComponent("\(stem).wav")) }
            let json = try JSONSerialization.data(withJSONObject: recipe.merging(["file": "\(stem).wav"]) { a, _ in a })
            try json.write(to: dir.appendingPathComponent("\(stem).generated.json"))
        }
        let crackle = folder.appendingPathComponent("2026-10-14-120000-crackle.wav")
        try write(folder, "2026-10-14-120000-crackle", ["kind": "sound", "prompt": "vinyl crackle", "created": "2026-10-14T12:00:00Z",
                                                        "seconds": 4.0, "parameters": ["duration_seconds": 4.0, "loop": false]])
        try write(samples, "0123456789ab_2026-10-14-120000-crackle", ["kind": "sound", "prompt": "vinyl crackle", "created": "2026-10-14T12:00:00Z",
                                                                      "seconds": 4.0, "generated_file": crackle.path,
                                                                      "parameters": ["duration_seconds": 4.0, "loop": false]])
        try write(samples, "abcdef012345_2026-10-13-090000-lofi", ["kind": "music", "prompt": "lofi", "title": "Late Bus", "created": "2026-10-13T09:00:00Z",
                                                                   "seconds": 60.0, "generated_file": folder.appendingPathComponent("gone.mp3").path,
                                                                   "parameters": ["music_length_ms": 60000, "force_instrumental": true]])
        try write(folder, "2026-10-15-080000-boom", ["kind": "sound", "prompt": "a boom", "created": "2026-10-15T08:00:00Z", "seconds": 2.0,
                                                     "bars": 2.0, "parameters": ["loop": true]])
        try write(folder, "2026-10-16-080000-plan", ["kind": "music", "prompt": NSNull(), "title": "From a Plan",
                                                     "created": "2026-10-16T08:00:00Z", "seconds": 30.0, "parameters": [:]])
        try write(folder, "2026-10-17-080000-lost", ["kind": "sound", "prompt": "no file", "created": "2026-10-17T08:00:00Z"], audio: false)

        let found = libraryGenerated(song: scratch.appendingPathComponent("song/song.yaml").path)
        XCTAssertEqual(found.map(\.name), ["crackle", "lofi", "plan", "boom"])
        XCTAssertEqual(found.map(\.inProject), [true, true, false, false])
        // The song's crackle plays from the folder's file, which an import
        // names as the song's copy is named.
        XCTAssertEqual(found[0].path, crackle.path)
        XCTAssertEqual(found[0].askedSeconds, 4)
        // A song whose original is gone plays from the song's copy.
        XCTAssertEqual(found[1].path, samples.appendingPathComponent("abcdef012345_2026-10-13-090000-lofi.wav").path)
        XCTAssertEqual(found[1].kind, "music")
        XCTAssertEqual(found[1].askedSeconds, 60)
        XCTAssertTrue(found[1].instrumental)
        XCTAssertEqual(Generator.shownName(found[1]), "Late Bus")
        XCTAssertTrue(found[2].fromPlan)
        XCTAssertEqual(found[3].kind, "loop")
        XCTAssertEqual(found[3].bars, 2)
        XCTAssertEqual(found[3].recipe, folder.appendingPathComponent("2026-10-15-080000-boom.generated.json").path)
        // With no song, the folder's alone; a recipe whose file is gone is not listed.
        XCTAssertEqual(libraryGenerated(song: nil).map(\.name), ["plan", "boom", "crackle"])

        // As a sample of the browser: played from its file, named by its ID.
        let sample = Generator.sample(found[3])
        XCTAssertEqual(sample.path, found[3].path)
        XCTAssertEqual(sample.name, "a boom")
        XCTAssertEqual(Browser.padName(of: sample), "boom")
        XCTAssertEqual(sample.kind, "loop")
        XCTAssertTrue(Generator.matches(found[1], "late bus"))
        XCTAssertTrue(Generator.matches(found[1], "lofi"))
        XCTAssertFalse(Generator.matches(found[1], "crackle"))
        let detail = Generator.detail(found[0], now: ISO8601DateFormatter().date(from: "2026-10-20T00:00:00Z")!)
        XCTAssertTrue(detail.hasPrefix("Sound · 4 s · "), detail)
        XCTAssertFalse(detail.contains(":"), "a day before today is shown by its date: \(detail)")
        let today = Generator.detail(found[0], now: ISO8601DateFormatter().date(from: "2026-10-14T13:00:00Z")!)
        XCTAssertTrue(today.contains(":"), "today's is shown by its time: \(today)")
        // There is no song.yaml to read, so no sample is named.
        XCTAssertEqual(Generator.help(found[0]), "vinyl crackle")
        XCTAssertEqual(Generator.help(result(kind: "sound")), "In the song as crackle. vinyl crackle")
        XCTAssertEqual(Generator.help(found[2]), "Made from a composition plan")
    }

    /// The pane's lines: whether a key is kept, the budgets offered with
    /// one set elsewhere among them, and today's spend.
    func testTheElevenLabsPaneSaysWhatIsKept() {
        XCTAssertEqual(ElevenLabsSettingsTab.keyLine(nil, checking: true), "Checking…")
        XCTAssertEqual(ElevenLabsSettingsTab.keyLine(KeyStatus(kept: false), checking: false), "None")
        XCTAssertEqual(ElevenLabsSettingsTab.keyLine(KeyStatus(kept: true), checking: false), "Kept in the Keychain")
        XCTAssertEqual(ElevenLabsSettingsTab.keyLine(KeyStatus(kept: nil, problem: "locked"), checking: false), "Could not be read")
        XCTAssertEqual(ElevenLabsSettingsTab.choices(120), ElevenLabsSettingsTab.budgets)
        XCTAssertEqual(ElevenLabsSettingsTab.choices(45), [0, 30, 45, 60, 120, 300, 600, 1800, 3600])
        XCTAssertEqual(ElevenLabsSettingsTab.budgetName(0), "None: ask every time")
        XCTAssertEqual(ElevenLabsSettingsTab.budgetName(120), "2 min a day")
        XCTAssertEqual(ElevenLabsSettingsTab.budgetName(90), "1:30 a day")
        XCTAssertEqual(ElevenLabsSettingsTab.budgetName(30), "30 s a day")
        let line = ElevenLabsSettingsTab.budgetLine(KeyStatus.Budget(perDay: 120, agentToday: 12.5, personToday: 0, left: 107.5))
        XCTAssertTrue(line.contains("the agent has made 12.5 s and you 0 s"), line)
    }

    /// Settings opens on the pane shown last, and on the one asked for.
    func testSettingsOpensOnThePaneAskedFor() {
        UserDefaults.standard.removeObject(forKey: SettingsWindowController.paneKey)
        let settings = SettingsWindowController { nil }
        XCTAssertEqual(settings.window?.title, "Audio")
        settings.show(.elevenLabs)
        XCTAssertEqual(settings.window?.title, "ElevenLabs")
        settings.window?.close()
        XCTAssertEqual(UserDefaults.standard.string(forKey: SettingsWindowController.paneKey), "ElevenLabs")
        let again = SettingsWindowController { nil }
        XCTAssertEqual(again.window?.title, "ElevenLabs")
        again.window?.close()
        UserDefaults.standard.removeObject(forKey: SettingsWindowController.paneKey)
    }
}
