---
name: song-edit
description: Edit a finished song from timecodes. Keeps the parts asked for, joins
  them on the beat so the joins cannot be heard, adds one-shots, checks the joins and
  the length, and exports a named file. Use when given a song and a request such as
  "edit from 0:41 to 1:43, take out the eight beats around 1:20, keep it under a
  minute". A personal skill can refer to this one for the mechanics.
---

# Song edit

A request names a song and the parts of it to keep, by time. The result is one
audio file in which the kept parts follow each other and the beat never slips.

This skill says what to do. How each step is done is in `daw describe edit`,
`daw describe beats`, `daw describe joins` and `daw describe export`; read them
before the step. Every command prints JSON.

## What a request means

- **A time** ("from 0:41") is approximate, often a second off. It means the
  strong beat nearest it, usually beat one of a bar.
- **A span to remove** ("take out the eight beats around 1:20") is removed in
  whole bars, from a downbeat to a downbeat, so the count carries across the join.
  Work out from the tempo how many bars the span holds.
- **A length limit** ("under a minute") is for the whole file, with every added
  sound in it.

If a time falls between two equally strong beats, or a cut would land in the
middle of a phrase, say which beat you chose and why, and offer the other.

## Steps

1. **Set up.** `daw init projects/NAME`, then import the song into it with
   `daw samples import PATH --project projects/NAME/song.yaml --id song`. An
   `.m4a` or `.mp3` is decoded there; use the path the import prints from then on.
2. **Find the beat.** `daw samples beats` on the imported file. Set
   `session.tempo` to its tempo. If the tempo is not `steady`, or the downbeat's
   confidence is low, or half or double the tempo would suit the request better,
   say so and settle it with the person before cutting; `--click` writes an
   audition they can check the grid with.
3. **Choose the cuts.** For each time in the request, `--near TIME` and take the
   nearest downbeat, preferring one that starts a phrase. Check that every part
   kept and every part removed is a whole number of bars. Tell the person the cuts,
   as times in the song and as bar numbers, before building.
4. **Build the edit.** One track, one pad for each kept part, placed end to end
   on session beats, with the fades `daw describe edit` gives. While the song is
   open in the app or a host is running, edit with commands so the person sees
   each change; otherwise one labelled `daw apply`.
5. **Add the other sounds.** Import each by path, on its own track. A sound that
   leads into a beat starts its own length before that beat; `daw timeline` gives
   the beat. Then fit the session's length to the sound with `daw timeline --fit`.
6. **Level.** The song stays as loud as the original. Put a limiter on the master
   so that what is added on top does not clip, and do not turn the song down.
7. **Render and check.** `daw render`, then `daw joins` on the render with
   `--limit` when the request gives a length. Fix every flag you can: a beat that
   slips, beats removed that are not whole bars, a first beat inside a fade, a
   step at a splice. If the file is too long, first look for another whole phrase
   that could come out, and ask. Otherwise raise the tempo just enough, with the
   parts set to keep their pitch; say how many percent, and above about 8 ask
   first.
8. **Export.** `daw export` to the project's `exports/` folder, under the name
   and in the format the person asked for.

## What to tell the person

- The cuts made, as times in the original song and as bar numbers.
- The final length.
- For each join, what was measured: how far the beat is from where it should be,
  and whether there is a step at the splice. Where its excerpt is, and what to
  listen for there.
- Anything uncertain: a downbeat that was a guess, a tempo that could be half or
  double, a join over a sustained vocal, a cut that falls in the middle of a phrase.
- Where the exported file is.

Do not say a join sounds good. Say what was measured and what they should hear
for themselves.

## Never

- Change the pitch of the song. A speed change keeps it (`stretch: preserve_pitch`).
- Leave a join that is off the beat, even by a little.
- Edit the original song file or anything in the sample library. Import, and work
  on the copy.
- Commit anything from `projects/` or `content/`.
