# Editing a range of bars — proposed October 4, 2026

Status: proposed, not built. Backlog: Next, Editing a range of bars.

## What

Commands that copy, insert, delete or clear a range of beats across every track
at once, with the automation and the sections in it: "repeat the verse", "add
four bars before the drop", "take the drums out for bars 17 to 20".

```sh
daw range copy   PROJECT 32 32 --to 96             # bars 9–16 again at bar 25
daw range insert PROJECT 64 16                     # four empty bars at bar 17
daw range delete PROJECT 64 16                     # and gone again
daw range clear  PROJECT 64 16 --track drums       # a breakdown
daw section duplicate PROJECT verse --to 96 --id verse2
```

## Why

A song is made at the level of sections: a verse played twice, a bridge put in,
an intro cut short. Today each of these is a command per clip per track, and
the automation lanes are left behind. A three-minute song of eight tracks is
hundreds of commands, any of them a chance to leave something stacked or
misplaced. Sections are only labels: `daw section move` moves the label and
nothing under it.

A person does this in Ableton by selecting a range across all tracks and copying,
inserting or deleting time. The agent needs the same verb.

## Design

**Positions are beats**, zero-based quarter notes as everywhere else: START and
LENGTH. A bar is four beats. Positions in bars or `m:ss` are a separate Later
line, for every command.

**What moves with a range.** Note clips, pattern clips and audio clips that start
in it; automation points in it, on tracks, returns and the master; and sections
that lie inside it. `--track T` (repeatable) limits it to some tracks; their
lanes go with them and the master's stays. Sections move only when every track
is in the range.

**The edges.** A clip that crosses an edge is split there, so that the range
holds exactly what plays in it:

- A note clip becomes two clips. A note crossing the edge is cut at it on the
  left, and the rest starts at the edge on the right, so the range plays what it
  played before.
- An audio clip is split as `daw audio split` splits it, with no fade, so what
  stays in place plays bit for bit as before. A piece that is copied or moved
  away from its neighbor gets a short fade at its cut edge, since it now joins
  other audio there.
- A pattern clip is split between repeats when the edge falls there. Inside a
  repeat it is refused, naming the clip, since a pattern cannot be cut without
  making a new one.

A lane gains a point at each edge with the value it had there, so the curve
outside the range is unchanged.

**The verbs.**

- `copy START LENGTH --to AT` puts a copy of the range at AT. The copies own their
  notes, as a duplicated clip's do. What was at the destination is replaced, as
  pasting over a selection in Ableton replaces it. `--insert` pushes it later
  instead.
- `insert AT LENGTH` opens empty time: everything at or after AT moves later and
  the song grows by LENGTH.
- `delete START LENGTH` removes the range and closes the gap; the song shrinks.
- `clear START LENGTH` removes what is in the range and moves nothing.
- `section duplicate`, `section delete --with-content` and `section move
  --with-content` are the same with a section's range, and keep its label.
  `section move` alone still moves only the label.

Each command is one step with one undo, through the host when one runs, with a
label such as "Copy beats 32–64 to 96". The reply lists the clips made, split,
moved and removed, by path.

**Tails.** A copy starts clean: a note held into the range from before it is
cut at the edge as above, and the reverb and delay tails of what came before are
not part of the range. That is what copying time in a DAW does, and the map and
checks show the result.

## Done when

- On a generated song, `range copy` of eight bars across five tracks makes what
  eight bars of `clip duplicate` and lane edits would, and renders to the same
  samples.
- `insert` then `delete` of the same range gives back the original song, byte
  for byte in its canonical form.
- A note clip, an audio clip and a lane crossing an edge are split as described.
  A pattern clip split inside a repeat is refused with its path.
- Undo restores the song in one step. `daw describe edit` documents the verbs.

## Open questions

- Whether a note crossing an edge should instead stay whole in the left clip,
  with nothing new on the right. That is simpler, and changes what the range
  plays.
- Whether `copy` should replace or insert by default. Ableton's paste replaces.
- How long the fade at a moved audio piece's cut edge is, and whether `daw joins`
  should check it as a join.
- Locks the host enforces ("don't touch the drums", a Later line) would decide
  which tracks a range can change.
- Which gesture makes the time selection in the app: a drag in the clear of
  the timeline selects clips by a rectangle (D80), so perhaps a drag in the
  ruler under the loop strip.
