# Editing a range of bars — implemented October 6, 2026

Status: implemented through `daw` (D82). A time selection across the tracks in
the app is a line of its own in [the backlog](../backlog.md).

## What

Commands that copy, insert, delete or clear a range of beats across every
track at once, with the clips, audio, automation and sections in it, and a
section duplicated, moved or removed with what is under it: "repeat the
verse", "add four bars before the drop", "take the drums out for bars 17 to 20".

```sh
uv run daw range copy   SONG 32 32 --to 96             # bars 9–16 again at bar 25, over what is there
uv run daw range copy   SONG 32 32 --to 96 --insert    # the same, pushing what follows later
uv run daw range insert SONG 64 16                     # four empty bars at bar 17
uv run daw range delete SONG 64 16                     # and gone again
uv run daw range clear  SONG 64 16 --track drums       # a breakdown
uv run daw section duplicate SONG verse                # the verse again, right after itself
uv run daw section duplicate SONG verse --to 96 --id verse2
uv run daw section move   SONG bridge 128 --with-content
uv run daw section remove SONG intro --with-content
```

`daw describe edit` has the same under `ranges`, `edges` and `sections`.

## Why

A song is made at the level of sections: a verse played twice, a bridge put
in, an intro cut short. Before this each of these was a command per clip per
track, and the automation lanes were left behind. A three-minute song of eight
tracks was hundreds of commands, any of them a chance to leave something
stacked or misplaced. Sections were only labels: `daw section move` moved the
label and nothing under it.

A person does this in Ableton by selecting a range across all tracks and
copying, inserting or deleting time. The agent has the same verb now.

## The range

**Positions are beats**, zero-based quarter notes as everywhere else: START and
LENGTH, or AT. A bar is four beats. A range is `[START, START + LENGTH)`.

**What a range holds.** Pattern clips, note clips and audio clips that start
in it; automation points in it, on tracks, returns and the master; and
sections that lie inside it. `--track T`, repeatable, limits a command to some
tracks; their lanes go with them. The sections, the returns' and master's
lanes and the song's length change only when every track is in the range.

**The edges.** A clip that crosses an edge is cut there, so that the range
holds exactly what plays in it:

- A note clip becomes two clips, the right one under a new `clipN`. A note
  across the cut is held to it on the left, and the rest starts the right
  clip at beat 0, so the range plays what it played. A note before its clip's
  start stays with the left clip, not playing, as before.
- An audio clip is split as `daw audio split` splits it, with no fade at the
  cut, so halves that stay together play bit for bit as before. A half that
  is moved or copied away from the other fades 4 ms in or 12 ms out at the
  cut edge, a crossfade's fades without the lead, since it now joins other
  audio there.
- A pattern clip is split between repeats, `repeats` divided between the
  halves. An edge inside a repeat refuses the whole command, naming the clip:
  `Beat 6 falls inside a repeat of clip tracks.drums.clips.0, whose pattern
  beat is 4 beats long; a pattern clip is cut only between repeats`. Move the
  range to a repeat's edge, or split the pattern.

A copy reads the range without touching the source: the clips it is cut from
stay whole, so a note held across the range's edge is not retriggered in the
original.

**Lanes.** A lane gains a point at each edge with the value it had there, so
the curve outside the range is unchanged; where two values meet, two points
share the position and the lane jumps, as the engine allows. A cleared range
runs straight between its edges; inserted beats hold the value the lane had
at the insertion. Points that change nothing, such as those an edit added on a
straight stretch, are taken out again, and so are points at the edit's edges
that lie on the line between their neighbours. A lane that holds one value
throughout is left as it is.

**Sections** go with the whole song: inserted beats push the sections after
the insertion later and stretch one around it; deleted beats shorten a section
across them, remove one inside them and move those after earlier; a copied
range's sections are copied under a free ID (`verse-2`), or the one given.
`clear` leaves the sections alone.

## The verbs

- **`range copy START LENGTH --to AT [--insert] [--track T]…`** puts a copy of
  the range at AT. What was at the destination is replaced, as pasting over a
  selection in Ableton replaces it: clips there are cut at the destination's
  edges and the pieces inside removed, and the lane's points too. `--insert`
  opens time for it instead, as `insert` does. A note clip's copy owns its
  notes under IDs of its own. A copy that reaches past the song's end grows
  the song to the end of that bar.
- **`range insert AT LENGTH`** opens empty time: everything at or after AT
  moves later, and the song grows by LENGTH. With `--track`, the song grows
  only if a clip was pushed past its end.
- **`range delete START LENGTH`** removes the range and closes the gap; the
  song shrinks by LENGTH. The two clips of a track that meet where the gap
  closed become one again when they are one music: a pattern clip and its
  continuation, an audio clip and the rest of its file playing on, a note clip
  and the one that follows it, with a note of the same pitch and velocity that
  ends at the join continued by one that starts there. So `insert` then
  `delete` of the same range gives back the original song, byte for byte in
  its canonical form.
- **`range clear START LENGTH`** removes what is in the range and moves
  nothing.
- **`section duplicate SECTION [--to AT] [--id ID]`** is `range copy --insert`
  of the section's beats, right after the section or at AT, with the copy of
  its label named ID or after it. Ableton's Duplicate inserts, and so does
  this.
- **`section move SECTION AT --with-content`** clears the section's beats where
  they were and puts them at AT over what is there, and the label with them.
  **`section remove SECTION --with-content`** is `range delete` of its beats.
  Without `--with-content`, `move` and `remove` touch the label alone, as
  before.

Each command is one step with one undo, through the host when one runs, with
a label such as `Copy beats 32–64 to 96` or `Duplicate section verse at 96 as
verse2`. The reply lists what the command made as `path` or `paths`, the clips
it `split`, `moved` and `removed` by path, the `sections` a copy made, and the
song's `length_beats` after it.

**Tails.** A copy starts clean: a note held into the range from before it is
cut at the edge as above, and the reverb and delay tails of what came before
are not part of the range. That is what copying time in a DAW does, and the
map and checks show the result.

## Limits

- Positions are beats. Bars and `m:ss` are a Later line, for every command.
- An audio clip cannot be merged with another after a delete unless the
  removed beats held none of its audio, so a delete inside an audio clip
  leaves two clips with faded cut edges, as `daw audio cut` does.
- A note that ends exactly where a deleted range began and a note of the same
  pitch and velocity that started where it ended become one held note.
- The fades at a cut edge are 4 ms in and 12 ms out, and `daw joins` does not
  check such a join yet.
- What sections mean to a `clear` is nothing: labels stay where they are.

## Open questions

- Which gesture makes the time selection in the app: a drag in the clear of
  the timeline selects clips by a rectangle (D80), so perhaps a drag in the
  ruler under the loop strip. The app's part is a line in the backlog.
- Whether a note crossing an edge should instead stay whole in the left clip,
  with nothing new on the right. That is simpler, and changes what the range
  plays.
- Locks the host enforces ("don't touch the drums", a Later line) would decide
  which tracks a range can change.

## Code

`engine/crates/aaw-host/src/range.rs` holds the range edits as methods of
`Edit`; `command.rs` has the commands (`range.copy`, `range.insert`,
`range.delete`, `range.clear`, `section.duplicate`, and `with_content` on
`section.move` and `section.remove`); `aaw-cli` the verbs; `aaw-model`'s
`contract.rs` the `describe edit` texts. `crates/aaw-host/tests/range.rs` holds
insert then delete to the same bytes, a copy to the render of the clips and
lane edits it stands for, the cut edges, the refusals and the section verbs;
`tests/test_ranges.py` drives the verbs through `daw`.
