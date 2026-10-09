# AAW docs

For a task, start with [AGENTS.md](../AGENTS.md): it routes music work and
development separately. The [project introduction](../README.md) covers setup.
[development.md](development.md) says how these files are kept.

| File | Holds | Tense |
|---|---|---|
| [music.md](music.md) | How to operate AAW; references loaded only for the current musical task | Do |
| [development.md](development.md) | How to change, test and document the software | Do |
| [concept.md](concept.md) | What AAW should be: the mission, the principles, everything the app does | Should |
| [backlog.md](backlog.md) | What is not built yet, in order, with an inbox for ideas | Next |
| [features/](features/) | One file per feature: its design before it is built, its reference after | Will, then is |
| [architecture.md](architecture.md) | How it is built: the parts, the song format, the semantics | Is |
| [decisions.md](decisions.md) | Why each choice was made, numbered and never rewritten | Because |
| [completed.md](completed.md) | What was built, when, and what a person tried | Was |
| [archive/](archive/) | Superseded documents, kept for history | — |

More exact than any of these, for what exists: `daw describe` prints the song's
schema and the authoring contract from the code, and
[../engine/README.md](../engine/README.md) and
[../apps/mac/README.md](../apps/mac/README.md) sit beside the code they describe.

## Features

Built:

| Feature | Covers |
|---|---|
| [metronome](features/metronome.md) | Playback monitoring click and its transport toggle |
| [manual BPM](features/tempo-control.md) | Editable session tempo in the transport bar |
| [effects](features/effects.md) | Insert effects on tracks, returns and the master, sidechain compression, the limiter, delay, reverb, and sends to returns |
| [automation](features/automation.md) | Lanes for levels, pans, sends and effect parameters |
| [perception](features/perception.md) | `daw listen` and `daw compare` |
| [sample analysis](features/sample-analysis.md) | `daw samples analyze`, measured search filters, root-note warnings |
| [sound descriptors](features/sound-descriptors.md) | What a sample sounds like in seven numbers and five words, `daw samples search` sorted and filtered by them, `daw samples like`, and a category from the audio |
| [beat map](features/beat-map.md) | `daw samples beats`: a song's tempo, beats, downbeats and phrases |
| [audio clips](features/audio-clips.md) | Parts of a file on a track, and the `daw audio` commands |
| [audio clips in the app](features/audio-clips-in-app.md) | A file dropped on the timeline, a clip moved, trimmed, faded and split there, the song growing to hold it, and the beat map on its waveform |
| [time stretch](features/time-stretch.md) | A pad or audio clip that follows the tempo at its own pitch |
| [timeline](features/timeline.md) | `daw timeline`: beats and seconds, end alignment, fitting the length |
| [join checks](features/join-checks.md) | `daw joins`: each join of an edited song, and the file's length |
| [export](features/export.md) | `daw export`: a named WAV, AAC or MP3 file from a render |
| [skills](features/skills.md) | Where a personal skill and its sounds live, and the tracked `song-edit` skill |
| [new and untitled projects](features/new-and-untitled-projects.md) | The app's launch on Untitled, File › New, Save As…, the project index, `daw projects`, `daw move` and `daw copy` |
| [MIDI tracks and note clips](features/midi-clips.md) | Note clips that own their notes, the sampler as their instrument, the piano roll, and MIDI files of one part in and out |
| [the browser](features/browser.md) | Samples, instruments, effects and shared folders at the window's left |
| [the Sampler device](features/sampler-device.md) | A sample loaded onto an empty Sampler and shaped in its panel |
| [learning the CLI quickly](features/cli-discovery.md) | Short `daw describe` topics, `daw describe start`, `--help` with fields, and errors that say what to do |
| [the arrangement map](features/arrangement-map.md) | `daw map`: the song as a grid of tracks by bars, the same music under the same letter |
| [musical checks](features/musical-checks.md) | `daw check`'s warnings as objects with codes, the checks of the music, and `clip duplicate` laying copies in a row |
| [selecting several things](features/selection.md) | Clips by a rectangle and copied by Option-drag, several pattern events and automation points selected, moved, copied and removed together |
| [the piano roll](features/piano-roll.md) | Writing notes by hand: previews through either instrument, the velocity lane, zoom up and down, a note's start, Option-drag copies, ⌘ off the grid and grids as note values |
| [the grid](features/grid.md) | The timeline's grid named in the transport bar and chosen in a Grid menu with ⌘1 to ⌘4, acting on the editor that has the keys; triplets, and Snap to Grid off |
| [watchable steps](features/watchable-steps.md) | The agent making the song a part at a time with a host running, each command as it is decided, and a hint in the next edit's reply after five minutes of silence |
| [time signature](features/time-signature.md) | One time signature for the song, `3/4` or `6/8`, typed in the transport bar or set with `daw`; positions stay quarter-note beats while the bars, grid, rulers, position, metronome, map, MIDI files and beat map count the meter |
| [the Synth](features/synth.md) | A polyphonic synthesizer as a MIDI track's instrument, its sound a patch in the song: the engine, `daw synth`, patches as files and factory patches, the panel, unison, wavetables and the patch's effects |
| [editing a range of bars](features/bar-ranges.md) | `daw range copy\|insert\|delete\|clear` across the tracks with the clips, audio, lanes and sections in a range, and a section duplicated, moved or removed with its content |
| [export in the app](features/export-in-app.md) | File › Export Audio…: the mix as a named WAV, AAC or MP3 at a stated level through the bundle's `daw export`, with its measurements in a banner |
| [clips that loop](features/looping-clips.md) | A note clip or an audio clip whose content repeats until the clip ends, and `daw clip loop` |
| [the live analyzer](features/analyzer.md) | An effect that changes nothing and shows the sound passing through it: a strip in the device panel and a window with levels, loudness, spectrum, stereo field, spectrogram and waveform |
| [comparing with a reference](features/reference-comparison.md) | `daw reference`: a song kept as what good sounds like, as measurements in the workspace library; `daw compare RENDER --reference NAME`: the mix against it, whole and section by section, with the differences in words |
| [overlap between stems](features/masking.md) | `overlap` in `daw listen`: the pairs of stems with energy in the same band at the same moments, the band, how much of the time and where; `--overlap A B` for one pair by band and section, with a picture; each pair's change in `daw compare` |
| [a finer spectrum in numbers](features/spectrum-detail.md) | `spectrum_db`, the mix and each stem in 31 third octaves, and `tilt_db_per_octave` in `daw listen`; each stem's `resonances`, the peaks that stay while the notes move, as a frequency, a height and a `q`; `--section ID` for one section's; their change in `daw compare`, and the balance against a reference in third octaves |
| [translation checks](features/translation-checks.md) | `translation` in `daw listen`: what the mix, each band and each stem loses summed to mono and through a small speaker's band, 200 Hz to 8 kHz, with the largest in words; `--write-translation` for `mono.wav` and `small-speaker.wav` to hear; each loss before and after in `daw compare` |
| [dynamics in detail](features/dynamics-detail.md) | In `daw listen`: the mix's and each section's `loudness_range_lu` and loudest 3 s and 400 ms, from the analyzer's meter; each track's `hits`, how hard they land at its stem, through its group and in the mix; `effects`, what each compressor and limiter took off over the song and in each section, which the render's report now holds; each of them before and after in `daw compare` |

Proposed, in the order of Next in the backlog:

| Feature | Backlog |
|---|---|
| [the agent panel](features/agent-panel.md) | Next |
| [generated audio](features/generated-audio.md) | Next |

## Archive

| File | Was |
|---|---|
| [idea.md](archive/idea.md) | The first statement of the idea; replaced by the concept |
| [spec.md](archive/spec.md) | The first specification, for a Python build that was not made this way |
| [plan.md](archive/plan.md) | The first build plan |
| [design-notes-2026-09-07.md](archive/design-notes-2026-09-07.md) | A first-principles review of the design before the MVP |
| [design-notes-2026-09-30.md](archive/design-notes-2026-09-30.md) | Ideas for the app, the workspace, memory, skills and devices |
| [Rust-Swift-Update.md](archive/Rust-Swift-Update.md) | The plan of the Rust and Swift rebuild, with each milestone's exit test |
| [new-features-2026-10-01.md](archive/new-features-2026-10-01.md) | What editing a finished song from timecodes needed |
| [2026-09-29-code-review.md](archive/2026-09-29-code-review.md) | A review of the automation pull request |

Conventions throughout: Ableton's vocabulary (session, track, clip, device, send,
return, automation), positions in zero-based quarter-note beats, and real units.
