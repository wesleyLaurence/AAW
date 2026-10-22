import AAWCore
import Foundation

/// Something in the song or the project the person names with `@` in the
/// field of [the agent panel]: a track, a group, a return, a clip, a sample,
/// a section, a marker, a reference kept in the library or a file in the
/// project's folder. `@name` in the words is drawn as a chip, and the thing
/// goes with the message as a line of the attachments list with the
/// reference `daw` takes for it, as a clip attached with `+` does.
struct AgentMention: Equatable, Hashable, Identifiable {
    /// The groups of the list, in its order.
    enum Group: Int, CaseIterable, Comparable {
        case tracks, clips, samples, sections, markers, references, files

        var title: String {
            switch self {
            case .tracks: "Tracks"
            case .clips: "Clips"
            case .samples: "Samples"
            case .sections: "Sections"
            case .markers: "Markers"
            case .references: "References"
            case .files: "Files"
            }
        }

        static func < (a: Group, b: Group) -> Bool { a.rawValue < b.rawValue }
    }

    var group: Group
    var kind: AgentAttachment.Kind
    /// What follows `@`: a track's ID, a clip's name, a sample's ID, a
    /// file's path within the project.
    var name: String
    /// The reference `daw` takes: `tracks.drums`, `@12`, `samples.kick`,
    /// `sections.verse`, `markers.m1`, a reference's name; nil for a file,
    /// which goes by its path.
    var reference: String?
    /// A file's path.
    var path: String?
    /// What the app knows, in a line.
    var detail: String

    var id: String { "\(kind.rawValue) \(name) \(reference ?? path ?? "")" }

    /// The line of the list, as a message carries it: a file read by its
    /// kind, as a dropped one is; anything else by its reference.
    var attachment: AgentAttachment {
        if let path {
            var made = AgentAttachments.make(file: URL(fileURLWithPath: path))
            made.name = name
            return made
        }
        return AgentAttachment(kind: kind, name: name, reference: reference, detail: detail)
    }
}

/// What can be mentioned, how the list is grouped and filtered, and where
/// the `@` names are in a text.
enum AgentMentions {
    struct Section: Equatable {
        var group: AgentMention.Group
        var mentions: [AgentMention]
    }

    /// Files this deep under the project's folder are listed, and this
    /// many at the most.
    static let fileDepth = 2
    static let mostFiles = 300
    /// Files and folders of the app's and the host's own, not listed; the
    /// project's `.aaw` and the folder's icon are the Finder's way in, not
    /// something an agent reads.
    static let hiddenFiles: Set<String> = [".daw", ".daw.lock", ".DS_Store", "Icon\r"]

    /// Everything the song and the project offer: the arrangement's rows,
    /// clips, samples, sections and markers, the library's references and
    /// the project's files. `bar` writes a beat as a position.
    static func candidates(arrangement a: Arrangement, project: URL?, references: [String], bar: (Double) -> String) -> [AgentMention] {
        var found: [AgentMention] = []
        for track in a.tracks {
            let clips = track.clips.count + track.audio.count + track.noteClips.count
            var parts = [track.midi ? "MIDI track" + (track.instrument.map { " with a \(readable($0))" } ?? " with no instrument") : "sample track"]
            parts.append("\(clips) clip\(clips == 1 ? "" : "s")")
            if let group = track.group, let g = a.groups.first(where: { $0.key == group }) { parts.append("in group \(g.id)") }
            if track.mute { parts.append("muted") }
            if track.solo { parts.append("soloed") }
            found.append(AgentMention(group: .tracks, kind: .track, name: track.id, reference: "tracks.\(track.id)", detail: parts.joined(separator: ", ")))
        }
        for group in a.groups {
            let members = a.tracks.filter { $0.group == group.key }.map(\.id) + a.groups.filter { $0.group == group.key }.map(\.id)
            let detail = members.isEmpty ? "group with nothing in it" : "group of \(members.joined(separator: ", "))"
            found.append(AgentMention(group: .tracks, kind: .group, name: group.id, reference: "groups.\(group.id)", detail: detail))
        }
        for bus in a.returns {
            let effects = bus.effects.map { readable($0.kind) }
            found.append(AgentMention(group: .tracks, kind: .bus, name: bus.id, reference: "returns.\(bus.id)",
                                      detail: effects.isEmpty ? "return with no effects" : "return with \(effects.joined(separator: ", "))"))
        }
        found += clips(arrangement: a, bar: bar)
        for sample in a.samples {
            let users = a.tracks.filter { t in t.pads.contains { $0.sample == sample.id } || t.audio.contains { $0.sample == sample.id } }.map(\.id)
            var parts = ["sample at \(sample.path)"]
            if let root = sample.rootNote { parts.append("root \(root)") }
            parts.append(users.isEmpty ? "on no track" : "on \(users.joined(separator: ", "))")
            found.append(AgentMention(group: .samples, kind: .sample, name: sample.id, reference: "samples.\(sample.id)", detail: parts.joined(separator: ", ")))
        }
        for section in a.sections {
            let beats = section.lengthBeats
            found.append(AgentMention(group: .sections, kind: .section, name: section.id, reference: "sections.\(section.id)",
                                      detail: "section from \(bar(section.at)) to \(bar(section.at + beats)), \(beatsText(beats)) beats"))
        }
        for marker in a.markers {
            found.append(AgentMention(group: .markers, kind: .marker, name: marker.id, reference: "markers.\(marker.id)",
                                      detail: "marker at \(bar(marker.at))" + (marker.text.isEmpty ? "" : ": \(marker.text)")))
        }
        for name in references {
            found.append(AgentMention(group: .references, kind: .reference, name: name, reference: name, detail: "reference in the library; compare a render with it"))
        }
        if let project { found += files(in: project) }
        return found
    }

    /// The clips on the timeline, each named by its note clip's ID, its
    /// pattern or its sample; where several share a name they are
    /// `name.1`, `name.2`… in time order.
    static func clips(arrangement a: Arrangement, bar: (Double) -> String) -> [AgentMention] {
        struct Found {
            var name: String
            var reference: String
            var at: Double
            var detail: String
        }
        var all: [Found] = []
        for track in a.tracks {
            let on = track.id
            for clip in track.clips {
                all.append(Found(name: clip.pattern, reference: clip.reference, at: clip.at,
                                 detail: "pattern clip of \(clip.pattern) on \(on) at \(bar(clip.at)), \(clip.repeats) repeat\(clip.repeats == 1 ? "" : "s")"))
            }
            for clip in track.audio {
                all.append(Found(name: clip.sample, reference: clip.reference, at: clip.at,
                                 detail: "audio clip of sample \(clip.sample) on \(on) at \(bar(clip.at)), \(beatsText(clip.lengthBeats)) beats"))
            }
            for clip in track.noteClips {
                all.append(Found(name: clip.id, reference: clip.reference, at: clip.at,
                                 detail: "note clip \(clip.id) on \(on) at \(bar(clip.at)), \(beatsText(clip.lengthBeats)) beats, \(clip.notes.count) note\(clip.notes.count == 1 ? "" : "s")"))
            }
        }
        all.sort { ($0.at, $0.name) < ($1.at, $1.name) }
        var counts: [String: Int] = [:]
        for clip in all { counts[clip.name, default: 0] += 1 }
        var seen: [String: Int] = [:]
        return all.map { clip in
            var name = clip.name
            if counts[clip.name, default: 0] > 1 {
                seen[clip.name, default: 0] += 1
                name = "\(clip.name).\(seen[clip.name]!)"
            }
            return AgentMention(group: .clips, kind: .clip, name: name, reference: clip.reference, detail: clip.detail)
        }
    }

    /// The project's files, two levels deep, by their path within the
    /// folder, the app's own left out; folders too, as `+` attaches one.
    static func files(in project: URL) -> [AgentMention] {
        var found: [AgentMention] = []
        func walk(_ folder: URL, prefix: String, depth: Int) {
            guard depth < fileDepth, found.count < mostFiles else { return }
            let names = ((try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? [])
                .filter { !$0.hasPrefix(".") && !hiddenFiles.contains($0) && !$0.lowercased().hasSuffix(".aaw") }
                .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
            for name in names {
                guard found.count < mostFiles else { return }
                let url = folder.appendingPathComponent(name)
                let relative = prefix + name
                var isDirectory: ObjCBool = false
                FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory)
                let kind = AgentAttachments.kind(of: url)
                found.append(AgentMention(group: .files, kind: kind, name: relative, path: url.path,
                                          detail: isDirectory.boolValue ? "folder in the project" : "\(kind.rawValue) in the project"))
                if isDirectory.boolValue { walk(url, prefix: relative + "/", depth: depth + 1) }
            }
        }
        walk(project, prefix: "", depth: 0)
        return found
    }

    /// The names of the references kept in the library, `library/references`
    /// under the workspace `daw` uses: `AAW_WORKSPACE`, or `~/Music/AAW`.
    static func references(environment: [String: String] = ProcessInfo.processInfo.environment,
                           home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [String] {
        let workspace = environment["AAW_WORKSPACE"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: ($0 as NSString).expandingTildeInPath) }
            ?? home.appendingPathComponent("Music/AAW")
        let folder = workspace.appendingPathComponent("library/references")
        let names = ((try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []).filter { !$0.hasPrefix(".") }
        return names.filter { name in
            var isDirectory: ObjCBool = false
            return FileManager.default.fileExists(atPath: folder.appendingPathComponent(name).path, isDirectory: &isDirectory) && isDirectory.boolValue
        }.sorted()
    }

    /// The mentions in their groups, in the list's order.
    static func sections(_ mentions: [AgentMention]) -> [Section] {
        AgentMention.Group.allCases.compactMap { group in
            let found = mentions.filter { $0.group == group }
            return found.isEmpty ? nil : Section(group: group, mentions: found)
        }
    }

    /// The sections with only the mentions that match what was typed after
    /// `@`: a name that starts with it first, then a name that has it, then
    /// a detail that has it, in any case; an empty query keeps all.
    static func filter(_ sections: [Section], query: String) -> [Section] {
        let q = query.lowercased()
        guard !q.isEmpty else { return sections }
        return sections.compactMap { section in
            func rank(_ m: AgentMention) -> Int {
                let name = m.name.lowercased()
                if name.hasPrefix(q) { return 0 }
                if name.contains(q) { return 1 }
                if m.detail.lowercased().contains(q) { return 2 }
                return 3
            }
            let kept = section.mentions.map { ($0, rank($0)) }.filter { $0.1 < 3 }
            guard !kept.isEmpty else { return nil }
            return Section(group: section.group, mentions: kept.enumerated().sorted { ($0.element.1, $0.offset) < ($1.element.1, $1.offset) }.map(\.element.0))
        }
    }

    // MARK: `@` in a text

    /// Whether a character ends a name: a space, a line, or punctuation
    /// that follows a word. A `.` is not one here, since a clip is `hook.2`
    /// and a file `kick.wav`; a name is tried before the `.` after it.
    static func ends(_ character: Character) -> Bool {
        character.isWhitespace || ",;:!?)]}\"'".contains(character)
    }

    private static func isEnd(_ text: Substring, at index: String.Index) -> Bool {
        index == text.endIndex || ends(text[index]) || text[index] == "."
    }

    /// The `@` the caret is in, as the range of `@` and what was typed after
    /// it: the `@` at the text's start or after a space, with no space
    /// between it and the caret. Nil when the caret is in no mention. The
    /// range is in UTF-16 units, as the text view counts.
    static func query(in text: String, caret: Int) -> NSRange? {
        let ns = text as NSString
        guard caret >= 0, caret <= ns.length else { return nil }
        var i = caret
        while i > 0 {
            let c = ns.character(at: i - 1)
            if let scalar = Unicode.Scalar(c), Character(scalar).isWhitespace { return nil }
            if c == 0x40 { // @
                guard i == 1 || Unicode.Scalar(ns.character(at: i - 2)).map({ Character($0).isWhitespace }) == true else { return nil }
                return NSRange(location: i - 1, length: caret - (i - 1))
            }
            i -= 1
        }
        return nil
    }

    /// Where `@name` stands in a text for any of the names: each `@` at the
    /// start or after a space, followed by the longest name that ends at a
    /// word's end, so a name with spaces or a `.` in it is found whole. A
    /// name that is a path matches its last part too, so a row read from
    /// the record finds `@samples/kick.wav` by `kick.wav`. In order of
    /// appearance, as UTF-16 ranges.
    static func found(in text: String, among names: Set<String>) -> [(range: NSRange, name: String)] {
        guard !names.isEmpty else { return [] }
        let longestFirst = names.sorted { ($1.count, $0) < ($0.count, $1) }
        var found: [(range: NSRange, name: String)] = []
        var index = text.startIndex
        var previous: Character?
        while index < text.endIndex {
            let character = text[index]
            let after = text.index(after: index)
            guard character == "@", previous?.isWhitespace ?? true else {
                previous = character
                index = after
                continue
            }
            let rest = text[after...]
            var match: (end: String.Index, name: String)?
            if let name = longestFirst.first(where: { rest.hasPrefix($0) && isEnd(rest, at: rest.index(rest.startIndex, offsetBy: $0.count)) }) {
                match = (rest.index(rest.startIndex, offsetBy: name.count), name)
            } else {
                // A path as the record has it: the run to a word's end,
                // whose last part is a name.
                var end = after
                while end < text.endIndex, !ends(text[end]) { end = text.index(after: end) }
                let run = text[after..<end]
                if let slash = run.lastIndex(of: "/"), names.contains(String(run[run.index(after: slash)...])) {
                    match = (end, String(run[run.index(after: slash)...]))
                }
            }
            if let match {
                let location = text.utf16.distance(from: text.startIndex, to: index)
                found.append((NSRange(location: location, length: text.utf16.distance(from: index, to: match.end)), match.name))
                index = match.end
                previous = nil
            } else {
                previous = character
                index = after
            }
        }
        return found
    }

    /// The mentions a text names, in order of appearance, each once; two
    /// things with one name both go, since the words say no more.
    static func mentioned(in text: String, among candidates: [AgentMention]) -> [AgentMention] {
        let names = Set(candidates.map(\.name))
        var out: [AgentMention] = []
        for (_, name) in found(in: text, among: names) {
            for candidate in candidates where candidate.name == name && !out.contains(candidate) { out.append(candidate) }
        }
        return out
    }

    private static func beatsText(_ value: Double) -> String {
        value == value.rounded() ? String(Int(value)) : String(format: "%.2f", value)
    }

    /// An effect's or an instrument's kind as a word: `synth` as "Synth",
    /// `parametric_eq` as "parametric eq".
    static func readable(_ kind: String) -> String {
        switch kind {
        case "synth": "Synth"
        case "sampler": "Sampler"
        default: kind.replacingOccurrences(of: "_", with: " ")
        }
    }
}
