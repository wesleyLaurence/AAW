# Completed

What was built, newest first. A line comes here from [backlog.md](backlog.md)
when its work is merged, with the date, the pull request, and what was and was
not tried by a person. Anything not tried also gets a line under Verify in the
backlog.

| Date | What | PR | Tried by a person |
|---|---|---|---|
| 2026-10-02 | The app opens on an Untitled project; File › New and Save As…; the question on closing; the project index, `daw projects`, `daw move`, `daw copy` and a folder as PROJECT ([new and untitled projects](features/new-and-untitled-projects.md), D58) | #33 | Scripted runs only: launch on Untitled, ⌘N, a move and a copy from a terminal with the app open, a project left after quitting. Not the Save As… panel, the question on closing or Open Recent |
| 2026-10-01 | A shape on an automation point that bends its segment ([automation](features/automation.md), D55) | #31 | Not heard. The app draws the segment straight |
| 2026-10-01 | Audio clips: parts of a song on a track, and `daw audio add`, `cut`, `split`, `trim`, `crossfade` ([audio clips](features/audio-clips.md), D54) | #30 | Not heard. The app plays them and does not draw them |
| 2026-10-01 | A pad or audio clip stretched in time at its own pitch ([time stretch](features/time-stretch.md), D53) | #29 | Not heard |
| 2026-10-01 | `daw timeline`: beats and seconds, a sound placed to end on a beat, fitting the length ([timeline](features/timeline.md)) | #28 | Generated songs only |
| 2026-10-01 | The `song-edit` skill, the `daw describe` topics for edits, and a place for personal skills ([skills](features/skills.md), D52) | #27 | Run once by hand on a generated song |
| 2026-10-01 | `daw joins`: each join's beat, splice and level, and the file's length ([join checks](features/join-checks.md), D51) | #26 | Generated songs only |
| 2026-10-01 | `daw export`: a named WAV, AAC or MP3 from a render ([export](features/export.md), D50) | #25 | Generated songs only |
| 2026-10-01 | `daw samples beats`: a whole song's tempo, beats, downbeats and phrases ([beat map](features/beat-map.md), D49) | #24 | Generated audio and five local renders; no released song |
| 2026-10-01 | `.m4a` and `.mp3` decoded on import (D48) | #23 | Audio encoded here; no purchased file, no drop on the app |
| 2026-10-01 | The app as a bundle with `daw` inside and a menu item that links it onto the PATH (rebuild M9, D47) | #22 | Not the menu item's alerts, not Developer ID signing, not another Mac |
| 2026-10-01 | Waveforms on clips, the pattern editor and the sample browser (rebuild M8, D46) | #21 | A WAV dropped on a track on 2026-10-02 became a pad. Not a drag from the browser or a drop where there is no track; nothing was played by script |
| 2026-10-01 | The Python engine retired; `daw` is the Rust binary (rebuild M7, D45) | #20 | Renders compared by measurement, not by ear |
| 2026-10-01 | Effects, sends, returns and automation in the Rust engine, device panels and lanes in the app (rebuild M6, D44) | #19 | Played through the speakers by script; nobody listened |
| 2026-10-01 | Editing in the app: mixer, clips, tracks and returns, undo by origin (rebuild M5, D43) | #18 | Scripted input only |
| 2026-10-01 | The Mac app: live view of the arrangement and transport (rebuild M4, D42) | #17 | The person tried the app and found it working |
| 2026-09-30 | The session host: commands, handles, undo, the change log and edits heard as they land (rebuild M3, D41) | #15 | Whether the level change was audible was left to the person |
| 2026-09-30 | The model, the sampler engine and real-time playback in Rust (rebuild M0 to M2, D33 to D40) | #14 | Played without dropouts by measurement |
| 2026-09-29 | Fixes from the review of automation: stable fingerprints, filter state, lane bounds, earlier reports ([review](archive/2026-09-29-code-review.md), D30 to D32) | #5 to #13 | — |
| 2026-09-22 | Automation lanes for levels, sends and effect parameters ([automation](features/automation.md), D29) | #4 | — |
| 2026-09-22 | Sends, return buses, delay and reverb ([effects](features/effects.md), D27, D28) | #3 | — |
| 2026-09-22 | Track and master insert effects, sidechain compression and the limiter ([effects](features/effects.md), D26) | #2 | — |
| 2026-09-22 | Pitch, onsets, tempo and kind measured from a sample's audio ([sample analysis](features/sample-analysis.md)) | #1 | — |
| 2026-09-22 | The sampler MVP: the song format, the sampler, rendering, the sample library and perception ([architecture](architecture.md), [perception](features/perception.md)) | — | — |

A dash under "Tried by a person" means the record does not say.
