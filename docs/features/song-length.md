# A song with no end to hit — proposed October 19, 2026

Status: proposed, not built. Backlog: Next, A song with no end to hit. Asked
for by the person on October 19, 2026: "I don't like the hard boundary of how
long a track is ... I like how Ableton works where you can just add more clips
and it kind of hides the concept of a length ... I don't ever want to see a
message about a clip being outside the boundary of the song. I see a place
where the number of bars in song is in the UI but it is not editable, I would
prefer to not even see that and to have it all managed in the background."

## What

The song's length stops being something a person sets or sees. A clip placed,
dragged, lengthened or looped past the end extends the song to hold it; a clip
removed from the end lets it shrink back; the lighter bars inside the song and
the darker bars outside it go, and the bars field leaves the transport bar.
Nothing is refused or warned for lying past the end, because nothing can.

What still reads an end: the loop brace when it is set to the whole song,
Play from the start to where it stops, `daw render`, `daw export` and
`daw timeline`, and the agent's `daw map`. Each reads the end of the last sound.

## Why

Today `session.length_beats` is a field of the song: a blank project is 32
bars, the model refuses a pattern or note clip that would sound past it or
start at or after it, `daw check` warns `song-ends-inside` for an audio clip
that does, an audio clip dragged past the end grows the song and a pattern clip
cannot be dragged further, and the number sits in the transport bar where the
person cannot type it (Next, D81). The person met the refusal and the two
shades of bar and wants neither. Ableton's arrangement has no length: the song
ends where the last clip or automation does, and the export asks for a range.

## Design

- **The field stays in the file, computed.** `session.length_beats` remains
  the one place every reader gets the end from, and the model sets it rather
  than the person: after every edit it is the next whole bar after the last
  sound, where the last sound is the latest of every clip's end (an audio
  clip's with its tail, a looped clip's with its loop), every automation
  point and every section's end, with a floor of one bar so an empty song
  still has a ruler. A song saved with a longer length than its content keeps
  it until its next edit, so old projects open unchanged and then settle.
  A `--fit` on `daw timeline` becomes what always happens.
- **Nothing refused.** The model's checks that a clip must lie inside the
  length go, and with them `song-ends-inside`; `daw check` keeps
  `clips-overlap` and the rest. A clip that starts past the old end is a clip
  that extends the song.
- **The end fade.** `session.end_fade_ms` still fades the last milliseconds,
  now of the computed end, which is a bar after the last sound at most; the
  render's tail past the last clip is what the fade works on. If the fade is
  wanted longer than the tail, the last clip's own fade is the tool.
- **Sections** keep their own `at` and `length_beats`; a section past the
  content extends the song, since a person may lay out a form before filling
  it. `daw range insert` and `delete` move the end with the content as now.
- **The window.** The ruler draws bars as far as the view scrolls, one shade;
  the end of the song is a thin line at the computed end, where the loop
  brace snaps when set to all; scrolling and zooming are not bounded by it; a
  drop or a drag past it extends it when the edit lands, and the window
  follows (Later › The app, the window following the song's end, is answered
  by this). The transport bar loses the bars field; the position field keeps
  its bar.beat.sixteenth.
- **The agent.** `daw map`'s length line reads the computed end; `daw set
  session.length_beats` is refused with "the length is the song's content"
  and `daw describe project` says so. `daw render --section` and `daw export`
  are unchanged; a render is the whole computed length.
- **The beat map and audio clips.** An audio clip dragged past the end
  already extends the song; the same code path serves pattern and note clips.

## Done when

- A note clip placed at bar 40 of a 32-bar song extends it to bar 41 or the
  clip's end, whichever is later, in a Rust test; removing it shrinks the song
  back to its content; an empty song is one bar.
- No validation path mentions the length, and the `song-ends-inside` code is
  gone from `daw check`, `daw describe check` and
  [musical checks](musical-checks.md).
- An old project with `length_beats: 128` and content to bar 64 opens at 128
  and reads 64 after its first edit.
- A scripted run drags a pattern clip past the end and the picture shows one
  shade of bar, no bars field, the clip in place and the end line after it.
- `daw timeline --fit` is a no-op and says so; `daw set session.length_beats`
  is refused with the reason.

## Open questions

1. Whether the end should be the next whole bar after the last sound, or the
   last sound itself. A bar reads better on the ruler; the export's tail is
   the difference.
2. Whether the end fade should stay at all once the end is computed, or go
   with the length, leaving fades to clips and the master lane.
3. Whether a section past the content should extend the song, as proposed, or
   sections should be clipped to content as a render reads them.
