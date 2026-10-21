# Freezing a track — implemented October 21, 2026

Asked for by the person on October 19, 2026 (D123), as the first note still
open after recording: "a track's instrument and effects rendered to a file
and played as an audio clip in its place, its devices kept and bypassed until
it is unfrozen; `daw track freeze|unfreeze`". Built as D138; there was no
design before it.

## What

A track is frozen: what its instrument, its effects and the lanes on them
make, over the whole song, is rendered once to a file that plays in their
place, before the fader, until the track is unfrozen. A song with a heavy
Synth or a long chain plays and exports with the file instead, and the
track's content stays in the song to come back to. `daw track freeze TRACK`
and `daw track unfreeze TRACK` from a terminal; Freeze Track in a track's
header menu and in the Track menu (⇧⌘F) in the app, which reads Unfreeze
Track while the track is frozen.

## How it works

- **The field is the song's.** `tracks[].frozen` is `{path: frozen/TRACK.wav}`,
  the rendered file relative to the project, or absent while the track plays
  its own content. It is on an audio track and a MIDI track alike. Everything
  the file was rendered from stays where it was: the pads, clips, audio clips,
  instrument, note clips, effects and lanes are in the song as before, and
  come back when the track is unfrozen.
- **The file.** `track.freeze` compiles the song for the track's stem with the
  track's own `frozen` set aside, so that a frozen track renders again from
  its content, and renders it with the tracks that key its sidechains playing
  along. The renderer is asked for each track's block as its inserts leave it,
  aligned to the timeline but before its gain, pan, mute and solo
  (`Renderer::tap_before_faders`), and the blocks are laid on the timeline by
  the frame each starts at, so the file's frame N is the song's frame N
  whatever the chain's latency. It is stereo 32-bit float at the song's rate,
  as long as the song, written atomically as `frozen/TRACK.wav`. The freeze
  does not depend on the block size, and a freeze of a frozen track writes the
  same bytes.
- **Playback and export.** A frozen track compiles as one voice, its file from
  frame 0 at full level, with no synth, no inserts and no lanes on them; its
  gain, pan, mute, solo, sends and the lanes on those are live, as are the
  returns it sends to, the group it is in and anything keyed from it, which
  taps the file. A chain with latency no longer delays the song. The track's
  waveform in the app is the file's. A render's identity counts each frozen
  file's digest, so a song frozen again renders again; a song with nothing
  frozen keeps the identity it had.
- **What is held.** While a track is frozen, the host refuses an edit of what
  its file was rendered from, in the words "T is frozen: daw track unfreeze T
  before changing its clips, pads, instrument, effects or their automation":
  its pads, clips, audio clips, instrument and note clips, its effects, the
  lanes on them, the patterns its clips play and the samples its pads and
  audio clips name. A lane on the fader, the pan or a send is not held, nor is
  the input. While any track is frozen, the tempo, the sample rate and the
  stretcher are held too. The check compares what the file was rendered from
  before and after every edit, so every command and merge patch meets it; a
  renamed track keeps its file and is matched by it. In the app the refusal
  shows as a banner, as any refused edit does. Recording into a frozen track
  and playing a note through its instrument are refused in the same words.
- **The edits.** Freeze and unfreeze are edits like any other: in the change
  log as `track.freeze` and `track.unfreeze`, labelled "Freeze T" and
  "Unfreeze T", undone and redone with the rest. Unfreezing removes the field
  and leaves the file, so that an undo finds it; the next freeze writes over it.
- **In the app.** A frozen track's header reads "frozen" before its devices,
  its clips are drawn iced, and the device panel says the devices are rendered
  to a file. The header's menu and the Track menu offer Freeze Track, or
  Unfreeze Track while the track is frozen.

## Tested

- `aaw-model`: the field validated, dumped and in the schema `daw describe`
  prints; the fingerprints of songs from before the field still verify.
- `aaw-engine/tests/freeze.rs`: a track with an equalizer, a keyed
  compressor, an automated filter, a limiter with latency, a gain lane and a
  send, frozen: the song with the file in its place renders the same mix
  within 1e-5, with less latency and no inserts on the track; the freeze is
  the same at two block sizes; a frozen track freezes again from its content.
- `aaw-host/tests/freeze.rs`: the file under `frozen/`, the field saved with
  the song, the content kept, the refusals of a clip, an effect, a pad, the
  pattern, the sample, a lane on an effect and the tempo in the words above;
  the fader, pan, mute, a send, the input, other tracks and a rename allowed,
  the renamed track still held; ten undos back to before the freeze; a frozen
  track frozen again writes the same bytes; a frozen song renders.
- `aaw-ffi/tests`: the freeze from the header, `frozen` in the track view,
  a clip drag refused in the host's words, the fader not, the unfreeze.
- Swift tests: Freeze Track in a track's menu and Unfreeze Track on a frozen
  one, with the Track menu's key beside it; none on a group.
- Scripted run of the app: see the Verify line in the backlog for what was
  and was not seen.

## Limits

- What keys a frozen track's sidechains from other tracks is as it was at the
  freeze: a change to the keying track does not change the file. Freeze the
  track again.
- A frozen track's effects' tails past the song's end are cut with the song,
  as a render's are.
- A `note.preview` through a frozen MIDI track's instrument is refused rather
  than heard through the file.
- The file is kept on unfreezing and on removing the track; nothing deletes
  it.
