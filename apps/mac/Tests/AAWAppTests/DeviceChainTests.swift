import AAWCore
import XCTest
@testable import AAWApp

final class DeviceChainTests: XCTestCase {
    func testAStripSaysWhereADraggedEffectGoesAndTheTwoBesideItRefuseIt() {
        // Four effects have five strips; strip i sits before effect i, and
        // the index given is where the effect ends up.
        XCTAssertEqual(DeviceChain.moveTarget(from: 2, strip: 0), 0)
        XCTAssertEqual(DeviceChain.moveTarget(from: 2, strip: 1), 1)
        XCTAssertNil(DeviceChain.moveTarget(from: 2, strip: 2), "the strip before it leaves it where it is")
        XCTAssertNil(DeviceChain.moveTarget(from: 2, strip: 3), "and so does the one after it")
        XCTAssertEqual(DeviceChain.moveTarget(from: 2, strip: 4), 3, "after the last: the fourth moves up")
        XCTAssertEqual(DeviceChain.moveTarget(from: 0, strip: 4), 3)
        XCTAssertEqual(DeviceChain.moveTarget(from: 3, strip: 0), 0)
        XCTAssertNil(DeviceChain.moveTarget(from: 0, strip: 1), "one effect: neither strip takes it")
    }

    private func effect(_ key: UInt64, kind: String = "compressor", effects: [EffectView] = []) -> EffectView {
        EffectView(key: key, kind: kind, id: nil, bypass: false, fields: [], bands: 0, name: nil, effects: effects, note: nil)
    }

    func testAnEffectIsFoundAndPlacedThroughTheRackItIsIn() {
        // A chain of a compressor, a rack of two, and a limiter.
        let chain = [effect(1), effect(2, kind: "rack", effects: [effect(21, kind: "filter"), effect(22, kind: "eq")]), effect(3, kind: "limiter")]
        XCTAssertEqual(DeviceChain.find(22, in: chain)?.kind, "eq")
        XCTAssertEqual(DeviceChain.find(2, in: chain)?.kind, "rack")
        XCTAssertNil(DeviceChain.find(9, in: chain))
        XCTAssertEqual(DeviceChain.place(of: 3, in: chain)?.index, 2)
        XCTAssertNil(DeviceChain.place(of: 3, in: chain)?.rack)
        XCTAssertEqual(DeviceChain.place(of: 21, in: chain)?.rack, 2)
        XCTAssertEqual(DeviceChain.place(of: 21, in: chain)?.index, 0)
        XCTAssertNil(DeviceChain.place(of: 9, in: chain))
        // The chain's effects cross into the rack and the rack's out, but
        // not a rack into a rack, and not within one list.
        XCTAssertTrue(DeviceChain.crosses(1, in: chain, to: 2))
        XCTAssertTrue(DeviceChain.crosses(22, in: chain, to: nil))
        XCTAssertFalse(DeviceChain.crosses(2, in: chain, to: 2), "a rack holds no rack")
        XCTAssertFalse(DeviceChain.crosses(1, in: chain, to: nil), "a move within the chain is the strip's own")
        XCTAssertFalse(DeviceChain.crosses(21, in: chain, to: 2), "and within the rack too")
        XCTAssertFalse(DeviceChain.crosses(9, in: chain, to: 2))
    }

    /// A chain of a compressor, a rack of a filter and an eq, and a limiter,
    /// laid out as the panel lays it: a 2-point gap, a device, a gap. The
    /// compressor spans 10–226, the rack 236–700 with its effects from 54
    /// points down, and the limiter 710–926.
    private var laidOut: (effects: [EffectView], geometry: ChainGeometry) {
        let effects = [effect(1), effect(2, kind: "rack", effects: [effect(21, kind: "filter"), effect(22, kind: "eq")]), effect(3, kind: "limiter")]
        func gap(_ x: CGFloat, _ index: Int, rack: UInt64? = nil) -> ChainGap {
            ChainGap(rack: rack, index: index, frame: CGRect(x: x, y: rack == nil ? 8 : 62, width: 2, height: 200))
        }
        let geometry = ChainGeometry(
            gaps: [gap(0, 0), gap(226, 1), gap(700, 2), gap(926, 3), gap(244, 0, rack: 2), gap(470, 1, rack: 2), gap(690, 2, rack: 2)],
            racks: [2: CGRect(x: 236, y: 40, width: 464, height: 230)])
        return (effects, geometry)
    }

    func testTheGapNearestThePointerIsChosenInTheChainOrTheRackItIsOver() {
        let g = laidOut.geometry
        func gap(_ x: CGFloat, _ y: CGFloat = 100, intoRacks: Bool = true) -> [Int?] {
            let gap = ChainDrop.gap(at: CGPoint(x: x, y: y), in: g, intoRacks: intoRacks)
            return [gap.map { Int($0.rack ?? 0) }, gap?.index]
        }
        XCTAssertEqual(gap(5), [0, 0], "the compressor's left half: before it")
        XCTAssertEqual(gap(150), [0, 1], "its right half: after it")
        XCTAssertEqual(gap(231), [0, 1], "between two devices: that gap")
        XCTAssertEqual(gap(300), [2, 0], "over the rack's effects: the rack's gaps")
        XCTAssertEqual(gap(400), [2, 1])
        XCTAssertEqual(gap(650), [2, 2])
        XCTAssertEqual(gap(300, 20), [0, 1], "over its title bar: the chain's")
        XCTAssertEqual(gap(650, 20), [0, 2])
        XCTAssertEqual(gap(300, intoRacks: false), [0, 1], "what goes into no rack: the chain's")
        XCTAssertEqual(gap(800), [0, 2])
        XCTAssertEqual(gap(1400), [0, 3], "past the end, as far as the panel goes: after the last")
    }

    func testAnEffectDraggedAlongTheChainMovesIntoOutOfOrAlongARackAndRefusesTheGapsBesideIt() {
        let (effects, g) = laidOut
        func landing(_ key: UInt64, _ x: CGFloat, _ y: CGFloat = 100, copying: Bool = false) -> ChainDrop.Landing? {
            ChainDrop.landing(.effect(key), at: CGPoint(x: x, y: y), in: g, effects: effects, copying: copying, inChain: { _ in true }, takes: { _ in true })?.landing
        }
        XCTAssertNil(landing(1, 5), "the gap before it")
        XCTAssertNil(landing(1, 231), "and after it")
        XCTAssertEqual(landing(1, 800), .move(1, index: 1), "after the rack")
        XCTAssertEqual(landing(1, 1000), .move(1, index: 2), "after the last")
        XCTAssertEqual(landing(3, 5), .move(3, index: 0))
        XCTAssertEqual(landing(1, 400), .cross(1, rack: 2, index: 1), "into the rack, between its two")
        XCTAssertEqual(landing(21, 1000), .cross(21, rack: nil, index: 3), "out of the rack, to the end")
        XCTAssertEqual(landing(21, 650), .move(21, index: 1), "along the rack")
        XCTAssertNil(landing(21, 400), "beside itself in the rack")
        XCTAssertNil(landing(2, 300), "the rack over its own effects is beside itself in the chain")
        XCTAssertEqual(landing(2, 5), .move(2, index: 0), "and goes into no rack")
        XCTAssertEqual(landing(1, 5, copying: true), .copy(1, rack: nil, index: 0), "a copy beside itself")
        XCTAssertEqual(landing(3, 300, copying: true), .copy(3, rack: 2, index: 0))
        XCTAssertNil(landing(9, 300), "an effect not in the chain moves nowhere")
        XCTAssertEqual(landing(9, 300, copying: true), .copy(9, rack: 2, index: 0), "but is copied in")
    }

    func testADeviceFromTheBrowserGoesInAtTheGapOrWhereItsKindGoes() {
        let (effects, g) = laidOut
        func landing(_ device: DraggedDevice, _ x: CGFloat, takes: Bool = true) -> (ChainDrop.Landing, Int?)? {
            ChainDrop.landing(.device(device), at: CGPoint(x: x, y: 100), in: g, effects: effects, copying: false,
                              inChain: { DeviceChain.kinds.contains($0) || $0 == "my-plugin" }, takes: { _ in takes })
                .map { ($0.landing, $0.gap?.index) }
        }
        let delay = DraggedDevice(kind: "delay")
        XCTAssertEqual(landing(delay, 800)?.0, .add(delay, rack: nil, index: 2))
        XCTAssertEqual(landing(delay, 400)?.0, .add(delay, rack: 2, index: 1), "an effect goes into a rack")
        let saved = DraggedDevice(kind: "rack", rack: "Wide")
        XCTAssertEqual(landing(saved, 400)?.0, .add(saved, rack: 2, index: 1), "a saved rack's effects are laid in")
        let empty = DraggedDevice(kind: "rack")
        XCTAssertEqual(landing(empty, 400)?.0, .add(empty, rack: nil, index: 1), "an empty rack goes into no rack")
        let plugin = DraggedDevice(kind: "my-plugin")
        XCTAssertEqual(landing(plugin, 400)?.0, .add(plugin, rack: nil, index: 1), "nor does a plugin")
        let synth = DraggedDevice(kind: "synth", patch: "Pad")
        XCTAssertEqual(landing(synth, 400)?.0, .device(synth), "an instrument goes where instruments go")
        XCTAssertNil(landing(synth, 400)?.1, "with no gap lit")
        XCTAssertNil(landing(delay, 400, takes: false), "nothing the row does not take")
    }

    @MainActor func testTheSynthPanelIsWiderByAStripBeforeEachEffectAndAfterTheLast() {
        let column = SynthPanel.column + SynthPanel.gap
        let strip = SynthPanel.strip + SynthPanel.gap
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 0), 5 * column + SynthPanel.gap, "no strips without effects")
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 1), 6 * column + 2 * strip + SynthPanel.gap)
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 3), 8 * column + 4 * strip + SynthPanel.gap)
    }
}
