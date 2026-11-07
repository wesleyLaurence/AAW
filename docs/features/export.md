# Export — implemented October 1, 2026; ranges, rates and formats November 7, 2026

`daw export` writes what is heard of a song, the whole of it or a range, as a
file with a name in the project's `exports/` folder. A render is a cache under
`renders/<id>/`; an export is the deliverable.

## Commands

```sh
uv run daw export projects/edit/song.yaml                     # exports/TITLE.wav, the whole song
uv run daw export SONG --to "Edit v1.wav"                     # a name alone goes in exports/
uv run daw export SONG --to Edit.wav --bars 2-35              # bars 2 to 35, both included
uv run daw export SONG --to Edit.wav --start 2:1 --end 36:1   # the same, as places
uv run daw export SONG --to Edit.wav --start 4 --end 140      # the same, in beats
uv run daw export SONG --to Drop.wav --section drop           # one section
uv run daw export SONG --to Edit.wav --bits 16 --sample-rate 44100
uv run daw export SONG --to Edit.aiff --bits 32               # 32-bit float
uv run daw export SONG --to Edit.flac                         # 24-bit FLAC
uv run daw export SONG --to Edit.m4a                          # AAC, 256 kb/s
uv run daw export SONG --to Edit.mp3 --bitrate 192            # 320 kb/s by default
uv run daw export SONG --to Edit.wav --match song             # as loud as a sample
uv run daw export SONG --to Edit.m4a --peak -1                # true peak at -1 dBTP
uv run daw export SONG --to Edit.wav --replace                # write over Edit.wav
uv run daw export SONG --to ~/Desktop/Edit.wav                # a path goes where it says
```

The file is cut from the song's latest full render. If the song has changed
since, or was never rendered, it is rendered first, and the result says so
(`render.rendered_now`). `--render DIR` exports a render named outright,
whatever the song is now, whole. A track or a section preview is not exported.

**Where.** With no `--to` the file is `exports/TITLE.wav` in the project's
folder: the session's title, or the folder's name while the title is still
`Untitled`. A name alone, `--to Edit.mp3`, goes in `exports/` too, made if it
is not there. A path with a folder in it goes where it says, from where the
command runs. An existing file is kept unless `--replace` is given.

## What is heard

The render is the mix as it plays: a muted track or group, one silenced by
another's solo, and a deactivated clip are not in it, though they still hold
the song's length. So a song whose muted parts run to bar 75 exports bars 2 to
35 with `--bars 2-35`, and nothing of what is muted. A range with nothing
heard in it is refused, naming the range.

## Range

The whole song unless one of these is given:

- `--start PLACE` and `--end PLACE`, either or both. A place is beats from the
  song's start, `66` or `16.5`, or `BAR:BEAT` counted from 1 in the song's
  meter, `17:1`, where the beat is the note the meter counts, an eighth in 6/8.
  `--end` is where the file stops, so `--start 2:1 --end 36:1` is bars 2 to 35.
- `--bars A-B`: whole bars, both included; `--bars 7` is one bar.
- `--section ID`: one section of the song.

The range is cut from the full render at its frames, so a reverb or a note from
before the start sounds in it as it does in the song, and whatever rings past
the end is cut there, as at the song's end. A range may not pass the song's end
(nothing is rendered there) and is refused with where the song ends. The level
policies below measure the range, not the song. The result's and the record's
`range` give `whole_song`, the start and end as beats, as `BAR:BEAT` and in
seconds of the song.

## Formats

- `.wav` and `.aiff`: 24-bit PCM, or `--bits 16`, or `--bits 32` for float.
- `.flac`: 24-bit, or `--bits 16`.
- Sixteen bits are dithered by one step, triangular, from a fixed seed, so the
  same audio gives the same file.
- `.m4a`: AAC, by `afconvert`, which macOS has, or `ffmpeg`; 256 kb/s unless
  `--bitrate`.
- `.mp3`: by `ffmpeg` or `lame`, neither of which comes with macOS; 320 kb/s
  unless `--bitrate`.

The channels are the render's. The sample rate is the song's unless
`--sample-rate HZ` resamples the range by a polyphase filter (scipy's
`resample_poly`), before the level is measured and set. AAC and MP3 are at most
48000 Hz: a song above that is written at 44100 or 48000, the one its rate
divides, and a higher `--sample-rate` with them is refused. A peak that
resampling pushes past full scale is clipped in a 16- or 24-bit file and
warned of; a 32-bit float file keeps it. A missing encoder is an error that
names what to install.

## Level

A render refuses to clip, and a mastered song is already at full scale, so an edit
with a one-shot over it needs room: a lower `master_gain_db`, or a master
`limiter`, which keeps the song at its level and takes only the overlaps down.
That is the song's work, and so is a true peak: a master limiter with
`true_peak: true` holds it at its `ceiling_db`
([true-peak-limiter.md](true-peak-limiter.md)), where `--peak` turns the
whole file down to reach one. The export changes the level by one gain for the whole
file, or not at all:

- No option: as rendered.
- `--gain DB`: that gain.
- `--peak DBTP`: the gain that puts the estimated true peak there.
- `--lufs LUFS`: the gain that reaches that integrated loudness.
- `--match SAMPLE`: the gain that makes the file as loud as that sample of the
  song, measured over the whole sample file.

A gain is held under `--ceiling`, a sample peak in dBFS, -0.1 by default. The
result's `level` says which policy applied, its `target`, the `gain_db` applied,
how much was `held_back_db` by the ceiling, and the loudness, peak and true peak
of the file and of the render it came from. A warning says when the level asked
for needed more than the ceiling allowed, and when a compressed file's true peak
is above -1 dBTP, where a decoder can clip what the encoder overshoots.

The levels are of the audio given to the encoder. A compressed file is not
decoded and measured again.

## Record

Beside `NAME.EXT` the export writes `NAME.EXT.json`: the file's hash and format,
its sample rate (and `resampled_from` when changed), its duration, its range,
the level policy and what it did, and the render it came from (`render_id`, the
song's and the mix's hashes, the engine's). It holds no local paths, so it can
travel with the file.

## Limits

One file per call; one range per file. No metadata tags and no loudness
normalization beyond one gain. A range never takes a tail past its end. `--match`
measures the whole sample file against the range: an edit that keeps the loud
half of a song measures louder than the song. `--render` takes no range, since
a range is read against the song as it is now.

Tests render a generated song and check each format and depth, the dither, a
change of rate, each kind of range against the render's frames, a muted track
left out, each level policy, the ceiling, the record and a stale render. Nobody
listened to an export.
