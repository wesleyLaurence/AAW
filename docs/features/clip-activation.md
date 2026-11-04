# Deactivated clips — implemented November 3, 2026

Asked for by the person on November 3, 2026 (D172): to silence one clip
without muting its whole track or deleting it, select it and press 0, and
press 0 again to hear it, as Ableton's Activate/Deactivate Clip(s) does.

## What

- **`active: false`** on a clip of any kind, a pattern clip in
  `tracks[].clips`, a note clip in a MIDI track's `clips` or an audio clip
  in `tracks[].audio`, deactivates it: it stays where it is, with
  everything it holds, and plays nothing. It is left out of the saved file
  while true, so a song with no deactivated clip is written as before.
- **0, or Edit › Deactivate Clip(s)**, in the app deactivates the selected
  clips, or activates them when every one of them is deactivated already,
  as one undo step. The item's title says which, Activate or Deactivate,
  Clip or Clips. It is in the clip's right-click menu too. A deactivated
  clip is drawn gray, darker than the clips of a muted track, and in a
  folded group's lane too.
- **`daw clip deactivate SONG CLIP...`** and **`daw clip activate SONG
  CLIP...`**: pattern clips and note clips by their paths or references,
  audio clips as `tracks.T.audio.N`, several as one step. The labels are
  `Deactivate clip riff at 0`, `Deactivate 2 clips`, and Activate the same.
- **`clip.active`** is the host command, `{"op": "clip.active", "clip":
  PATH, "active": false}`; true removes the field.

## How it works

- **Nothing of it is scheduled.** The note schedule, the pattern triggers
  and the audio voices skip a deactivated clip in `aaw-model/src/schedule.rs`
  and `played_audio` in `aaw-engine/src/program.rs`, so the player, a
  render, its stems, a freeze and a MIDI effect's input all leave it out
  alike. An audio clip that ends where a deactivated one starts fades from
  its own end, as if nothing followed it.
- **It keeps its place.** The song's length still counts it, so
  deactivating the last clip does not shorten the song under it; it moves,
  copies, splits and duplicates as any clip, and the copies are deactivated
  too.
- **Join** refuses a deactivated clip with one that plays: `bass at 8 is
  deactivated and bass at 0 is not; activate or deactivate both to join
  them`. Two deactivated clips join into a deactivated clip.
- **The map** gives a deactivated clip a letter of its own, `deactivated:`
  before its music in the legend, and shows it only where no clip that
  plays is; it counts toward no track's notes or hits a bar, makes no `#`,
  and is not among what plays at a marker. **`daw check`** leaves it out of
  the overlapping, stacked and retriggered warnings and of the pads' range;
  `inspect` lists it with `active: false`. Python's `timeline.regions` and
  the listening report's placements skip it.
- **In the app**, the FFI's `ClipView`, `NoteClipView` and `AudioClipView`
  carry `active`; `Edit::ClipsActive {clips, active}` sends one
  `clip.active` a clip. `SongModel.clipsActivation` is what 0 would do,
  nil with no clip selected; the menu item is enabled while clips are
  selected and the keys are the arrangement's, not the piano roll's, the
  pattern editor's or a text field's. The arrangement's `keyDown` takes 0
  too, for when the menu does not act.
- **The fingerprint** of a song from before the field verifies through a
  first form in `legacy_fields()` without `active: true`.

## Checked

- `aaw-host/tests/active.rs`: a pattern clip's hits and a note clip's notes
  gone and back, the labels, the saved file with and without the field, the
  song's length kept, undo, a path that is not a clip refused; an audio
  clip by its path; the map's own letter under a clip that plays, no
  overlap warnings, nothing deactivated sounding at a marker; join refused
  and two deactivated clips joined.
- `aaw-engine/tests/audio.rs`: no voice for a deactivated audio clip, the
  clip before it fading from its own end, a deactivated looped clip
  rendering silence.
- `aaw-ffi/tests/song.rs`: a pattern clip and a note clip deactivated in
  one step and undone in one, one clip deactivated and activated, the
  labels.
- `tests/test_midi.py`: `daw clip deactivate` and `activate` on a note clip
  and an audio clip, a render whose stem is silent and then sounds, the map,
  `describe project`, Python's timeline.
- `ContextMenuTests`: the clip menu's item, its titles and its key.
- Scripted runs of the built app on a song of three note clips: clip2
  clicked and 0 pressed, drawn gray and written `active: false`; clicked
  again and 0, active, then clip1 Shift-clicked and 0, the mixed selection
  both deactivated; both selected and 0, both active, then clip3 clicked, 0
  and ⌘Z, all three active and no `active` left in the file. Nothing
  pressed by hand, nothing heard from the app.

## Limits

- Clips only: Ableton's 0 also deactivates notes in its piano roll, and a
  note here has no such field; in the piano roll and the pattern editor 0
  does nothing.
- A frozen track's clips are not deactivated until it is unfrozen, as any
  edit of its clips is refused.
