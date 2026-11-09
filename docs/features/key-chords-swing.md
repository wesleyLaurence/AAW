# Key, chords and swing — implemented November 8, 2026

Next item 17 of the backlog: key and chord detection, and swing detection, of
samples and songs, so a loop is matched to the song's key before it is placed
(D177). A song declares no key (D6); these measure one. Every value is an
estimate from decoded audio or from the song's notes, local, numpy only, and
nothing here listens.

## What

- **`daw samples analyze`** gives every sample a `key` and a loop its
  `chords` and `rhythm.swing`, kept in the index with the rest of its
  measurements. A sound with no key, a drum, a noise or a note alone, has
  `tonal: false` and `name: null`.
- **`daw samples search --measured-key KEY [--key-within N]`** keeps the
  samples measured in a key or its relative, and with `--key-within` those a
  transpose of up to N semitones puts there, nearest first. Each row's
  `measured` has `key`, `key_confidence` and `swing`, and with the filter
  `to_key`: `transpose`, `then_key` and `relation`. A key is written `A
  minor`, `Am`, `amin`, `F#`, `Gb major` or `Bbm`.
- **`daw samples chords FILE [--near TIME] [--window S]`** gives a whole
  song's key and its chords on its beat map, each chord with its `bar`,
  `beat`, `seconds`, `until_seconds`, `beats`, `chord` and `fit`. It uses
  the map `daw samples beats` keeps beside the file, with its corrections.
- **`daw samples beats`** gives the song's `swing` with the map.
- **`daw key PROJECT [--sample SAMPLE]...`** gives the key of what the song
  plays and what it was measured `from`; with `--sample`, each sample's key,
  the `transpose` that puts it in the song's key, `then_key`, `relation`,
  and for a loop `source_bpm`, `repitch_semitones` and its swing. A sample
  is a library ID, a file, or one of the song's own samples by its ID.

`daw describe samples.key`, `samples.chords` and `samples.swing` are the
contract.

## How a key is measured

1. **A chromagram.** Frames of 8192 samples, a quarter apart, in magnitudes
   from 55 Hz to 2.5 kHz. Each bin is given to the semitone it is nearest by
   the square of the cosine of its distance, so a bin between two semitones
   belongs to neither. The bass, from 40 to 220 Hz, is kept apart for chords.
2. **Less the overtones.** Each semitone's level is reduced by half of the
   level 19 semitones below it and a third of the level 28 below, the third
   and fifth harmonics of those notes, which otherwise read as a fifth and a
   major third that nobody plays. Without this a key a fifth above the true
   one was chosen often.
3. **Pitch classes,** compressed as `log(1 + 100 x / loud)` against the
   99th percentile frame, so quiet passages count and a loud hit does not
   rule.
4. **The key** is the one of the 24 whose profile the sum of every frame is
   most like, by correlation. The profiles are Temperley's from the
   Kostka–Payne corpus: how often each degree of the scale sounds in music in
   a key. The odds of each key are `exp(25 (r - r_best))`, normalized:
   `confidence` is the chosen key's and `scale_confidence` its and its
   relative's together, since the two have the same notes.
5. **Tonal** when the profile's spread, its standard deviation over its
   mean, is at least 0.12 and the best correlation at least 0.6. One note
   alone correlates about 0.55.

## How chords are measured

Each beat's pitch classes, the mean of its frames, centered, are held to 84
chords, 12 roots of a major or minor triad, 7, maj7, m7, dim and sus4,
each as its notes centered and made unit length. A seventh costs 0.04 and a
dim or sus4 0.05 against a triad, so they are named where they are heard. A
Viterbi path through the beats takes a chord for each: a change costs 0.15
within a bar and 0.05 on a bar line, no chord, `N`, scores 0.35 on every
beat, so a beat no chord is more like than that is `N`, and a quiet beat,
under a twentieth of the median, is `N`.
A run of one chord is named with `/BASS` when the bass band's strongest
class over it, holding a fifth of the band or more, is another of the
chord's notes. Names are in flats in a key whose signature has flats, else sharps.

## How swing is measured

Swing is written as a pattern's `swing` is: where the second note of a pair
falls within the pair, 0.5 straight, two thirds a triplet feel, 0.75 at most.

1. **Onsets:** the beat map's spectral flux, a frame every 256 samples,
   peaks that are the largest within 30 ms, at least 0.04 of the strongest
   and one and a half times the mean of the half second around them, each
   moved to the sharpest rise within 12 ms as the beat map places its beats.
2. **Places in the beat:** each onset as a fraction of the beat it falls in.
   The onsets within 0.08 of a beat say how far the onsets sit from the
   beats, and that is taken off every place.
3. **Eighths and sixteenths.** An onset from 0.42 to 0.735 of the beat is a
   second eighth, its place the swing; one from 0.17 to 0.42 or 0.735 to 0.92
   is a second or fourth sixteenth, its place in the half beat the swing.
   Each kind's swing is the weighted median, by the root of the onset's
   strength, of those within 0.06 of the kind's median; a kind needs 8 such
   onsets holding 40% of its weight, so the attacks inside held chords, which
   fall anywhere, are no swing.
4. **The grid** is 1/16 when the sixteenths' clustered weight is a quarter
   of the off-beat onsets' or more, and 1/8 otherwise. `feel` is straight
   under 0.52, triplet within 0.02 of two thirds, swung otherwise. `pattern`
   is the grid and swing to write on a pattern, and `delay_beats` how far
   to move each second note when writing notes.

A loop's beats are its measured tempo's, from 0 when the loop is whole bars
and from its first onset otherwise (`rhythm.tempo.first_beat_seconds`). A
song's are its beat map's.

## How a song's key is measured

The tracks that are heard count: not muted, not in a muted group, and soloed
or in a soloed group when anything is. Each note `aaw_model::schedule::sounded`
lists adds its length in beats, a quarter at least, to its pitch class, a
Synth's note once for each oscillator. A pad's hit with no note, and each
active audio clip, adds the profile of the part of its sample it plays, rolled
by its transpose and by the repitch of playing it at the song's tempo when
`source_bpm` is set with `stretch: repitch`, times the beats it sounds, so a
sample counts as one voice. A sample whose audio has no key adds nothing and
is listed under `without_key`. The sum is held to the profiles as a sample's
own is.

**The transpose to a key** moves a sample's scale onto the key's, from −6
to 5 semitones, so a sample in the key's relative needs none and
`relation` says `relative key`. It is for the sample at its own pitch: with
`--source-bpm` and the default `repitch`, playing a loop at the song's tempo
moves it by `repitch_semitones` as well; `--stretch preserve_pitch` does not.

## What was checked

- **Generated chords with overtones,** five progressions in each of the 24
  keys with and without drums: the scale in 239 of 240 and the key in 190.
  The rest are a key and its relative, as in i–VI–III–VII. Krumhansl and
  Kessler's profiles found the scale in 214, Aarden's in 226 and Albrecht
  and Shanahan's in 239 with two more keys; stronger or weaker overtone
  shares, a finer low band, a tuning estimate and a median filter over
  time did no better on the three sets here.
- **214 loops of the person's sample library whose names give a key:** 137
  measured with a key, of which 107 the named key (78%), 2 its relative and
  28 another, 14 of those a fifth above. 77 measured with no key; given one
  anyway, two thirds of them would have been wrong.
- **17 released songs whose keys are well known,** from the person's DJ
  folder: 10 the key; one in E♭ major where its chords Fm, Cm, E♭ and B♭
  are F dorian; one in B minor that is usually written F♯ minor; a funk vamp
  of seventh chords and a distorted rock song with no key; and four others
  different, two of them files whose tempo is not the record's. A song took
  under a second.
- **Chords** of generated progressions on the beat, a C over an E as C/E,
  silence as N; a song's on its beat map, a chord a bar on the bar lines.
- **Swing** of generated grooves at 90, 128 and 174 BPM, straight, 0.58,
  0.6, a triplet feel and 0.72, on either grid: within 0.015, the grid
  right. Six released songs gave two near straight, 0.51 and 0.525, two
  funk and disco songs 0.57 on sixteenths, one 0.64 on eighths, and one
  known for its shuffle straight at 160 BPM; nobody listened to say whether
  those are right.
- **`daw key`** on songs of note clips, of a loop moved by its transpose, of
  drums, and with a track muted.

## Limits

- **Modes are not named.** A song in a mode reads as the major or minor key
  with the same notes, D dorian as C major or A minor, and its tonic is
  wrong. A song that changes key is given one key for the whole.
- **A fifth off with confidence.** The commonest wrong key is a fifth from
  the true one, with a confidence as high as a right one's, so read the
  candidates and the chords before transposing far.
- **Dense or distorted mixes,** rock guitars, a funk vamp of seventh chords,
  often measure with no key.
- **Chords** are triads, sevenths, dim and sus4 only, a beat at a time:
  a chord that changes within a beat is the beat's mix, and a passage of
  drums alone is given some chord with a lower `fit`.
- **Swing** is of the whole file, on the beat map's beats; a song whose feel
  changes gets the mix of its parts, and a beat map at double tempo reads
  swung eighths as sixteenths.
- **`daw samples analyze`** measures a sample's key, as the rest of it,
  from its first two minutes; `daw samples chords` and the samples `daw key`
  measures for a song read up to ten minutes.
- **`daw key`** counts a MIDI effect's notes as the schedule has them, a
  device instrument as the note's pitch, and a sample as one voice whatever
  it holds.
