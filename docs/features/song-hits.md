# A song's hits — implemented November 15, 2026

The first half of Next item 7 of the backlog, why the agent missed the kicks
and the downbeat (D180). `daw listen` lists the hits a song schedules, so
nothing listed the hits sounding in a recording: a song's or a stem's drums.
Placing a sub kick under a separated drum stem went wrong two rounds running
when the agent found the kicks with a hand-made detector and checked it with
another; one picture of the stem's 45–90 Hz level showed the kicks plainly.
`daw samples hits` is that picture and that list, on the song's beat map. All
computation is local, numpy and scipy; no model or network call is made, and
nothing here listens.

## Commands

```sh
uv run daw samples hits projects/NAME/separated/ID-drums.wav --band kick
uv run daw samples hits FILE --band snare --picture /tmp/snare.png
uv run daw samples hits FILE --band 45-90 --near 1:20 --window 4
uv run daw samples hits FILE --band kick --min-level -6     # the loud cluster only
uv run daw samples hits FILE --band hats --min-rise 9       # only sharper rises
```

`FILE` is a path or a library index ID, as for `daw samples beats`. The bands
are named for a drum stem: `kick` 45–90 Hz, `snare` 1500–4000 Hz, the crack,
`hats` 7000–16000 Hz, and `all`, the default; any other band is `LOW-HIGH` in
Hz. On a full mix the kick band holds the bass too, so separate the drums first
(`daw separate PROJECT SOURCE --split stems`) and run it on the drums stem.

## Report

- `band`: its `name`, `low_hz` and `high_hz`.
- `tempo_bpm`, `first_downbeat_seconds` and `beat_map`: the map the hits are
  placed on, the one `daw samples beats` keeps beside the file, with its
  corrections. A file too short for a map, a loop or a one-shot, has its hits
  in seconds only, and `no_beat_map` says why.
- `loudest_dbfs`: the level of the band's loudest hit.
- `levels`: how many hits lie in each 3 dB under the loudest, down to −30,
  whatever is listed. One instrument's hits are usually a cluster near 0 with
  a gap under it; under the gap are quieter hits, other drums leaking into the
  band and swells where another drum's attack falls.
- `hits`, in order, each with:
  - `seconds`: where its sound starts, to within about 2 ms.
  - `bar` and `beat`, the beat from 1 with a fraction between beats, and
    `sixteenth`, the nearest as `BAR.BEAT.N` with `N` from 1 to 4, with
    `off_grid_ms` from that sixteenth's time on the map.
  - `level_db` against the loudest hit, and `rise_db` over the 100 ms before.
  - `gap_seconds` and `gap_beats` to the next listed hit, so a missing hit or an
    extra one stands out in a run of equal gaps.
- `min_level_db` (−20) and `min_rise_db` (6): what was listed, from
  `--min-level` and `--min-rise`. With `--near TIME --window S` (5) only the
  hits around a time are listed, and `near.of` says how many there are in all.
- `picture`, with `--picture OUT.png`: the band's level against its loudest hit
  over the file's peak level, a row of eight bars at a time with the bar lines
  numbered, each hit a line at its time and a mark at its level; with `--near`,
  the window alone.

## How hits are found

1. **The band** is filtered forward and back (a fourth-order Butterworth each
   way), so nothing is moved in time, and its power averaged a millisecond at a
   time over a period of its lowest frequency, from 2 to 25 ms, so a low band's
   own cycles do not read as hits.
2. **A hit** is a peak of that level, at least 50 ms from a louder one, that
   stands `--min-rise` dB over the quietest point of the 100 ms before it.
3. **Its time.** The averaged level starts to climb before the sound does, slowly
   where something else sounds in the band, and can go on climbing long after,
   so the attack is looked for over the whole climb, from just before the level
   gains a tenth of it to its peak or 150 ms on. The attack is the sharpest rise
   in the file's own level there, the energy of its first difference over 2 ms
   against the 2 ms before, counted against a tenth of the loudest energy in
   reach, so a blip in the quiet before the hit does not outrank it. A hit's
   sound can begin a little before its loudest attack, a beater's click before a
   kick's body, so from there the sharpest rise as the beat map's transients
   weigh it, in the 25 ms before, where the sound is within 30 dB of the
   attack's, is its start. The beat map is fitted to the same transients, so a
   hit on the beat is `off_grid_ms` 0.
4. **A climb with no attack in it,** a swell or two notes beating, is not a
   hit. Two hits whose attacks are within 50 ms are one, the louder, so a
   quieter sound just before a loud one does not lend it its time.

## Limits

- A quiet hit under a held note in the same band, a ghost kick under a bass,
  can be missed: the two partly cancel and the level rises less than
  `--min-rise`.
- A quiet hit listed can be a swell where another drum's attack falls, at about
  the level of a ghost note; level does not tell them apart. Read the picture.
- Hits closer than 50 ms are one: a flam is one hit.
- The band says where a drum sounds most, not which drum it is: a snare's crack
  and a beater's click both sound in the snare band, a tom in the kick band.
- On a beat map whose downbeat is wrong, every bar and beat is wrong with it.
  Check the map's `downbeat.confidence` first (`daw describe beats.downbeat`).

## What was checked

- **A generated drum part** at 120 BPM over 16 bars, with a bass held through
  each bar in the kick's band: every kick within 2 ms and the only hits within
  6 dB of the loudest, the levels counting none from −3 to −6; one kick with a
  beater click 8 ms before its body timed from the click, and one after a faint
  blip 30 ms before it timed from itself; ghost kicks 9 dB down listed at
  −6 to −13 on their sixteenth; every snare in the snare band with the click
  as one more, and every hat in the hats band but the one 8 ms after the click,
  which is one hit with it; bars, beats, sixteenths and gaps of 2.0 beats.
  A one-bar loop gives its kicks in seconds. A ghost 12 dB down under the
  bass was missed, which is the first limit.
- **The beat map's own test song** (`beat_song`): 48 of 48 kicks, 48 of 48
  snares and 96 of 96 beats in the full band, each at the sample.
- **A separated drum stem of a live-played rock song** at about 133 BPM whose
  tempo drifts ±157 ms (the stems model, `fast`): 104 hits within 6 dB of the
  loudest in the kick band, two beats apart with one-beat pairs, 87 in the top
  3 dB; 101 of the 104 start where the beat map's transients put them. Its
  snare band showed the snares tall and the kicks between them low, and the
  same command on the full mix found the bass in the kick band, which is why
  the drums stem is the input. By the stem's beat map, whose downbeat had a
  confidence of 0.77, the snares fell on beats 1 and 3, which reads as a
  downbeat a beat off: the backlog's second half.

Nobody has heard any of it, and no agent session has used the command on a song.
