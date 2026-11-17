# Sound and effects from audio — implemented November 17, 2026

Item 5 of the reverse-engineer track (D182, D188): how a finished song and each
part `daw separate` made of it sound, read for what a producer sets when mixing
it. `daw samples sound` gives each part's tone as the bands of an `eq` effect,
its width whole and section by section, its level dipping with the kick as a
sidechain ducks it, the decay of its tails as a reverb's and their repeats as a
delay's, and the marks compression leaves. Everything is measured on the Mac
with numpy and scipy; no model is installed.

## Command

```sh
uv run daw samples sound projects/NAME/samples/ID_song.wav
uv run daw samples sound SONG.wav --part vocals --part other --picture /tmp/sound.png
uv run daw samples sound SONG.wav --part bass --against projects/NAME/renders/ID/stems/bass.wav
```

`FILE` is a file or a library sample, as for `daw samples form`, whose beat map
it reads with its corrections and whose sections it takes from the form. With
the parts `daw separate PROJECT FILE --split stems` made of it, found by their
recipe as the form finds them, it reads the song, `mix`, and each part.
`--part NAME`, repeated, reads only those. Without parts it reads the song
alone, and `no_parts` says how to get them.

`--against FILE` is a part made in a session, such as a render's stem: each
part's `eq` is then the bands that turn that file into the part read, and
`dynamics.against` says how far apart their crest, level range, time near the
top and level are. `--picture OUT.png` draws a row a part: its tone against
flat with the fitted eq's curve, its level around the kick with the mix's, and
its width by section, titled with its tail and repeats.

## What is read

1. **Tone.** The part's spectrum in the 31 third octaves `daw listen` uses,
   its tilt, and `plays_hz`, the third octaves within 30 dB of its loudest.
   `eq` is the bands that turn flat, pink noise, into it: a highpass and a
   lowpass where it ends, and shelves and bells within 15 dB inside the range
   it plays in, a high shelf over 500 Hz and a low one under 2 kHz, Q from 0.3
   to 10. Each band is the one from a table of shapes, sixth-octave
   frequencies, widths and gains that most lowers what is left, each band's
   response averaged over the third octave as the spectrum is measured; then
   all the bands are moved together to the least error (least squares), and a
   band moved onto another of its shape is barred and the next tried. It stops
   at eight bands, at an error of 0.5 dB, or when a band gains under 0.1 dB.
   The bands are the engine's own biquads, so an `eq` effect with them makes
   the curve.
2. **Width.** The side's share of the power and the channels' correlation,
   whole, under 250 Hz, to 4 kHz and over, and on each of the form's sections.
3. **Dynamics.** Over the 50 ms stretches within 30 dB of its loudest: the
   crest, the median of each 400 ms's peak over its RMS; the level range from
   the 10th to the 95th percentile; how much of the time it is within 3 dB of
   its 95th; its peak and its level while it sounds, and a part's level against
   the song's. No compressor setting is read from them: on a held part a
   compressor narrows the level range, but on drums a slow attack lets the
   transients through and raises the crest, and a fast one lowers it.
4. **Pumping.** For each part but the drums: its level a millisecond at a
   time, after each kick against the 15 to 60 ms before it, the median over the
   kicks it sounds at. The dip's depth, when it is deepest within 200 ms, and
   the release, from the deepest point until the dip is a factor e shallower,
   as the compressor's `release_ms` is set. The same is read halfway to the
   next kick, where a sidechain does not duck: a dip of 1 dB or more and 1 dB
   deeper than there is heard. The kicks are the drums part's, as `daw
   transcribe --drums` hears them, or the song's kick band. The song itself is
   read the same way over the kick's body, 300 Hz to 2 kHz, under the hats, as
   `kicks.the_mix_dips`, and each heard dip says whether the mix shares it
   (`in_the_mix`): a dip the mix shares is in the record, a sidechain or a bus
   compressor the kick drives; one it does not may be the separation taking
   the kick out of the part.
5. **Tails.** The part's level from 200 Hz to 6 kHz every 10 ms. An end is
   where it falls 6 dB within 150 ms after 300 ms of sounding and stays 3 dB
   under for 400 ms, placed at the fall's steepest step, so a dip between
   syllables is not an end; what follows until the part plays again is its
   tail, and the repeats within it are not ends. Two lines are fitted to each
   tail, the sound stopping and then the tail, the second at least half its
   length, down to 50 dB under the sound or the part's floor; its slope is the
   time to fall 60 dB, a reverb's `decay_seconds`. The median over the ends,
   their middle half and whether that is within a ratio of 1.6 (`consistent`),
   and how far under the sound the tail starts.
6. **Echoes.** A delay plays the last notes again a spacing later, so the
   first half second of the tail is a quieter copy of the level the spacing
   before the end. For each spacing from 0.1 to 2 s, how the level moves step
   to step in each tail is compared with the stretch that spacing earlier, as
   a correlation, weighted over the ends. The spacing they are most alike at is
   the delay's when they are 0.5 alike or more, 0.3 over the typical spacing,
   and the tail is 3 dB quieter or more than what it repeats; a part that plays
   the same figure on at the spacing is not a repeat. Then the first repeat's
   level, how much each repeat falls a spacing later, the `feedback_percent`
   that makes, and the nearest division of the beat.

## Checked on renders with known settings

Each estimate was read back from renders the engine made through one effect
with known settings, and the tests in `tests/test_sound.py` keep them:

| Effect | Set | Read |
|---|---|---|
| reverb, 30% wet, on phrases | decay 0.6, 1.5, 3.0 s | 0.62, 1.65, 3.16 s |
| delay, 30% wet | 1/8 at 30%, dotted 1/8 at 40%, 1/4 at 50% | the division each time; 30%, 39%, 50%; first repeat −6.8 to −7.3 dB (−7.4 set) |
| sidechain compressor, ratio 8 | −30 dB, 100 ms; −24, 200; −12, 60 | depth 22.8, 16.9, 7.9 dB against 22.8, 18.6, 7.7 done; release 108, 165, 62 ms against 114, 202, 61 |
| eq on pink noise | highpass 120 Hz/24, bell 300 Hz −4 dB Q 2, high shelf 6 kHz −6 dB | highpass 121, bell 306 −4.2 Q 2.13, high shelf 6022 −6.0; the curve within 0.3 dB |
| none | a dry part, a pad no kick ducks | no tail, no repeats, no pumping |

The depth and release done are the render against its input, the median after
each kick. A 200 ms release reads short because the dip has not come all the
way back before the next kick, half a second on.

## On two records

On two pop records split with htdemucs_ft, nobody listening along: one at 124
BPM gave a voice whose tails fall in about 1.5 s with their ends disagreeing,
the voice and the rest of the band dipping about 8 dB 20 to 40 ms after each
kick and back by 150 ms, and the mix itself dipping 4 dB over the kick's body,
so the duck is in the record; a bass in mono and the rest wide (side share
0.39). One at 171 BPM gave a bass and the rest of the band dipping 6 to 7 dB
with the kick, the mix 3 dB, a voice that does not dip, and synths very wide
(side share 0.44, correlation 0.12). A full mix's tails disagree widely:
a mix seldom stops.

## Limits

- A ringing sound, a cymbal or a held piano, reads as a tail.
- Echoes are looked for from 0.1 s, past the sound's own fall; a slap shorter
  than that is not found. A delay too quiet to be 3 dB under, or fed back so
  little that the tail does not copy the notes, is not either.
- A part separated from a mix carries what the separation left: a dip where
  the separation took the kick out reads as pumping, which `in_the_mix` is
  for.
- Compression is described, not set: the crest and the level range move with
  a compressor's attack as much as its ratio, so a made part is matched to a
  record's with `--against`.
- The eq from flat describes a whole part against pink noise; between a made
  part and a record's, `--against`, the bands are the ones to set.
