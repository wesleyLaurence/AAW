# A song with no end to hit — implemented 2026-10-20, play past the end 2026-10-25

Status: implemented (D130, D163). Asked for by the person on October 19, 2026: "I
don't like the hard boundary of how long a track is ... I like how Ableton
works where you can just add more clips and it kind of hides the concept of a
length ... I don't ever want to see a message about a clip being outside the
boundary of the song. I see a place where the number of bars in song is in the
UI but it is not editable, I would prefer to not even see that and to have it
all managed in the background."

## What

The song's length is its content. Nobody sets it: after every edit the host
sets `session.length_beats` to the next whole bar after the last sound, one
bar at the least, so a clip placed, dragged, copied, lengthened or looped past
the end moves the end to hold it, and a clip taken away from the end lets the
song shrink back. Nothing is refused or warned for lying past the end, because
nothing can lie there.

What still reads an end: the loop brace when it is set to the whole song,
`daw render`, `daw export`, `daw timeline`, the agent's `daw map` and a `daw
play` that hosts the song for its own duration. Each reads the computed end.
Play in the window does not stop there: asked for by the person on October
25, 2026, after hearing a blank song's four clicks and the stop, it rolls on
through empty bars, the metronome with it, until stopped (D163).

## How it works

- **The field stays in the file, computed.** `session.length_beats` remains
  the one place every reader gets the end from. `aaw_model::content_end` is
  the latest of every clip's end (a pattern clip's last repeat, a note clip's
  length, an audio clip's sound with its fade-out tail, a looped clip's
  length), every automation point, every section's end and every marker;
  `fitted_length` rounds it up to a whole bar of the time signature, one bar
  at the least, so an empty song still has a ruler. The host settles it in
  `Session::edit` after validation (`Session::settle`), reading the length of
  a sample's file from the project's folder for an audio clip that plays to
  its file's end; a clip whose file cannot be read counts from its start. A
  merge patch (`daw apply`) is settled the same way.
- **An old project.** A song saved with a longer length than its content
  opens unchanged, and settles at its next edit: a project with
  `length_beats: 128` and content to bar 64 reads 128 until something is
  edited, then 64.
- **Nothing refused.** The model's rules that a clip, a section, a marker or
  an automation point must lie inside the length are gone, and so is the
  `song-ends-inside` check; `daw check` keeps `clips-overlap` and the rest,
  and `release-cut`, which is a Synth's release past the computed end, not a
  clip. A range command past the end (`daw range clear 32 4` on a 32-beat
  song) changes nothing rather than refusing.
- **Play past the end.** The player rolls on past the end when the host
  asks it to (`Control::set_roll_on`), which every host does but the one a
  `daw play` with no host running opens for its own duration: that one
  stops at the end and exits, as before. Past the end the transport moves
  on over nothing (`Renderer::pass`): the playhead and the position go on,
  the metronome clicks every beat and accents every bar, and what still
  rings at the end, a reverb's or a Synth's tail, is heard whole, as after
  a stop. An edit that moves the end past the playhead, a clip dropped
  ahead of it, is heard when the playhead gets there. A take records on
  past the end until stopped, and its clip moves the end to hold it. Play
  and locate take a start position anywhere from the song's start on (`daw
  play --from` with no host still refuses one at or past the end, where it
  would have nothing to play); the loop stays inside the song. A marker
  dropped while play is past the end lands where it is heard and moves the
  end to hold it.
- **A new song.** A song with nothing in it is eight bars long
  (`aaw_model::EMPTY_BARS`), a phrase to start from, so the loop set to all
  and a render of it have bars to count; once anything is placed, the end is
  the next whole bar after it, one bar at the least, so a short sketch
  renders without silent bars. `daw init` and a new Untitled project write
  the eight bars.
- **The end fade.** `session.end_fade_ms` fades the last milliseconds of the
  computed end, which is at most a bar after the last sound. A longer fade is
  the last clip's own.
- **Sections and markers** extend the song, since a form is laid out before it
  is filled and a marker is left where something was heard; a marker past
  the end holds the end there until it is moved or removed.
- **The agent.** `daw set session.length_beats` is refused: "The length is the
  song's content: the next whole bar after the last clip, automation point,
  section or marker, set after every edit. Place something where the song
  should reach". `daw describe project` says so under `length`; `daw timeline
  --fit` (and `--tail`) is kept for old scripts and does nothing, which its
  reply says under `fitted`. `daw init` has no `--bars`: a new song is
  eight bars long until something is put in it. `daw map`'s grid runs to the computed end. `daw midi import` and
  a range copy still say the song grows, which it does, as after any edit.
- **The window.** The ruler draws bars as far as the view scrolls, one shade,
  and the view scrolls a screen past the end (two bars at the least), so that
  a clip can be dropped or dragged there; the end is a thin line, where the
  loop brace snaps when set to all; zooming to fit shows the song and one bar
  more. A drop, a drag, ⌘D, a nudge, a note clip's end dragged, a pattern
  clip's repeats and an automation point may all go past the end, which
  moves when the edit lands. The transport bar has no bars field; the
  position field keeps its bar.beat.sixteenth, and the tempo and meter light
  for a change of their own, not for the end moving. A click on the ruler
  places the start position wherever it lands, past the end too, where play
  rolls on through empty bars; the loop brace stays inside the song. While
  play rolls on past the end the view scrolls a screen past the playhead
  (`TimelineLayout.playedBeats`), so the playhead stays in view. The app no longer puts
  a `set session.length_beats` before an audio clip's edit: the host settles
  every edit.

## Done

- A note clip resized past the end moves the end to the bar after it; a
  marker at beat 33 of a 32-beat song makes it 36 and moving the marker back
  makes it 32 again; a section moved past the content and back does the same;
  a pattern clip moved or copied past the end, a point added past it and a
  marker at beat 400 move the end in the app's bridge, and undo brings it
  back: Rust tests in `aaw-model`, `aaw-host` and `aaw-ffi`.
- No validation rule reads the length; `song-ends-inside` is gone from the
  code, `daw describe check` and [musical checks](musical-checks.md).
- `daw timeline --fit` says it does nothing and leaves the file as it was;
  `daw set session.length_beats` is refused with the reason: a Python test.
- The app's layout tests hold the step past the end, the scroll a screen past
  it, repeats and points past it, and a blank project one bar long.
- A scripted run on a scratch song of two note clips, the last ending at bar
  8: zoomed out, the clip dragged four bars past the end landed at bar 11 as
  one step by the user, the song read 44 beats, and the picture showed one
  shade of bar across the view, no bars field, the clip where it was dropped
  and the thin end line at bar 12. The tempo and meter lit up after the drag,
  since the host's length change touched the session; the transport bar now
  lights them only when the tempo, title or time signature changed.
- Play past the end (D163), in Rust tests of the player: a song played with
  roll-on equals its render to the end, is silent after it and goes on
  playing, its position going on, until a stop, and a play from past the end
  rolls from there; the metronome clicks every beat after the end with bar 9
  accented, and without roll-on stops with the song; a clip moved past the
  end while the playhead is past it is heard as its render when reached. In
  the host: a locate past the end is taken and one before the start refused,
  and a blank song is eight bars. The app's layout tests hold a click past
  the end and the view scrolling with the playhead past it.
- Nothing was tried by hand; see Verify.

## Open questions, answered alone (D130)

1. The end is the next whole bar after the last sound, not the sound itself:
   a bar reads on the ruler, and the export's tail is at most a bar.
2. The end fade stays: one field that does what it did.
3. A section past the content extends the song.
