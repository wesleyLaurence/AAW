import CoreGraphics
import Foundation

/// Where the controls of a row's header are, for drawing them and for knowing
/// what a click is on. Geometry only.
public struct HeaderLayout: Equatable {
    public enum Kind: Equatable {
        case track, bus, master
    }

    public enum Part: Equatable {
        /// The mark that shows or hides a track's sends.
        case fold
        case name, mute, solo, volume, pan
        /// The send to the return at this place among the returns.
        case send(Int)
        /// The mark that shows or hides the row's automation lanes.
        case auto
        /// The header of the lane at this place among the row's lanes, and
        /// the mark that removes that lane.
        case lane(Int)
        case laneRemove(Int)
        /// The mark under the lanes that adds one.
        case laneAdd
        case body
    }

    /// The height of one send under a track's header.
    public static let sendHeight: CGFloat = 17
    /// Space under the last send.
    public static let sendPadding: CGFloat = 5
    /// The height of one automation lane, and of the strip under the lanes
    /// with the mark that adds one.
    public static let laneHeight: CGFloat = 46
    public static let laneAddHeight: CGFloat = 18
    public static let width: CGFloat = TimelineLayout.headerWidth - 1

    public var kind: Kind
    /// The header's top, in the view.
    public var top: CGFloat
    /// Sends shown under a track's header: none while they are folded away.
    public var sends: Int
    /// Whether the track has a fold mark, which it has when the song has returns.
    public var folds: Bool
    /// The row's automation lanes while they are shown; nil while folded away.
    public var lanes: Int?

    public init(kind: Kind, top: CGFloat, sends: Int = 0, folds: Bool = false, lanes: Int? = nil) {
        self.kind = kind
        self.top = top
        self.sends = sends
        self.folds = folds
        self.lanes = lanes
    }

    /// How much taller a track's header is with `sends` sends shown.
    public static func extraHeight(sends: Int) -> CGFloat {
        sends == 0 ? 0 : CGFloat(sends) * sendHeight + sendPadding
    }

    /// How much taller a row is with its lanes shown: the lanes, and the
    /// strip that adds one.
    public static func extraHeight(lanes: Int?) -> CGFloat {
        lanes.map { CGFloat($0) * laneHeight + laneAddHeight } ?? 0
    }

    /// The row's height without its lanes.
    public var baseHeight: CGFloat {
        kind == .track ? TimelineLayout.trackHeight + Self.extraHeight(sends: sends) : TimelineLayout.busHeight
    }

    public var height: CGFloat {
        baseHeight + Self.extraHeight(lanes: lanes)
    }

    public var fold: CGRect {
        CGRect(x: 8, y: top + 7, width: 12, height: 12)
    }

    public var name: CGRect {
        switch kind {
        case .track:
            let left: CGFloat = folds ? 23 : 14
            return CGRect(x: left, y: top + 5, width: auto.minX - 6 - left, height: 16)
        case .bus, .master:
            return CGRect(x: 14, y: top + 7, width: auto.minX - 6 - 14, height: 16)
        }
    }

    /// The mark that shows or hides the row's automation lanes.
    public var auto: CGRect {
        switch kind {
        case .track: CGRect(x: Self.width - 64, y: top + 6, width: 16, height: 14)
        case .bus, .master: CGRect(x: volume.minX - 20, y: top + 9, width: 16, height: 14)
        }
    }

    public var mute: CGRect {
        switch kind {
        case .track: CGRect(x: Self.width - 46, y: top + 6, width: 16, height: 14)
        case .bus, .master: CGRect(x: Self.width - 24, y: top + 9, width: 16, height: 14)
        }
    }

    public var solo: CGRect {
        CGRect(x: Self.width - 28, y: top + 6, width: 16, height: 14)
    }

    /// What a drag on the volume starts from: a track's bar with its value, or
    /// the value box of a return or the master.
    public var volume: CGRect {
        switch kind {
        case .track: CGRect(x: 12, y: top + 22, width: 150, height: 18)
        case .bus: CGRect(x: Self.width - 28 - 58, y: top + 8, width: 58, height: 16)
        case .master: CGRect(x: Self.width - 8 - 58, y: top + 8, width: 58, height: 16)
        }
    }

    /// A track's volume bar.
    public var volumeBar: CGRect {
        CGRect(x: 14, y: top + 30, width: 84, height: 4)
    }

    public var volumeText: CGRect {
        switch kind {
        case .track: CGRect(x: volumeBar.maxX + 6, y: top + 25, width: 56, height: 13)
        case .bus, .master: volume.insetBy(dx: 4, dy: 1.5)
        }
    }

    public var pan: CGRect {
        CGRect(x: Self.width - 8 - 36, y: top + 24, width: 36, height: 15)
    }

    /// The row of the send at `index`.
    public func send(_ index: Int) -> CGRect {
        CGRect(x: 0, y: top + TimelineLayout.trackHeight + CGFloat(index) * Self.sendHeight - 2,
               width: Self.width, height: Self.sendHeight)
    }

    public func sendName(_ index: Int) -> CGRect {
        CGRect(x: 23, y: send(index).minY + 2, width: 70, height: 13)
    }

    public func sendBar(_ index: Int) -> CGRect {
        CGRect(x: 98, y: send(index).midY - 2, width: 54, height: 4)
    }

    public func sendText(_ index: Int) -> CGRect {
        CGRect(x: 156, y: send(index).minY + 2, width: Self.width - 8 - 156, height: 13)
    }

    /// The header of the lane at `index`, beside the lane itself.
    public func lane(_ index: Int) -> CGRect {
        CGRect(x: 0, y: top + baseHeight + CGFloat(index) * Self.laneHeight, width: Self.width, height: Self.laneHeight)
    }

    public func laneName(_ index: Int) -> CGRect {
        CGRect(x: 23, y: lane(index).minY + 5, width: Self.width - 23 - 28, height: 13)
    }

    /// The range the lane covers, under its name.
    public func laneRange(_ index: Int) -> CGRect {
        CGRect(x: 23, y: lane(index).minY + 22, width: Self.width - 23 - 8, height: 13)
    }

    public func laneRemove(_ index: Int) -> CGRect {
        CGRect(x: Self.width - 22, y: lane(index).minY + 5, width: 14, height: 14)
    }

    /// The mark under the lanes that adds one.
    public var laneAdd: CGRect {
        CGRect(x: 23, y: top + baseHeight + CGFloat(lanes ?? 0) * Self.laneHeight + 2, width: 64, height: 14)
    }

    /// What of the header is at a point of the view.
    public func part(at p: CGPoint) -> Part {
        if auto.insetBy(dx: -1, dy: -2).contains(p) { return .auto }
        if let lanes, p.y >= top + baseHeight {
            for index in 0..<lanes where lane(index).contains(p) {
                return laneRemove(index).insetBy(dx: -3, dy: -3).contains(p) ? .laneRemove(index) : .lane(index)
            }
            return laneAdd.insetBy(dx: -2, dy: -2).contains(p) ? .laneAdd : .body
        }
        switch kind {
        case .track:
            if folds, fold.insetBy(dx: -4, dy: -4).contains(p) { return .fold }
            if mute.contains(p) { return .mute }
            if solo.contains(p) { return .solo }
            if pan.contains(p) { return .pan }
            if volume.contains(p) { return .volume }
            for index in 0..<sends where send(index).contains(p) { return .send(index) }
            return name.contains(p) ? .name : .body
        case .bus, .master:
            if kind == .bus, mute.contains(p) { return .mute }
            if volume.contains(p) { return .volume }
            return name.contains(p) ? .name : .body
        }
    }
}

/// How a level control reads a drag and fills its bar.
public enum Fader {
    /// The range a volume drag covers, in dB; the song allows more, which an
    /// agent or the file can set.
    public static let gain: ClosedRange<Double> = -60...6
    public static let send: ClosedRange<Double> = -60...0
    public static let pan: ClosedRange<Double> = -1...1

    /// How far along its bar a level is, from 0 to 1.
    public static func fraction(_ value: Double, in range: ClosedRange<Double>) -> CGFloat {
        CGFloat(min(max((value - range.lowerBound) / (range.upperBound - range.lowerBound), 0), 1))
    }

    /// A value after a drag of `dx` points at `perPoint` a point, a tenth of
    /// that with `fine`, kept in `range`.
    public static func dragged(_ value: Double, byX dx: CGFloat, perPoint: Double, fine: Bool,
                               in range: ClosedRange<Double>) -> Double {
        min(max(value + Double(dx) * perPoint * (fine ? 0.1 : 1), range.lowerBound), range.upperBound)
    }

    /// A value on its control's steps: tenths of a dB, hundredths of pan.
    public static func stepped(_ value: Double, step: Double) -> Double {
        ((value / step).rounded() * step * 1000).rounded() / 1000
    }

    public static func text(db: Double) -> String {
        String(format: "%+.1f dB", db).replacingOccurrences(of: "-", with: "−")
    }

    public static func text(pan: Double) -> String {
        let percent = Int((abs(pan) * 100).rounded())
        return percent == 0 ? "C" : (pan < 0 ? "L\(percent)" : "R\(percent)")
    }
}

/// How a parameter maps to the height of an automation lane or the width of a
/// knob's bar, and what a drag sets it to: linearly, or in equal ratios for a
/// frequency.
public struct ValueScale: Equatable {
    public var min: Double
    public var max: Double
    public var log: Bool

    /// Space kept clear above and below a lane's values, in points.
    public static let inset: CGFloat = 7

    public init(min: Double, max: Double, log: Bool) {
        self.min = min
        self.max = max
        self.log = log && min > 0
    }

    /// How far up its range a value is, from 0 to 1. Values outside the range
    /// are at its edge.
    public func fraction(_ value: Double) -> Double {
        guard max > min else { return 0 }
        let v = Swift.min(Swift.max(value, min), max)
        return log ? Foundation.log(v / min) / Foundation.log(max / min) : (v - min) / (max - min)
    }

    /// The value a fraction of the way up the range, on the scale's steps.
    public func value(at fraction: Double) -> Double {
        let f = Swift.min(Swift.max(fraction, 0), 1)
        let raw = log ? min * pow(max / min, f) : min + (max - min) * f
        return Swift.min(Swift.max(stepped(raw), min), max)
    }

    /// A value rounded as a person would set it: to three figures on a ratio
    /// scale, else to a step of about a five-hundredth of the range.
    public func stepped(_ value: Double) -> Double {
        let step: Double
        if log {
            guard value > 0 else { return value }
            step = pow(10, log10(value).rounded(.down) - 2)
        } else {
            step = pow(10, log10((max - min) / 200).rounded(.down))
        }
        return ((value / step).rounded() * step * 1e9).rounded() / 1e9
    }

    /// The y of a value in a lane's rectangle.
    public func y(_ value: Double, in rect: CGRect) -> CGFloat {
        rect.maxY - Self.inset - CGFloat(fraction(value)) * (rect.height - 2 * Self.inset)
    }

    /// The value at a y of a lane's rectangle.
    public func value(atY y: CGFloat, in rect: CGRect) -> Double {
        value(at: Double((rect.maxY - Self.inset - y) / (rect.height - 2 * Self.inset)))
    }

    /// The number a person typed into a control, read in the control's unit:
    /// the unit may follow it ("800 Hz", "-6 dB", "50%"), the minus may be the
    /// typographic one a control shows, and a frequency may be given in
    /// thousands ("2.5k", "2.50 kHz"). Nil for anything that is not a number.
    public static func parse(_ typed: String, unit: String) -> Double? {
        var text = typed.lowercased().replacingOccurrences(of: "−", with: "-").trimmingCharacters(in: .whitespaces)
        let suffix = unit.lowercased()
        var factor = 1.0
        if !suffix.isEmpty, text.hasSuffix(suffix) {
            text.removeLast(suffix.count)
            text = text.trimmingCharacters(in: .whitespaces)
        }
        if suffix == "hz", text.hasSuffix("k") {
            text.removeLast()
            factor = 1000
            text = text.trimmingCharacters(in: .whitespaces)
        }
        guard let value = Double(text), value.isFinite else { return nil }
        return value * factor
    }

    /// A number as it is written: without a fraction where it has none, and
    /// else to three places at the most.
    public static func plain(_ value: Double) -> String {
        if value == value.rounded() { return String(Int(value)) }
        var text = String(format: "%.3f", value)
        while text.hasSuffix("0") { text.removeLast() }
        return text
    }

    /// A value with its unit, as a control shows it. A level that can be
    /// negative shows its sign.
    public static func text(_ value: Double, unit: String, signed: Bool = true) -> String {
        func minus(_ s: String) -> String { s.replacingOccurrences(of: "-", with: "−") }
        switch unit {
        case "Hz":
            if value >= 1000 { return String(format: "%.2f kHz", value / 1000) }
            return String(format: value < 100 ? "%.1f Hz" : "%.0f Hz", value)
        case "dB": return minus(String(format: signed ? "%+.1f dB" : "%.1f dB", value))
        case "%": return String(format: "%.0f%%", value)
        case "ms": return String(format: value < 10 ? "%.1f ms" : "%.0f ms", value)
        case "s": return String(format: "%.2f s", value)
        case ":1": return String(format: "%.1f:1", value)
        case "": return minus(String(format: "%.2f", value))
        default: return minus(String(format: "%g", value)) + " " + unit
        }
    }
}
