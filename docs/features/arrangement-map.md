# The arrangement map — proposed October 4, 2026

Status: proposed, not built. Backlog: Later, under What the agent reads and
writes. What it is for is in [concept.md](../concept.md#what-the-agent-works-with)
as Views.

## What

`daw map PROJECT` prints the song as a grid of tracks by bars in a few hundred
tokens: where each track plays, which clips are the same music, where two clips
on one track sound at once, and where the sections fall.

```text
bar       1   5   9   13  17  21  25  29
sections  intro---verse-----------drop----
drums     ....AAAAAAAAAAAAAAAA#AAABBBBBBBB
bass      ........CCCCCCCCCCCCCCCCCCCCCCCC
keys      DDDDDDDDDDDDDDDDDDDD....DDDDDDDD
lead      ........................EEEEFFFF
```

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

## Design

**The grid.** One row a track, in the song's order, and one cell a bar by default
(`--per beat`, `--per 2`). `--from` and `--to` in beats keep a range, and
`--track` keeps some tracks. The rows are a `text` field in the JSON result, so
the output stays JSON as every command's does, beside the structured `rows`
the text is made from.

**What a cell says.**

| Mark | Meaning |
|---|---|
| A letter | A clip plays here; the same letter is the same music |
| `#` | Two or more clips of the track sound in this cell |
| `:` | A clip is there, but nothing starts in this cell: a held note, a gap in the notes |
| `.` | Nothing |

The same music means the same content, not the same clip: a note clip's notes
relative to its start with their velocities, and its length; a pattern clip's
pattern and velocity scale; an audio clip's sample and the part of it played.
Two copies of a phrase share a letter, and a copy that was changed gets its own.
A legend gives each letter's clips, their length and their note count.

**Rows above the tracks.** The bar numbers, counting from 1, and the sections
with their IDs. A muted or soloed track is marked in its name. `--lanes` adds a
row a lane, with `~` where its value moves.

**A line a track under the grid:** its instrument or patch, its gain, its lowest
and highest note, and its notes per bar. This is what is needed to judge a part
without listing its notes.

**Size.** Eight tracks of 64 bars is about 700 characters of grid and a legend of
a line a letter. `--per` is the way to keep a long song small.

**Where it is built.** A Rust command beside `inspect`, read from the model, so
it runs with or without a host and answers for the song as it is now.

## Done when

- A generated song of eight tracks and 64 bars maps in under 2,000 characters.
- Three duplicates of one clip on the same beat show as `#`. Copies of one
  phrase share a letter, and a copy with one note changed does not.
- A held chord shows `:` after its first bar. A section, a muted track and a
  lane show in their rows.
- `daw describe` names the command and says what a mark means.

## Open questions

- Whether a pattern clip with `repeats` shows its repeats as one letter or a
  letter per repeat.
- Whether `inspect` should list clips in the order they play, not the order they
  were made. The map makes this matter less.
- A text piano roll of one clip, which is a separate Later line.
