# Groups within groups — proposed October 19, 2026

Status: proposed, not built. Backlog: Next, Groups within groups. Asked for by
the person on October 19, 2026: "group tracks, then also have groups within
groups, with ability to solo and mute groups, and also apply an FX chain to a
group, control volume", and "selecting a set of groups and saving them into an
even higher level group, so nested groups". Mute, solo, a chain, sends and a
fader on a group are built ([groups that fold](groups.md), D92); nesting was
left out of that design on purpose and is what this adds.

## What

A group can be in a group. Drums (kick, snare, hats) and Percussion (shaker,
congas) are two groups; both selected and ⌘G pressed makes Rhythm, a group of
the two, named in the same sheet, with its own chain, fader, pan, mute, solo and
sends. Drums and Percussion are summed into Rhythm, Rhythm into the master.
Any depth is allowed; two or three is what a song uses.

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

## Why

A song of twenty tracks is mixed as four or five things, and some of those are
made of others: all the rhythm, which is drums and percussion; all the music,
which is keys, bass and guitars; the vocals, which are lead and backing. One
fader and one compressor over the rhythm needs a group of groups. Every DAW
the person knows has it.

## Design

- **The same kind of channel.** A group stays `groups[]`, with one more
  optional field, `group`. No new kind, no tree in the document: the engine's
  group stage runs the groups in dependency order, innermost first, which a
  topological sort over `group` gives; a cycle is refused by the model with the
  two names.
- **Order in the window.** A group's members, tracks and groups, sit next to
  each other in their lists, as a group's tracks do now, and the inner group's
  row sits above its tracks under its parent's row, set in a step further. The
  model's contiguity rule extends to groups: a group's child groups are
  contiguous in `groups[]` and their tracks contiguous in `tracks[]`, and
  `daw group add` moves them together as it moves tracks.
- **Latency.** A grouped track already runs ahead by its group's latency; a
  grouped group runs ahead by its parent's, so the sum of the chain latencies
  down the path is what the member is advanced by. The master waits for the
  slowest path, as it waits for the slowest group now.
- **Mute and solo.** A muted group silences everything under it and their
  sends. A soloed group is heard with everything under it; a soloed member is
  heard through its parents, alone. Same words as D92's, applied down the tree.
- **Stems.** A render writes a stem for each group as now, a parent's being
  the sum of its children through its chain; `report.stems_sum_to_mix` stays
  false when anything is grouped, and the report's group entry lists its
  `groups` beside its `tracks`.
- **Commands.** `daw group add SONG rhythm --groups drums,percussion` and
  `--tracks` together; `daw set SONG groups.G.group P` and `daw remove SONG
  groups.G.group`; `daw group remove` on a parent leaves its children where it
  was, on its own parent or the master. `daw inspect` and `daw map` show the
  tree as indented lines.
- **The app.** ⌘G on a selection of groups, or of groups and tracks mixed,
  makes a group of them; Ungroup on a parent leaves the children; Remove from
  Group on a child. The fold folds the whole subtree. A drag of a group's row
  out of its parent's run is refused as a track's is.

## Done when

- The YAML above validates, renders, and the rhythm stem equals the drums and
  percussion stems summed through rhythm's chain, in a Rust test.
- A cycle, a child outside its parent's run and a group that names itself are
  refused with the names.
- ⌘G over two selected groups makes the parent in a scripted run, and the
  picture shows the parent's row above them, each set in.
- Mute, solo and latency tests of D92 pass down two levels.
- [groups.md](groups.md) rewritten as the reference, "No nesting" gone.

## Open questions

1. How deep the window should draw: a third level set in again, or the same
   indent as the second.
2. Whether a group should be able to send to another group, which D92 refused
   and this does not change.
3. Whether `daw group add` of groups that are not contiguous should move them,
   as it moves tracks, or refuse, since moving a group moves its tracks too.
