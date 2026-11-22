# A track or a group dragged into a group — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 1. The person's note of November
22, 2026 (D194), from using the app: "I created a group and now I'm trying to
drag a new track into the group and getting this error: dub: the tracks of
group group-1 must be next to each other; move it beside them with daw track
move", and "I should be able to have groups within a group ... And each one
of those could have its own processing chain on it." What is said here about
the app was read from the checkout on November 22, 2026.

## What

A track's header dragged among a group's tracks, or onto the group's own
header, joins that group and lands where it was dropped; dragged out of the
group's tracks, above or below them, it leaves the group. A group's header
dragged the same way moves the group with everything under it, into another
group or out to the master, which is how a group within a group is made by
hand. The line drawn while dragging says where it will land: set in under
the group it joins, at the group's depth. Nothing a drag can make is refused.

The same is in a header's menu for a person who does not drag: Move to Group
▸ lists the song's groups and New Group, with Remove from Group as now.

## Why

Groups within groups, each with its own chain, fader, pan, mute, solo and
sends, are built ([groups within groups](nested-groups.md), D129), but the
only way to make one in the app is Shift-click on the rows and ⌘G, which
makes a new group; there is no way to put a track or a group into a group
that already exists. A drag of a header today is only a reorder
(`ArrangementView`'s `.reorder` drag, ending in `Edit::Move`, the host's
`track.move` or `group.move`): it never changes the row's `group`. Dropped
among a group's tracks, the track splits the group's run of tracks, and the
song's rule that a group's tracks are next to each other
(`aaw-model/src/rules.rs`) refuses the edit with a message written for the
agent, naming the track after the gap and telling a person to use `daw track
move`. The same refusal is met by Add Track with a track of a group selected
when it is not the group's last, since the new track goes after the
selected one in no group, and by Remove from Group on a track in the middle
of its group.

## Design

- **One edit that moves and regroups.** The app sends the row's new index
  and its new group as one edit: a host command, `track.place` (and
  `group.place`), taking `index` and `group` and applied as one change so
  the rule is checked once, after both. The CLI gets it as `daw track move
  SONG ID --to INDEX [--group G | --no-group]`.
- **What a drop means.** The slot between two rows decides the group: a slot
  between two rows under the same group joins that group; a slot right under
  a group's header joins it; a slot after a group's last row is ambiguous and
  is decided by the pointer's x, set in for the group, at the left edge for
  the group above it, as Ableton's and Logic's track lists do. A group
  dropped into itself or a group under it is not offered.
- **The other paths that split a group.** Add Track and Add MIDI Track with a
  grouped track selected make the new track in that group. Remove from Group
  on a track in the middle of its group moves it to just after the group's
  last track. A refusal the app still meets is said in the person's words,
  never with a `daw` command.

## Done when

- A track dragged into, within and out of a group lands there and plays
  through that group's chain, with no error; the same for a group dragged
  into another group and out of it.
- Add Track with a grouped track selected, and Remove from Group on a middle
  track, leave a song the host accepts.
- Move to Group ▸ does what the drag does.

## Open questions

1. Whether the drop's group is decided by the pointer's x after a group's
   last row, as proposed, or by a modifier key.
