import CoreGraphics

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
        case body
    }

    /// The height of one send under a track's header.
    public static let sendHeight: CGFloat = 17
    /// Space under the last send.
    public static let sendPadding: CGFloat = 5
    public static let width: CGFloat = TimelineLayout.headerWidth - 1

    public var kind: Kind
    /// The header's top, in the view.
    public var top: CGFloat
    /// Sends shown under a track's header: none while they are folded away.
    public var sends: Int
    /// Whether the track has a fold mark, which it has when the song has returns.
    public var folds: Bool

    public init(kind: Kind, top: CGFloat, sends: Int = 0, folds: Bool = false) {
        self.kind = kind
        self.top = top
        self.sends = sends
        self.folds = folds
    }

    /// How much taller a track's header is with `sends` sends shown.
    public static func extraHeight(sends: Int) -> CGFloat {
        sends == 0 ? 0 : CGFloat(sends) * sendHeight + sendPadding
    }

    public var height: CGFloat {
        kind == .track ? TimelineLayout.trackHeight + Self.extraHeight(sends: sends) : TimelineLayout.busHeight
    }

    public var fold: CGRect {
        CGRect(x: 8, y: top + 7, width: 12, height: 12)
    }

    public var name: CGRect {
        switch kind {
        case .track:
            let left: CGFloat = folds ? 23 : 14
            return CGRect(x: left, y: top + 5, width: mute.minX - 6 - left, height: 16)
        case .bus, .master:
            return CGRect(x: 14, y: top + 7, width: volume.minX - 6 - 14, height: 16)
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

    /// What of the header is at a point of the view.
    public func part(at p: CGPoint) -> Part {
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
