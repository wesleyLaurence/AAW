import AAWCore
import CoreGraphics
import Foundation

/// An audio clip as the arrangement draws and drags it: from its beat to where
/// its sound ends, with its fades inside. The song writes a fade out after the
/// place a clip leaves; here that place is the fade's length before the end,
/// as other DAWs draw it. Geometry only: the host works out what a drag means
/// for the song.
public struct AudioClipLayout: Equatable {
    /// The beat the clip starts on, and the beat its sound ends on.
    public var start: Double
    public var end: Double
    /// The beats its file starts and ends on, where its edges can go no
    /// further; nil for a file that cannot be read.
    public var fileStart: Double
    public var fileEnd: Double?
    /// The fades, in beats.
    public var fadeIn: Double
    public var fadeOut: Double

    /// How near an edge a press trims, in points.
    public static let edge: CGFloat = 5
    /// The side of a fade's handle, and how near it a press takes it.
    public static let handle: CGFloat = 7
    public static let reach: CGFloat = 6
    /// The least a clip can be trimmed to, in beats.
    public static let least = 1.0 / 64

    public enum Part: Equatable {
        case body, start, end, fadeIn, fadeOut
    }

    public init(start: Double, end: Double, fileStart: Double, fileEnd: Double?, fadeIn: Double, fadeOut: Double) {
        self.start = start
        self.end = end
        self.fileStart = fileStart
        self.fileEnd = fileEnd
        self.fadeIn = fadeIn
        self.fadeOut = fadeOut
    }

    /// The beats a fade of so many milliseconds lasts.
    public static func beats(ms: Double, tempo: Double) -> Double {
        ms / 1000 * tempo / 60
    }

    public static func ms(beats: Double, tempo: Double) -> Double {
        guard tempo > 0 else { return 0 }
        return beats * 60_000 / tempo
    }

    /// A clip of the song at `tempo`, whose file is `seconds` long when it
    /// can be read.
    public init(_ clip: AudioClipView, tempo: Double, seconds: Double?) {
        let perBeat = clip.secondsPerBeat > 0 ? clip.secondsPerBeat : 1
        let length = clip.lengthBeats + clip.tailBeats
        let fileStart = clip.at - clip.sourceStartSeconds / perBeat
        self.init(
            start: clip.at, end: clip.at + length, fileStart: fileStart,
            fileEnd: seconds.map { fileStart + $0 / perBeat },
            fadeIn: min(Self.beats(ms: clip.fadeInMs, tempo: tempo), length),
            fadeOut: min(Self.beats(ms: clip.fadeOutMs, tempo: tempo), length)
        )
    }

    public var length: Double { end - start }

    /// Where the fades' handles are, in a clip's rectangle: at the top, where
    /// the fade in ends and where the fade out begins.
    public func handles(in rect: CGRect) -> (fadeIn: CGPoint, fadeOut: CGPoint) {
        let perBeat = length > 0 ? rect.width / CGFloat(length) : 0
        return (CGPoint(x: rect.minX + CGFloat(fadeIn) * perBeat, y: rect.minY),
                CGPoint(x: rect.maxX - CGFloat(fadeOut) * perBeat, y: rect.minY))
    }

    /// What a press at `p` takes hold of, in the clip's body under its title:
    /// a fade's handle at the top, an edge at the sides, or else the clip. A
    /// clip too narrow for its parts only moves.
    public func part(at p: CGPoint, in body: CGRect) -> Part {
        guard body.width >= 16 else { return .body }
        let handles = handles(in: body)
        // The top third is the handles'. Where both are near, the nearer.
        if p.y <= body.minY + max(Self.handle, body.height / 3) {
            let (near, far) = (abs(p.x - handles.fadeIn.x), abs(p.x - handles.fadeOut.x))
            if min(near, far) <= Self.reach {
                // Two handles on one spot: the side of the clip the press is on.
                if near == far { return p.x < body.midX ? .fadeIn : .fadeOut }
                return near < far ? .fadeIn : .fadeOut
            }
        }
        if p.x <= body.minX + Self.edge { return .start }
        if p.x >= body.maxX - Self.edge { return .end }
        return .body
    }

    /// Where the start can be dragged to: no earlier than the file starts or
    /// the song does, and short of where the fade out begins.
    public func start(draggedTo beat: Double) -> Double {
        min(max(beat, fileStart, 0), end - fadeOut - Self.least)
    }

    /// Where the end can be dragged to: no later than the file ends, and
    /// after the start.
    public func end(draggedTo beat: Double) -> Double {
        max(min(beat, fileEnd ?? end), start + Self.least)
    }

    /// The fade in a handle dragged to `beat` gives, in beats: from none to
    /// where the fade out begins.
    public func fadeIn(draggedTo beat: Double) -> Double {
        min(max(beat - start, 0), length - fadeOut)
    }

    /// The fade out a handle dragged to `beat` gives: from none to where the
    /// fade in ends, and short of the whole clip, which has to play before it
    /// leaves.
    public func fadeOut(draggedTo beat: Double) -> Double {
        max(0, min(end - beat, length - max(fadeIn, Self.least)))
    }

    /// A fade's level at `x` of the way from silence to full, as the engine
    /// plays it.
    public static func level(_ x: Double, linear: Bool) -> Double {
        let x = min(max(x, 0), 1)
        return linear ? x : sin(x * .pi / 2)
    }
}

/// How far along a segment of automation is at `t` of its time, from 0 to 1,
/// for a point's `shape`: the engine's formula. Above zero the segment starts
/// slowly, as its progress to the power 4 to the shape; below zero it is the
/// mirror; zero is a straight line.
public func shapedProgress(_ t: Double, shape: Double) -> Double {
    let t = min(max(t, 0), 1)
    guard shape != 0 else { return t }
    let power = pow(4, abs(shape))
    return shape > 0 ? pow(t, power) : 1 - pow(1 - t, power)
}
