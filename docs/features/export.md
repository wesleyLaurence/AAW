# Export — implemented October 1, 2026

`daw export` writes a song's render as a file with a name, in a folder the person
keeps. A render is a cache under `renders/<id>/`; an export is the deliverable.

## Commands

```sh
uv run daw export projects/edit/song.yaml --to projects/edit/exports/Song-Edit-v1.wav
uv run daw export SONG --to exports/NAME.wav --bits 16
uv run daw export SONG --to exports/NAME.m4a                  # AAC, 256 kb/s
uv run daw export SONG --to exports/NAME.mp3 --bitrate 192    # 320 kb/s by default
uv run daw export SONG --to exports/NAME.wav --match song     # as loud as a sample
uv run daw export SONG --to exports/NAME.m4a --peak -1        # true peak at -1 dBTP
uv run daw export SONG --to exports/NAME.wav --replace        # write over NAME.wav
```

The file is made from the song's latest full render. If the song has changed
since, or was never rendered, it is rendered first, and the result says so
(`render.rendered_now`). `--render DIR` exports a render named outright, whatever
the song is now. Only a whole mix is exported, not a track or a section preview.

An existing file is kept unless `--replace` is given.

## Formats

- `.wav`: 24-bit PCM as rendered, or 16-bit with `--bits 16`. Sixteen bits are
  dithered by one step, triangular, from a fixed seed, so the same audio gives the
  same file.
- `.m4a`: AAC, by `afconvert`, which macOS has, or `ffmpeg`.
- `.mp3`: by `ffmpeg` or `lame`. Neither comes with macOS.

The sample rate and channels are the render's. A missing encoder is an error that
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
its duration, the level policy and what it did, and the render it came from
(`render_id`, the song's and the mix's hashes, the engine's). It holds no local
paths, so it can travel with the file.

## Limits

One file per call. No sample rate conversion, no metadata tags and no loudness
normalization beyond one gain. `--match` compares whole files: an edit that keeps
the loud half of a song measures louder than the song.

Tests render a generated song and check each format, the dither, each level
policy, the ceiling, the record and a stale render. Nobody listened to an export.
