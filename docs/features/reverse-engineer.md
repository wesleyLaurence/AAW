# Reverse-engineering a song — proposed November 16, 2026

Status: item 1, notes from audio, built November 16, 2026 as `daw transcribe`
([transcribe.md](transcribe.md), D183); item 2, the drum groove, built the
same day as `daw transcribe --drums` ([drum-groove.md](drum-groove.md), D185);
item 3, the form, built the same day as `daw samples form`
([form.md](form.md), D186); items 4–7 proposed, now Next items 1–4 of the
backlog, in that order (D182).
Each item is its own branch and pull request, built one at a time; where a
model is new to the project, a trial in a throwaway environment on the
person's songs comes first, as the downbeat model's did (D181).

## What

From a finished song's audio alone, two things:

1. **Know everything about it.** One analysis of the song, as data (JSON) and as
   a report (markdown): its tempo, meter and bars; its key and every key change;
   its chords bar by bar; its form, which bars and sections are the same music;
   its words by bar; the notes each part plays, the drum groove, the bass line,
   the chords' voicings and the melodies, as MIDI on its bars; and how it
   sounds, each part's tone, width, level and the effects on it.
2. **Recreate its session.** From that analysis, a session that plays the same
   music: the drums programmed as note clips with sounds near the stem's, the
   bass, chords and melodies as note clips on instruments, the arrangement in
   its sections, the mix's balance and tone matched, rendered and compared
   against the master until the differences are said and small.

The tools are `daw` commands; the person's reverse-engineer skill only
orchestrates them, in the order a session needs, and writes the report. A
measurement a skill needs that `daw` lacks becomes a command, not a script in
the skill.

## Why

The person studies records they admire and wants to recreate them: to learn
from a mix, to rebuild a part, to start a song from a sound they like. Each
measure answers a question a producer asks of a record, and the recreation is
the proof that the analysis is complete: what the comparison still finds
different is what was missed.

## What exists

| Measure | Command | State |
|---|---|---|
| Tempo, beats, bars, bars of another length | `daw samples beats` | The downbeat model leads (D181); not heard |
| Meter | `daw samples beats`, checked by `daw samples form` | The model's usual bar; a real 3/4 held, a real 6/8 did not, and the form's check says so (D186) |
| Key, chords by bar and beat | `daw samples chords` | Built (D177); key changes by section from `daw samples form` (D186) |
| Drum hits in a band | `daw samples hits` | Kick, snare, hats by bar and sixteenth (D180) |
| Stems | `daw separate` | Vocals, drums, bass, other; six with guitar and piano |
| Loudness, spectrum, width, resonances, overlap, translation | `daw listen` | Third octaves and seven bands; a stem's own |
| Swing | `daw samples beats` | By eighths and sixteenths |
| A mix against a reference | `daw reference` | Loudness, tone, width and contour by section |
| Words | the skill, through mlx-whisper | Not a command |
| Notes of a part | `daw transcribe` | Basic Pitch on a stem, on the beat map (D183); not heard |
| The drum groove as notes | `daw transcribe --drums` | Kick, snare and hat as notes, grooves and fills, a drum track (D185); not heard |
| Form: which bars are the same | `daw samples form` | Sections labelled and named, keys and key changes, the meter and bars checked (D186); not heard |
| Effects: reverb, delay, sidechain, compression | — | Missing |
| One analysis file | — | The skill writes markdown by hand |

The Fourier analysis the person asked about is already under most of this:
`daw listen` measures each stem in 31 third octaves, the beat map and the hits
work on short-time spectra and the chords on a chromagram. What is missing is
not a transform but what is read from it.

## Design

### 1. Notes from audio: `daw transcribe`

Built November 16, 2026; [transcribe.md](transcribe.md) is its reference. What
follows is the design as proposed.

A stem's notes as MIDI on the song's beat map: each note's pitch, start and
length in seconds and in bar, beat and sixteenth, its velocity, its pitch bend
and the model's confidence; written as note text, as a `.mid` file and, asked
for, as a note clip in the song at the stem's place.

- **Model.** Spotify's Basic Pitch (Apache-2.0): polyphonic, any instrument,
  MIDI with pitch bends, best on one instrument at a time, which the stems
  give. It runs through TensorFlow, Core ML, TFLite or ONNX; the separation
  environment already has ONNX Runtime. Its README supports Python 3.7 to 3.11,
  and 3.10 on Apple silicon; the environment is 3.12. The trial settles whether
  it installs there, in an environment of its own, or as its ONNX model alone.
- **Trial first,** on a bass stem, a vocal stem and an "other" stem of the
  person's songs: notes against what the person hears, placement against the
  beat map, what a full "other" stem's chords come out as.
- **On the grid.** Notes are placed by the beat map's beats, odd bars included;
  `--quantize` snaps them to a grid as `daw note quantize` does, and their
  distance from it is kept as the feel.
- **Chords from notes.** The notes of the "other" stem by bar give the chords'
  voicings, beside `daw samples chords`' names.

Done when: a bass stem's notes come out as a note clip that, played on a bass
patch under the drums, the person recognizes as the line; and the report says
how many notes it was sure of.

### 2. The drum groove as notes

Built November 16, 2026; [drum-groove.md](drum-groove.md) is its reference.
What follows is the design as proposed.

`daw samples hits` already gives each kick, snare and hat by bar and sixteenth
with its level and how far it is off the grid. This turns them into notes in a
note clip on a MIDI track, as every drum part is now (D184): a Sampler mapping
the General MIDI drum notes, 36 kick, 38 snare and 42 hat, to pads named for the
bands, a hit's level as its velocity, its offset as its timing. The reply says
which bars share a groove and where the fills fall; each groove is a clip
looped over its bars and each fill a clip of its own. The part is then read and
edited as note text and goes to another DAW as a MIDI file.
Toms and cymbals are bands too wide to name today; a band for each is an open
question, and a drum-transcription model is the alternative if the bands fall
short.

Done when: the clip played on a kit with `daw samples like` sounds near the
stem's, under the original, in time.

### 3. The form: which bars are the same

Built November 16, 2026; [form.md](form.md) is its reference. What follows
is the design as proposed.

Each bar is compared with every other on its chords, its notes where item 1
gives them, and its sound in bands: a self-similarity matrix of the bars. Runs
of bars that repeat are sections, labelled A, B, A′ by what they share and
named by what they hold (the words' chorus, a drop). Each section's key, from
its chords, gives the key changes. The meter is checked here against real
songs in 3/4 and 6/8, and the drums' kick-and-snare cycle checks the bars
(the Idea from D181).

Done when: a pop song's sections come out as the person would name them, a
repeated chorus marked as the same, and a song with a key change says where.

### 4. Words by bar: a command

The skill runs mlx-whisper today; this makes it a command over the vocal stem:
each word with its time, bar and beat, the lines, and the doubtful ones read
again on short clips with the larger model, as the clean-edit skill does.

Done when: a verse's lines come out with their bars, as the skill gets them now,
from one command.

### 5. Sound and effects from audio

Estimates, each with what it was read from:

- **Tone.** Each stem's spectrum as an equalizer curve against a flat or a
  reference one, for matching a recreated part.
- **Sidechain pumping.** A stem's level dipping in time with the kick, its depth
  and release.
- **Reverb and delay.** A tail's length after a hit or a phrase end; repeats
  in time with the beat.
- **Compression.** Crest and the level's envelope against the hits'.
- **Width** per stem, which `daw listen` has, by section.

Done when: each estimate is checked on a render the project made with known
settings, and said as an estimate on a real song.

### 6. One analysis file

`daw study FILE` (name open) runs every measure above on a song and its stems
and writes one JSON, sections, bars, chords, notes, drums, words and sound, in
one schema with each measure's confidence, and a markdown summary of it. The
reverse-engineer skill reads it and writes the report's readings; the file is
what a later session or a recreation starts from.

Done when: the skill's report is made from the file, and the file holds
everything the report says.

### 7. Recreate a session from the master

The recreate loop, a skill over the commands with whatever they lack added as
commands: the session at the song's tempo and bars; the drum clips on a kit
of the nearest samples; the bass, chords and melodies as note clips on Synth or
Sampler patches; the sections arranged; the mix balanced by the stems' levels
and tone; rendered and compared against the master with `daw reference`, and
adjusted. What the comparison still finds different is said, and what no tool
can do yet becomes a backlog line.

Done when: one song the person picks is rebuilt and the person can hear it is
the same song, with the differences listed.

## Open questions

1. Answered by the trial (D183): Basic Pitch installs without its
   dependencies beside the separation environment's and runs its ONNX model.
   Still open: its accuracy on a full "other" stem against stems6's guitar
   and piano.
2. Drums beyond kick, snare and hats: more bands, or a drum-transcription
   model. Kick, snare and hat are built from the bands with each drum's sound
   learned from the part (D185); the rest is a Later line.
3. The analysis file's name and schema, and whether it lives beside the song
   or in the project.
4. How a recreated part's sound is matched: a patch searched against the stem,
   or a sample of the stem itself where the person wants it.
5. What "the same" means for the recreation's comparison: which differences
   the person hears and which the measures say.
