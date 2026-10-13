import AVFoundation
import XCTest
@testable import AAWApp

/// A file as an attachment: its kind by extension, an image scaled and
/// encoded under the API's limits, a sound's length, a MIDI file's tracks
/// and notes, a document's size, and a pasted image written to the project.
final class AgentAttachmentsTests: XCTestCase {
    private var scratch: URL!

    override func setUpWithError() throws {
        scratch = FileManager.default.temporaryDirectory.appendingPathComponent("aaw attachments \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try FileManager.default.removeItem(at: scratch)
    }

    /// Four generated files in a folder: a 40×20 PNG, half a second of
    /// silence as a mono WAV, a MIDI file of two named tracks with three
    /// notes, and an empty `.mov`.
    static func files(in folder: URL) throws -> (png: URL, wav: URL, mid: URL, mov: URL) {
        let png = folder.appendingPathComponent("shot.png")
        try XCTUnwrap(image(width: 40, height: 20).representation(using: .png, properties: [:])).write(to: png)
        let wav = folder.appendingPathComponent("kick.wav")
        let format = try XCTUnwrap(AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 1))
        let file = try AVAudioFile(forWriting: wav, settings: format.settings)
        let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 22050))
        buffer.frameLength = 22050
        try file.write(from: buffer)
        let mid = folder.appendingPathComponent("two.mid")
        try midi().write(to: mid)
        let mov = folder.appendingPathComponent("clip.mov")
        try Data([0, 0, 0, 0]).write(to: mov)
        return (png, wav, mid, mov)
    }

    /// A bitmap, red on its left half and blue on its right when painted;
    /// a large one is left blank, since painting it pixel by pixel is slow.
    static func image(width: Int, height: Int, painted: Bool = true) -> NSBitmapImageRep {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4,
                                   hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        guard painted else { return rep }
        for x in 0..<width {
            for y in 0..<height {
                rep.setColor(x < width / 2 ? .red : .blue, atX: x, y: y)
            }
        }
        return rep
    }

    /// A type 1 MIDI file: a Piano track of two notes and a Bass track of
    /// one, each named by a meta event, with running status in the second.
    static func midi() -> Data {
        func chunk(_ type: String, _ body: [UInt8]) -> [UInt8] {
            Array(type.utf8) + [UInt8(body.count >> 24 & 0xFF), UInt8(body.count >> 16 & 0xFF), UInt8(body.count >> 8 & 0xFF), UInt8(body.count & 0xFF)] + body
        }
        let piano: [UInt8] = [0x00, 0xFF, 0x03, 0x05] + Array("Piano".utf8)
            + [0x00, 0x90, 60, 100, 0x60, 0x80, 60, 0, 0x00, 0x90, 64, 90, 0x60, 0x80, 64, 0, 0x00, 0xFF, 0x2F, 0x00]
        let bass: [UInt8] = [0x00, 0xFF, 0x03, 0x04] + Array("Bass".utf8)
            + [0x00, 0x90, 36, 100, 0x81, 0x40, 36, 0, 0x00, 0xFF, 0x2F, 0x00]
        return Data(chunk("MThd", [0, 1, 0, 2, 0, 96]) + chunk("MTrk", piano) + chunk("MTrk", bass))
    }

    func testAFileIsReadByItsKind() throws {
        let files = try Self.files(in: scratch)
        XCTAssertEqual(AgentAttachments.kind(of: files.png), .image)
        XCTAssertEqual(AgentAttachments.kind(of: files.wav), .sound)
        XCTAssertEqual(AgentAttachments.kind(of: files.mid), .midi)
        XCTAssertEqual(AgentAttachments.kind(of: files.mov), .video)
        XCTAssertEqual(AgentAttachments.kind(of: scratch), .folder)
        XCTAssertEqual(AgentAttachments.kind(of: URL(fileURLWithPath: "/x/notes.md")), .document)
        XCTAssertEqual(AgentAttachments.kind(of: URL(fileURLWithPath: "/x/thing.xyz")), .other)

        let png = AgentAttachments.make(file: files.png)
        XCTAssertEqual(png.kind, .image)
        XCTAssertEqual(png.name, "shot.png")
        XCTAssertEqual(png.path, files.png.path)
        XCTAssertEqual(png.detail, "40×20; seen")
        XCTAssertEqual(png.picture?.mediaType, "image/png")
        XCTAssertEqual(png.line, "- image: \(files.png.path) (40×20; seen)")
        XCTAssertFalse(png.missing)
        let wav = AgentAttachments.make(file: files.wav)
        XCTAssertEqual(wav.detail, "0.50 s, mono, 44.1 kHz")
        XCTAssertEqual(AgentAttachments.make(file: files.mid).detail, "2 tracks: Piano, Bass; 3 notes")
        let mov = AgentAttachments.make(file: files.mov)
        XCTAssertEqual(mov.detail, "4 bytes, not read")
        let folder = AgentAttachments.make(file: scratch)
        XCTAssertEqual(folder.kind, .folder)
        XCTAssertEqual(folder.detail, "4 items")
        let doc = scratch.appendingPathComponent("notes.md")
        try String(repeating: "a", count: 2048).write(to: doc, atomically: true, encoding: .utf8)
        XCTAssertEqual(AgentAttachments.make(file: doc).detail, "2 KB")
        try FileManager.default.removeItem(at: doc)
        XCTAssertTrue(AgentAttachments.make(file: doc).missing)
        let broken = scratch.appendingPathComponent("broken.png")
        try Data([1, 2, 3]).write(to: broken)
        XCTAssertEqual(AgentAttachments.make(file: broken).detail, "could not be read")
        XCTAssertNil(AgentAttachments.make(file: broken).picture)
    }

    /// A large image is scaled to the API's longest side, and a wide one by
    /// its width; a small one keeps its pixels.
    func testAnImageIsScaledToTheLimit() throws {
        let tall = NSImage(size: NSSize(width: 100, height: 200))
        tall.addRepresentation(Self.image(width: 1000, height: 4000, painted: false))
        let picture = try XCTUnwrap(AgentAttachments.picture(of: tall))
        XCTAssertEqual(picture.image.size, NSSize(width: 392, height: 1568))
        let rep = try XCTUnwrap(NSBitmapImageRep(data: picture.data))
        XCTAssertEqual(rep.pixelsWide, 392)
        XCTAssertEqual(rep.pixelsHigh, 1568)
        XCTAssertLessThanOrEqual(picture.data.count, AgentAttachments.mostBytes)
        let small = NSImage(size: NSSize(width: 40, height: 20))
        small.addRepresentation(Self.image(width: 40, height: 20))
        XCTAssertEqual(AgentAttachments.picture(of: small)?.image.size, NSSize(width: 40, height: 20))
    }

    /// An image with no file, as one pasted, is written under the project's
    /// `.daw/attachments/` and attached by that path.
    func testAPastedImageIsWrittenToTheProject() throws {
        let image = NSImage(size: NSSize(width: 40, height: 20))
        image.addRepresentation(Self.image(width: 40, height: 20))
        let now = Date(timeIntervalSince1970: 1_760_050_000)
        let first = try XCTUnwrap(AgentAttachments.make(image: image, project: scratch, now: now))
        XCTAssertEqual(first.kind, .image)
        XCTAssertTrue(first.name.hasPrefix("pasted-2025-10-09-"), first.name)
        XCTAssertEqual(first.path, scratch.appendingPathComponent(".daw/attachments/\(first.name)").path)
        XCTAssertTrue(FileManager.default.fileExists(atPath: first.path ?? ""))
        XCTAssertEqual(first.detail, "40×20; seen")
        let second = try XCTUnwrap(AgentAttachments.make(image: image, project: scratch, now: now))
        XCTAssertNotEqual(second.path, first.path, "a second paste in the same second gets its own name")
        XCTAssertTrue(second.name.hasSuffix("-2.png"), second.name)
    }

    func testAMIDIFileIsSummedUp() {
        let summary = MIDISummary(Self.midi())
        XCTAssertTrue(summary.read)
        XCTAssertEqual(summary.format, 1)
        XCTAssertEqual(summary.tracks, ["Piano", "Bass"])
        XCTAssertEqual(summary.notes, 3)
        XCTAssertEqual(summary.detail, "2 tracks: Piano, Bass; 3 notes")
        XCTAssertFalse(MIDISummary(Data("not midi".utf8)).read)
        XCTAssertEqual(MIDISummary(Data("not midi".utf8)).detail, "not a MIDI file")
        XCTAssertEqual(MIDISummary(Data()).read, false)
    }
}
