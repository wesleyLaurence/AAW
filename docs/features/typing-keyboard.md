# The typing keyboard, and notes recorded — implemented October 30, 2026

Asked for by the person on October 30, 2026: "the midi keyboard with the
typing keyboard so I can record enable a synth or sampler then record a
pattern by playing my typing keyboard." The first half of the backlog's MIDI
keyboard item (D168); a keyboard plugged in through CoreMIDI, and quantize,
stay on the backlog.

## What

The Mac's keys are a piano, in Ableton's layout: A S D F G H J K L ; ' are the
white keys from C, W E T Y U O P the black keys above them, Z and X move the
octave down and up, C and V the velocity down and up by twenty. A is C4,
middle C, until the octave is moved. A key held holds its note through the
MIDI tracks it plays and lets it go when lifted, so chords and long notes
play as on a keyboard. The keys play the armed MIDI tracks, or the selected
MIDI track when none is armed.

A MIDI track with an instrument, a Synth, a Sampler or a real-time
instrument of the person's own, is armed by the dot before its M, as an audio
track with an input is, and the typing keyboard comes on with it. Record then
counts the bar in and the song plays, and every note held and let go through
the armed MIDI tracks is taken into one note clip a track, from the start
position, whole bars long, as one edit by the person that one undo takes
back, beside any audio takes of the same recording. Nothing is heard from
the take itself: the notes sound as they are played.

## How it works

- **The keyboard is the app's.** The transport bar's piano icon, yellow while
  it is on, and Transport › Typing Keyboard (⇧⌘K) turn it on and off; while
  it is on the octave of the A key and the velocity stand beside the icon,
  `C4 · 100`. Arming a MIDI track turns it on, so the person can play at
  once, and disarming the last one turns it off again unless it was turned on
  by hand. While it is on the note keys are the keyboard's and nothing else
  sees them: L, M and A play notes rather than toggling the loop, dropping a
  marker or showing automation, and the piano roll's and the pattern
  editor's letter keys are taken too. Space, Return, Home, R, Escape, Delete,
  the arrows and every key with ⌘, ⌃ or ⌥ are as they were. A text being
  typed, a name or the agent panel's message, has the keys while it has the
  focus. Repeats of a key held play nothing more. `SongWindow` takes the keys
  in `performKeyEquivalent`, before the menus' key equivalents, and the keys
  lifted in `sendEvent`, which only the window sees; `TypingKeyboard` is the
  layout and the octave and velocity; the model keeps the notes held by key
  code, so a key lifted lets go what it pressed even after the octave moved.
- **`note.on` and `note.off`** are the host's transport commands, beside
  `note.preview`: `{"op": "note.on", "track": T, "pitch": 60, "velocity":
  100}` holds a note through the track's instrument until `{"op": "note.off",
  "track": T, "pitch": 60}` lets it go through its release, a note of the
  same pitch already held on the track let go first. Neither is an edit. A
  held note is a preview held for an hour, and the engine's `let_go_preview`
  lets go one pitch of a track's previews and leaves the rest held, through
  the Synth, a real-time instrument and a Sampler's prepared voices, which
  now carry their pitch. `daw note on PROJECT TRACK PITCH [--velocity V]`
  and `daw note off PROJECT TRACK PITCH` are the terminal's way, for an
  agent or a script with a host running. The app sends them through
  `Song.note_on` and `Song.note_off`.
- **The arm.** `arm` and `daw record arm` take a MIDI track with an
  instrument, refuse one without, in the words `has no instrument to record
  notes through`, and refuse a frozen track as before. In the app the dot is
  faint on a MIDI track with no instrument, and a click on it says so. The
  transport's `armed` lists MIDI tracks among the rest.
- **The take.** `record.start` gives every armed MIDI track a `NoteTake`. A
  `note.on` or `note.off` on a recording track is logged at the frame being
  heard: the output's position less the device's latency, as the clock the
  app's playhead reads. A note that began during the count-in and is still
  held at the start begins at the start; one that ended before the start is
  dropped. When the recording ends, by Record again, Stop, a locate, the
  loop's end or a change of devices, a note still held ends there, and the
  notes become `clip.add` on the track at the start position, with the
  positions rounded to a thousandth of a beat as the audio take's `at_beat`
  is, the clip lasting whole bars of the song's meter from the start, one at
  least, as a MIDI file imported does. The clip goes into the same batch as
  the audio takes, labelled `Record notes on TRACK (S s)`, by the person. A
  take with no notes left makes no clip and no edit. The reply's `recorded`
  lists the take with `notes`, `at_beat`, `length_beats` and `seconds`.

## Checked

- `aaw-engine/tests/midi.rs`: two notes held through a Sampler and one let
  go by its pitch, the other still sounding until it is let go too.
- `aaw-host/src/record.rs`: the note take's clip: a note held across the
  start begins there, one ended before it is dropped, a chord's notes end
  apart, a pitch played again while held ends the first, the length is
  whole bars and the positions are thousandths of a beat; nothing played is
  no clip.
- `aaw-host/tests/record.rs`: a MIDI track armed once it has an instrument,
  refused without; `note.on`'s checks of the track, the pitch and the
  velocity before any output; `note.off` of a pitch nothing holds.
- `tests/test_host.py`, through BlackHole: a Synth track armed and
  recording, `daw note on` and `daw note off` while it runs, the clip's two
  notes within a quarter of a second of when each command was sent, the
  note held at the end ending there, the clip whole bars from beat 0, one
  undo, and a recording in which nothing was played leaving no edit.
- Swift tests: the layout from middle C, the octave and velocity within
  their bounds, the help's words, a scripted key lifted with `up+`, Record's
  words.
- A scripted run of the built app through BlackHole: a Synth track armed
  from the terminal, the icon lit and `C4 · 100` beside it in the picture, R,
  keys pressed and lifted with `--key a` and `--key up+a`, R again, and the
  song read back with the clip. Nothing played by hand; nothing heard.

## Limits

- Notes land where they were played, to a thousandth of a beat: there is no
  quantize, and no option to record on the grid.
- A note played during the count-in lands at the start position.
- The keys play the armed MIDI tracks or the selected one; there is no
  choice of track apart from that, and no MIDI effect hears a note played
  now (`daw describe midi`).
- One key, one note: a key pressed again while held plays nothing until it
  is lifted, and the Mac's key repeat is ignored.
- No MIDI hardware: a keyboard plugged in through CoreMIDI is the backlog's.
- The piano roll's previews are still a beat long under the mouse; holding a
  drawn note down while the mouse is down could use `note.on` and `note.off`
  now (an idea on the backlog).
