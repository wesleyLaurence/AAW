import AAWCore
import AppKit
import SwiftUI

/// A gap of the device panel's chain, or of a rack's (`rack`), before the
/// effect of `index` or after the last; `frame` is where it sits in the
/// chain, the line drawn down its middle when a drag would land there.
struct ChainGap: Equatable {
    var rack: UInt64?
    var index: Int
    var frame: CGRect
}

/// Where the device panel's gaps and racks are, in the chain's own
/// coordinates, as the panels report them.
struct ChainGeometry: Equatable {
    var gaps: [ChainGap] = []
    /// Each rack's frame of effects, under its title bar, by the rack's key.
    var racks: [UInt64: CGRect] = [:]

    /// The coordinate space the frames are in: the chain's.
    static let space = "chain"
}

struct ChainGeometryKey: PreferenceKey {
    static let defaultValue = ChainGeometry()
    static func reduce(value: inout ChainGeometry, nextValue: () -> ChainGeometry) {
        let next = nextValue()
        value.gaps += next.gaps
        value.racks.merge(next.racks) { $1 }
    }
}

/// What a drag over the device panel's chain would do where it is let go.
/// Pure, so that it can be tested.
enum ChainDrop {
    /// What is being dragged: an effect of the panel's by its title, or a
    /// device out of the browser.
    enum Dragged: Equatable {
        case effect(UInt64)
        case device(DraggedDevice)
    }

    enum Landing: Equatable {
        /// The effect moves within its list, the chain or a rack, to `index`.
        case move(UInt64, index: Int)
        /// The effect moves into a rack, or out of one to the chain (nil),
        /// in at `index`.
        case cross(UInt64, rack: UInt64?, index: Int)
        /// A copy of the effect goes in at `index` of the chain or a rack.
        case copy(UInt64, rack: UInt64?, index: Int)
        /// An effect, a rack or a plugin from the browser goes in at `index`.
        case add(DraggedDevice, rack: UInt64?, index: Int)
        /// An instrument, a MIDI effect, a Synth patch or an instrument rack
        /// from the browser, which goes where its kind goes, not at a gap.
        case device(DraggedDevice)
    }

    /// The gap nearest `point`: among a rack's when the point is over its
    /// effects and what is dragged can go into a rack, else among the
    /// chain's.
    static func gap(at point: CGPoint, in geometry: ChainGeometry, intoRacks: Bool) -> ChainGap? {
        let rack = intoRacks ? geometry.racks.first { $0.value.contains(point) }?.key : nil
        return geometry.gaps.filter { $0.rack == rack }.min { abs($0.frame.midX - point.x) < abs($1.frame.midX - point.x) }
    }

    /// What letting go of `dragged` at `point` does, and the gap lit for it;
    /// nil where it would do nothing: an effect on either side of itself, a
    /// device the row does not take. `effects` is the chain's; `inChain`
    /// says whether a kind from the browser goes into the effect chain, and
    /// `takes` whether the row takes it at all.
    static func landing(_ dragged: Dragged, at point: CGPoint, in geometry: ChainGeometry, effects: [EffectView], copying: Bool,
                        inChain: (String) -> Bool, takes: (String) -> Bool) -> (landing: Landing, gap: ChainGap?)? {
        switch dragged {
        case .effect(let key):
            // A rack goes into no rack: over one, it is the chain's gap.
            let isRack = DeviceChain.find(key, in: effects)?.kind == "rack"
            guard let gap = gap(at: point, in: geometry, intoRacks: !isRack) else { return nil }
            if copying { return (.copy(key, rack: gap.rack, index: gap.index), gap) }
            guard let place = DeviceChain.place(of: key, in: effects) else { return nil }
            if place.rack == gap.rack {
                guard let to = DeviceChain.moveTarget(from: place.index, strip: gap.index) else { return nil }
                return (.move(key, index: to), gap)
            }
            return (.cross(key, rack: gap.rack, index: gap.index), gap)
        case .device(let device):
            guard takes(device.kind) else { return nil }
            guard inChain(device.kind) else { return (.device(device), nil) }
            // An effect goes into a rack, and a saved rack's effects are laid
            // in; an empty rack and a plugin go in the chain alone.
            let intoRacks = device.kind == "rack" ? device.rack != nil : DeviceChain.kinds.contains(device.kind)
            guard let gap = gap(at: point, in: geometry, intoRacks: intoRacks) else { return nil }
            return (.add(device, rack: gap.rack, index: gap.index), gap)
        }
    }
}

/// The gap of a chain or a rack, between two devices or after the last,
/// reporting where it is so that the chain's drop target can draw a line
/// down it.
struct ChainGapMark: View {
    var rack: UInt64?
    let index: Int
    let height: CGFloat

    var body: some View {
        Color.clear
            .frame(width: 2, height: height)
            .background(GeometryReader { g in
                Color.clear.preference(key: ChainGeometryKey.self,
                                       value: ChainGeometry(gaps: [ChainGap(rack: rack, index: index, frame: g.frame(in: .named(ChainGeometry.space)))]))
            })
    }
}

/// The device panel's chain as one drop target, over its whole width: a
/// device or an effect dragged over it lands in the gap nearest the
/// pointer, with a line down that gap, as Ableton's chain does.
struct ChainDropTarget: NSViewRepresentable {
    let model: SongModel
    let chain: DeviceChain
    let geometry: ChainGeometry

    func makeNSView(context: Context) -> ChainDropView {
        ChainDropView(model: model, chain: chain, geometry: geometry)
    }

    func updateNSView(_ view: ChainDropView, context: Context) {
        view.chain = chain
        view.geometry = geometry
    }
}

/// Laid over the chain's panels, taking no clicks: AppKit offers a drag
/// to the frontmost view registered for its type under the pointer, so
/// this view takes every drag of a device or an effect over the chain,
/// while a click goes through to the panel under it.
final class ChainDropView: NSView {
    private let model: SongModel
    var chain: DeviceChain
    var geometry: ChainGeometry
    /// The gap lit for the drag over the view.
    private var lit: ChainGap? {
        didSet { if lit != oldValue { needsDisplay = true } }
    }

    /// The types of a device dragged out of the browser and an effect
    /// dragged out of the panel.
    static let types: [NSPasteboard.PasteboardType] =
        ([Browser.deviceType, Browser.patchType, Browser.rackType, Browser.pluginType, Browser.instrumentRackType, DeviceChain.effectType]
            + (DeviceChain.kinds + Browser.midiEffects + Browser.instruments).map { Browser.deviceType + "." + $0 }).map { NSPasteboard.PasteboardType($0) }

    init(model: SongModel, chain: DeviceChain, geometry: ChainGeometry) {
        self.model = model
        self.chain = chain
        self.geometry = geometry
        super.init(frame: .zero)
        registerForDraggedTypes(Self.types)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }

    /// Clicks go to the panels under it.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// What is being dragged: an effect of the panel's, or a device of the
    /// browser's, each held from when its drag began, since a drag's data
    /// is not readable until it is let go.
    private func dragged(_ sender: NSDraggingInfo) -> ChainDrop.Dragged? {
        guard sender.draggingSource != nil, let types = sender.draggingPasteboard.types else { return nil }
        if types.contains(NSPasteboard.PasteboardType(DeviceChain.effectType)), let key = model.draggedEffect { return .effect(key) }
        if types.contains(where: { Self.types.contains($0) || $0.rawValue.hasPrefix(Browser.deviceType + ".") }),
           let device = model.browser.draggedDevice {
            return .device(device)
        }
        return nil
    }

    private func landing(_ dragged: ChainDrop.Dragged, at point: CGPoint, copying: Bool) -> (landing: ChainDrop.Landing, gap: ChainGap?)? {
        let row = chain.row
        return ChainDrop.landing(dragged, at: point, in: geometry, effects: chain.effects, copying: copying,
                                 inChain: { [model] in model.goesInChain($0) }, takes: { [model] in model.canAddBrowserDevice($0, to: row) })
    }

    private func land(_ landing: ChainDrop.Landing) {
        let row = chain.row
        switch landing {
        case .move(let key, let index): model.edit(.effectMove(effect: key, index: UInt32(index)))
        case .cross(let key, let rack, let index): model.moveEffect(key, in: row, to: rack, index: UInt32(index))
        case .copy(let key, let rack, let index): model.copyEffect(key, to: row, index: UInt32(index), into: rack)
        case .add(let device, let rack, let index):
            model.addBrowserDevice(device.kind, to: row, index: UInt32(index), patch: device.patch, rack: device.rack, into: rack)
        case .device(let device): model.addBrowserDevice(device.kind, to: row, patch: device.patch, rack: device.rack)
        }
    }

    /// Lets go of `dragged` at a point of the view, as a drag that ends
    /// there does; false where it would do nothing. Scripted runs hand
    /// their drops here, since no drag can be made up.
    @discardableResult
    func drop(_ dragged: ChainDrop.Dragged, at point: CGPoint, copying: Bool = false) -> Bool {
        guard let landing = landing(dragged, at: point, copying: copying) else { return false }
        land(landing.landing)
        return true
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        draggingUpdated(sender)
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard let dragged = dragged(sender),
              let landing = landing(dragged, at: convert(sender.draggingLocation, from: nil), copying: NSEvent.modifierFlags.contains(.option)) else {
            lit = nil
            return []
        }
        lit = landing.gap
        switch landing.landing {
        case .move, .cross: return .move
        default: return .copy
        }
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        lit = nil
    }

    override func draggingEnded(_ sender: NSDraggingInfo) {
        lit = nil
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        lit = nil
        guard let dragged = dragged(sender) else { return false }
        return drop(dragged, at: convert(sender.draggingLocation, from: nil), copying: NSEvent.modifierFlags.contains(.option))
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let gap = lit else { return }
        NSColor.controlAccentColor.setFill()
        NSBezierPath(roundedRect: CGRect(x: gap.frame.midX - 1.5, y: gap.frame.minY, width: 3, height: gap.frame.height), xRadius: 1.5, yRadius: 1.5).fill()
    }
}
