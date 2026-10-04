# The arrangement map — implemented 2026-10-04

Status: implemented (D77). What it is for is in [concept.md](../concept.md#what-the-agent-works-with)
as Views.

## What

`daw map PROJECT` prints the song as a grid of tracks by bars in a few hundred
tokens: where each track plays, which clips are the same music, where two clips
on one track sound at once, and where the sections fall.

```text
bar           1   5   9   13  17  21  25  29
sections      int-verse-------drop------------
drums         ....AAAAAAAAAAAA#..BAAAAAAAAAAAA
bass          ........CCCCCCCCDDDD............
keys (muted)  E:::E:::........................
  gain_db     ~~~~------------------------~---

A  pattern beat, 4 beats, 8 hits; drums bars 5 ×12, 17, 17, 21 ×12
B  pattern fill, 4 beats, 16 hits; drums bar 20
C  notes b1, 16 beats, 4 notes C2–G2; bass bars 9, 13
D  notes b3, 16 beats, 4 notes C2–G2; bass bar 17
E  notes pad, 32 beats, 4 notes A#3–G4; keys bar 1

drums: pads h; 0 dB; 8.6 hits/bar
bass: no instrument; 0 dB; C2–G2; 1 notes/bar
keys: no instrument; 0 dB; A#3–G4; 0.5 notes/bar
```

Here two copies of the drum pattern start on bar 17, the bass's third phrase
has one note changed, and the keys hold a chord through bars 2 to 4.

## Why

On October 4, 2026, an agent asked to try making a song found that
`clip duplicate`, run three times on one drum clip, put all three copies on the
same bar. The render was 7 dB louder there, and the drums stopped halfway through.
`daw check` reported nothing, and `daw inspect` listed the clips in the order they
were made, so nothing showed that three of them started on the same beat. The
energy chart from `daw listen` was the first sign.

An agent reads a song as a list of objects. Whether the arrangement makes sense
as a whole is a question of where things fall in time, and a list does not show
it. A person sees the arrangement view at a glance; this is the agent's equivalent.

## How it works

**The grid.** One row a track, in the song's order, and one cell a bar.
`--per beat` makes a cell a beat and `--per 2` two bars; `--per` takes any
positive number of bars, `1/2` among them. `--from` and `--to` keep a range of
beats, the last cell perhaps shorter, and `--track drums,bass` (or the flag
repeated) keeps some tracks. Without `--to` the grid runs to the song's end, or
to the end of an audio clip that plays past it, which a line under the grid
then says: the song ends where that bar begins, and nothing after it is heard.
Only an audio clip can run past the end; validation keeps pattern and note
clips inside the song.

**What a cell says.**

| Mark | Meaning |
|---|---|
| A letter | A clip plays here and something starts in the cell; the same letter is the same music |
| `#` | Two or more clips of the track sound at once in this cell |
| `:` | A clip is there, but nothing starts in this cell: a held note, a gap in the notes |
| `.` | Nothing |

The same music means the same content, not the same clip:

- a note clip's notes that play, each by its beat from the clip's start, its
  pitch, its length and its velocity, and the clip's length;
- a pattern clip's pattern, its hits as the pattern expands them, and its
  velocity scale. A clip's repeats are the same music again, so they share its
  letter, and copies of it with other repeats share it too;
- an audio clip's sample, the part of it played, its tempo and how it stretches.

Two copies of a phrase share a letter, and a copy with one note or one velocity
changed gets its own. Letters are given in reading order, down the tracks and
along each one, from `A` to `Z`, `a` to `z` and `0` to `9`; music past the
sixty-second is `*`. The same music on two tracks has one letter, so a doubled
part shows.

An audio clip sounds throughout, so every cell it covers has its letter. Clips
that only meet in a cell, one ending where the next starts, are not stacked; the
cell has the letter of the one that covers more of it among those that start
something there.

**Rows above the tracks.** The bar numbers at every fourth cell, counting from
1, as a DAW does: a cell that does not start on a bar is labeled with its beat
of the bar, `16.3`. The sections, when the song has any: each one's ID where it
starts, cut to leave a dash before the next, and dashes to its end. A muted
track is marked `(muted)` in its name and a soloed one `(solo)`.

**Lanes.** `--lanes` adds a row under its track for each of a track's lanes,
indented, and, when no track is picked, a row for each lane of a return and
of the master: `~` where
the value moves in the cell, a ramp between two values or a held value's jump,
and `-` where it stays.

**The legend.** A line a letter: what the music is, its length, its hits or its
notes with their lowest and highest note, and where it is, by track and bar,
`17` or `5 ×12` for a pattern clip that repeats. Tracks that have it in the same
places are named together. Six places are listed and the rest counted.

**A line a track:** what plays it (the pads, a synth and its patch's name, a
sampler and its pads, or audio), its gain, its lowest and highest note, and its
hits or notes per bar, counted over the bars it plays in the range shown. This
is what is needed to judge a part without listing its notes.

**The result.** JSON, as every command's is. `map` is the text as a list of
lines, so printed JSON keeps the grid's columns without `jq`; `clips` gives each
letter's clips, by the references commands take, in the order they play, space
separated; `from`, `to` and `per_beats` are the range and the cell in beats.
While a host runs the references are handles, `@12`.

**Size.** A generated song of eight tracks, 64 bars and twelve kinds of clip
maps in about 1,900 characters; the grid itself is about 700. `--per` keeps a
long song small, and `--track` and `--from` keep part of it.

**Where it is.** `aaw-host`'s `map` module, a read like `inspect` that runs
through a host when one is running and headless otherwise, and answers for the
song as it is now. It reads an audio file's header for the length of a clip that
plays to the end of its file. `daw describe project` says what the marks mean.

## Limits

- 4/4 and one tempo are assumed, as everywhere.
- The legend names a note clip by the first clip with its music, so copies
  with other IDs are found through `clips`.
- A pattern clip's hits count every pad, so a track with two stacked copies
  counts both in its hits per bar.
- Clips are drawn by where they are, not by how long their sound rings: a pad
  that rings past its pattern does not fill the next cell.
