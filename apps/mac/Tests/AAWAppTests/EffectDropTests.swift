import AAWCore
import AppKit
import UniformTypeIdentifiers
import XCTest
@testable import AAWApp

/// An effect dragged out of the device panel onto a row of the timeline, and
/// the drag types the app's drags carry.
final class EffectDropTests: XCTestCase {
    /// Runs the main run loop until `done` or two seconds pass.
    @MainActor
    private func wait(_ done: () -> Bool) {
        let end = Date().addingTimeInterval(2)
        while !done(), Date() < end {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
    }

    /// Two audio tracks and a return: the drums with a filter and a
    /// saturation whose drive has a lane, the bass with a compressor keyed
    /// from the drums.
    @MainActor
    private func open() throws -> (SongModel, URL) {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("aaw effect drop \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let song = folder.appendingPathComponent("song.yaml")
        try """
        session: {length_beats: 16, tempo: 120}
        tracks:
        - id: drums
          effects:
          - {type: filter, mode: lowpass, cutoff_hz: 8000}
          - {type: saturation, id: warm, drive_db: 6}
          automation:
          - {param: effects.warm.drive_db, points: [{at: 0, value: 3}]}
        - id: bass
          effects:
          - {type: compressor, threshold_db: -20, ratio: 4, sidechain: drums}
        returns:
        - id: plate
          effects:
          - {type: reverb}

        """.write(to: song, atomically: true, encoding: .utf8)
        return (try SongModel(url: song), folder)
    }

    @MainActor
    func testAnEffectLetGoOverAnotherRowMovesThereAndWithOptionIsCopied() throws {
        let (model, folder) = try open()
        defer {
            model.close()
            try? FileManager.default.removeItem(at: folder)
        }
        let a = model.arrangement
        let drums = RowID.track(a.tracks[0].key), bass = RowID.track(a.tracks[1].key), plate = RowID.bus(a.returns[0].key)
        let warm = a.tracks[0].effects[1].key, duck = a.tracks[1].effects[0].key
        XCTAssertEqual(model.row(ofEffect: warm), drums)
        XCTAssertEqual(model.row(ofEffect: duck), bass)

        // Without Option a move, to any row but its own; with it a copy, to its own too.
        XCTAssertEqual(model.effectLanding(warm, on: plate, copying: false), .move)
        XCTAssertEqual(model.effectLanding(warm, on: .master, copying: false), .move)
        XCTAssertNil(model.effectLanding(warm, on: drums, copying: false), "onto its own row it stays")
        XCTAssertEqual(model.effectLanding(warm, on: drums, copying: true), .copy)
        XCTAssertNil(model.effectLanding(99_999, on: plate, copying: false), "an effect no chain has")

        // Let go over the return: last in its chain, the lane with it, the
        // drums keeping the filter alone, and the return shown with it selected.
        model.dropEffect(warm, on: plate, copying: false)
        wait { model.arrangement.returns[0].effects.count == 2 }
        var now = model.arrangement
        XCTAssertEqual(now.returns[0].effects.map(\.kind), ["reverb", "saturation"])
        XCTAssertEqual(now.returns[0].effects[1].key, warm, "the effect itself, not a copy")
        XCTAssertEqual(now.tracks[0].effects.map(\.kind), ["filter"])
        XCTAssertEqual(now.returns[0].lanes.map(\.param), ["effects.warm.drive_db"])
        XCTAssertTrue(now.tracks[0].lanes.isEmpty)
        wait { model.selectedEffect == warm }
        XCTAssertEqual(model.deviceRow, plate)
        XCTAssertEqual(model.selectedEffect, warm)

        // With Option over the master, a copy; the return keeps its own.
        model.dropEffect(warm, on: .master, copying: true)
        wait { model.arrangement.master.effects.contains { $0.kind == "saturation" } }
        now = model.arrangement
        XCTAssertEqual(now.master.effects.last?.kind, "saturation")
        XCTAssertNotEqual(now.master.effects.last?.key, warm)
        XCTAssertEqual(now.returns[0].effects.last?.key, warm)

        // The compressor onto the track that keys it: refused, and it stays.
        model.dropEffect(duck, on: drums, copying: false)
        wait { model.refusal != nil }
        XCTAssertTrue(model.refusal?.contains("sidechain") == true, model.refusal ?? "no refusal")
        XCTAssertEqual(model.arrangement.tracks[0].effects.map(\.kind), ["filter"])
        XCTAssertEqual(model.arrangement.tracks[1].effects.first?.key, duck)
    }

    /// The scripted drop of a device by kind.
    func testADeviceIsLetGoAtAPointOfTheTimeline() {
        let launch = Launch(arguments: ["--drop-device", "compressor,400,120", "--drop-device", "compressor"])
        XCTAssertEqual(launch.actions, [.dropDevice("compressor", CGPoint(x: 400, y: 120))])
    }

    /// The scripted drop of one of the panel's effects, with Option or not.
    func testAnEffectIsLetGoAtAPointWithOptionOrNot() {
        let launch = Launch(arguments: ["--drop-effect", "lim,445,620", "--opt-drop-effect", "comp,720,620", "--drop-effect", "lim"])
        XCTAssertEqual(launch.actions, [.dropEffect("lim", CGPoint(x: 445, y: 620), copying: false),
                                        .dropEffect("comp", CGPoint(x: 720, y: 620), copying: true)])
    }

    /// macOS carries a drag's type only if an app declares it: every type
    /// the app's drags carry is in its Info.plist, conforming to data.
    @MainActor
    func testEveryDragTypeIsDeclared() throws {
        let plist = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Info.plist")
        let info = try XCTUnwrap(NSDictionary(contentsOf: plist) as? [String: Any])
        let declared = try XCTUnwrap(info["UTExportedTypeDeclarations"] as? [[String: Any]])
        let types = Dictionary(uniqueKeysWithValues: declared.compactMap { d in (d["UTTypeIdentifier"] as? String).map { ($0, d) } })
        let dragged = [Browser.deviceType, Browser.patchType, Browser.rackType, Browser.pluginType, Browser.instrumentRackType,
                       DeviceChain.effectType, SynthPanel.effectType, SynthPanel.sourceType]
        for type in dragged {
            let entry = try XCTUnwrap(types[type], "\(type) is not declared")
            XCTAssertEqual(entry["UTTypeConformsTo"] as? [String], ["public.data"], type)
        }
    }
}
