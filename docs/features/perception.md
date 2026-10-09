# Perception — implemented September 22, 2026

The perception layer measures saved audio and reads the arrangement snapshot that
produced it. Reports are diagnostics for an agent and a person to use while revising
music. They do not establish musical quality, diagnose masking, or constitute a
listening audition: where two stems overlap is measured, as
[overlap](masking.md), and is not masking as an ear does it. All computation is
local.

## Commands

```sh
uv run daw listen projects/my-beat/renders/latest.json
uv run daw listen projects/my-beat/renders/latest-preview.json --no-images
uv run daw listen projects/my-beat/renders/latest.json --overlap kick bass
uv run daw listen projects/my-beat/renders/latest.json --section drop
uv run daw listen projects/my-beat/renders/latest.json --write-translation
uv run daw compare projects/my-beat/revisions/before-render.json projects/my-beat/renders/latest.json
```

These are generic paths for a local project. Before revising it, save a copy of
`renders/latest.json` as `revisions/before-render.json` and retain the render
directory it references. After rendering the revision, compare that saved pointer
with the new `latest.json`. Project audio and generated analysis stay local.

Both commands accept a render directory, its `report.json`, its `mix.wav`, or a
render pointer containing `directory`. Standalone audio files also work, without
project sections, stems or musical context. Supported audio is nonempty, finite,
mono/stereo at 44.1 or 48 kHz, decoded by SoundFile. Missing or invalid artifacts
produce JSON errors and a nonzero exit code.

`listen` prints a JSON report and saves it under `analysis/<identity>/listen.json`
inside the render directory (beside the audio for standalone files). It generates
`overview.png` with energy, section markers and a log-frequency spectrogram, and
`spectrum.png` with the mix's third octaves over its stems'.
`compare` saves both listen reports and a `compare.json` under the after report's
analysis directory; aligned timelines also produce `comparison.png`. Paths are
returned in `report_path` and `images`. Use `--no-images` to omit PNG generation.
These commands never modify the audio, project, render report or latest pointers.
`--write-translation` adds two files of its own beside the report and changes none.
Analysis reports record dependencies, analyzer code hash and source hashes.

A render of two stems or more also has `overlap`: the pairs of stems that put
their energy in the same band at the same moments, with the band, how much of
the time and where. `daw listen RENDER --overlap A B` prints one pair by band
and by section in place of the report, with a picture, and `daw compare` has
each pair's change. The fields and the method are in [masking.md](masking.md).

Every measurement also has the finer `spectrum_db`, its power in 31 third
octaves, and `tilt_db_per_octave`, and each stem has `resonances`: narrow peaks
that stay where they are while the notes move, each a frequency, a height and a
`q`. A section's third octaves and resonances are written and not printed;
`daw listen RENDER --section ID` prints one section's in place of the report.
`daw compare` has their change. The fields and the method are in
[spectrum-detail.md](spectrum-detail.md).

The report also has `translation`: what the mix, each of its sections and each
stem loses summed to mono, over all frequencies and in each of the seven
bands, and through the band a small speaker plays, with the largest losses in
words. `daw listen RENDER --write-translation` writes `mono.wav` and
`small-speaker.wav` beside the report, the one audio these commands write, and
`daw compare` has each loss before and after. The fields and the method are in
[translation-checks.md](translation-checks.md).

## Measurements

The `mix`, each section's `audio`, and each track's `audio` and `sections` contain:

- `integrated_lufs`: pyloudnorm's BS.1770-4 K-weighting with 400 ms absolute and
  relative gating. Audio below the gate or shorter than 400 ms returns `null`.
- `peak_dbfs`, `rms_dbfs`, `crest_db`: sample peak, mean-square power averaged
  across channels, and peak minus RMS. The dBFS reference is unit sample amplitude;
  a unit-peak sine has RMS around -3.01 dBFS.
- `estimated_true_peak_dbtp`: 4× polyphase oversampling estimate, not a certified
  true-peak measurement.
- `band_dbfs`: Welch spectral power integrated in the ranges below. It uses Hann
  windows of up to 8192 frames with 50% overlap, averaging channel powers rather
  than downmixing the audio. Resolution degrades for very short audio. Welch's
  constant detrending excludes DC from this spectral description.
- `band_fraction`: each band's share of total measured 20 Hz–20 kHz band power.
- `spectrum_db`, `tilt_db_per_octave`: the power in each third octave from a
  spectrum of its own, with windows of up to 32 768 frames, and the slope of a
  line through them; see [spectrum-detail.md](spectrum-detail.md).
- `stereo_correlation`: Pearson correlation between channels; `null` for mono,
  silence, or a channel without enough variance to define correlation.
- `side_energy_fraction`: side/(mid+side) power, where mid=(L+R)/2 and side=(L−R)/2.
  Identical channels give 0; equal antiphase channels give 1. This describes stereo
  energy, not perceived width or a universal target. Mono/silence return `null`.
- `silent`: all samples at or below -120 dBFS peak. `over_range_samples` counts
  channel samples with absolute amplitude at least 1; it is not a clipping diagnosis.

| Band | Hz, lower inclusive / upper exclusive |
|---|---|
| sub | 20–60 |
| low | 60–250 |
| low_mid | 250–500 |
| mid | 500–2000 |
| high_mid | 2000–4000 |
| high | 4000–8000 |
| air | 8000–20000 |

`timeline.energy` contains one RMS value per quarter-note beat, or per second for
standalone audio. The first/last bin can be partial. `timeline.short_term_loudness`
contains ungated K-weighted 3-second windows at 1-second hops; it is empty for audio
shorter than 3 seconds. All timeline seconds are absolute project seconds for
renders, including previews. Beats are zero-based quarter notes. Undefined dB
values are `null`, never an invented floor; only the overview displays silence at
-120 dBFS. The spectrogram has a fixed -100 to -20 dB/Hz color range for comparability.

## Snapshot and artifact integrity

The mix hash and project snapshot hash must agree with the render manifest. The
project hash covers the saved form, so fields at their defaults do not count and a
new optional field leaves it unchanged. Reports from earlier engines hashed the full
model and are verified in that form too. Original
sample files and the current editable project are not required. Preview section
ranges intersect the original project timeline, preserving the preview's original
offset; audio measurements include any tails in the rendered region.

Return buses render as stems too. Each entry under `tracks` carries the render's
`kind`, `track` or `return`; returns have no `musical_context` because they
schedule no triggers.

Stems must match the mix's frame count, channel count and sample rate. New renders
record stem hashes, which are verified before analysis. Older render reports lack
these hashes; analysis still records each actual stem hash, with
`hash_verified_against_render: false`. Such stems cannot be authenticated against
the original render manifest. The earliest reports also lack `render_id` and
`target`. They predate previews, so they are read as full renders, which the
timeline check confirms, and `source.render_id` is null.

`musical_context` is separately labeled as **project-derived**. It reports scheduled
trigger counts, triggers per beat and pattern occurrences for each track over the
whole render and named sections. A trigger belongs to the region containing its
start; a repeated pattern occurrence counts if it overlaps the region. Muted/soloed
tracks retain scheduled counts and an `audible_in_render` flag. These are not audio
onset detection, repetition recognition, or a judgment about musical coherence.
Different pattern IDs can contain identical music; repeated IDs can sound different
under track/clip settings.

## Comparisons

Every delta is **after minus before**. Positive level deltas mean the after render
has more measured level. `actual_delta` contains raw metric changes.

`matching.after_gain_db` is before integrated LUFS minus after integrated LUFS.
This single global offset is applied analytically to the after render's peak, RMS
and band levels, including its tracks, sections and energy bins. It preserves
relative mix changes: a louder bass is not normalized away by independently
matching that track. No gain is applied to saved audio and no A/B audio is exported.
`loudness_matched_delta` omits gated integrated LUFS because absolute gating can
change when gain changes. Gain-invariant metrics (crest, correlation, band shares,
side fraction) are available in `actual_delta`. When either mix's integrated
loudness is undefined, the matching gain and matched values are `null`.

Whole-file mix and common-track aggregates are always compared descriptively, even
when durations differ. `timeline_aligned` requires matching sample rate, frame
count, start seconds and tempo; only then are energy-bin deltas and the comparison
chart produced. Named sections must have both the same ID and the same absolute
start/end seconds. Unmatched/moved sections and added/removed tracks are listed
explicitly. This is not automatic reference-song alignment or tempo warping:
a mix against another song is `daw compare RENDER --reference NAME`, in
[reference-comparison.md](reference-comparison.md).
`musical_context_delta` provides trigger count/density changes and before/after
pattern occurrence maps for aligned regions.

## Boundaries and verification

It does not infer taste, key, tempo or transients, overlap between stems
is not masking as an ear does it, a resonance is a measured peak, not a
fault, and the small speaker is a filter that stands in for a phone, not a
model of one. It does
not introduce effects or change rendering semantics. Analysis currently rereads
and measures the saved audio each time; there is no incremental analysis cache.
Long songs and many stems take longer than a section preview. One measurement's
spectrum, each channel's oversampled peak and the stereo figures are worked out
on threads beside its loudness, about 1.7 s for three minutes of stereo where
it was 3.5 s, with the same numbers.

Tests cover known 1 kHz loudness, +6 dB gain and matching, band changes, silence,
stereo polarity, one-sided stereo, short files, 44.1/48 kHz, localized arrangement
changes, preview alignment, snapshot independence, artifact tampering, legacy
manifests and CLI JSON/PNG output. Overlap has tests of its own, on generated
songs whose contests are known, and so has the finer spectrum, on noise, a tone
and a Synth's line through a bell, and translation, on channels that are the
same, unrelated and turned over, and on a sine bass.
