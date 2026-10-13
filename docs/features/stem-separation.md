# Stem separation — stage 1 implemented October 13, 2026

`daw separate` takes a song apart on this Mac, by an open-source model, into
the parts it was mixed from: the vocals and the instrumental, or drums, bass,
vocals and the rest. Each part is written into the project as a sample, which
the agent or the person places, slices and mixes like any other. Stage 1,
the command with vocals and stems, is built (D119); more splits, Separate… in
the app and a reference's stems are Later lines in [the backlog](../backlog.md).

No model is trained here (D4). The models are the ones the Ultimate Vocal
Remover community runs, chosen by what each split needs and what each model's
license allows; nothing is bundled or rehosted.

## Commands

```sh
uv run daw separate PROJECT mix                       # vocals and instrumental, best model
uv run daw separate PROJECT mix --quality fast        # the small, quick model
uv run daw separate PROJECT tracks.song.audio.0 --split stems --tracks
uv run daw separate PROJECT ~/Music/Song.m4a --split stems6 --id song
uv run daw separate models [--split vocals]           # what can run, its size and license, what is here
uv run daw separate setup [--models NAME...] [--all] [--runtime-only]
```

`SOURCE` is a sample's ID in the song, an audio clip as `tracks.T.audio.N`, or
a file by path, which is imported first as `daw samples import` does, `.m4a`
and `.mp3` decoded. `--split vocals` (the default) gives `vocals` and
`instrumental`; `stems` gives `drums`, `bass`, `vocals` and `other`; `stems6`
adds `guitar` and `piano`. `--quality` picks the split's model from the
manifest, `best` unless given; `--model` names one. The parts are samples named
`PREFIX-PART`, `PREFIX` the source's ID unless `--id` gives one.

| Split | `fast` | `best` (default) |
|---|---|---|
| `vocals` | UVR-MDX-NET Inst HQ 4 (59 MB) | Kimberley Jensen's Mel-Band RoFormer (913 MB) |
| `stems` | `htdemucs` (84 MB) | `htdemucs_ft` (336 MB, a bag of four) |
| `stems6` | `htdemucs_6s` (55 MB) | `htdemucs_6s` |

The reply lists the source, the model with its author and license, each part
(`sample_id`, `path`, `frames`, `peak_dbfs`, `integrated_lufs`, whether it was
added now), `residual_db`, the seconds it took, the memory, the runtime's
versions and device, the recipe's path, and once, the first time on this Mac,
the rights line. `daw describe separate` says the same for an agent.

## The parts

Each part is 32-bit float WAV at 44.1 kHz, the models' rate, under
`separated/` in the project, named by the first twelve characters of the
source file's hash, the model and the part. Every part has the source's length
to the frame, so a part placed where the source was plays in time with it and a
beat map of the source is a beat map of each part. A 48 kHz source is resampled
to 44.1 kHz once, by the runner, and its parts stay there; the engine resamples
them as it plays.

The runner gives the model the source with its peak at -6 dBFS and scales each
part back by the same gain, because audio-separator scales any stem that passes
full scale down to it, which would lose the parts' level against the source;
the recipe records the gain. Nothing else is normalized.

**What the parts lose is measured.** `residual_db` is the parts summed against
the source, in dB under it, read by the runner at 44.1 kHz. A low residual
says the parts add up to the song; a high one says the model dropped something.
It does not say a part is clean, and a model whose instrumental is the song
minus its vocals adds up by construction. The command measures; nobody heard
the parts unless the person did.

**Placing is asked for.** `--tracks` adds a track a part, `PREFIX-PART`,
right after the source's track and in its group, each with an audio clip at the
source clip's place with the clip's fields, and the source clip falls silent:
its track is muted when the clip is alone there, or else the clip's `gain_db`
goes to -96, since a clip has no mute of its own. The song plays the same until
a part is changed. From a sample with no clip, each part plays whole from beat 0
and the song grows to the end of the bar it ends in. The samples and the tracks
land as one edit through `daw apply`, so a running host hears it and one undo
takes them out; the files stay.

**The recipe is beside the parts.** `separated/HASH-MODEL.separated.json`
holds the source (sample, path, hash, its rate), the split, the model with its
author and license, the hashes of its weights, the runtime's versions and
device, the parameters, the residual, the seconds and memory, each part's hash
and measurements, and the date. The same source through the same model is found
there and returned at once, with `reused: true`; a part missing is made again.
A model on the GPU is not bit-deterministic, so a remade part can hash
differently from the one the song recorded, and the engine then refuses the song
as a changed asset, as it does for any sample edited in place: the sample has to
be removed from the song (or its `sha256` corrected) before the part is remade.

## The runtime and the models

Separation runs in a Python environment of its own under the app's data folder,
`~/Library/Application Support/AAW/separation/` (or `AAW_DATA_DIR`), not in
this package's dependencies: torch is a gigabyte that no checkout or bundle
should carry. `daw separate setup` makes it with `uv` and installs
[audio-separator](https://github.com/nomadkaraoke/python-audio-separator)
0.47.0, which runs every family the manifest names (MDX-Net as ONNX, Demucs,
RoFormer) on Apple's GPU through MPS and CoreML, with `audioread`, which that
version imports without depending on. The interpreter is an Apple silicon
Python 3.12 that `uv` downloads (`cpython-3.12-macos-aarch64-none`): the
checkout's own `uv` and Python here are Intel builds under Rosetta, and torch
ships no Intel Mac wheels new enough, so an Intel Mac is refused with that
reason. `daw` starts the environment's Python on
`src/agent_daw/separate_runner.py` as a process, as it runs `afconvert` and
`rubberband`, and reads its progress as lines of JSON, which `daw` passes on
as notes on stderr; the last line carries the parts and the measurements. A
process started by a Rosetta parent answers `uname -p` with `i386`, which is
what audio-separator reads to decide between the GPU and the CPU, so the runner
tells it `arm` on an arm64 machine; without that every model ran on the CPU,
Inst HQ 4 in four minutes for four minutes of audio. PyTorch's Metal backend
(2.14) also refuses a one-dimensional convolution whose output is longer than
65536 frames ("Output channels > 65536 not supported at the MPS device"),
which Demucs's first encoder and last decoder layers are on its 7.8 s
segments, so the runner runs a convolution the GPU refuses on the CPU and the
rest of the network stays on the GPU; those two layers have few channels.

The manifest, `src/agent_daw/separation_models.json`, lists each model's name,
family, split, quality, audio-separator file name, parts, author, license and
its address, and each of its files' URL, size and sha256. Weights go to
`models/` beside the environment and are checked against the manifest's hash on
download and before each run. Setup also fetches the three small data files
audio-separator reads at load (`download_checks.json`, `mdx_model_data.json`,
`vr_model_data.json`), which are not hashed since their authors change them.

| Model | Family | Files | Author | License |
|---|---|---|---|---|
| `mdx-inst-hq4` | MDX-Net (ONNX) | `UVR-MDX-NET-Inst_HQ_4.onnx` from TRvlvr/model_repo | Anjok07 and aufr33 (UVR) | MIT by UVR's README, which asks for credit; no license file |
| `kim-mel-roformer` | Mel-Band RoFormer | `vocals_mel_band_roformer.ckpt` and its YAML, from audio-separator's model-configs release (the UVR host has none) | Kimberley Jensen | MIT (model card) |
| `htdemucs`, `htdemucs_ft`, `htdemucs_6s` | Demucs v4 | `.th` weights from dl.fbaipublicfiles.com, YAML from TRvlvr/model_repo | Alexandre Défossez, Meta AI | MIT |

A command that needs the runtime or a model it lacks says what is missing, how
large it is and the command that gets it, and downloads nothing by itself;
`setup` says on stderr what it downloads and how large before it does. Only the
downloads leave the Mac. No audio does; there is no service and no key.

**The song's rights do not change.** Stems of a record are copies of it. The
first reply on a Mac carries the rights line, `daw separate models` repeats it,
and each recipe names its source.

## What it costs on this Mac

Measured on October 13, 2026 on the person's Mac, an Apple M2 with 16 GB on
macOS 14.8, over four minutes of generated stereo audio at 44.1 kHz (a bass
line, a kick and a hat, a chord pad and a harmonic voice with vibrato), the
song not playing and nothing else asked of the Mac. Memory is the runner's
peak resident set; the GPU's share of unified memory is in it. The residual is
of generated audio and says nothing about a record.

| Model | Device | Loading | Separating | Peak memory | Residual |
|---|---|---|---|---|---|
| `mdx-inst-hq4` (vocals, fast) | CoreML | 4 s | 92 s | 4.0 GB | -41 dB |
| `kim-mel-roformer` (vocals, best) | MPS | 4 s | 199 s | 1.5 GB | -149 dB |
| `htdemucs` (stems, fast) | MPS | 2 s | 51 s | 1.4 GB | -20 dB |
| `htdemucs_ft` (stems, best) | MPS | 2 s | 269 s | 1.7 GB | -17 dB |
| `htdemucs_6s` (stems6) | MPS | 2 s | 81 s | 1.3 GB | -15 dB |

On the CPU alone, before the runner told audio-separator the processor is
`arm`, Inst HQ 4 took 240 s and `htdemucs` 260 s for the same four minutes,
at 2.3 and 2.8 GB. The RoFormer's residual says its instrumental is the song
minus its vocals; Inst HQ 4's is the model's own instrumental, with the vocals
as the song minus it times UVR's compensation, so the two nearly add up; Demucs
gives four parts of its own, which lose 15 to 20 dB of the song summed.

The environment is 1.0 GB and the five models 1.45 GB (the four defaults
1.39 GB). Each part of a four-minute stereo song is 42 MB.

## Tests

`tests/test_separation.py` runs `daw separate` over a stand-in,
`tests/stand_in_separator.py`, named by `AAW_SEPARATOR`: the runner's arguments
and lines, with the parts cut from a generated mix of a sine bass, a noise vocal
and clicks by frequency band, so that they sum to it. The tests hold the parts'
format, length and alignment, the residual, the recipe and its reuse, a part
remade when its file is gone, a file imported first, a 48 kHz source's parts at
44.1 kHz, `--tracks` from a clip alone on its track and among others, from a
sample with the song grown, and in a group, the render unchanged by the parts
on tracks under the muted source, the one edit undone under a host, the
refusals when the runtime or a model is missing or a file does not match the
manifest, the manifest's fields, and `daw describe separate`. One test runs
`htdemucs` over eight seconds of generated audio through the real runtime when
`AAW_SEPARATE_REAL` is set and the runtime is installed; no test downloads a
model. With `AAW_SEPARATOR` set no model file is required.

## Limits

- An Apple silicon Mac on macOS 14 or later. An Intel Mac is refused: torch
  ships nothing new enough for it.
- The parts are at 44.1 kHz whatever the song's rate.
- The runner's lines say the stage, not how far along the model is; a bar of
  progress, which the app will want, is a Later line.
- The model takes the GPU and every core while it runs. Whether the audio
  thread drops out while a song plays was not tried; separate while the song
  is stopped.
- No lead from backing vocals, drum kit pieces, de-reverb or crowd models; no
  model without a license or with a non-commercial one is in the manifest;
  Separate… in the app and a reference's stems are not built. Each is a Later
  line.
- Nobody has heard a part. The defaults were chosen from the community's
  scores; `fast` against `best` on a record is the first thing to listen to.

## Open questions

1. **Which model for `best` vocals.** Kim Mel-RoFormer is MIT and among the
   best scored, at 913 MB and the longest wait; whether `fast` is good enough
   that `best` is rarely worth it is for the person's ears.
2. **The instrumental as a model's output or as the song minus the vocals.**
   Inst HQ 4 gives the instrumental and subtracts for the vocals; Kim's model
   gives the vocals and audio-separator subtracts for the rest. Which sounds
   better is for ears, and either can be offered.
3. **UVR's weights.** Its README says MIT and asks for credit; its repository
   and the weights' host have no license file. Whether to ask the authors, and
   whether `fast` vocals should be an MIT model instead.
4. **Places in the song.** Whether `--tracks` should be the default in the
   app, a group holding the parts, and where the source clip goes.
5. **Separating while the song plays.** Not tried.
6. **UVR 6**, due by late November 2026: whether it brings new permissively
   licensed models worth adding to the manifest.
