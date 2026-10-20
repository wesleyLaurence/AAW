# Groups within groups (implemented 2026-10-20)

Status: built in the pull request that closed the person's note of October 19,
2026, "group tracks, then also have groups within groups, with ability to solo
and mute groups, and also apply an FX chain to a group, control volume", and
"selecting a set of groups and saving them into an even higher level group, so
nested groups" (D123), decision D129. Mute, solo, a chain, sends and a fader on
a group were built on October 7 ([groups that fold](groups.md), D92), which
left nesting out on purpose; this adds it.

## What

A group can be in a group. Drums (kick, snare, hats) and Percussion (shaker,
congas) are two groups; both selected and ⌘G pressed makes a group of the two,
named in the same sheet, with its own chain, fader, pan, mute, solo and sends.
Drums and Percussion are summed into it, and it into the master. Any depth is
allowed; two or three is what a song uses.

```yaml
groups:
- id: drums
  group: rhythm
  effects: [{type: compressor, threshold_db: -18, ratio: 3}]
- id: percussion
  group: rhythm
- id: rhythm
  gain_db: -1
  effects: [{type: eq, bands: [{shape: highpass, freq_hz: 30}]}]
```

`groups[].group` names the group a group is summed into, as `tracks[].group`
does for a track, or is left out for the master.

| Command | What it does |
|---|---|
| `daw group add SONG rhythm --groups drums,percussion [--tracks a,b] [--group P]` | A group of the groups and tracks named, in group P if given; their tracks move together to where the first of them is |
| `daw set SONG groups.G.group P`, `daw remove SONG groups.G.group` | Puts a group in another; takes it out, to the master |
| `daw group remove SONG P` | Removes P; the tracks and groups in it go to P's own group, or to the master |
| `daw group rename SONG P NEW` | The groups in P follow, as its tracks do |
| `daw inspect SONG` | Each group's `groups` beside its `tracks`, and its `group` |

In the app a group's row is set in under the group it is in, as a track's is
under its group, once more for each level, and the band in the group's shade
spans the indent. The mark before its name folds everything under it away,
tracks and groups. Shift-click on a track's or a group's header adds it to the
selected rows; ⌘G, or Group Tracks in a header's menu, groups the selected
rows, tracks and groups, when they are all in the same group or all in none,
and puts the new group there. Ungroup on a group leaves what was in it on the
group's own group, or on the master. Remove from Group in a group's menu takes
it out of its group. The group's lane draws the clips of every track under it.

## Why

A song of twenty tracks is mixed as four or five things, and some of those are
made of others: all the rhythm, which is drums and percussion; all the music,
which is keys, bass and guitars; the vocals, which are lead and backing. One
fader and one compressor over the rhythm needs a group of groups. Every DAW
the person knows has it.

## Design

- **The same kind of channel.** A group stays `groups[]`, with one more
  optional field, `group`. No new kind, no tree in the document: the engine's
  group stage runs the groups with every group after the groups in it, which
  `Project::group_order` gives, so a group's sum is made before its own
  group's. A group that names itself, an unknown group or a group under it is
  refused with the names: "kit: cannot be in group music, which is in kit".
- **Order in the window.** The tracks under a group, its own and those of the
  groups in it, are next to each other in `tracks[]`; the rule that held a
  group's tracks together now holds the whole subtree, and the window draws
  each group's row above the first track under it, set in by its depth.
  `groups[]` itself is in any order: the tree is read from `group`, not from
  the list. `daw group add --groups` moves every track under the groups named
  together with the tracks named, to where the first of them is, as it moved
  tracks (open question 3 of the proposal, answered by moving).
- **Latency.** A group's lead is its chain's latency over its own group's
  lead: the sum of the chain latencies down the path to the master. Its
  members run ahead by it, its own output lands at its group's input, its
  sends are delayed back to the timeline as a grouped track's are, and the
  master waits for the slowest path. A song whose groups have no latency is
  the same samples as the flat mix, to rounding.
- **Mute and solo.** A muted group silences everything under it and their
  sends; a mute over a solo wins. A soloed group is heard with everything
  under it; a soloed track or group is heard through its groups, alone.
- **Stems.** A render writes a stem for each group, a nested group's being its
  sound before its group, at its group's input; so the stems of the ungrouped
  tracks, the outermost groups and the returns sum to the mix, and
  `report.stems_sum_to_mix` is false when anything is grouped. A group's
  report entry lists `tracks`, every track under it, since that is whose
  sound the stem holds and what `daw listen` reads it for, `groups`, the
  groups in it, and its `group`. `render --track G` renders the group with
  everything under it.
- **The app's selection.** The window had one selected row; Shift-click on a
  track's or a group's header adds it to a set beside it, cleared by the next
  click, and Group Tracks reads both, or the tracks of the selected clips as
  before. The rows must share a group, or all be on the master, since the new
  group is made where they are; the item is otherwise disabled.
- **The fold** is the window's, as before, and hides the whole subtree.

## Limits

- `daw map` still draws the tracks without their groups.
- A group does not send to a group, as D92 decided.
- A group's row is not dragged into or out of another group: the menu's
  Remove from Group and ⌘G move it.
- Rows selected with Shift are read by Group Tracks alone: Delete, Mute and
  the rest act on the selected row.

## Open questions

- Whether `daw map` should show the tree.
- Whether a third level should be set in again, as it is, or stop at the
  second's indent for a narrow header.
