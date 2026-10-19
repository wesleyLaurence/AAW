# Recording audio — proposed October 19, 2026

Status: proposed, not built. Backlog: Next, Recording audio; and A sound
recorded into the conversation, which builds on it. Asked for by the person on
October 19, 2026: "I need the ability to record instruments, for example
through the built in microphone, or a Scarlett interface", "support for
resampling which is basically recording audio from an internal source like
another track, group or all sounds", and in the agent panel "record audio, it
will use your configured audio input ... you can pass it along with a chat
message to the agent ... record my beatboxing a rhythm, then you say program
this beat for me". The concept said "No recording of new audio"; the person's
request revises it (D123).

## What

Three ways in:

| | Where | What is made |
|---|---|---|
| An audio track recorded | The track armed, Record in the transport bar, a count-in, the metronome; stop ends the take | A WAV under the project's `samples/`, an audio clip on the track from the bar the take began, named for the track and the take |
| Resampling | The track's input set to another track, a group or the mix instead of a device; armed and recorded the same way | The same, from the engine's own sum rather than a device |
| A sound into the conversation | `+` › Record Audio or a mark in the agent panel's field; a level meter while it records; stop | A WAV under `.daw/attachments/`, attached to the next message as a file is ([attachments](agent-attachments.md)) |

## Why

The person plays guitar, hums melodies and beatboxes rhythms, and wants each
to reach the song: a take on a track, or an idea the agent turns into notes.
Resampling is how a sound is made from the sounds already in the song, a
chord through its effects bounced and chopped. Freezing (Next) is resampling
a track by itself. All three need an input device chosen
([audio settings](audio-settings.md)).

## Design

- **The input is a property of an audio track**, `input`: a device's channel
  or pair (`device: "Scarlett 2i2", channels: [1]`), a track, a group or
  `master`, or none, kept in the song since a session's input setup is part
  of the song, with `monitor` auto, on or off, as Ableton's. A record arm,
  `armed`, is host state and not saved.
- **Recording is a host operation, not an edit.** Record starts the transport
  with a count-in of a bar or two from the start position; frames from the
  input ring buffer are written to a WAV at the song's rate as they come,
  through the device's input callback, never touching the audio thread's
  allocation rule; stop, or the end of the loop when looping, closes the file,
  hashes it and applies one edit: the sample added to the song and an audio
  clip placed at the take's start, with the clip trimmed to the frames
  recorded after the count-in. One undo step. The change log records it as
  the person's.
- **Latency.** A take is pulled earlier by the device's reported input and
  output latency and the resampler's, so a note played on the beat lands on
  the beat. The offset is written in the sample's record so a later check can
  see it; the tab shows the measured round trip when a loopback is run.
- **Resampling** reads the engine's own buffers: a track after its fader, a
  group after its chain, or the master after its effects, taken from the
  same stage the stems come from, so a resample equals the stem for that
  span. The source is not muted while it is recorded.
- **Monitoring.** `auto` hears the input through the track's chain while
  armed, `on` always, `off` never; the input's level shows in the header's
  meter.
- **In the conversation.** The panel's recording goes through the same host
  path with no track: the host records the chosen input to a file and hands
  the path back; the panel shows a level and the seconds; stop makes a chip.
  The attachment carries the song's tempo and the start position, so the
  agent can hear a count-in was not given and quantize to the beat map
  (`daw samples beats` on the file) rather than to the grid. The agent
  transcribes with what the tool has: `daw samples beats` for a rhythm,
  pitch detection for a hummed line, the stem's notes through the piano roll;
  what is missing becomes a backlog line.
- **Commands.** `daw record start SONG tracks.T [--count-in BARS]` and `daw
  record stop`, so a skill can run a take; `daw set tracks.T.input`; `daw
  record capture --input NAME --seconds N FILE` for the panel and for an
  agent's quick capture.

## Done when

- A track whose input is a virtual device fed a known tone records a WAV
  whose tone is at the right frequency, starts within a millisecond of the
  bar after the count-in, and is an audio clip on the track, in a test.
- A track whose input is another track records the same frames as that
  track's stem over the span, bit for bit at matching rates.
- Record, stop and undo in a scripted run leave the song as it was.
- `+` › Record Audio in a scripted run with `AAW_INPUT_DEVICE` set to a
  virtual device makes a chip, and the sent message lists the file with the
  tempo.
- The concept's "No recording of new audio" rewritten, and the microphone
  entitlement's use described in [speaking to the agent](voice-input.md),
  which already asks for the microphone.

## Open questions

1. Whether a take should land as a new clip over existing ones, or replace
   what it covers, as Ableton's does when overdub is off. A new clip, and
   `clips-overlap` says so.
2. Punch in and out, loop recording into takes, and comping: not in this
   stage.
3. Whether the panel's recording should also be able to record from a track
   or the mix, to ask about a sound as it plays.
4. Whether MIDI recording from a keyboard, which is Next as its own item,
   shares the arm and the count-in. It should.
