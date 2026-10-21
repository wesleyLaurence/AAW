import XCTest
@testable import AAWApp

/// What `@` names in the field: where the `@` the caret is in begins, where
/// `@name` stands in a text, which things a text names, the groups and the
/// filter, the project's files and the library's references.
final class AgentMentionsTests: XCTestCase {
    private func mention(_ group: AgentMention.Group, _ kind: AgentAttachment.Kind, _ name: String, _ reference: String? = nil, detail: String = "") -> AgentMention {
        AgentMention(group: group, kind: kind, name: name, reference: reference, detail: detail)
    }

    func testTheAtTheCaretIsIn() {
        XCTAssertEqual(AgentMentions.query(in: "@dr", caret: 3), NSRange(location: 0, length: 3))
        XCTAssertEqual(AgentMentions.query(in: "hey @dr", caret: 7), NSRange(location: 4, length: 3))
        XCTAssertEqual(AgentMentions.query(in: "@", caret: 1), NSRange(location: 0, length: 1))
        XCTAssertEqual(AgentMentions.query(in: "a\n@k", caret: 4), NSRange(location: 2, length: 2))
        XCTAssertNil(AgentMentions.query(in: "a@b", caret: 3), "an @ inside a word is an @")
        XCTAssertNil(AgentMentions.query(in: "@dr x", caret: 5), "a space ends the query")
        XCTAssertNil(AgentMentions.query(in: "@dr", caret: 0))
        XCTAssertNil(AgentMentions.query(in: "plain", caret: 5))
        XCTAssertNil(AgentMentions.query(in: "@dr", caret: 9), "a caret past the end")
    }

    func testWhereNamesStand() {
        let names: Set<String> = ["drums", "kick.wav", "hook.2", "hook", "24s It's War (Clean).m4a", "samples/kick.wav"]
        let found = AgentMentions.found(in: "put @drums under @hook.2, then @hook.", among: names)
        XCTAssertEqual(found.map(\.name), ["drums", "hook.2", "hook"])
        XCTAssertEqual(found.map(\.range), [NSRange(location: 4, length: 6), NSRange(location: 17, length: 7), NSRange(location: 31, length: 5)])
        XCTAssertEqual(AgentMentions.found(in: "@kick.wav.", among: names).map(\.name), ["kick.wav"], "the longest name first, then the full stop")
        XCTAssertEqual(AgentMentions.found(in: "@24s It's War (Clean).m4a ok", among: names).map(\.range), [NSRange(location: 0, length: 25)], "a name with spaces is found whole")
        XCTAssertEqual(AgentMentions.found(in: "@samples/kick.wav", among: names).map(\.name), ["samples/kick.wav"])
        XCTAssertEqual(AgentMentions.found(in: "@samples/kick.wav x", among: ["kick.wav"]).map { ($0.name, $0.range) }.first.map { "\($0.0) \($0.1)" },
                       "kick.wav {0, 17}", "a path matches its last part, for a row read from the record")
        XCTAssertTrue(AgentMentions.found(in: "x@drums", among: names).isEmpty, "an @ inside a word")
        XCTAssertTrue(AgentMentions.found(in: "@drumsy", among: names).isEmpty, "a name must end at a word's end")
        XCTAssertTrue(AgentMentions.found(in: "@drums@hook", among: names).isEmpty)
        XCTAssertTrue(AgentMentions.found(in: "@drums", among: []).isEmpty)
        XCTAssertEqual(AgentMentions.found(in: "(@drums)", among: names).map(\.name), [], "an @ after a bracket is not after a space")
        XCTAssertEqual(AgentMentions.found(in: "see @drums, @drums", among: names).count, 2)
    }

    func testWhatATextNames() {
        let candidates = [
            mention(.tracks, .track, "kick", "tracks.kick"),
            mention(.samples, .sample, "kick", "samples.kick"),
            mention(.clips, .clip, "lead1", "@12"),
            mention(.files, .sound, "samples/kick.wav"),
        ]
        let named = AgentMentions.mentioned(in: "@lead1 over @kick and @kick again, not @lead", among: candidates)
        XCTAssertEqual(named.map(\.reference), ["@12", "tracks.kick", "samples.kick"], "each once, in order; two things with one name both go")
        XCTAssertEqual(AgentMentions.mentioned(in: "nothing", among: candidates), [])
        XCTAssertEqual(AgentMentions.mentioned(in: "@samples/kick.wav", among: candidates).map(\.name), ["samples/kick.wav"])
    }

    func testTheGroupsAndTheFilter() {
        let candidates = [
            mention(.files, .document, "SONG.md", detail: "document in the project"),
            mention(.tracks, .track, "drums", "tracks.drums", detail: "sample track, 2 clips"),
            mention(.tracks, .bus, "verb", "returns.verb", detail: "return with reverb"),
            mention(.clips, .clip, "hook.1", "@3", detail: "pattern clip of hook on drums at 1.1.1, 1 repeat"),
            mention(.markers, .marker, "m1", "markers.m1", detail: "marker at 2.1.1: too busy"),
        ]
        let sections = AgentMentions.sections(candidates)
        XCTAssertEqual(sections.map(\.group), [.tracks, .clips, .markers, .files], "in the list's order, empty groups left out")
        XCTAssertEqual(sections[0].mentions.map(\.name), ["drums", "verb"])
        XCTAssertEqual(AgentMentions.filter(sections, query: "").count, 4)
        XCTAssertEqual(AgentMentions.filter(sections, query: "dr").flatMap(\.mentions).map(\.name), ["drums", "hook.1"], "a name first, then a detail")
        XCTAssertEqual(AgentMentions.filter(sections, query: "BUSY").flatMap(\.mentions).map(\.name), ["m1"], "in any case")
        XCTAssertEqual(AgentMentions.filter(sections, query: "zzz"), [])
        XCTAssertEqual(AgentMentions.filter(sections, query: "ook").flatMap(\.mentions).map(\.name), ["hook.1"])
    }

    func testTheProjectsFilesAndTheLibrarysReferences() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("aaw mentions \(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let project = root.appendingPathComponent("My Song")
        for folder in ["samples", "renders/mix", ".daw", "library/references/ref-a", "library/references/ref-b"] {
            try FileManager.default.createDirectory(at: root.appendingPathComponent(folder == "samples" || folder == "renders/mix" || folder == ".daw" ? "My Song/\(folder)" : folder), withIntermediateDirectories: true)
        }
        for file in ["song.yaml", "SONG.md", "samples/kick.wav", "renders/mix/a.wav", ".daw/state.json", ".daw.lock", ".DS_Store"] {
            try Data().write(to: project.appendingPathComponent(file))
        }
        try Data().write(to: root.appendingPathComponent("library/references/notes.txt"))
        let files = AgentMentions.files(in: project)
        XCTAssertEqual(files.map(\.name), ["renders", "renders/mix", "samples", "samples/kick.wav", "SONG.md", "song.yaml"], "two levels, sorted, the app's own left out")
        XCTAssertEqual(files.map(\.kind), [.folder, .folder, .folder, .sound, .document, .document])
        XCTAssertEqual(files.map(\.group), Array(repeating: .files, count: 6))
        XCTAssertEqual(files[3].path, project.appendingPathComponent("samples/kick.wav").path)
        XCTAssertEqual(files[3].detail, "sound in the project")
        XCTAssertEqual(files[0].detail, "folder in the project")
        XCTAssertNil(files[3].reference, "a file goes by its path")
        let attachment = files[4].attachment
        XCTAssertEqual(attachment.kind, .document)
        XCTAssertEqual(attachment.name, "SONG.md")
        XCTAssertEqual(attachment.path, project.appendingPathComponent("SONG.md").path)
        XCTAssertEqual(AgentMentions.files(in: root.appendingPathComponent("nowhere")), [])

        XCTAssertEqual(AgentMentions.references(environment: ["AAW_WORKSPACE": root.path]), ["ref-a", "ref-b"], "folders only")
        XCTAssertEqual(AgentMentions.references(environment: [:], home: root.appendingPathComponent("nohome")), [])
        let home = root.appendingPathComponent("home")
        try FileManager.default.createDirectory(at: home.appendingPathComponent("Music/AAW/library/references/mine"), withIntermediateDirectories: true)
        XCTAssertEqual(AgentMentions.references(environment: ["AAW_WORKSPACE": ""], home: home), ["mine"])
    }

    func testAMentionsLineAndItsReading() {
        let track = mention(.tracks, .track, "drums", "tracks.drums", detail: "sample track, 2 clips").attachment
        XCTAssertEqual(track.line, "- track: tracks.drums (sample track, 2 clips)")
        let read = AgentAttachment(line: track.line)
        XCTAssertEqual(read?.kind, .track)
        XCTAssertEqual(read?.name, "drums", "read back as it was mentioned")
        XCTAssertEqual(read?.reference, "tracks.drums")
        XCTAssertNil(read?.path)
        XCTAssertEqual(AgentAttachment(line: "- return: returns.verb (return with reverb)")?.name, "verb")
        XCTAssertEqual(AgentAttachment(line: "- return: returns.verb (return with reverb)")?.kind, .bus)
        XCTAssertEqual(AgentAttachment(line: "- sample: samples.kick (sample at samples/kick.wav, on drums)")?.name, "kick")
        XCTAssertEqual(AgentAttachment(line: "- section: sections.verse (section from 1.1.1 to 3.1.1, 8 beats)")?.name, "verse")
        XCTAssertEqual(AgentAttachment(line: "- marker: markers.m1 (marker at 2.1.1: too busy)")?.name, "m1")
        let reference = AgentAttachment(line: "- reference: Night Drive (reference in the library; compare a render with it)")
        XCTAssertEqual(reference?.name, "Night Drive")
        XCTAssertEqual(reference?.reference, "Night Drive")
        // A clip read from the record is named by its detail, not `@12`.
        XCTAssertEqual(AgentAttachment(line: "- clip: @12 (note clip lead1 on keys at 2.1.1, 4 beats, 2 notes)")?.name, "lead1")
        XCTAssertEqual(AgentAttachment(line: "- clip: @3 (pattern clip of hook on drums at 1.1.1, 1 repeat)")?.name, "hook")
        XCTAssertEqual(AgentAttachment(line: "- clip: @4 (audio clip of sample kick on bed at 1.1.1, 8 beats)")?.name, "kick")
        XCTAssertEqual(AgentAttachment(line: "- clip: @4 (on bed at 1.1.1)")?.name, "@4")
        XCTAssertNil(AgentAttachment.clipName(in: "note clip  on x"))
        XCTAssertTrue(AgentAttachment.Kind.track.byReference)
        XCTAssertFalse(AgentAttachment.Kind.sound.byReference)
        XCTAssertEqual(AgentAttachment.Kind.bus.rawValue, "return")
        XCTAssertEqual(AgentMentions.readable("parametric_eq"), "parametric eq")
        XCTAssertEqual(AgentMentions.readable("synth"), "Synth")
    }
}
