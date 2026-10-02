# AAW docs

Start with the [project introduction](../README.md). Every kind of statement has
one home here, and [AGENTS.md](../AGENTS.md) says how the files are kept.

| File | Holds | Tense |
|---|---|---|
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
| [effects](features/effects.md) | Insert effects on tracks, returns and the master, sidechain compression, the limiter, delay, reverb, and sends to returns |
| [automation](features/automation.md) | Lanes for levels, pans, sends and effect parameters |
| [perception](features/perception.md) | `daw listen` and `daw compare` |
| [sample analysis](features/sample-analysis.md) | `daw samples analyze`, measured search filters, root-note warnings |
| [beat map](features/beat-map.md) | `daw samples beats`: a song's tempo, beats, downbeats and phrases |
| [audio clips](features/audio-clips.md) | Parts of a file on a track, and the `daw audio` commands |
| [time stretch](features/time-stretch.md) | A pad or audio clip that follows the tempo at its own pitch |
| [timeline](features/timeline.md) | `daw timeline`: beats and seconds, end alignment, fitting the length |
| [join checks](features/join-checks.md) | `daw joins`: each join of an edited song, and the file's length |
| [export](features/export.md) | `daw export`: a named WAV, AAC or MP3 file from a render |
| [skills](features/skills.md) | Where a personal skill and its sounds live, and the tracked `song-edit` skill |
| [new and untitled projects](features/new-and-untitled-projects.md) | The app's launch on Untitled, File › New, Save As…, the project index, `daw projects`, `daw move` and `daw copy` |

Proposed:

| Feature | Backlog item |
|---|---|
| [the agent panel](features/agent-panel.md) | 5 |

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
