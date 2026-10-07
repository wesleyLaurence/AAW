# Groups that fold (implemented 2026-10-07)

Status: built in the pull request that closed the person's note of October 6,
2026 asking for tracks summed into a group with its own effects and level, a
drum bus, folded to one row on the timeline, decision D92. Before it, a track
went to the master and nowhere else: a drum kit of four tracks had four
faders and no compressor over the kit, and a song of twelve tracks was twelve
rows however it was looked at.

## What

A group is a bus between its tracks and the master, as Ableton's group track
is: the tracks in it are summed, the sum runs through the group's effects, its
gain and pan, and goes to the master sum and to the returns the group sends
to.

```yaml
tracks:
- id: kick
  group: drums
  ...
- id: snare
  group: drums
  ...
groups:
- id: drums
  gain_db: -2
  effects: [{type: compressor, threshold_db: -18, ratio: 3}]
  sends: [{to: room, gain_db: -12}]
```

| Field | What it is |
|---|---|
| `tracks[].group` | The group the track is summed into, or left out for the master |
| `groups[].id` | Unique across tracks, groups and returns |
| `gain_db`, `pan`, `mute`, `solo` | As a track's; lanes move `gain_db` and `pan` |
| `effects` | A chain run on the sum of the tracks, before the gain and pan; an equalizer in it draws its spectrum as any does |
| `sends` | To returns, post-fader or pre-fader, as a track's; `sends.R.gain_db` is a lane target |
| `automation` | Lanes `{param, points}` as a track's, less `instrument.FIELD` |

`daw group add SONG drums --tracks kick,snare,hats [--gain-db -2]` makes one
of the tracks named, which are moved together to where the first of them is;
`daw group remove|rename|move` do what the track verbs do, a removed group
leaving its tracks on the master and a renamed one carrying their `group`
along. `daw set SONG tracks.T.group G` puts one track in, `daw remove SONG
tracks.T.group` takes it out, and `daw send set`, `daw effect add` and `daw
lane set` take `groups.G` as they take a track. `daw inspect` lists the groups
with their tracks, and `daw describe project` and `daw describe effects` say
what a group is.

In the app a group is a row above its tracks with a track's header: name,
chain, mute, solo, volume, pan, the A mark, and its sends under the mark by
its name. Before the name a mark folds its tracks away under it and shows them
again, with Option for every group. Its lane draws its tracks' clips small, a
strip a track, folded or not. The tracks in a group sit in from the edge.
Group Tracks (⌘G) groups the selected track or the tracks of the selected
clips and asks for the group's name; Ungroup (⇧⌘G) takes the selected group,
or the selected track's, away; the header's menu has both, and Remove from
Group on a track in one. The group's devices are in the detail panel as a
track's are.

## Why

A drum kit of four tracks is mixed as one thing: one compressor over the kit,
one fader, one send to the room. Without a group each track carries its own,
and glue compression over the kit is impossible. A song of twelve tracks also
wants to fold to the few things it is made of.

## Design

- **A group is a channel kind of its own,** `groups[]` beside `returns[]`,
  rather than a track with tracks inside it: paths stay `tracks.T` and
  `groups.G`, every command that knows tracks still works, and the engine's
  routing gains one stage rather than a tree.
- **A group's tracks are next to each other** in `tracks[]`, so the app can
  draw the group's row above them and the song's order is the window's. The
  model refuses a `group` on a track that is not beside the others and names
  the move to make; `daw group add --tracks` moves them together itself.
- **Routing and latency.** A grouped track's post-fader output enters its
  group instead of the master sum. Its fader runs ahead of the common
  timeline by the group's latency, so that the group's chain adds its
  latency and the group's output lands on the timeline with every ungrouped
  track, which waits for the slowest group as it waits for the slowest
  track's inserts. A grouped track's own sends are delayed by its group's
  latency so that they land with the group's output. A group with no latency
  changes the mix by nothing but the order of two additions.
- **Mute and solo.** A muted group silences its tracks and their sends. A
  soloed group is heard with all its tracks; a soloed track inside a group is
  heard through its group, alone. A group's compressor may be keyed by a
  track, as a return's may; a sidechain cannot name a group.
- **Stems.** A render writes a stem for each track, each group and each
  return. A grouped track's stem is its sound before the group, since `daw
  listen` reads each track's stem, so the stems of the ungrouped tracks, the
  groups and the returns sum to the mix, and `report.stems_sum_to_mix` is
  true only when nothing is grouped. `render --track G` renders the group
  with its tracks, the group's stem; `render --track T` of a grouped track is
  its stem before the group. A group's report entry has `kind: group` and
  its `tracks`, and a grouped track's names its `group`.
- **The fold is the window's.** Which groups are folded is kept by the app,
  not the song, as the sends' fold is: folding is not an edit, and a fresh
  window opens with every group shown.
- **No nesting.** A group cannot be in a group, and a group does not send to
  another group. A track dragged in the window out of the run of its group's
  tracks is refused by the host, and the row goes back.

## Limits

- A sidechain cannot name a group: a drum bus cannot key the bass's
  compressor. The kick's track can.
- The fold is not saved with the project.
- `daw map` draws the tracks as before and does not show their groups.

## Open questions

- Whether the fold should be saved with the project.
- Whether a group's lane should draw the members' clips, as it does, or stay
  blank when the group is unfolded.
- Whether a drum bus should be able to key a compressor.
