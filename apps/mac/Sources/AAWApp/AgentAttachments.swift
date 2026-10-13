import AAWCore
import AppKit
import AVFoundation

/// Something that goes with the next message to the agent: a file by its
/// path, with what the app knows about it; an image as a picture the agent
/// sees; a clip on the timeline by its reference; a patch or a rack by its
/// name. Shown as a chip above the field, then on the message.
struct AgentAttachment: Equatable, Identifiable {
    enum Kind: String, CaseIterable {
        case image, sound, midi, document, folder, video, other, clip, patch, rack

        /// The chip's symbol.
        var symbol: String {
            switch self {
            case .image: "photo"
            case .sound: "waveform"
            case .midi: "pianokeys"
            case .document: "doc.text"
            case .folder: "folder"
            case .video: "film"
            case .other: "doc"
            case .clip: "rectangle.on.rectangle"
            case .patch: "slider.horizontal.3"
            case .rack: "square.stack.3d.up"
            }
        }
    }

    /// The picture the agent sees, as PNG or JPEG under the API's limits,
    /// and the image to draw a thumbnail from. Compared by identity, so
    /// that rows of the conversation compare cheaply; never changed once
    /// made, so safe to hand between threads.
    final class Picture: Equatable, @unchecked Sendable {
        let data: Data
        /// `image/png` or `image/jpeg`.
        let mediaType: String
        let image: NSImage

        init(data: Data, mediaType: String, image: NSImage) {
            (self.data, self.mediaType, self.image) = (data, mediaType, image)
        }

        static func == (a: Picture, b: Picture) -> Bool { a === b }
    }

    /// Given by the panel, in order.
    var id = 0
    var kind: Kind
    /// Shown on the chip: a file's name, a clip's ID, a patch's name.
    var name: String
    /// The file's path; nil for a clip.
    var path: String?
    /// A clip's reference, as `daw` takes it: `@12`.
    var reference: String?
    /// What the app knows, in a line: a sound's length, a MIDI file's
    /// tracks, a clip's track and bar.
    var detail: String
    var picture: Picture?

    init(kind: Kind, name: String, path: String? = nil, reference: String? = nil, detail: String = "", picture: Picture? = nil) {
        (self.kind, self.name, self.path, self.reference, self.detail, self.picture) = (kind, name, path, reference, detail, picture)
    }

    /// Whether the file is no longer where it was.
    var missing: Bool {
        guard let path else { return false }
        return !FileManager.default.fileExists(atPath: path)
    }

    /// The line in the message's `<attachments>` list: the kind, the path
    /// or the reference, and the detail in parentheses.
    var line: String {
        let what = path ?? reference ?? name
        return "- \(kind.rawValue): \(what)" + (detail.isEmpty ? "" : " (\(detail))")
    }

    /// An attachment read back from its line in a message's record; nil for
    /// a line that is none.
    init?(line: String) {
        guard line.hasPrefix("- "), let colon = line.range(of: ": ") else { return nil }
        guard let kind = Kind(rawValue: String(line[line.index(line.startIndex, offsetBy: 2)..<colon.lowerBound])) else { return nil }
        var rest = String(line[colon.upperBound...])
        var detail = ""
        if rest.hasSuffix(")"), let open = rest.range(of: " (", options: .backwards) {
            detail = String(rest[open.upperBound..<rest.index(before: rest.endIndex)])
            rest = String(rest[..<open.lowerBound])
        }
        self.kind = kind
        self.detail = detail
        switch kind {
        case .clip:
            reference = rest
            name = rest
        case .patch, .rack:
            name = rest
        default:
            path = rest
            name = (rest as NSString).lastPathComponent
        }
    }
}

/// How a file, a pasteboard or a clip becomes an attachment, and how the
/// attachments go in a message.
enum AgentAttachments {
    static let open = "<attachments>"
    static let close = "</attachments>"
    /// The API takes many images a message; the panel takes this many.
    static let mostImages = 10
    /// The API's limits on an image: its long side and its bytes.
    static let longestSide: CGFloat = 1568
    static let mostBytes = 5 * 1024 * 1024

    static let images: Set<String> = ["png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "tif", "tiff", "bmp"]
    static let sounds: Set<String> = Browser.extensions.union(["aifc", "caf", "ogg", "wma", "aac", "opus"])
    static let midis: Set<String> = ["mid", "midi"]
    static let documents: Set<String> = ["pdf", "txt", "md", "markdown", "yaml", "yml", "json", "csv", "rtf", "html", "xml", "toml"]
    static let videos: Set<String> = ["mov", "mp4", "m4v", "avi", "mkv", "webm"]

    /// The kind a file is, by its extension, or a folder.
    static func kind(of url: URL) -> AgentAttachment.Kind {
        var isDirectory: ObjCBool = false
        if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory), isDirectory.boolValue { return .folder }
        let ext = url.pathExtension.lowercased()
        if images.contains(ext) { return .image }
        if sounds.contains(ext) { return .sound }
        if midis.contains(ext) { return .midi }
        if documents.contains(ext) { return .document }
        if videos.contains(ext) { return .video }
        return .other
    }

    /// A file as an attachment: an image read and scaled to the API's
    /// limits, a sound's length, channels and rate, a MIDI file's tracks
    /// and notes, a document's size, and a video or anything else by its
    /// path and size. `sample` is what the browser knows of a sample it
    /// dragged.
    static func make(file url: URL, sample: SampleInfo? = nil) -> AgentAttachment {
        let kind = kind(of: url)
        let name = url.lastPathComponent
        switch kind {
        case .image:
            if let picture = picture(of: url) {
                let size = picture.image.size
                return AgentAttachment(kind: .image, name: name, path: url.path, detail: "\(Int(size.width))×\(Int(size.height)); seen", picture: picture)
            }
            return AgentAttachment(kind: .image, name: name, path: url.path, detail: "could not be read")
        case .sound:
            var parts: [String] = []
            if let file = try? AVAudioFile(forReading: url) {
                let rate = file.fileFormat.sampleRate
                let channels = file.fileFormat.channelCount
                if rate > 0 { parts.append(String(format: "%.2f s", Double(file.length) / rate)) }
                parts.append(channels == 1 ? "mono" : channels == 2 ? "stereo" : "\(channels) channels")
                if rate > 0 { parts.append(rate.truncatingRemainder(dividingBy: 1000) == 0 ? "\(Int(rate / 1000)) kHz" : String(format: "%.1f kHz", rate / 1000)) }
            }
            if let sample {
                parts.append("library ID \(sample.id)")
                if !sample.category.isEmpty { parts.append(sample.category) }
                if let bpm = sample.bpm { parts.append("\(bpm) BPM") }
                if let note = sample.note { parts.append(note) }
            }
            return AgentAttachment(kind: .sound, name: name, path: url.path, detail: parts.joined(separator: ", "))
        case .midi:
            let detail = (try? Data(contentsOf: url)).map(MIDISummary.init).map(\.detail) ?? "could not be read"
            return AgentAttachment(kind: .midi, name: name, path: url.path, detail: detail)
        case .folder:
            let count = (try? FileManager.default.contentsOfDirectory(atPath: url.path))?.filter { !$0.hasPrefix(".") }.count ?? 0
            return AgentAttachment(kind: .folder, name: name, path: url.path, detail: "\(count) item\(count == 1 ? "" : "s")")
        case .video:
            return AgentAttachment(kind: .video, name: name, path: url.path, detail: [size(of: url), "not read"].compactMap { $0 }.joined(separator: ", "))
        case .document, .other, .clip, .patch, .rack:
            return AgentAttachment(kind: kind, name: name, path: url.path, detail: size(of: url) ?? "")
        }
    }

    /// An image with no file, as one pasted: written under the project's
    /// `.daw/attachments/`, so that the agent can read it again and the
    /// record points at something.
    static func make(image: NSImage, project: URL, now: Date = Date()) -> AgentAttachment? {
        guard let picture = picture(of: image) else { return nil }
        let folder = project.appendingPathComponent(".daw/attachments", isDirectory: true)
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd-HHmmss"
        var file = folder.appendingPathComponent("pasted-\(formatter.string(from: now)).png")
        var n = 1
        while FileManager.default.fileExists(atPath: file.path) {
            n += 1
            file = folder.appendingPathComponent("pasted-\(formatter.string(from: now))-\(n).png")
        }
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let data = picture.mediaType == "image/png" ? picture.data : (png(of: picture.image) ?? picture.data)
            try data.write(to: file)
        } catch {
            return nil
        }
        let size = picture.image.size
        return AgentAttachment(kind: .image, name: file.lastPathComponent, path: file.path, detail: "\(Int(size.width))×\(Int(size.height)); seen", picture: picture)
    }

    /// A patch or a rack from the browser, by its name and its file.
    static func make(kind: AgentAttachment.Kind, name: String, file: String?) -> AgentAttachment {
        AgentAttachment(kind: kind, name: name, path: file, detail: file == nil ? "factory" : "")
    }

    /// A clip on the timeline: its reference, its kind, its track and bar,
    /// and for an audio clip its sample's name.
    static func make(clip: PlacedClip, track: TrackView?, bar: (Double) -> String) -> AgentAttachment {
        let on = track.map { $0.id } ?? "?"
        if let pattern = clip.clip {
            return AgentAttachment(kind: .clip, name: pattern.pattern, reference: pattern.reference,
                                   detail: "pattern clip of \(pattern.pattern) on \(on) at \(bar(clip.at)), \(pattern.repeats) repeat\(pattern.repeats == 1 ? "" : "s")")
        }
        if let audio = clip.audio {
            return AgentAttachment(kind: .clip, name: audio.sample, reference: audio.reference,
                                   detail: "audio clip of sample \(audio.sample) on \(on) at \(bar(clip.at)), \(Self.beats(audio.lengthBeats)) beats")
        }
        if let notes = clip.notes {
            return AgentAttachment(kind: .clip, name: notes.id, reference: notes.reference,
                                   detail: "note clip \(notes.id) on \(on) at \(bar(clip.at)), \(Self.beats(notes.lengthBeats)) beats, \(notes.notes.count) note\(notes.notes.count == 1 ? "" : "s")")
        }
        return AgentAttachment(kind: .clip, name: "clip", reference: "@\(clip.key)", detail: "on \(on) at \(bar(clip.at))")
    }

    private static func beats(_ value: Double) -> String {
        value == value.rounded() ? String(Int(value)) : String(format: "%.2f", value)
    }

    private static func size(of url: URL) -> String? {
        guard let bytes = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber)?.int64Value else { return nil }
        if bytes < 1024 { return "\(bytes) bytes" }
        if bytes < 1024 * 1024 { return "\(bytes / 1024) KB" }
        return String(format: "%.1f MB", Double(bytes) / (1024 * 1024))
    }

    // MARK: Images

    static func picture(of url: URL) -> AgentAttachment.Picture? {
        guard let image = NSImage(contentsOf: url) else { return nil }
        return picture(of: image)
    }

    /// The image scaled to the API's longest side and encoded as PNG, or as
    /// JPEG when the PNG is over its bytes.
    static func picture(of image: NSImage) -> AgentAttachment.Picture? {
        guard let scaled = scaled(image) else { return nil }
        if let png = png(of: scaled), png.count <= mostBytes {
            return AgentAttachment.Picture(data: png, mediaType: "image/png", image: scaled)
        }
        for quality in [0.85, 0.7, 0.5] {
            if let jpeg = jpeg(of: scaled, quality: quality), jpeg.count <= mostBytes {
                return AgentAttachment.Picture(data: jpeg, mediaType: "image/jpeg", image: scaled)
            }
        }
        return nil
    }

    /// The image at its pixels, no larger than the longest side, drawn into
    /// a new bitmap so that its size is its pixels.
    private static func scaled(_ image: NSImage) -> NSImage? {
        let pixels = image.representations.map { CGSize(width: $0.pixelsWide, height: $0.pixelsHigh) }
            .max { $0.width * $0.height < $1.width * $1.height } ?? image.size
        guard pixels.width > 0, pixels.height > 0 else { return nil }
        let scale = min(1, longestSide / max(pixels.width, pixels.height))
        let size = CGSize(width: max(1, (pixels.width * scale).rounded()), height: max(1, (pixels.height * scale).rounded()))
        guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width), pixelsHigh: Int(size.height), bitsPerSample: 8,
                                         samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else { return nil }
        rep.size = size
        NSGraphicsContext.saveGraphicsState()
        defer { NSGraphicsContext.restoreGraphicsState() }
        guard let context = NSGraphicsContext(bitmapImageRep: rep) else { return nil }
        NSGraphicsContext.current = context
        context.imageInterpolation = .high
        image.draw(in: NSRect(origin: .zero, size: size), from: .zero, operation: .copy, fraction: 1)
        let out = NSImage(size: size)
        out.addRepresentation(rep)
        return out
    }

    private static func bitmap(of image: NSImage) -> NSBitmapImageRep? {
        if let rep = image.representations.compactMap({ $0 as? NSBitmapImageRep }).first { return rep }
        guard let tiff = image.tiffRepresentation else { return nil }
        return NSBitmapImageRep(data: tiff)
    }

    static func png(of image: NSImage) -> Data? {
        bitmap(of: image)?.representation(using: .png, properties: [:])
    }

    static func jpeg(of image: NSImage, quality: Double) -> Data? {
        bitmap(of: image)?.representation(using: .jpeg, properties: [.compressionFactor: quality])
    }

    // MARK: The message

    /// The list that goes in the message, between its tags; nothing for no
    /// attachments.
    static func list(_ attachments: [AgentAttachment]) -> String {
        guard !attachments.isEmpty else { return "" }
        return ([open] + attachments.map(\.line) + [close]).joined(separator: "\n")
    }

    /// The attachments a message's record lists, in order.
    static func listed(in message: String) -> [AgentAttachment] {
        guard let start = message.range(of: open), let end = message.range(of: close, range: start.upperBound..<message.endIndex) else { return [] }
        return message[start.upperBound..<end.lowerBound].split(whereSeparator: \.isNewline).compactMap { AgentAttachment(line: String($0)) }
    }

    /// The message's `content` as the engine takes it: the words alone
    /// when nothing is attached, else the images as blocks and then the
    /// text as one.
    static func content(text: String, attachments: [AgentAttachment]) -> Any {
        let pictures = attachments.compactMap(\.picture)
        guard !attachments.isEmpty else { return text }
        var blocks: [[String: Any]] = pictures.map {
            ["type": "image", "source": ["type": "base64", "media_type": $0.mediaType, "data": $0.data.base64EncodedString()]]
        }
        blocks.append(["type": "text", "text": text])
        return blocks
    }
}

/// What a standard MIDI file holds, read for an attachment's line: its
/// tracks, with their names where they have one, and its notes.
struct MIDISummary: Equatable {
    var format: Int = 0
    var tracks: [String] = []
    var notes: Int = 0
    /// False for a file that is no MIDI file.
    var read = false

    init(_ data: Data) {
        let bytes = [UInt8](data)
        guard bytes.count >= 14, bytes[0..<4] == [0x4D, 0x54, 0x68, 0x64] else { return }
        format = Int(bytes[8]) << 8 | Int(bytes[9])
        let count = Int(bytes[10]) << 8 | Int(bytes[11])
        var i = 8 + (Int(bytes[4]) << 24 | Int(bytes[5]) << 16 | Int(bytes[6]) << 8 | Int(bytes[7]))
        for _ in 0..<count {
            guard i + 8 <= bytes.count, bytes[i..<i + 4] == [0x4D, 0x54, 0x72, 0x6B] else { break }
            let length = Int(bytes[i + 4]) << 24 | Int(bytes[i + 5]) << 16 | Int(bytes[i + 6]) << 8 | Int(bytes[i + 7])
            let end = min(bytes.count, i + 8 + length)
            tracks.append(Self.read(track: bytes, from: i + 8, to: end, notes: &notes))
            i = end
        }
        read = true
    }

    /// One track's events: its name from the first name meta event, and its
    /// note-ons with a velocity counted.
    private static func read(track bytes: [UInt8], from start: Int, to end: Int, notes: inout Int) -> String {
        var i = start
        var name = ""
        var status: UInt8 = 0
        func variable() -> Int {
            var value = 0
            while i < end {
                let b = bytes[i]
                i += 1
                value = value << 7 | Int(b & 0x7F)
                if b & 0x80 == 0 { break }
            }
            return value
        }
        while i < end {
            _ = variable()
            guard i < end else { break }
            if bytes[i] & 0x80 != 0 {
                status = bytes[i]
                i += 1
            }
            switch status {
            case 0xFF:
                guard i < end else { break }
                let type = bytes[i]
                i += 1
                let length = variable()
                if type == 0x03, name.isEmpty, i + length <= end {
                    name = String(decoding: bytes[i..<i + length], as: UTF8.self)
                }
                i += length
            case 0xF0, 0xF7:
                i += variable()
            default:
                let kind = status & 0xF0
                let size = (kind == 0xC0 || kind == 0xD0) ? 1 : 2
                guard i + size <= end else { return name }
                if kind == 0x90, bytes[i + 1] > 0 { notes += 1 }
                i += size
            }
        }
        return name
    }

    /// The line: "2 tracks: Piano, Bass; 120 notes".
    var detail: String {
        guard read else { return "not a MIDI file" }
        let named = tracks.filter { !$0.isEmpty }
        var text = "\(tracks.count) track\(tracks.count == 1 ? "" : "s")"
        if !named.isEmpty { text += ": " + named.joined(separator: ", ") }
        text += "; \(notes) note\(notes == 1 ? "" : "s")"
        return text
    }
}
