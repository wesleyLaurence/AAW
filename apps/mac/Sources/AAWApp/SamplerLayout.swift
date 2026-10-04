import CoreGraphics
import Foundation

/// Where a Sampler's panel draws its file and the markers for the part the
/// pad plays: seconds of the file to points across the waveform's width,
/// which marker a point takes hold of, and how far a dragged marker goes.
public struct SamplerLayout: Equatable {
    public enum Marker: Equatable {
        case start, end
    }

    /// The file's length.
    public var seconds: Double
    /// The waveform's width in points.
    public var width: CGFloat
    /// The seconds the pad plays from and to.
    public var start: Double
    public var end: Double

    /// How close to a marker a point takes hold of it.
    public static let grab: CGFloat = 6
    /// The least a start and an end are apart: the song wants the end after
    /// the start, and a thousandth is the finest a marker is written to.
    public static let least = 0.001

    public init(seconds: Double, width: CGFloat, start: Double, end: Double) {
        self.seconds = seconds
        self.width = width
        self.start = start
        self.end = end
    }

    public var pointsPerSecond: CGFloat {
        seconds > 0 ? width / CGFloat(seconds) : 0
    }

    /// Where a second of the file is, from the waveform's left edge.
    public func x(_ seconds: Double) -> CGFloat {
        CGFloat(seconds) * pointsPerSecond
    }

    /// The second of the file at a point, inside the file.
    public func seconds(atX x: CGFloat) -> Double {
        guard pointsPerSecond > 0 else { return 0 }
        return min(max(Double(x / pointsPerSecond), 0), seconds)
    }

    /// The marker within reach of a point, the nearer when both are.
    public func marker(atX x: CGFloat) -> Marker? {
        let (toStart, toEnd) = (abs(x - self.x(start)), abs(x - self.x(end)))
        guard min(toStart, toEnd) <= Self.grab else { return nil }
        // Markers on top of each other: the start is taken from its left.
        if toStart == toEnd { return x < self.x(start) ? .start : .end }
        return toStart < toEnd ? .start : .end
    }

    /// Where a marker dragged to a point lands, to a thousandth of a second:
    /// inside the file, and never past the other marker.
    public func dragged(_ marker: Marker, toX x: CGFloat) -> Double {
        let at = Self.rounded(seconds(atX: x))
        switch marker {
        case .start: return min(at, Self.rounded(end - Self.least))
        case .end: return max(at, Self.rounded(start + Self.least))
        }
    }

    /// Seconds as the song writes a marker: to a thousandth.
    public static func rounded(_ seconds: Double) -> Double {
        (seconds * 1000).rounded() / 1000
    }

    /// Seconds as the panel shows them, to a thousandth, without a fraction
    /// where there is none.
    public static func text(_ seconds: Double) -> String {
        let r = rounded(seconds)
        return r == r.rounded() ? String(Int(r)) : String(r)
    }
}
