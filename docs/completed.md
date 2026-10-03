# Completed

What was built, newest first. A line comes here from [backlog.md](backlog.md)
when its work is merged, with the date, the pull request, and what was and was
not tried by a person. Anything not tried also gets a line under Verify in the
backlog.

| Date | What | PR | Tried by a person |
|---|---|---|---|
| 2026-10-03 | A metronome icon in the transport bar, with a beat-aligned monitoring click during playback, including an empty song ([metronome](features/metronome.md), D67) | PR pending | Automated audio timing and host tests, scripted UI input and screenshots; not heard or tried by hand |
| 2026-10-03 | Editable BPM in the transport bar, saved through the host with validation and undo ([manual BPM](features/tempo-control.md)) | #45 | Automated host tests and scripted UI checks; not tried by hand or heard |
| 2026-10-03 | A sample dropped on a MIDI track plays at every note's pitch, as it is at middle C, as in Ableton's Simpler; a pitched map entry whose sample has no root note repitches from middle C (D66) | #42 | The person heard a percussion hit on their song play at each note's pitch once its map entry was made pitched by command; the drop in the rebuilt app not tried by hand |
| 2026-10-03 | Standard MIDI files of one part: `daw midi import` makes a note clip of a file's notes and velocities on a MIDI track or a new one, counting what the song cannot hold, and `daw midi export` writes a note clip as a type 0 file; a `.mid` dropped on the timeline, and File › Export MIDI Clip… ([MIDI tracks and note clips](features/midi-clips.md), D65) | #41 | Generated files and scripted runs only: files written by mido imported, one with a pedal, pitch bend and a program change, one of two parts refused; an exported file read by mido; `--drop` on a header, a MIDI lane and past the song's end, seen in a picture. No file dragged from the Finder, the export panel not used, nothing heard |
| 2026-10-03 | Note clips in the app: Add MIDI Track, a note clip made by a double-click, the piano roll (notes added, selected, moved, stretched, typed and removed, on and off the grid, all 128 notes), note clips moved, trimmed at either edge and duplicated, Copy, Cut and Paste of clips and notes, a sample on a MIDI track's header as its instrument, removed in the device panel; a note may start before its clip and `daw clip trim` ([MIDI tracks and note clips](features/midi-clips.md), D64) | #40 | Scripted runs only, on a generated song and a blank project: each edit made by scripted clicks and keys and read back from the song, the window seen in pictures. Nothing held by hand, nothing heard |
| 2026-10-03 | Note clips in the song and the commands: MIDI tracks of clips that own their notes, with IDs; a sampler that maps notes to pads, attached, replaced or removed without touching the notes; `daw note`, `daw instrument`, `daw note list` and `daw describe midi`; playback and render; schema version 2; the app draws a note clip's notes ([MIDI tracks and note clips](features/midi-clips.md), D63) | #39 | Generated audio and scripted runs only: a phrase and a drum clip made by commands and rendered, and the window seen in a picture. Nothing heard or tried by hand |
| 2026-10-02 | Audio clips in the app: a file dropped on the timeline is a clip, drawn with its file's waveform, moved, trimmed, faded and split there; the song grows to hold it; the beat map on the waveform; a clip's panel; `daw audio move`; a shaped automation segment drawn as its curve ([audio clips in the app](features/audio-clips-in-app.md), D59) | #35 | Scripted runs only, on generated audio: a WAV and an `.m4a` handed to the timeline of a blank project, a trim, a fade, a move past the song's end, a split, the beat ticks and a shaped lane, each seen in a picture. Nothing dragged from the Finder, and nothing heard |
| 2026-10-02 | The app opens on an Untitled project; File › New and Save As…; the question on closing; the project index, `daw projects`, `daw move`, `daw copy` and a folder as PROJECT ([new and untitled projects](features/new-and-untitled-projects.md), D58) | #33 | Scripted runs only: launch on Untitled, ⌘N, a move and a copy from a terminal with the app open, a project left after quitting. Not the Save As… panel, the question on closing or Open Recent |
| 2026-10-01 | A shape on an automation point that bends its segment ([automation](features/automation.md), D55) | #31 | Not heard. The app drew the segment straight until #35 |
| 2026-10-01 | Audio clips: parts of a song on a track, and `daw audio add`, `cut`, `split`, `trim`, `crossfade` ([audio clips](features/audio-clips.md), D54) | #30 | Not heard. The app played them and did not draw them until #35 |
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
