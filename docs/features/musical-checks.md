# Musical checks — implemented 2026-10-04

Status: implemented (D78). `daw check` warns about the music and not only the
file: clips on one track stacked or sounding at once, a note struck again while
it sounds, an audio clip the song ends inside, a track with nothing to play, two
tracks crowding one low octave, a sample repitched far from its root and a Synth
note below hearing.
`clip duplicate` lays repeated copies in a row. What it is for is in
[concept.md](../concept.md#what-the-agent-works-with) as Checks: a linter for
music.

## Why

An agent writes notes by the hundred, through scripts, and cannot hear the
result as it goes. The mistakes it makes most are mechanical and visible in the
song file. A check catches them before a render, every time, cheaply.

On October 4, 2026, an agent ran `clip duplicate` three times on one drum clip.
Each copy was placed right after the original, so all three landed on the same
bar, and `daw check` returned `[]`.

## Warnings are objects

```json
{"code": "clips-stacked", "level": "warning",
 "message": "drums: tracks.drums.clips.1 and tracks.drums.clips.2 start on beat 16 with the same music, so it plays twice at once",
 "paths": ["tracks.drums.clips.1", "tracks.drums.clips.2"], "at": 16}
```

- `code` says what kind; `daw describe check` lists every code and what it means.
- `level` is `warning`, or `info` for what is often deliberate. Only
  `clips-overlap` is `info`.
- `message` is the warning in words. The warnings `daw check` gave before
  objects are the messages of theirs, unchanged.
- `paths` names what it is about as commands do: `tracks.drums.clips.1` (a
  pattern clip by index), `tracks.keys.clips.chords` (a note clip by ID),
  `tracks.edit.audio.0`, `tracks.bass`, `tracks.bass.instrument.synth`,
  `tracks.kit.instrument.sampler.pads.kick`, `samples.kick`. A warning about two
  things names both.
- `at` is the song beat, written as commands take one, when the warning has a
  place in time.

The warnings are made in Rust, in `aaw-model`'s `check` module, so a host can
give the same ones. Python's `daw check` adds the two that need the audio
(`sample-unreadable`, `root-note-mismatch`) and passes each readable sample's
length in seconds, which an audio clip without an end needs.

## The codes

| Code | When |
|---|---|
| `clips-stacked` | Two or more clips of a track start on the same beat with the same music: the same pattern, the same playing notes, or the same file from the same second. One warning a group |
| `clips-overlap` | Two clips of a track sound at once, as information. A pair already stacked is not reported again |
| `note-retriggered` | A note starts while one of the same pitch on its track still sounds. One warning a clip, with the count and the first |
| `song-ends-inside` | An audio clip sounds past `session.length_beats`. Validation refuses a pattern or note clip that would, or any clip that starts at or after the end |
| `track-empty` | An instrument and no notes, notes and no instrument, pads and no clips, or nothing |
| `register-crowded` | Two unmuted tracks hold notes in the same octave below C3 in the same bars. One warning a pair of tracks and an octave, the bars listed |
| `pad-far-from-root` | A hit repitches its sample more than 24 semitones: the note against the sample's root (middle C without one), plus the event's and the pad's transpose. One warning a pad |
| `note-below-hearing` | A Synth note whose lowest pitched oscillator, with its octave, semitones and detune, is under 20 Hz. One warning a track |
| `notes-outside-clip`, `notes-unmapped` | Notes that start outside their clip, and notes a sampler maps to no pad |
| `macro-unused`, `voices-exceeded`, `release-cut` | A Synth macro that moves nothing, more notes at once than voices, a release the song's end cuts |
| `stretch-without-tempo`, `stretch-audible` | `preserve_pitch` without `source_bpm`, and a stretch past about 8% |
| `lane-on-bypassed-effect` | A lane that moves a bypassed effect |
| `sample-unreadable`, `root-note-mismatch` | From Python: a file the engine cannot read, and a root note the audio disagrees with |

What counts as a pitched note, for `register-crowded`: a Synth's notes, a
sampler's notes that a pitched map entry plays, and pattern events with a
`note`, a beat long when they have no duration. A drum map's kick on C2 has no
register.

## `clip duplicate` lays copies in a row

Without `--at`, the copy goes right after the original; if a clip of the
destination track already starts there, the copy goes after the run of clips
that follow one another from there, each starting where the last ends. Three
duplicates of one clip lay out four in a row, as pressing ⌘D three times in
Ableton does. `--times N` makes N copies in a row as one undo step, labeled
"Duplicate clip NAME N times", and the reply lists their paths. With `--at`,
the copies go from that beat one after another, whatever is there. `--id`
names one copy and is refused with `--times` above one. The app's duplicate
passes a beat, so it is unchanged.

## Limits

- Checks read the song, not the audio. Two tracks masking each other is a
  perception line in the backlog.
- `register-crowded` knows notes, not instruments: a bass patch an octave above
  its notes, or a pad filtered thin, is counted at its written pitch.
- A warning cannot be marked as meant. `clips-overlap` is information for that
  reason.
- `daw render` does not run the checks.
- A section's feel is not in the song, so a check such as a snare off the
  backbeat in a half-time section cannot be made.
