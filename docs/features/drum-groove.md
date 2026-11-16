# The drum groove as notes — implemented November 16, 2026

Item 2 of the reverse-engineer track (D182, D185): a drum part's kicks, snares
and hats, heard from its audio, as notes on the song's beat map, with the bars
that share a groove and the fills said, and the part written as a drum track:
a MIDI track whose Sampler plays the part's own hits, a looped clip a groove
and a clip a fill, as every drum part is now (D184). `daw transcribe --drums`
does it. Everything is measured on the Mac with numpy and scipy; no model is
installed, and no audio leaves it.

## Commands

```sh
uv run daw transcribe projects/NAME/separated/ID-drums.wav --drums --text
uv run daw transcribe tracks.T.audio.N --drums --project projects/NAME
uv run daw transcribe STEM --drums --midi /tmp/drums.mid      # every hit as played
uv run daw transcribe STEM --drums --project SONG --as-played  # one clip, no grooves
uv run daw transcribe STEM --drums --near 1:20 --window 4
```

`SOURCE` is what `daw transcribe` takes: a file, a library sample, or with
`--project` one of the song's samples or an audio clip, `tracks.T.audio.N`.
Give it the drums stem from `daw separate`; a part `daw separate` made is
placed on its source song's beat map, and `--beats FILE` names another.

## How the hits are heard

1. **Moments.** `daw samples hits` finds the hits in the kick (45–90 Hz), snare
   (1.5–4 kHz) and hats (7–16 kHz) bands, down to 35 dB under each band's
   loudest. Hits in two bands within 30 ms are one moment, timed from the hit
   that rises most.
2. **What sounds there.** A band says where a drum sounds most, not which drum
   it is. On a drum-machine stem a kick's beater click was louder in the
   snare's band than the snares, and a snare's rattle reaches the hats'. So each
   moment's sound is read as the change in 24 bands from 40 Hz to 16 kHz over
   the 90 ms after it, against just before: the bands under 300 Hz from a long
   window, the rest from a short one 5 ms on, after the click. Each drum's sound
   starts from the moments only its band shows: a kick whose snare band does
   not ring (a click falls 15 dB or more in 50 ms, a snare does not), a snare
   with the kick band 15 dB down, a hat with both down. The three sounds are
   then fitted to every moment together, each moment the sum of the drums its
   bands allow (non-negative matrix factorization, 30 multiplicative updates),
   so a hat on every kick is not learned as the kick's. More updates overfit a
   busy stem: at 100 a real stem's hats drifted and its main groove halved.
3. **Which drums play.** A drum plays at a moment when its band has a hit there
   and its share is within 15 dB of its loudest hits (the 99th percentile).
   That level is `level_db` and gives the velocity, 127 at 0 and 1 at 40 dB
   under, as for notes. A moment no drum explains, a tom, a cymbal or a swell,
   is counted as `unnamed`. A drum on one sixteenth twice is once, the louder.
4. **On the beat map.** Each hit is placed as `daw samples hits` places one,
   with `beats` from bar 1. Its bar and sixteenth in the bar come from the
   map's bars, odd bars included.

## Grooves and fills

A bar's pattern is the set of its hits on their sixteenths. A groove is the
hits most of its bars play, and a bar is of it when at least four in five of
the hits either has are shared, so a bar a hit or two from its groove, as the
hearing misses or adds on a busy stem, is still it. The pattern most bars are
near is groove A, refined three times to the hits most of its bars play; then
B from the bars left, and so on while one holds four bars. `differs` says, for
each bar of a groove, the hits it has extra or missing. The bars of no groove
are fills.

## Writing them

- `--text`: each clip as note text, a row a drum, kick first, `|` between bars,
  on `--grid` (1/4); a hit's cell is its sixteenth, and `between_cells` counts
  those a coarser grid cannot hold. The groove clips hold their loop.
- `--midi OUT.mid`: every hit as played, at 960 ticks a beat, bar 1 at
  `bar_one_beat`.
- `--project SONG`: one `daw batch`, the song's one change, made last:
  - A new MIDI track `drums` (`drums-2` when taken), or `--track`, a MIDI
    track. A track without an instrument gets the Sampler; one with an
    instrument keeps it and gets the clips only.
  - Pads `kick`, `snare` and `hat` mapped to C2, D2 and F#2, each the part's
    own clearest hit of that drum, cut from its start to the next moment or a
    second on, faded out and imported as the sample `TRACK-DRUM`. The reply's
    `pads` gives where each was cut. A name the song already has is refused
    before anything changes.
  - A run of one groove is one clip looped a bar, and two grooves in turn one
    clip looped two bars. Each note sits at the median velocity and timing its
    bars play it with, so the groove keeps its feel. A fill is a clip of its
    own, as played. `--as-played` writes one clip of every hit instead.
  - Bar 1 lands at `--at` (0), or where an audio clip plays the part's first
    downbeat, a beat of the part as many of the song's beats as the clip plays
    it in, as for notes. Clips before the song's start are left out and their
    hits counted in `before_the_song`. A clip ID the track has is not reused.

## Report

`path`, `beat_map`, `beat_map_of`, `tempo_bpm`, `meter`,
`first_downbeat_seconds`, `odd_bars`; `drums`, each with `note`, `midi`,
`count` and `clearest_seconds`; `unnamed`, `count`, `grooves` (`id`, `bars` as
`[from, to]` runs, `count`, `beats`, `hits`, `differs`), `fills`, `left_out`;
then `text`, `midi` and `clip` (`track`, `pads`, `clips`, `bar_one_at`,
`song_beats_a_beat`, `project_sha256`) when asked, and `hits`, each with
`drum`, `note`, `midi`, `seconds`, `bar`, `beat`, `sixteenth`, `off_grid_ms`,
`beats`, `level_db` and `velocity`. A part whose beats move 20 ms or more from
one tempo is warned of with the drift: its notes go by its beats, so in a song
of one tempo they leave its audio by that much. `--line`, `--range`,
`--min-confidence` and `--quantize` are refused: they are for pitched parts,
and `daw note quantize` moves the written notes. `daw describe beats.drums` is
the contract.

## What was checked

On the day it was built, with nobody listening:

- **A generated part** at 120 BPM over 16 bars, placed on a generated song's
  beat map: a kick with a beater click as loud in the snare's band as the
  snare, a snare on two and four, hats on the eighths, and two fill bars with
  a snare roll and no hats on the last beat. Every kick and snare on its
  sixteenth and no snare where a kick's click is; at least nine hats in ten,
  the missed ones on a kick or a snare, none made up. One groove of 14 bars
  and the two fills, written as four clips; the track's Sampler, samples and
  clips; a MIDI file; one clip as played; a track that has clips; a part from
  an audio clip; refusals, and a refused part leaving the song as it was.
- **A 4:54 pop record's drum-machine part,** separated with
  `--split stems --quality best`, in about 33 s. One groove over 106 of its
  bars: kick on one and three, snare on two and four, hats on every eighth.
  Two smaller ones of 7 bars and 17 runs of fills, 49 clips in all. Bars with
  a kick on the last sixteenth of beat three came out as fills. Written into a
  song at the record's tempo and rendered, 274 of 277 kicks, 407 of 419 snares
  and 544 of 562 hats sounded within 30 ms of where the song places the
  record's, the median within 2 ms. The record's beats move 135 ms from one
  tempo (115.6 to 118.5 BPM), so against the audio clip the notes drift by
  that much; the warning says so.
- **A live-played rock song's part** at about 133 BPM, separated with the
  `fast` stems model, in about 4 s. Its main groove, 19 bars of kick on one
  and three and snare on two and four, and three smaller ones. An earlier
  section came out as kicks on two and four with a hat or cymbal on one and
  three, and no snare: those hits matched the kick's learned sound closely
  and the snare's poorly, while ringing in the snare's band. It may be a beat
  map a beat off, or a floor tom; nobody has heard it.

Nobody has heard a written part played back.

## Limits

- Three drums: kick, snare and closed hat. Toms, crashes, rides and open hats
  are `unnamed` or read as the drum nearest them, a floor tom as a kick, a
  crash as a hat. A drum-transcription model or more bands would add them.
- A hat struck with a kick or a snare is the hit most often missed or added;
  a groove's majority decides it, and `differs` lists the bars.
- A groove clip plays its median: a bar that differs from it by a hit or two
  is played as the groove, and only `differs` and `--as-played` keep it.
- Notes go by the part's beats; in a song of one tempo a part whose tempo
  moves leaves its audio by the drift given in the warning.
- The pads are the part's own hits, with whatever bled into them on the stem;
  `daw samples like` on one finds library sounds near it, and
  `daw set SONG tracks.drums.instrument.sampler.pads.kick.sample ID` plays one.
