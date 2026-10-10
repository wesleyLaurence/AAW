# Stem separation — proposed October 10, 2026

Status: proposed, not built. Backlog: Next, Stem separation. What is said here
about models, licenses and tools was read from their repositories, model cards,
papers and package indexes on October 10, 2026. Nothing was installed or run,
and no time on a Mac was measured; every speed below is an estimate.

## What

A song taken apart into the parts it was mixed from, on this Mac, by an
open-source model: the vocals removed for an instrumental, the vocals alone for
an a cappella, or the stems of drums, bass, vocals and the rest. Each part is
written into the project as a sample, which the agent or the person places,
slices and mixes like any other. The agent runs it as a `daw` command; the
person runs it from a sample or an audio clip in the app.

No model is trained here (D4). The models are the ones the Ultimate Vocal
Remover community runs, chosen by what each split needs and what each model's
license allows.

## Why

A finished song is the material a person most often wants to work with and
the one this app can do least with today. A whole song can be imported, mapped
by `daw samples beats` and cut on its downbeats, but every cut carries the
whole mix. Separation opens what a person does with a record:

- **A vocal over a new beat:** the a cappella imported, mapped and put over a
  song made here.
- **An instrumental:** the song without its voice, to sing or rap over, or to
  sample a bar where the voice was in the way.
- **A part to learn from or to sample:** the drums of a break, sliced into
  pads (Slicing a break, in Next); the bass line, to read its notes (Key and
  chord detection, in Next).
- **A reference by its parts:** [the reference comparison](reference-comparison.md)
  says a reference has no stems. With them, the agent can hold a mix's kick
  and bass to a record's, and [overlap](masking.md) can read a record's pairs.

The person asked for it by name: "take an audio track and remove the vocals or
isolate the vocals into an a cappella, or look into the stems of a song."

## Stages

1. **`daw separate`, vocals and four stems.** From the command line, with a
   small set of permissively licensed models downloaded on first use.
2. **More splits.** Six stems, lead and backing vocals, a drum kit by its
   pieces, and reverb taken off a vocal; community models the person chooses
   to download, with each license shown.
3. **Separate in the app.** A sample in the browser or an audio clip on the
   timeline separated from its menu, with progress, and the stems laid on
   tracks under it.
4. **A reference's stems.** `daw reference add --stems`, so `daw compare` can
   compare track to track.

Each is a backlog line and a pull request of its own when it is picked up.

## What exists

### Ultimate Vocal Remover

[Ultimate Vocal Remover](https://github.com/Anjok07/ultimatevocalremovergui)
(UVR) is a desktop program, not a library: a Tkinter window for Windows, macOS
and Linux, version 5.6, with no command line. Its README says the code is MIT,
though the repository has no LICENSE file. It runs several families of model
and downloads their weights from GitHub releases of
[TRvlvr/model_repo](https://github.com/TRvlvr/model_repo), which states no
license either. A rewrite, UVR 6, was announced on October 6, 2026
([issue 2399](https://github.com/Anjok07/ultimatevocalremovergui/issues/2399))
for within six weeks; it does not say whether it has a command line.

What this app takes from UVR is its models and the community's knowledge of
them, not its program.

### The models

| Family | What it is good for | Size | License of the weights |
|---|---|---|---|
| **Demucs v4** (`htdemucs`, `htdemucs_ft`, `htdemucs_6s`) | Four stems; six with guitar and piano | 84 MB; 336 MB for `_ft`, a bag of four; 55 MB for `_6s` | MIT, by the repository; no separate file |
| **MDX-Net** (`Kim_Vocal_2`, `UVR-MDX-NET-Inst_HQ_3` to `_5`, `KARA_2`) | Vocals and instrumental, small and fast, as ONNX | 53 to 67 MB | "MIT, credit UVR" in UVR's README; no file |
| **MDX23C** (`MDX23C-8KFFT-InstVoc_HQ`) | Vocals and instrumental, better than MDX-Net | 448 MB | As MDX-Net |
| **Mel-Band and BS-RoFormer** (community checkpoints) | The best vocals and instrumentals that can be downloaded | 200 MB to 1.7 GB, most 640 to 913 MB | Varies; see below |
| **VR** (`HP-Karaoke`, `DeEcho-DeReverb`) | Lead from backing vocals, reverb and echo off a vocal | about 127 MB | As MDX-Net |
| **SCNet** | Four stems, faster than Demucs at about its quality | — | Code MIT; few public weights |

How good each is, as signal-to-distortion ratio in dB (higher is better).
The numbers are from different test sets, so compare within a column, not across:

| Model | Vocals | Instrumental | Drums | Bass | Source |
|---|---|---|---|---|---|
| Kim Mel-RoFormer | 12.60 | — | — | — | audio-separator's scores |
| viperx BS-RoFormer 1297 | 11.77 | 16.45 | — | — | audio-separator's scores |
| unwa BS-Roformer Leap Xe | 11.79 | 18.10 | — | — | MVSEP multisong board |
| MDX23C InstVoc HQ | 10.56 | 15.83 | — | — | audio-separator's scores |
| Kim_Vocal_2 (MDX-Net) | 10.18 | 15.36 | — | — | audio-separator's scores |
| UVR-MDX-NET-Inst_HQ_4 | — | 15.51 | — | — | audio-separator's scores |
| htdemucs_ft | 10.79 | — | 10.02 | 12.02 | audio-separator's scores |
| htdemucs_6s | 9.57 | — | 8.47 | 10.10 | audio-separator's scores |
| BS-RoFormer SW (six stems) | 11.30 | — | 14.11 | 14.62 | MVSEP multisong board |

So the ranking the community uses holds: for vocals and instrumental,
RoFormer before MDX23C before MDX-Net before Demucs; for four stems,
BS-RoFormer SW before `htdemucs_ft` before `htdemucs`; for six, BS-RoFormer
SW well before `htdemucs_6s`, whose piano its own README calls full of bleed.
SDR is not everything: the community also scores "fullness" against
"bleedless", since a full instrumental keeps a ghost of the voice and a clean
one sounds thinner. That choice is for ears.

The papers: [BS-RoFormer](https://arxiv.org/abs/2309.02612),
[Mel-RoFormer](https://arxiv.org/abs/2310.01809),
[SCNet](https://arxiv.org/abs/2401.13276). ByteDance released no weights; every
RoFormer checkpoint is a community one, most trained with
[ZFTurbo's Music-Source-Separation-Training](https://github.com/ZFTurbo/Music-Source-Separation-Training) (MIT).
The live board is [MVSEP's quality checker](https://mvsep.com/quality_checker/multisong_leaderboard?sort=vocals);
its best entries are MVSEP's own models, which cannot be downloaded.

### Licenses

Code and weights are licensed apart, and most of the best weights have no
license at all, which by default reserves every right.

| License | Weights |
|---|---|
| MIT or Apache | Demucs v4; Kim Mel-RoFormer ([model card](https://huggingface.co/KimberleyJSN/melbandroformer)); SYH99999's fine-tunes and four-stem RoFormers |
| MIT by statement, no file | UVR's own MDX-Net, MDX23C and VR models |
| GPL-3.0 | anvuew's de-reverb and karaoke RoFormers |
| Non-commercial (CC BY-NC, BY-NC-SA) | becruily's "deux" and "delimit"; Sucial's de-reverb, echo and chorus models |
| None stated | viperx's RoFormers; most of unwa's, Gabox's and becruily's; BS-RoFormer SW, whose first host is gone and whose rehost says "license: unknown" |

This app is free and open source and ships from a public repository. So:

- **Nothing is bundled or rehosted.** Weights are downloaded on the person's
  Mac from where their authors put them, the way UVR does. Only a manifest is
  committed: each model's name, address, size, hash, author and license.
- **The default models are MIT or Apache:** Demucs and Kim Mel-RoFormer. UVR's
  MDX-Net models are used with the credit its README asks for, and the missing
  file is an open question.
- **Any other model is the person's choice.** A model with no license, or a
  non-commercial or GPL one, is listed with its license and author and
  downloaded only when the person asks for it by name.

**The song's rights do not change.** Stems of a record are copies of it. To
take apart a song for practice, study or a private remix is what people use
UVR for; to publish or sell stems or a remix needs the rights to the song. The
command and the panel say once, the first time, that the person answers for
the audio they separate, and the recipe beside each stem names its source.

### The tools that run them

| Tool | What it is | License | For this app |
|---|---|---|---|
| [audio-separator](https://github.com/nomadkaraoke/python-audio-separator) 0.47.0 | A command and Python API running every UVR family: MDX (ONNX), VR, Demucs, MDX23C and RoFormer; ensembles and presets; MPS and CoreML on Apple silicon | MIT | One dependency for every model; a large set of packages; its default cache is `/tmp`; it rehosts some community checkpoints itself |
| [Demucs](https://github.com/adefossez/demucs) 4.1.0 | The maintained fork; weights on Hugging Face as safetensors; `uvx demucs song.mp3` works alone | MIT | Demucs only, small and steady |
| [demucs-mlx](https://github.com/andrade0/demucs-mlx) | HTDemucs on Apple's MLX, 3 minutes of audio in 5.3 s on an M4 Max, within 1 ppm of PyTorch | MIT | Fast; still needs torch for STFT and loading |
| [demucs-rs](https://github.com/nikhilunni/demucs-rs) | HTDemucs in Rust on Burn and wgpu, Metal on a Mac | Apache-2.0 | No Python at all, and young: about 150 stars and no benchmarks |
| MLX and CoreML conversions of Mel-RoFormer | e.g. `mlx-community/mel-roformer-zfturbo-vocals-v1-mlx` | MIT, per their cards | A vocal model without torch; unproven |

On Apple silicon the gap is complex numbers: PyTorch's Metal backend lacks some
FFT operations, so Demucs and audio-separator run the STFT on the CPU and the
network on the GPU. audio-separator needs torch 2.13 or later, which needs
macOS 14.

### What it costs

- **Disk.** torch is about 524 MB installed and onnxruntime 78 MB; a whole
  audio-separator environment is likely about 1 GB. The default models add
  about 1.4 GB (`htdemucs_ft` 336, Kim Mel-RoFormer 913, `htdemucs` 84,
  `Inst_HQ_4` 59). Each stem of a four-minute song is about 85 MB as float WAV.
- **Memory.** Demucs needs 3 GB and about 7 GB at its defaults; a large
  RoFormer likely 4 to 10 GB, less with smaller chunks or half precision. An
  8 GB Mac is marginal for the large models.
- **Time** for a four-minute song, estimated: `htdemucs` tens of seconds on
  the GPU, about six minutes on the CPU alone; `htdemucs_ft` about four times
  `htdemucs`; a large RoFormer one to five minutes on a base M1 or M2. The
  first stage measures them.

## Design

**One command, a split and a quality.**

```
daw separate PROJECT SOURCE [--split vocals|stems|stems6] [--quality fast|best]
                            [--model NAME] [--id NAME] [--tracks]
daw separate models [--split vocals]     # what can run, its size, license, and whether it is here
daw separate setup                       # install the runtime and the default models
```

`SOURCE` is a sample ID in the song, an audio clip, or a file by path, which is
imported first as `daw samples import` does. `--split vocals` (the default)
gives `vocals` and `instrumental`; `stems` gives `drums`, `bass`, `vocals` and
`other`; `stems6` adds `guitar` and `piano`. `--quality` picks the model from
the manifest; `--model` names one. The reply lists each new sample's ID, path
and length, the model and its license, and the seconds it took.

| Split | `fast` | `best` (default) |
|---|---|---|
| `vocals` | UVR-MDX-NET-Inst_HQ_4 (59 MB) | Kim Mel-RoFormer (913 MB) |
| `stems` | `htdemucs` (84 MB) | `htdemucs_ft` (336 MB) |
| `stems6` | `htdemucs_6s` (55 MB) | `htdemucs_6s` |

**The parts are samples.** Each is written under `separated/` in the project
as 32-bit float WAV at the model's 44.1 kHz, named by the source's hash and
the part, and added to the song as a sample (`song-vocals`, `song-drums`). It
places nothing unless asked: `--tracks` adds a track a part, each with an
audio clip at the source clip's place and length, and mutes the source clip,
so the song plays the same until a part is changed. With a host running, that
is one undoable step with the origin of whoever asked; undo takes the samples
and tracks out and leaves the files.

**The parts line up with the source.** Every part has the source's length to
the frame, so a part placed where the source was plays in time with it, and a
beat map of the source is a beat map of each part.

**What the parts lose is measured.** The reply gives each part's loudness and
the residual: the parts summed against the source, in dB under it. A low
residual says the parts add up to the song; a high one says the model dropped
something. The agent reads these numbers instead of guessing, and still says
that a measurement is not a listen.

**The recipe is beside it.** `NAME.separated.json` holds the source's hash,
the model, its version, the hash of its weights, its license and author, every
parameter and the date. The same source through the same model and
parameters is found in the project and not separated again. Unlike a
generated sound, a recipe here remakes the same audio, within the noise of the
GPU.

**The runtime lives outside the app and the project.** Separation runs in its
own Python environment, made by `uv` under the app's data folder
(`~/Library/Application Support/AAW/separation/`), not in this package's
dependencies: torch would add a gigabyte to every checkout and to the bundle.
Weights go to `models/` beside it, checked against the manifest's hash. `daw
separate setup` makes both and says how much it will download first; a
command that needs a missing model says how big it is and how to get it, and
downloads nothing unasked. `daw` runs the environment as a separate process,
as it runs `afconvert` and `rubberband`, and reads progress as JSON lines.

**One adapter, so the runner can change.** The process takes a source, a
model from the manifest and an output folder, and writes the parts and their
measurements. Stage 1 implements it with audio-separator, which runs every
family the manifest names. Demucs through MLX or demucs-rs can replace it for
the stem splits later without changing the command.

**What leaves the machine** is a download of a model, from the address in the
manifest, when the person or the agent asks for it. No audio leaves; there is
no service and no key.

**Separate in the app.** A sample in the browser and an audio clip on the
timeline get Separate… in their menus: the split, the quality, and a line with
the model, its size and its license. It runs off the main thread with
progress and can be cancelled. When it ends, the parts are on tracks under the
source, as `--tracks` lays them.

**Tests never download a model.** A stand-in runner on this machine "separates"
a generated mix of a sine bass, a click and a noise vocal by filtering, and
tests the command, the files, the alignment, the residual, the recipe, the
reuse and the undo. One test that runs `htdemucs` on generated audio is
opt-in, for a Mac with the runtime.

## Depends on

- Nothing for stage 1 beyond `uv`, which the checkout already needs.
- An app that stands alone (Shipping) for stage 3 on a Mac without this
  checkout: the app must find `uv`, or carry it, to make the environment.
- A decision when picking it up. D4 names Demucs; using RoFormer and MDX-Net
  models, a runner of another project's, and a license rule for downloaded
  weights is a new entry in [decisions.md](../decisions.md).

## Done when

- `daw separate` of a song puts an a cappella and an instrumental in the
  project as samples a pad or an audio clip plays, with the recipe beside
  them, and `--split stems` puts drums, bass, vocals and other.
- Each part has the source's length to the frame, and the parts on tracks
  with `--tracks` play the song as it was.
- The reply gives each part's loudness, the residual, the model, its license
  and the seconds taken.
- The same command again finds the parts and returns at once.
- With the runtime or a model missing, the command says what is missing, how
  big it is and the command that gets it, and downloads nothing.
- No weights are in the repository; each model in the manifest has its
  license and author; a model with no license downloads only by name.
- The time and memory of each default model on a four-minute song are
  measured on the person's Mac and written here.
- An agent asked for an a cappella of a song, or its drums, makes it and uses
  it.
- A person has done each of the above by hand and heard the parts, `fast`
  against `best`.

## Open questions

1. **The runner.** audio-separator runs every model and brings a large tree of
   packages, a cache in `/tmp` to point elsewhere, and community checkpoints it
   rehosts. Demucs alone plus onnxruntime for MDX-Net is smaller and covers
   stage 1 except Kim Mel-RoFormer. A spike installs each and runs a song.
2. **No Python at all.** demucs-rs runs the stem splits from Rust on Metal and
   would suit an app that stands alone; it is young. Whether to try it once
   stage 1 works, and whether a RoFormer in MLX or CoreML can do the vocals.
3. **UVR's weights.** Its README says MIT and asks for credit; its repository
   and the weights' host have no license file. Whether to ask the authors,
   and whether `fast` vocals should be an MIT model instead.
4. **Models with no license.** The best instrumentals (unwa, Gabox, becruily)
   and the best six stems (BS-RoFormer SW) have none. Downloaded by name with
   the license shown, as drawn here, or left out of the manifest.
5. **The instrumental as a model's output or as the song minus the vocals.**
   An instrumental model is fuller; the subtraction always adds up to the
   song. Which sounds better is for ears, and either can be offered.
6. **Which model for `best` vocals.** Kim Mel-RoFormer is MIT and among the
   best, at 913 MB and minutes a song. Whether `fast` is good enough that `best`
   is rarely worth the wait.
7. **Separating while the song plays.** The model takes the GPU and every
   core. Whether the audio thread drops out, and whether separation should
   wait for the transport to stop or run at a lower priority.
8. **The time on this Mac.** Every number under What it costs is an estimate.
   The first stage measures the default models on the person's Mac, and an
   8 GB Mac if one is near.
9. **Places in the song.** Whether `--tracks` should be the default in the
   app, a group holding the parts, and where the source clip goes.
10. **The sample rate.** The models work at 44.1 kHz. A 48 kHz song resamples
    the parts as it plays; whether to resample them back at write time.
11. **Further splits.** Lead from backing vocals, a drum kit by its pieces
    (DrumSep), reverb and echo off a vocal, crowd noise: each has models, most
    with no license or a non-commercial one. Stage 2 picks them.
12. **UVR 6.** Due by late November 2026. Whether it brings a command line or
    new permissively licensed models worth adding to the manifest.
