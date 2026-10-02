# Backlog

Everything between [concept.md](concept.md) and what exists, as one list. An item
is a line: what it is, and a link to its file in [features/](features/) once it
has a design. When an item is finished its line moves to
[completed.md](completed.md). [AGENTS.md](../AGENTS.md) says how these files are
kept.

- **Next** is in the order to build.
- **Verify** is work that is merged and that nobody has heard or tried by hand.
- **Later** is wanted and not ordered.
- **Ideas** is the inbox: anything said out loud lands there in a line, and is
  later moved up, written into the concept or dropped.

## Next

1. **Audio clips in the app.** Draw a track's audio clips with their waveform,
   drag their edges and fades, and show the beat map on the waveform. An audio
   file dropped on a track's lane lands as an audio clip at that beat, not as a
   pad, and a clip is dragged to move it. Draw a shaped automation segment as the
   curve it plays. Today the app plays audio clips without drawing them, and
   draws a shaped segment as a straight line.
2. **An app that stands alone on this Mac.** Sample search and import in Rust, so
   the browser needs no Python. The sample index in the app's data folder, found
   from any project. An app icon. An install step that puts the app in
   Applications. The Python tools found through a setting, not a path compiled in.
3. **The `.aaw` file type and the project ID.** The project file under the app's
   own extension, opened by double-click, and an ID in it that the index matches a
   moved project by.
4. **The workspace and the two levels.** The workspace folder with its managed
   instructions and `profile/`, `SONG.md` in a project, skills listed by name and
   description, the fixed project layout created by the tool, and `daw check`
   flagging files outside it.
5. **The agent panel.** A conversation in the window, per project, over the
   person's own Claude Code. [features/agent-panel.md](features/agent-panel.md)
6. **Other Macs.** The rest of the Python ported or carried in the bundle,
   signing with a Developer ID and notarizing, the licenses of the Rust crates
   gathered into the bundle, and first launch creating the workspace.
7. **Sign-in buttons and other agents,** as the companies' terms allow.

## Verify

Merged, and never heard or tried by a person. Each wants a short session at the
Mac, and what is found becomes a fix or a line in [completed.md](completed.md).

- **How the app sounds.** Nobody listened during the scripted runs of the rebuild:
  the glide of a level or knob, the 10 ms dip when an effect is added or removed
  while playing, tails ringing through an edit, a locate and a stop.
- **Dragging a sample** from the browser onto the arrangement, and a drop where
  there is no track. Scripted input cannot start a drag. A WAV dropped on a
  track was tried on 2026-10-02 and became a pad, as built; the person expected
  an audio clip, which is item 1 of Next.
- **Install Command Line Tool:** its alerts and the request for an administrator's
  password.
- **A stretched render, by ear.** The default stretcher was chosen on licensing,
  speed and timing measurements.
- **The song-edit path on a real song:** a purchased file decoded and beat-mapped,
  an edit made with the `song-edit` skill, its joins heard, the export played. The
  beat tracker's thresholds were tuned on generated audio and five local renders.
- **An `.m4a` or `.mp3` dropped on the app's window.**
- **Signing with a Developer ID and notarizing,** and the app on another Mac
  (with item 6).
- **Save As… in the app:** the panel on an Untitled project and on one that has
  a name, a name that is taken, and work carried on in the copy. Scripted input
  cannot answer a panel; `daw move` and `daw copy` were run in its place.
- **Closing an Untitled project that holds something:** the question, and each
  of Save…, Delete and Cancel, at a window's close and at Quit with several open.
- **Open Recent** after a project's folder was moved in the Finder with the app
  closed, after one was put in the Trash, and with an Untitled project a crash
  left.
- **Opening a project while its copy is open,** so that its path answers for it
  again.

## Later

### Working together

- Turns: an agent request's commands grouped, named for the request, kept or
  reverted as one; the song restored to before any turn.
- A/B while looping: one key flips between before and after a turn.
- Variations and versions: `daw version save`, and versions the person flips
  through on the loop.
- Locks the host enforces: "don't touch the drums".
- Listening markers dropped at the playhead during playback.
- How much the agent may do: Ask, Edit, Propose, Background.
- The undo history kept across closing a project.
- IDs on clips in the schema, so a handle survives between sessions.

### What the agent reads and writes

- An arrangement map: tracks by bars in a few hundred tokens.
- A text piano roll of one clip or a few bars.
- Queries: which bars the bass plays in, every snare off the backbeat.
- Musical checks in `daw check`: a note cut off by its clip's end, two tracks in
  one register, a snare off the backbeat in a half-time section.
- A client library over the host, where a block of calls is one turn.
- Structure in the document: chord symbols, named motifs, a clip defined by its
  relation to another.
- Positions as `m:ss` wherever a command takes a beat.
- An MCP adapter, only if it proves more reliable for agents than the CLI.

### Sound

- Saturation and a clipper. Agents making loud mixes stopped at the limiter.
- A subtractive synth with text patches, and modulation: LFOs, envelopes,
  velocity, seeded randomness. Agents wrote scripts to synthesize subs, pads and
  glides the DAW then could not see.
- Glide and a sustain loop on the sampler.
- More effects: utility, chorus, phaser, gate, transient shaper, multiband
  compressor, pitch shift.
- Groups, and sends from a return to a return.
- Tempo and time signature changes. 4/4 and one tempo are assumed throughout,
  the beat map included.
- Warp markers for a song whose tempo drifts.
- Slicing a break into pads with the pattern that replays it.
- Stem separation of a song.
- Note transformations (quantize, humanize, arpeggiate, vary) and a theory
  library.
- MIDI file import and export, and a MIDI keyboard the person plays a riff on.
- Audio clips: a cut that moves the other tracks with it; clips that loop,
  reverse or repeat; perhaps one list for pattern clips and audio clips.
- A join check that places a pad transposed by an event at its transposed length.
- A long song streamed from disk; a decoded song is held whole in memory.
- An event's offset in milliseconds, either side of its beat. A push or a drag
  is written in beats today, so it changes with the tempo, and a hit cannot sit
  ahead of its pattern's first beat.
- A grid for each step row. A pattern has one, so triplet hats over straight
  kicks are steps for one and events for the other.

### Tools and devices the agent writes

- A convention for one-off scripts: a known Python environment, results imported
  with the recipe that made them, edits through the host.
- Offline devices: a script as an effect, re-run when its input changes.
- Real-time devices in a sandboxed language, after a spike (FAUST is the
  candidate).
- Presets: effect chains, pad setups and whole tracks saved as text and loaded
  into any song.
- `daw promote`: a render, slice or pattern into the library with its recipe.

### Library and perception

- The person's own tags and notes on a sample.
- Search by description with an audio-text model, only if a labeled test shows
  it beats names and measurements.
- What the person declares about their right to use a sound, carried with it.
- Key and chord detection, swing detection.
- Masking between two tracks, with the bars where it is worst.
- A reference library, and a mix compared with a reference section by section.
- An audio-capable model as an optional critic, with its own consent.
- A certified true-peak measurement; the engine's 4× oversampled peak is an
  estimate.
- Render caching per track.

### Export

- Sample rate conversion, metadata tags, loudness normalization, several files in
  one call.
- A level match that compares the kept parts, not whole files.
- Stems, MIDI and a project another DAW opens.

### The app

- A song that grows when a clip is placed past its end. A blank project is 32
  bars, and a clip cannot be dragged further.
- Save As… over a folder that is already there, after asking.
- The Python commands (`check`, `timeline`, `export`, `samples import`)
  following a project that was saved under another name, as the Rust ones do.
- A project made with `daw init` in the app's index.
- A track's color saved in the song.
- A pad's own settings (level, tuning, held or not) in the device panel.
- A sample added as a pad shows where it went. Nothing on the timeline changes,
  and the person looked for the file on the track.
- The pattern's step menu in note values: 1/16 and 1/8T, as other DAWs write
  them. It lists beats, so its 1/4 is a sixteenth note.
- A loop from the browser fitted to the song's tempo when it is added.
- Several events selected, copied and pasted; several clips selected by dragging
  over them; several automation points selected.
- A sample auditioned through the song's engine, at the song's tempo and level.
- Names that are not IDs: spaces and any character in a track's name.
- An effect's ID set in its panel.
- A skills panel, and `@track`, `@clip`, `@section`, `@skill` in the chat.
- Ghost clips: the agent proposes the next bars, drawn translucent, and Tab
  accepts.

### Long work on its own

- A definition of done as a skill, and a bounded, resumable loop that keeps the
  best so far.
- What the agent learned, said at the end of a session and written to
  `taste.md` with evidence.
- Craft skills shipped with the app: mix to a reference, finish a song, prepare
  stems.
- Agent roles: a drummer and a mix engineer at once, each an origin.

### Shipping

- A record of every dependency and model: version, license, whether it may be
  redistributed.
- A statement of what leaves the machine, tested with the network off.
- Skills, devices and presets packaged for other people.

## Ideas

One line each, newest first.

- A `.mid` file dropped on a track lands as pattern clips: its notes as events
  of a pitched pad, or of the track's pads by drum note. Wants the MIDI import
  under Sound, and says what plays a file dropped where there is no pad.
