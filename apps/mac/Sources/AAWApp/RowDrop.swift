import Foundation

/// Where a track's or a group's header dragged among the tracks and groups
/// lands: its place among the tracks and the group it goes into. The rows
/// are listed as the arrangement draws them: a group's header over the
/// first of the tracks under it, set in under the group it is in, folded
/// groups hiding what is under them, groups with no tracks under the
/// tracks. A slot is a gap between two of those rows; after a group's last
/// row the slot could be in that group or in a group over it, and the
/// pointer's distance from the left edge chooses, set in for the group, at
/// the edge for the one over it, as Ableton's and Logic's track lists do.
/// Pure, so that it can be tested.
public struct RowDrop {
    public enum Row: Hashable {
        case track(UInt64)
        case group(UInt64)
    }

    /// A row as drawn: what it is, and the groups over it, the outermost
    /// first.
    public struct Entry: Equatable {
        public var row: Row
        public var path: [UInt64]
        public var depth: Int { path.count }
    }

    /// Where a drop lands: `index` among the tracks, counting after the
    /// dragged ones are taken out; `group` the group it goes into, none
    /// for the master; `depth` how far in its row will sit; `slot` the gap
    /// among `entries` the line is drawn in.
    public struct Landing: Equatable {
        public var index: Int
        public var group: UInt64?
        public var depth: Int
        public var slot: Int
    }

    /// The rows as drawn, from the top.
    public private(set) var entries: [Entry] = []
    private let tracks: [(key: UInt64, group: UInt64?)]
    private let parents: [UInt64: UInt64]
    private let folded: Set<UInt64>

    /// `tracks` in their order with their groups; `groups` in theirs, with
    /// the group each is in; `folded` the groups folded away.
    public init(tracks: [(key: UInt64, group: UInt64?)], groups: [(key: UInt64, group: UInt64?)], folded: Set<UInt64>) {
        self.tracks = tracks
        var parents: [UInt64: UInt64] = [:]
        for g in groups {
            if let p = g.group { parents[g.key] = p }
        }
        self.parents = parents
        self.folded = folded
        var placed = Set<UInt64>()
        // As the arrangement places them: each track's groups first, the
        // outermost first, skipping what a fold hides.
        func place(_ path: [UInt64]) -> Bool {
            var hidden = false
            for (depth, group) in path.enumerated() {
                if !hidden, !placed.contains(group) {
                    entries.append(Entry(row: .group(group), path: Array(path.prefix(depth))))
                }
                placed.insert(group)
                if folded.contains(group) { hidden = true }
            }
            return hidden
        }
        for track in tracks {
            let path = self.path(over: track.group)
            if place(path) { continue }
            entries.append(Entry(row: .track(track.key), path: path))
        }
        for group in groups where !placed.contains(group.key) {
            _ = place(self.path(over: group.key))
        }
    }

    /// A group and the groups over it, the outermost first; none for none.
    private func path(over group: UInt64?) -> [UInt64] {
        var out: [UInt64] = []
        var at = group
        while let g = at, !out.contains(g) {
            out.insert(g, at: 0)
            at = parents[g]
        }
        return out
    }

    /// Whether a row is the dragged one or under it.
    private func isDragged(_ entry: Entry, _ dragged: Row) -> Bool {
        switch dragged {
        case .track: entry.row == dragged
        case .group(let g): entry.row == dragged || entry.path.contains(g)
        }
    }

    /// The tracks a drag takes: the track, or every track under the group.
    private func draggedTracks(_ dragged: Row) -> Set<UInt64> {
        switch dragged {
        case .track(let key): [key]
        case .group(let g): Set(tracks.filter { path(over: $0.group).contains(g) }.map(\.key))
        }
    }

    /// The slot nearest a height: above the first row whose middle is
    /// below it. `middles` are the rows' middles, in `entries` order.
    public static func slot(at y: CGFloat, middles: [CGFloat]) -> Int {
        middles.filter { $0 < y }.count
    }

    /// The depth the pointer asks for at `x` in the headers: one more for
    /// each `step` in from the left edge.
    public static func depth(atX x: CGFloat, step: CGFloat = 16) -> Int {
        max(0, Int((x - 6) / step))
    }

    /// Where `dragged` lands dropped in gap `slot` of `entries` with the
    /// pointer asking for `depth`; nil inside the dragged group's own rows.
    public func landing(of dragged: Row, slot: Int, depth wanted: Int) -> Landing? {
        guard slot >= 0, slot <= entries.count else { return nil }
        // Inside the dragged group's own rows there is nowhere to go.
        if slot > 0, slot < entries.count, isDragged(entries[slot - 1], dragged), isDragged(entries[slot], dragged) {
            return nil
        }
        // The rows and tracks left when the dragged ones are taken out.
        let left = entries.enumerated().filter { !isDragged($0.element, dragged) }
        let place = left.filter { $0.offset < slot }.count
        let rows = left.map(\.element)
        let taken = draggedTracks(dragged)
        let remaining = tracks.filter { !taken.contains($0.key) }
        let above = place > 0 ? rows[place - 1] : nil
        let below = place < rows.count ? rows[place] : nil

        // The groups it can be in: down the groups the row above is in, or
        // the group whose header it is, as far as the groups the row below
        // is in too.
        var deepest: [UInt64] = []
        if let above {
            deepest = above.path
            if case .group(let g) = above.row { deepest.append(g) }
        }
        let under = below?.path ?? []
        var shared = 0
        while shared < min(deepest.count, under.count), deepest[shared] == under[shared] { shared += 1 }
        let depth = min(max(wanted, shared), deepest.count)
        let group = depth == 0 ? nil : deepest[depth - 1]

        // Where among the tracks: after the row above and what it holds,
        // or, right under a group's header over its tracks, before them.
        func first(under g: UInt64) -> Int? {
            remaining.firstIndex { path(over: $0.group).contains(g) }
        }
        func end(of g: UInt64) -> Int {
            // After its last track; with none, after those of the nearest
            // group over it with tracks; else after every track.
            var at: UInt64? = g
            while let group = at {
                if let last = remaining.lastIndex(where: { path(over: $0.group).contains(group) }) { return last + 1 }
                at = parents[group]
            }
            return remaining.count
        }
        let index: Int
        switch above?.row {
        case nil:
            index = 0
        case .track(let key):
            index = (remaining.firstIndex { $0.key == key } ?? remaining.count - 1) + 1
        case .group(let g):
            if !folded.contains(g), depth == deepest.count, let first = first(under: g) {
                index = first
            } else {
                index = end(of: g)
            }
        }
        return Landing(index: index, group: group, depth: depth, slot: slot)
    }

    /// Whether a landing leaves `dragged` where it is, in its own group.
    public func isStill(_ dragged: Row, _ landing: Landing) -> Bool {
        let taken = draggedTracks(dragged)
        let current: UInt64?
        switch dragged {
        case .track(let key): current = tracks.first { $0.key == key }?.group
        case .group(let g): current = parents[g]
        }
        guard current == landing.group else { return false }
        // With no tracks under it, a group's place is its group alone.
        guard let from = tracks.firstIndex(where: { taken.contains($0.key) }) else { return true }
        return from == landing.index
    }
}

extension RowDrop.Row {
    /// The row as the arrangement names it.
    var id: RowID {
        switch self {
        case .track(let key): .track(key)
        case .group(let key): .group(key)
        }
    }
}
