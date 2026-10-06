# Backlog

Everything between [concept.md](concept.md) and what exists, as one list. An item
is a line: what it is, and a link to its file in [features/](features/) once it
has a design. When an item is finished its line moves to
[completed.md](completed.md). [development.md](development.md) says how these files are
kept.

- **Next** is in the order to build: by how much each item helps the person
  and the agent make good music with the tool (D75).
- **Verify** is work that is merged and that nobody has heard or tried by hand,
  with what is most worth hearing first.
- **Later** is wanted and not ordered.
- **Ideas** is the inbox: anything said out loud lands there in a line, and is
  later moved up, written into the concept or dropped.

## Next

Between items, the sessions at the top of Verify come first: they find what is
wrong with the sounds everything else is built on.

1. **Clips that loop.** Note clips and audio clips that repeat until they end,
   by one rule (D63), so a phrase is changed once.
   [features/looping-clips.md](features/looping-clips.md)
2. **Export in the app.** File › Export Audio…: a named WAV, AAC or MP3 of the
   mix at a stated level, through `daw export`, with its measurements shown.
   The app's only export is a MIDI clip, so the person finishes a song through
   the agent or a terminal (D81).
   [features/export-in-app.md](features/export-in-app.md)
3. **A time selection in the app.** A range of beats dragged across the
   tracks and copied, inserted, deleted or cleared as `daw range` does it,
   and a section dragged with its content; which gesture makes it is the
   open question in [features/bar-ranges.md](features/bar-ranges.md).
4. **A MIDI keyboard.** Notes played through the selected track, held and let
   go rather than a beat long, through a Synth and through a Sampler, whose
   pitch needs audio prepared on the way; recording into note clips; quantize.
   The person plays a riff and the agent builds around it.
5. **Listening markers.** The person drops a marker at the playhead while the
   song plays, with a word if they want, and the agent reads them with `daw`:
   "this bar", without a timecode.
6. **Sections and the song's length in the app.** A section made, named,
   moved and removed in the ruler, which only draws them today; the length,
   master gain and end fade typed in the transport bar, as the tempo is. The
   agent has `daw section` and `daw set session.FIELD`; the person has neither
   (D81).
7. **A project made with `daw init` in the app's index,** so a song the agent
   starts in a terminal opens from Open Recent.
8. **Turns and A/B.** An agent request's commands grouped, named for the
   request, kept or reverted as one, and the song restored to before any turn;
   one key flips between before and after a turn while the loop plays.
9. **A clipper and a utility.** Agents making loud mixes stopped at the
   limiter; saturation is built, a clipper that holds a ceiling as a limiter
   does without its look-ahead is not. A utility for gain, width, mono below a
   frequency and phase.
10. **Groups.** Tracks summed into a group with its own effects and level, a
    drum bus, as in any DAW.
11. **Comparing with a reference.** A song the person names as a reference,
    analyzed once and compared with a mix section by section, so the agent has
    targets for the low end, the brightness and the loudness of each section.
    [features/reference-comparison.md](features/reference-comparison.md)
12. **Sound descriptors.** Each sample measured for brightness, attack, decay,
    low end and noise, with search sorted by them and samples like a chosen
    one, so the agent chooses sounds by more than their names.
    [features/sound-descriptors.md](features/sound-descriptors.md)
13. **Note transformations and chords.** Humanize with a seed, arpeggiate and
    vary; chord symbols written as notes; a theory library of scales and chords.
14. **Slicing a break.** A loop cut at its onsets into pads, with the note clip
    that replays it.
15. **Key and chord detection,** and swing detection, of samples and songs: a
    loop matched to the song's key before it is placed.
16. **Presets.** Effect chains, pad setups and whole tracks saved as text and
    loaded into any song.
17. **A pad's settings in the device panel.** Level, pan, transpose, mode,
    start and end, attack and release, choke group, reverse, source tempo,
    stretch and mono for each pad of a pattern track, which lists its pads and
    edits none; a loop fitted to the song by its tempo. The Sampler's one pad
    has most of them, and `daw pad set` has them all (D81).
18. **The workspace and the two levels.** The workspace folder with its managed
    instructions and `profile/`, `SONG.md` in a project, skills listed by name
    and description, the fixed project layout created by the tool, and
    `daw check` flagging files outside it; what the agent learned, said at the
    end of a session and written to `taste.md` with evidence.
19. **The agent panel.** A conversation in the window, per project, over the
    person's own Claude Code. [features/agent-panel.md](features/agent-panel.md)
20. **Stem separation** of a song.
21. **Generated audio.** A sound, a loop or a whole song from a description, with
    the person's own ElevenLabs key kept in the Keychain, saved in the project as
    a sample; a `daw generate` command for the agent and a panel in the app.
    [features/generated-audio.md](features/generated-audio.md)
## Verify

Merged, and never heard or tried by a person, in the order worth doing: the
sounds everything else is built from first. Each wants a short session at the
Mac, and what is found becomes a fix or a line in [completed.md](completed.md).

- **The factory patches by ear, and the browser's Synth by hand:** each of
  the twelve factory patches on a MIDI track of its kind of part, a chord or
  a bass line, heard through `daw synth audition` and in the app, and whether
  each sounds like its name; a patch saved with `daw patch save` and loaded
  into another song by hand; in the browser, Synth and a patch dragged onto a
  MIDI track's header, its device panel and under the tracks, + and a
  double-click on a patch, and a search by tag. Tests render every factory
  patch under full scale and hold a saved patch's audition in two songs byte
  for byte; the browser's drops ran through the model and its list was seen in a
  scripted picture; nothing was dragged by hand.
- **The Synth by ear:** `daw synth add` on a MIDI track of a few chords and a
  bass line, each factory-style recipe from `daw describe synth` built with
  `daw synth set` and heard through `daw synth audition` and in the app: the
  plain saw, a sub, a pluck with a filter envelope, a pad, a glide with one
  voice, a kick from a pitch envelope; a knob dragged in the panel while the
  song plays, a wave chosen, a lane on the cutoff, a locate into a held note
  and a stop. Engine tests hold a render to playback, to every block size and
  to itself, and measure a sine's pitch, an envelope, a sweep, a glide and
  stealing; nobody listened, and the panel was built and tested but not seen
  in a picture.
- **Unison, wavetables and the patch's effects by ear and by hand:** Supersaw,
  Soft Pad, Pluck and Riser heard for their unison, with the detune and width
  dragged while a note sounds and the Spread macro moved; each built-in table
  heard on a chord and seen drawn in the panel, and a cycle cut from a sample
  with `daw samples import` named as a table; the Organ patch against the old
  sines; a chorus, a saturation of each mode, a delay and a reverb heard in a
  patch and on a track, the chorus on a return; an effect added to the patch
  from the + menu, its knob dragged, its lane added from the diamond, moved
  with ◂ ▸ and removed with ×; a limiter inside a patch with a kick on
  another track, for alignment. Tests measured unison's spread, level and
  beating, a table's harmonics and bandlimit, the chorus's sweep, the
  saturation's harmonics and the chain's latency; the panel's effect columns
  were not seen in a picture and nothing was heard.
- **The Synth's panel by hand and ear:** a patch loaded with ◂ ▸ and from
  the menu while the song plays, Save… over one of Mine and Save As… with a
  name that is taken; the filter's corner and an envelope's handles dragged
  while a note sounds; an envelope's tab dropped on the cutoff and a macro's
  on a level, and the reach lines read against the amounts; + for each part
  and × on one the matrix names; a key pressed and dragged across the
  octave, the octave stepped, and `daw synth audition --play` heard in the
  app; the detail panel dragged taller and the columns scrolled at the least
  height. The layout is tested, the edits ran through the model, and the
  header, drawings, keys, reach lines and the taller panel were seen in
  scripted pictures; nothing was dragged by hand, no key was pressed and
  nothing was heard.
- **The piano roll's preview, velocity and zoom by hand and ear:** notes
  drawn, clicked and dragged across pitches on a Sampler track and a Synth
  track, heard as they go, and the keys pressed and dragged; the headphones
  turned off; a chord's velocities dragged in the lane and heard; rows zoomed
  with Option-scroll; a note's start dragged; notes Option-dragged into a copy;
  ⌘ held for off the grid on the timeline and in the pattern, where Option was
  before; whether the grids named as note values read right. Scripted drags
  (with `--opt-drag`) made the velocity, start and copy edits, read from the
  song and seen in a picture, and engine tests held a Sampler's preview to
  the same note in a clip; nobody listened, scrolled or held a key.
- **The piano roll by hand and ear:** a MIDI track and a clip made, a chord
  and a melody drawn and moved on and off the grid, ends dragged, several notes
  selected with Shift and a drag, velocity, a clip copied and its copy changed,
  a left edge dragged past notes and back, Copy and Paste of clips and notes,
  and a sample dropped on the track's header, swapped and removed while the
  song plays. Scripted clicks and keys did each of these but the drag of a
  sample, which `--drop` stood in for, and the results were read from the song;
  nobody held the mouse or listened. Whether a note is easy to take hold of at
  10 points a row, and whether its end is.
- **Selecting several things by hand:** a rectangle dragged over clips on
  several tracks, and with Shift; clips Option-dragged into copies, to
  another track and past the song's end with an audio clip, with Option
  pressed and let go during the drag; a rectangle over events in the
  pattern editor, events moved together up an octave and copied with
  Option, ⌘C and ⌘V into another pattern, ⌘D and Delete; a rectangle over
  automation points of two lanes, the points dragged together while the
  song plays and heard, and deleted. Scripted drags made a rectangle of
  clips and their Option copy, a rectangle of events and their Option copy,
  a paste and a delete of events, and a move of two points, each read from
  the song and seen in a picture, with engine tests of each edit; nothing
  was dragged by hand or heard, and Shift, the arrow keys and ⌘A on events
  were not scripted.
- **Note clips by ear:** a phrase of chords and an off-beat melody played by
  a pitched sampler, a drum clip played by a mapped kit, a sampler swapped
  while the song plays, and the new MIDI track and its notes in the window. The
  engine's tests hold a note clip to the same hits written as a pattern, and
  the window was seen in a picture; nobody listened or looked by hand.
- **The Sampler device by hand and ear:** drag Sampler from the browser onto a
  MIDI track, drop a sample on the device from the browser and from the
  Finder, draw a few notes and listen across the keys; drag the markers, type
  a root note and press Measure on a note and on a drum, switch Held and hear
  the release, and drop a second sample over the first. Scripted `--drop`,
  marker drags and a click on Measure were seen in pictures and read from the
  song; nothing was dragged by hand or heard, and the outline of a drop over
  the panel was not seen.
- **A sample dropped on a MIDI track's header in the rebuilt app,** played on
  notes above and below middle C, from the browser and from the Finder.
- **How the app sounds.** Nobody listened during the scripted runs of the rebuild:
  the glide of a level or knob, the 10 ms dip when an effect is added or removed
  while playing, tails ringing through an edit, a locate and a stop.
- **Dragging a sample** from the browser, or a file from the Finder, onto the
  timeline and onto a header: the outline while it is dragged, the audio clip or
  the pad it makes, and a drop where there is no track. Scripted input cannot
  start a drag; `--drop` handed a WAV and an `.m4a` to where a drag would have
  left them, and both became clips.
- **Audio clips by hand and by ear:** moving one, trimming each edge, both fade
  handles, a split and what follows it deleted, on a song of a few minutes.
  Whether the edges and the handles are easy to take hold of, how a faded and a
  trimmed end sound, and how long a trim of a long file takes.
- **MIDI files by hand:** a `.mid` saved from a synth or another DAW dragged
  from the Finder onto a MIDI track's lane and onto the timeline, its outline
  on the way, and the chords heard through a sampler; File › Export MIDI
  Clip… and its panel, and the file opened in Ableton or Logic. Scripted
  `--drop` handed a file written by mido to a header, a MIDI lane and the
  timeline past the song's end, and mido read an exported file; nobody
  dragged a file, used the panel or listened.
- **Metronome by hand and ear:** toggle it on an empty song, change BPM while playing, loop, and listen for alignment and a comfortable click level. Engine timing tests and scripted UI input do not replace listening.
- **Manual BPM by hand and ear:** type a tempo while playing, undo it, and hear the new tempo. Scripted input and engine tests cover validation and undo.
- **A stretched render, by ear.** The default stretcher was chosen on licensing,
  speed and timing measurements.
- **The beat map on a clip,** on a real song measured with `daw samples beats`:
  whether the ticks sit on the hits, and how they read in a song at another
  tempo.
- **The song-edit path on a real song:** a purchased file decoded and beat-mapped,
  an edit made with the `song-edit` skill, its joins heard, the export played. The
  beat tracker's thresholds were tuned on generated audio and five local renders.
- **An `.m4a` or `.mp3` dropped on the app's window.**
- **Browser folders and device drags:** use Add Folder with multiple directories, reopen another project, search across selected folders, refresh and remove a source; drag effects onto track, return and master headers and between devices, and Sampler onto a MIDI track and below the rows. Automated tests and scripted snapshots passed; the native picker and actual mouse drags have not been tried by hand.
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
- **A range edit by ear:** on a song of several tracks with an audio clip and
  a lane, a verse repeated with `daw section duplicate`, four bars inserted
  before a drop and deleted again, and a breakdown cleared on the drums alone
  with `daw range clear --track`; whether the 4 and 12 ms fades at a copied or
  moved audio piece's cut edge are heard as a click or a dip, and whether a
  note divided at a range's edge retriggers audibly. Tests held insert then
  delete to the same bytes and a copy to the render of the clips and lane
  edits it stands for; nothing was heard, and no agent session has used the
  verbs.
- **A fresh agent reading `daw check`'s codes:** an agent session asked to
  make a song of several sections, with `daw check` run before each render;
  whether it acts on `clips-stacked`, `note-retriggered` and
  `register-crowded`, reads `clips-overlap` as information, and whether
  `register-crowded` is right about what sounds muddy when the song is heard.
  Each code was raised by a generated song in tests; no agent session or
  person has used them.
- **A fresh agent reading `daw map`:** an agent session asked to make a song
  of several sections, then to change one, with `daw map` as its view; whether
  it reads the marks without being told, catches stacked or misplaced clips
  before rendering, and finds the clip to change from `clips`. Generated songs
  and a song made by `clip duplicate` were mapped in tests; no agent session
  has used it.
- **A fresh agent learning `daw` from `daw describe start`:** a new Claude or
  Codex session asked for a short song, with nothing else read first; whether
  it starts from `describe` and `start`, how many errors it meets, and whether
  each error's hint is the fix. The recipe runs in a test with a generated kick,
  and the hints, help and clipping message are held by tests; no agent session
  has used them.
- **Fresh-agent task routing:** start new Claude and Codex sessions for a simple song edit and a development task; verify only the applicable guide loads and song-edit is discoverable. Links, ignore rules and the shared skill path were checked locally; fresh sessions have not been tried.
- **Install Command Line Tool:** its alerts and the request for an administrator's
  password.
- **Signing with a Developer ID and notarizing,** and the app on another Mac
  (Later, under Shipping).

## Later

### Working together

- Variations and versions: `daw version save`, and versions the person flips
  through on the loop.
- Locks the host enforces: "don't touch the drums".
- How much the agent may do: Ask, Edit, Propose, Background.
- The undo history kept across closing a project.
- Persistent IDs on legacy pattern clips and audio clips; note clips and
  notes have them.

### What the agent reads and writes

- A text piano roll of one clip or a few bars.
- Queries: which bars the bass plays in, every snare off the backbeat.
- A client library over the host, where a block of calls is one turn.
- Structure in the document: named motifs, a clip defined by its relation to
  another. Chord symbols are Next.
- Positions as `m:ss` wherever a command takes a beat.
- An MCP adapter, only if it proves more reliable for agents than the CLI.
- `daw metronome on|off`, and a note previewed through a Sampler from the
  terminal as `daw synth audition --play` previews through a Synth, so the
  agent can play the person a sound on the keys. The host has both; only the
  CLI lacks them (D81).

### Sound

- Beyond the Synth: multi-frame wavetables with a sweepable position and
  Serum's or other WAV tables read; a second filter with routing; an LFO's
  rate and phase as matrix targets; a
  bandlimited triangle; a locate that carries a chased voice's oscillator
  phases and filter state as a render has them.
- Glide and a sustain loop on the sampler.
- More effects: phaser, gate, transient shaper, multiband
  compressor, pitch shift. An effect dragged from the browser into a Synth's
  patch, as the + menu adds one.
- Sends from a return to a return.
- Tempo and time signature changes. 4/4 and one tempo are assumed throughout,
  the beat map included.
- Warp markers for a song whose tempo drifts.
- MIDI controller and expression editing, including pedal, pitch bend and MPE.
- MIDI files beyond one part (D65): a file of several tracks or channels made
  into several tracks, the file's tempo taken when asked, the pedal, pitch
  bend and controllers kept once the song holds them, and a whole song
  exported as a type 1 file.
- Explicit enharmonic spelling, only if numeric-pitch agent workflows show a
  need; first-round MIDI notes store numbers alone (D61).
- A legacy track converted into a MIDI track when asked: its pads the sampler's,
  each pattern clip a note clip that owns its events, and the sound unchanged
  against generated fixtures (D62). What a track with two pitched pads maps to
  is its open question.
- Audio clips that reverse; perhaps one list for pattern clips and audio clips.
  A cut that moves the other tracks with it is `daw range delete`
  ([bar ranges](features/bar-ranges.md)).
- A join check that places a pad transposed by an event at its transposed length.
- A long song streamed from disk; a decoded song is held whole in memory.
- A trimmed audio clip prepared from audio already at the song's rate. A trim
  of a long file at another rate converts the part again, about half a second
  for two minutes.
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
- `daw promote`: a render, slice or pattern into the library with its recipe.

### Library and perception

- The person's own tags and notes on a sample.
- Search by description with an audio-text model, only if a labeled test shows
  it beats names and measurements.
- What the person declares about their right to use a sound, carried with it.
- Masking between two tracks, with the bars where it is worst.
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

- A song that grows when a pattern clip is placed past its end, as it does for
  an audio clip. A blank project is 32 bars, and a pattern clip cannot be
  dragged further.
- Audio clips: a crossfade made by dragging one clip over another, where today
  both play; an edge that snaps to the beats of the file's beat map; a clip's
  lead shown and dragged; several clips trimmed at once.
- Positions in bars or `m:ss` for every command, beside beats.
- A beat map measured from the app when a song is dropped. It is drawn when
  `daw samples beats` has left one.
- A larger picture of a clip's file in the detail panel, with the part the clip
  plays marked.
- An automation segment's shape set by dragging it. The app draws the curve and
  `daw` sets it.
- The window following the song's end when the song grows.
- Save As… over a folder that is already there, after asking.
- The Python commands (`check`, `timeline`, `export`, `samples import`)
  following a project that was saved under another name, as the Rust ones do.
- A track's color saved in the song.
- A sample added as a pad shows where it went. Nothing on the timeline changes,
  and the person looked for the file on the track.
- A drum kit made in the app: a sample dropped on a key of the piano roll as a
  pad that the note plays. A sample on a MIDI track's header replaces the
  instrument with one pad on every note, and `daw instrument map` makes kits.
- A loop from the browser fitted to the song's tempo when it is added.
- A sample auditioned through the song's engine, at the song's tempo and level.
- Names that are not IDs: spaces and any character in a track's name.
- The fields only `daw` sets, in the app: a send's pre-fader switch, a clip's
  velocity scale, an event's transpose, an audio clip's lead, an effect's ID,
  a Synth part's or macro's name (D81).
- `daw check`'s warnings in the window, by code, on the clips and tracks they
  name, so the person has the linter the agent has (D81).
- A skills panel, and `@track`, `@clip`, `@section`, `@skill` in the chat.
- Ghost clips: the agent proposes the next bars, drawn translucent, and Tab
  accepts.

### Long work on its own

- A definition of done as a skill, and a bounded, resumable loop that keeps the
  best so far.
- Craft skills shipped with the app: mix to a reference, finish a song, prepare
  stems.
- Agent roles: a drummer and a mix engineer at once, each an origin.

### Shipping

Waiting since October 4, 2026, while the app is built and opened from this
checkout (D75):

- **An app that stands alone on this Mac.** Sample search and import in Rust, so
  the browser needs no Python. An app icon. An install step that puts the app in
  Applications. The Python tools found through a setting, not a path compiled in.
- **The `.aaw` file type and the project ID.** The project file under the app's
  own extension, opened by double-click, and an ID in it that the index matches a
  moved project by.
- **Other Macs.** The rest of the Python ported or carried in the bundle,
  signing with a Developer ID and notarizing, the licenses of the Rust crates
  gathered into the bundle, and first launch creating the workspace.
- **Sign-in buttons and other agents,** as the companies' terms allow.
- A record of every dependency and model: version, license, whether it may be
  redistributed.
- A statement of what leaves the machine, tested with the network off.
- Skills, devices and presets packaged for other people.

## Ideas

One line each, newest first.

- `daw render` printing the codes `daw check` would give on stderr, so a render
  made without a check still says what is stacked or crowded.
