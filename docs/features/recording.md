# Recording audio — implemented October 21, 2026

Asked for by the person on October 19, 2026: "I need the ability to record
instruments, for example through the built in microphone, or a Scarlett
interface", and "support for resampling which is basically recording audio
from an internal source like another track, group or all sounds". The
concept had said "No recording of new audio"; the person's request revised
it (D123). Built as D136; the proposal of October 19 is what this replaces.
A sound recorded into the conversation, the third way in that proposal,
is the next item and builds on this.

## What

An audio track has an `input`: an input device's channel or pair, the
microphone or an interface, or the song's own sound, another track, a group
or the mix, which is resampling. The dot before a track's M arms it, Record
in the transport bar counts a bar in with the metronome and the song plays
while every armed track takes what its input gives, and the take ends with
Record again, Stop or the end of the loop: a 32-bit float WAV under the
project's `samples/`, named for the track and the take, added to the song
with an audio clip of it on the track from the start position, as one edit
by the person that one undo takes back. From a terminal, `daw record arm`,
`daw record start` and `daw record stop` do the same, and `daw record
capture` records an input to a file with no song.

## How it works

- **The input is the song's.** `tracks[].input` is `{device: NAME, channels:
  [1]}`, NAME part of an input device's name, ignoring case, or `default` for
  the input chosen in Settings › Audio, with one channel for a mono take or
  two for a stereo one, counted from 1; `{track: ID}`, `{group: ID}` or
  `{master: true}` for the song's own sound; `{}` is the chosen device's
  first channel. A MIDI track has none. The model refuses a track that names
  itself, an unknown track or group, and more than one source. `daw set
  tracks.T.input` and `daw remove tracks.T.input` change it; in the app the
  header's menu has an Input submenu of the Mac's input devices, mono and
  stereo, the song's other tracks, its groups and the master, and a click on
  the dot of a track with no input opens the same menu.
- **The arm is the host's.** `arm` and `daw record arm` arm a track that has
  an input, refused without one; the arm is state like the loop, not saved,
  reported in `daw status` as `armed` and in the transport the app draws, so
  the dot is red while a track is armed.
- **Record.** `record.start` arms the tracks named, if any, and takes one of
  every armed track. A count-in of `count_in` bars (1 unless given, 0 for
  none; Transport › Count-In in the app, kept across launches) is the player's
  pre-roll: the song stands at the start position while the metronome clicks
  the bars before it, whatever the metronome toggle, then the song rolls as
  a plain play does. The tap and the strips having settled during the
  count-in, the first frames of a take play in full where a play from a stop
  fades them in over 5 ms. A locate, a play from elsewhere, a change of
  devices and Stop end the take where it stands; so does the song's end, the
  loop wrapping (the take ends at the loop's end), the input device stopping,
  and the host closing. `record.stop` ends it with the song playing on.
- **An input device** is opened by its own stream (`capture::open_input`), at
  the song's rate when it offers it and else at its own, with the channels
  wanted picked out of its frames; `default` means the device chosen in the
  audio settings, else `AAW_INPUT_DEVICE` for a scripted run, else the Mac's
  default input. Each block goes to the host through a lock-free ring headed
  by the instant it was captured. The output callback publishes an anchor,
  the instant its block plays and the timeline frame that plays then, a
  count-in's frames counted negative, and the host maps the take's first
  frame onto the timeline through it, so the mapping does not depend on when
  the host ran. Nothing on either audio thread allocates. A take whose device
  ran at another rate is resampled to the song's by `resample_poly`.
- **Latency.** A take is pulled earlier by the input device's reported
  latency and the output device's, the device latency, its safety offset and
  its stream's, as CoreAudio reports them for the devices named, in the
  song's frames (`devices::latency_frames`); the driver's own lead is already
  in the anchor. Through BlackHole, which reports none, a burst played at
  beat 2 comes back at beat 2 of the take within a millisecond, with a
  count-in and without. The reply says `pulled_ms`.
- **Resampling** is a tap in the player (`player::Message::Tap`): the track's
  or the group's block after its fader, scaled by the master's gain and end
  fade as the render's stem is, or the mix after the master's effects before
  the metronome, each block headed by its exact timeline frame, taken from
  the same stage the stems come from, so a take equals the stem for that
  span frame for frame. The source is not muted while it is recorded; a
  source that is muted or silenced by a solo is refused before the output
  opens. A take follows the song's edits as they land, since the tap finds
  its stem by name each block. Up to eight tracks record the song's own
  sound at once.
- **The end.** Each take is cut to the span from the start position to where
  it ended, written as `samples/TRACK-take-N.wav`, N the first number no
  sample of the song and no file has, hashed, and added with `set
  samples.ID` and `audio.add` in one batch labelled `Record ID on TRACK (S
  s)`, by the person, so one undo removes the clip and the sample; the file
  stays, and the next take is numbered past it. The sample's `source` says
  what it was recorded from. The reply, and `stop`'s when it ended a take,
  carry `recorded`: each take's track, sample, path, input, device, seconds,
  the beat its clip begins at, `pulled_ms` and `lost_frames`, the frames the
  rings had no room for, which are also warned of. A take that ended before
  the count-in was over is warned of and not kept.
- **In the app**, Record in the transport bar, red while a take runs, dim
  while nothing is armed, and R; Transport › Record, which reads Stop
  Recording while a take runs, and Transport › Count-In. The arm is a dot
  before each track's M, red while armed, faint on a track with no input.
- **`daw record capture FILE --input NAME --seconds N [--channels 1,2]
  [--sample-rate 48000]`** records an input to a float WAV with no song and
  no host, for the panel's recording to come and an agent's quick capture;
  the reply gives the device, rate, channels, frames and peak.

## Checked

- `aaw-engine/tests/recording.rs`: a tapped stem and the mix equal the
  render frame for frame in 128-frame callbacks, the taps come back to the
  host; a count-in holds the song at the start, clicks once a beat and
  anchors the timeline a bar before it, then the bars are the render's; and
  through BlackHole, a burst played at beat 2 comes back through the input
  and lands within a millisecond of its beat by the anchor.
- `aaw-host/tests/record.rs`: the input's validation, the arm's refusals and
  state, `record.start` refused with nothing armed and with a muted source
  before any output opens.
- `tests/test_host.py`, through BlackHole: a track whose input is another
  track records that track's stem, frame for frame, as one undoable edit,
  with the names of the takes past a file left by an undone one and Stop
  ending a take; and a track whose input is the device, after a bar of
  count-in, has the burst at beat 2 within a millisecond. Skipped without the
  device.
- `aaw-ffi/tests`: the input set from the header's choices and drawn in the
  track view, the arm as shared transport state that is no edit, Record
  refused with nothing armed.
- Swift tests: the Input submenu's items, the arm's place in the header, the
  count-in kept, the words of Record.
- Scripted runs of the app through BlackHole: a click on the dot armed a
  track whose input was another track, drawn red; R started a take, Record
  red and the playhead rolling after the count-in in a picture; the take
  ended at the song's end on its own and R again began another, the clip
  `bounce-take-2` drawn on the track from bar 1 and the take begun last
  kept by the host closing. Nothing recorded through a microphone or an
  interface, and nothing heard or clicked by hand.

## Limits

- No monitoring of the input through the track's chain while recording: the
  person hears the input as the Mac gives it, or through the interface's
  own monitoring.
- A take's start is exact by the anchor; its end drifts by the difference
  between the input device's clock and the output's, which is nothing for
  one device and a few milliseconds a minute across two.
- Punch in and out, loop recording into takes and comping are not built; a
  take lands as a new clip over what is there, and `clips-overlap` says so.
- A device named in `input` that is not connected refuses the start with its
  name; nothing falls back to another input.
