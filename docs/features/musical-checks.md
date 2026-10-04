# Musical checks — proposed October 4, 2026

Status: proposed, not built. Backlog: Later, under What the agent reads and
writes. What it is for is in [concept.md](../concept.md#what-the-agent-works-with)
as Checks: a linter for music.

## What

More warnings from `daw check`, about the music and not only the file: clips on
one track sounding at once, notes that strike twice, a song that ends inside a
part, two parts crowding one register, a pad played far from its root. Also a
change to `clip duplicate` so that repeating it does not stack the copies.

## Why

An agent writes notes by the hundred, through scripts, and cannot hear the
result as it goes. The mistakes it makes most are mechanical and visible in the
song file. A check catches them before a render, every time, cheaply.

On October 4, 2026, an agent ran `clip duplicate` three times on one drum clip.
Each copy was placed "right after the original", so all three landed on the same
bar, and `daw check` returned `[]`. Today it warns only about lanes on bypassed
effects, pads stretched far enough to hear or with no tempo to stretch to, and
notes that cannot play (they start past their clip's end, or no pad is mapped to
them).

## Design

**Warnings become objects.** Each has a `code`, a `message` in words, the `path`
of what it is about, and `at` in beats when it has a place in time. Today's
string warnings become the `message` of their objects. A command that lists only
the codes, or reads only errors, can ignore the rest. The warnings are made in
Rust in the model crate (they are in `aaw-py` today), so that `daw check` and a
future host check share them.

**New checks.**

| Code | When |
|---|---|
| `clips-stacked` | Two clips on one track start on the same beat with the same music. This is almost always a mistake |
| `clips-overlap` | Two clips on one track sound at once. This is sometimes deliberate, and is reported as information |
| `note-retriggered` | A note starts while another of the same pitch on the same track is still sounding |
| `song-ends-inside` | A clip or a note runs past `session.length_beats` and is cut off |
| `clip-after-end` | A clip starts at or after the song's end and is never heard |
| `track-empty` | A track has an instrument and nothing to play, or notes and no instrument |
| `register-crowded` | Two tracks hold notes in the same octave below C3 in the same bars, the beats listed. Two low parts in one register are the most common cause of a muddy mix |
| `pad-far-from-root` | A pitched pad is played more than two octaves from its root, where repitching sounds artificial |
| `note-below-hearing` | A Synth note whose fundamental is under 20 Hz |

Checks read only the song. A check that needs audio, such as two tracks masking
one another, belongs to perception, a separate Later line.

**`clip duplicate` without `--at` chains.** The copy goes right after the
original, as now. If a clip already starts there, the copy goes after the run of
clips that follow one another from there, each starting where the last ends. So
duplicating one clip three times lays out four in a row, as pressing ⌘D three
times in Ableton does. `--times N` makes N copies in a row as one step. The reply
gives each copy's beat, as now.

**Ignoring a warning.** A deliberate overlap can be marked with `check_ignore`,
a list of codes on the clip or the track, so that a song does not keep reporting
it. This is an open question below.

## Done when

- Three duplicates of one clip lay out four in a row. A generated song with two
  clips stacked on purpose reports `clips-stacked` with both paths and the beat.
- Each code has a generated song that raises it and one that does not, in Rust
  tests, and `daw check` prints the objects.
- `daw describe` lists the codes and says what each means.
- The existing warnings are unchanged in substance and carry codes.

## Open questions

- Whether `register-crowded` should know about an instrument's own range: a
  bass patch an octave above its notes, or a pad that has been filtered thin.
  Notes alone cannot tell.
- Whether `check_ignore` belongs in the song, or the agent should simply read
  `clips-overlap` as information and move on.
- Whether `daw render` should run the checks and print their codes on stderr.
- Checks that need to know a section's feel, such as a snare off the backbeat in
  a half-time section, which the backlog's line named. The song does not record
  a feel today.
