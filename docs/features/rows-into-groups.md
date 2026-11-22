# A track or a group dragged into a group — implemented November 22, 2026

Status: built (D195). The person's note of November 22, 2026 (D194), from
using the app: "I created a group and now I'm trying to drag a new track
into the group and getting this error: dub: the tracks of group group-1 must
be next to each other; move it beside them with daw track move", and "I
should be able to have groups within a group ... And each one of those could
have its own processing chain on it."

## What

A track's header dragged among a group's rows, or right under the group's
header, joins that group and lands where it was dropped; dragged above or
below the group's rows, it leaves the group. A group's header dragged the
same way moves the group with everything under it, into another group or
out to the master: a group within a group made by hand. The line drawn
while dragging is set in as the row will be, with a short upright mark at
its start, so it says which group the row lands in. Nothing a drag offers
is refused. A group is not dropped among its own rows.

The same is in a header's menu: Move to Group ▸ lists the song's groups,
set in as the arrangement draws them, the one the row is in checked, and
New Group, which groups the row alone where it is, as ⌘G. Remove from Group
takes a track or a group one group out, to the group over its own or the
master, and puts it after the group it leaves. Add Track and Add MIDI Track
with a grouped track selected make the new track after it in its group;
with a group selected, after the group's last track, in it.

## Why

Groups within groups, each with its own chain, fader, pan, mute, solo and
sends, were built ([groups within groups](nested-groups.md), D129), but the
only way to make one in the app was Shift-click on the rows and ⌘G, which
makes a new group. A drag of a header was only a reorder (`Edit::Move`,
`track.move`, `group.move` among `groups[]`, which left a group's tracks
where they were): it never changed a row's `group`. A track dropped among a
group's tracks split the group's run of tracks, and the song's rule that a
group's tracks are next to each other refused the edit with a message
written for the agent. Add Track with a grouped track selected and Remove
from Group on a track in the middle of its group met the same refusal.

## Design

- **One edit that moves and regroups.** `track.move` takes `index`
  (optional now) and `group`: a group's ID puts the track in it, null takes
  it to the master, absent keeps its group. `group.move` takes `index` among
  the tracks, where the group's first track goes, and `parent`, the same
  way; its tracks move together. The index counts after the moved tracks are
  taken out. The song is checked once, after both, as for every command.
  With a group and no index, a row goes after the last track under that
  group, or under the nearest group over it with tracks; leaving for a
  group it was in, or the master, it goes after the last track of the
  outermost group it leaves. The CLI is `daw track move SONG T [INDEX]
  [--group G | --no-group]` and `daw group move SONG G [INDEX] [--group P |
  --no-group]`. `group.move` with an index among `groups[]`, its tracks
  staying, is gone: a group's place is its tracks'.
- **What a drop means** (`RowDrop`, in the app, pure and tested). The rows
  are listed as the arrangement draws them; the pointer's height picks the
  gap. The groups the row can be in run down the groups the row above is
  in, or the group whose header it is, as far as the groups the row below
  is in too; so a gap between two rows of a group, or right under a group's
  header over its rows, is in that group, and the gap after a group's last
  row could be in it or in any group over it. There the pointer's distance
  from the left edge chooses, one group in for every 16 points, as Ableton's
  and Logic's track lists do (the feature's open question, answered so). A
  folded group's header is joined at its end, set in. A group with no
  tracks, drawn under the tracks, is joined after the tracks of the nearest
  group over it with tracks.
- **The app's edits.** A drop sends `Edit::Place { row, index, group }`; a
  drop that leaves the row where it was sends nothing. `Edit::TrackGroup`
  and `Edit::GroupParent` send `track.move` and `group.move` with a group
  and no index; with none they take the row one group out.
  `Edit::TrackAdd` takes the new track's `group`. A refusal the app shows
  loses a clause that names a `daw` command (`SongModel.forPerson`): the
  host's words stay the agent's.

## Limits

- The order of groups with no tracks under them, drawn under the tracks, is
  `groups[]`'s; a drag does not change it.
- A track is not dropped onto a folded group's header to join it at its
  start; it joins at the end, set in under the header.
