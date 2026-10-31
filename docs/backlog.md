# Backlog

Everything between [concept.md](concept.md) and what exists, as one list. An item
is a line: what it is, and a link to its file in [features/](features/) once it
has a design. When an item is finished its line moves to
[completed.md](completed.md). [development.md](development.md) says how these files are
kept. What is merged and not yet heard or tried by hand is in
[verify.md](verify.md), which is the person's and not read by an agent.

- **Next** is in the order to build: by how much each item helps the person
  and the agent make good music with the tool (D75), with what lets the agent
  understand the sound of a session first (D98), and the agent's
  conversations by project before either (D109); what every session is given
  and the agent panel went first and are built, and what the person asked
  for after using the panel goes first now (D113).
- **Later** is wanted and not ordered.
- **Ideas** is the inbox: anything said out loud lands there in a line, and is
  later moved up, written into the concept or dropped.

## Next

Between items, the sessions at the top of Verify come first: they find what is
wrong with the sounds everything else is built on.

Items 1–6 are what a code review of play past the song's end (D163) found
on October 28, 2026, in the order of how much each one harms a song, each a
fix of its own. Items 7–9 are the person's notes from using the app on
October 19, 2026 (D123). Items 10–21 are the order as it stood.

1. **The loop toggled with the cue past the end has a negative length.** The
    app's default loop takes the smaller of one bar and the distance to the
    song's end; with the start position past the end, which a ruler click and
    `daw locate` now allow, that distance is negative and L sends a loop the
    host refuses.
2. **A take from a stem tap stops at the end while a device take runs on.**
    Stem taps are fed only while the player rolls, so a track recorded from a
    stem ends its clip at the song's end and loses its tail, while a
    microphone take in the same recording goes on to the stop; the comment at
    record_tick promises both run on.
3. **Full DSP runs forever past the end.** The whole renderer graph keeps
    processing silence every block while play rolls on, and the resting path
    engages only when stopped, so a heavy song left playing costs its full
    CPU until someone stops it. Once past the end and quiet for a second, as
    the stopped branch already counts, skip the graph and only pass the
    block, count the beats and click.
4. **Zero-extent content at beat 0 reads as an empty song.** The fitted
    length returns eight bars when the content end is zero, so a song whose
    only item is an automation point at 0 or an audio clip at 0 whose file
    is missing flips by 28 beats as a one-beat clip is added and removed.
    Guard on no clips, points or sections instead.
5. **The timeline's width past the end only grows while playing.** The
    played beats are set from the playhead's tick and never reset, so a cue
    located past the end while stopped is drawn beyond the scrollable area,
    and after a stop far past the end the view keeps the empty width until
    the next play. Derive the content width from the largest of the length,
    the cue and the playhead in one place.
6. **A marker click still clamps the cue.** The marker-click path keeps the
    cue just under the song's length, the rule D163 removed from the ruler's
    target; dead today, and the old limit in one more place. Replace it with
    a plain locate.


7. **The person's skills: reverse-engineer, mix, master, arrange.** Written
    with the agent in the ignored skills folder, each a music session over
    `daw listen`, `compare`, `separate` and the Synth: a sound or a track
    rebuilt from scratch as near as the tool allows, a session mixed, a mix
    mastered, parts arranged into a form. What each finds missing becomes a
    line here.
8. **Why the agent missed the kicks and the downbeat.** Thunderstruck's sub
    kick layered under the wrong hits several times in a row, where the
    waveform made them plain, and a mashup of two songs matched in tempo but a
    beat ahead or behind: a song's drum hits as a list and a picture, `daw
    samples hits FILE` or on a stem, the transients in a band (a kick's
    45–90 Hz) with each one's time, level and the spacing to the next, since
    `daw listen` hits are scheduled hits only; and `daw samples beats` on a
    song with an odd bar, where the downbeat phase flips after it and bar.beat
    labels after it read two beats off, said when it changes or re-fit each
    phrase; each checked on those songs.
9. **Lyrics by bar, faster.** Whisper through anaconda takes minutes on a
    vocal stem; a smaller model, or Whisper through MLX on this Mac, as a
    command of the tool's own, the lyric's bar still read from the stem.

10. **`daw audio crossfade` with an option, while the app has the project
    open.** `--lead-ms`, alone or with `--in-ms` and `--ms`, is refused with
    "Bad request: invalid type: map, expected f64", where the same command with
    no option, and with the options on a project no host has open, sets the
    join. Until fixed, `daw set` on the clip's `lead_ms` and `fade_in_ms` and
    on `fade_out_ms` of the clip before does the same. `daw audio cut` takes the
    same three options and has not been tried with a host.
11. **A time selection in the app.** A range of beats dragged across the
    tracks and copied, inserted, deleted or cleared as `daw range` does it,
    and a section dragged with its content; which gesture makes it is the
    open question in [features/bar-ranges.md](features/bar-ranges.md).
12. **A MIDI keyboard plugged in.** The typing keyboard plays and records
    notes ([typing keyboard](features/typing-keyboard.md)) and ⌘U or `daw
    note quantize` snaps what was played ([quantize](features/quantize.md));
    a keyboard through CoreMIDI, with its velocity and its own latency, is
    the other half.
13. **Sections, the master gain and the end fade in the app.** A section
    made, named, moved and removed in the ruler, which only draws them today;
    the master gain and end fade typed in the transport bar, as the tempo is.
    The agent has `daw section` and `daw set session.FIELD`; the person has
    neither (D81). The length is managed in the background (D130).
14. **A project made with `daw init` in the app's index,** so a song the agent
    starts in a terminal opens from Open Recent.
15. **Turns and A/B.** An agent request's commands grouped, named for the
    request, kept or reverted as one, and the song restored to before any turn;
    one key flips between before and after a turn while the loop plays.
16. **Note transformations and chords.** Humanize with a seed, arpeggiate and
    vary; chord symbols written as notes; a theory library of scales and chords,
    the person's own tables of chords by intervals, scales and modes, and note
    frequencies its data where the model's own knowledge is not to be trusted.
17. **Slicing a break.** A loop cut at its onsets into pads, with the note clip
    that replays it.
18. **Key and chord detection,** and swing detection, of samples and songs: a
    loop matched to the song's key before it is placed.
19. **Presets.** Pad setups and whole tracks saved as text and loaded into any
    song. Effect chains and an instrument with its chain are built
    ([effect racks and instrument racks](features/effect-racks.md)).
20. **A pad's settings in the device panel.** Level, pan, transpose, mode,
    start and end, attack and release, choke group, reverse, source tempo,
    stretch and mono for each pad of a pattern track, which lists its pads and
    edits none; a loop fitted to the song by its tempo. The Sampler's one pad
    has most of them, and `daw pad set` has them all (D81). The app no longer
    makes pads (D85): they are the agent's, through `daw pad add`.
21. **The workspace as a folder of its own.** `~/Music/AAW/` with its managed
    instructions, `profile/` and skills, in place of this checkout, so a
    project kept anywhere has them; the fixed project layout created by the
    tool, and `daw check` flagging files outside it; what the agent learned,
    said at the end of a session and written to `taste.md` with evidence.

## Later

### Working together

- Variations and versions: `daw version save`, and versions the person flips
  through on the loop.
- Markers further: a marker over a range of beats; "done" on one the agent
  answered, drawn dim until the person removes it; who left it, kept with
  it; a marker dropped while playing moved earlier by a reaction's time, if
  listening shows it lands late; several selected, moved and copied
  together; a marker is among the `@` mentions in the agent panel by its id
  and its text is searched, so a marker's text as the chip's own words stays
  ([listening markers](features/listening-markers.md)).
- Locks the host enforces: "don't touch the drums".
- How much the agent may do: Ask, Edit, Propose, Background.
- The undo history kept across closing a project.
- Persistent IDs on legacy pattern clips and audio clips; note clips and
  notes have them.

### What the agent reads and writes

- `daw check` and `daw listen`'s `sounded` through a MIDI effect of the
  person's own: both read the song without the host's caches, so they
  hear the notes as written where the device changes them; read its
  cached notes from `.daw/devices/` as a render does
  ([devices anyone builds](features/custom-devices.md)).
- A text piano roll of one clip or a few bars.
- Queries: which bars the bass plays in, every snare off the backbeat.
- A client library over the host, where a block of calls is one turn.
- Structure in the document: named motifs, a clip defined by its relation to
  another. Chord symbols are Next.
- Positions as `m:ss` wherever a command takes a beat.
- An MCP adapter, if it proves more reliable for agents than the CLI, and
  as the route that lets other people's own Claude, ChatGPT or Codex work
  in the window without an engine of the app's
  ([the agent's engine](features/agent-engine.md), Shipping).
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
- More effects: phaser, gate, transient shaper, multiband compressor, pitch
  shift; an oversampled overdrive and saturation, as the clipper is, if what
  folds back is heard ([overdrive](features/overdrive.md)). What a clipper is
  taking off shown in its panel while the song plays,
  which only a render's report says today; and, if its last clip is heard, a
  switch to leave it out before a limiter, or more passes of the oversampled
  stage ([clipper](features/clipper.md)). An effect dragged from the browser
  into a Synth's patch, as the + menu adds one.
- The limiter on a true peak further: a second stage that takes off what
  the first leaves where its gain is moving, so that a look-ahead under
  1 ms holds too; the estimate a section preview gives without its cut
  edges, which read as peaks of their own; the 0.01 dB it aims under said
  in the report ([a limiter that holds a true
  peak](features/true-peak-limiter.md)).
- Sends from a return to a return.
- Tempo and time signature changes within a song, asked about again on
  October 19, 2026. One tempo and one meter hold throughout, the beat map
  included; the song's one time signature is built
  ([time signature](features/time-signature.md)).
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
- **More splits for `daw separate`** (stage 2 of
  [stem separation](features/stem-separation.md)): lead from backing vocals,
  a drum kit by its pieces, reverb and echo taken off a vocal, crowd noise,
  and the best instrumentals, each a community model the person chooses to
  download by name with its license shown, since most have none or a
  non-commercial one; whether Demucs through MLX or demucs-rs should run the
  stem splits faster; `fast` vocals from an MIT model instead of UVR's; and
  progress in the runner's lines, which say the stage and not how far.

### Tools and devices the agent writes

- A convention for one-off scripts: a known Python environment, results imported
  with the recipe that made them, edits through the host.
- Offline devices: a script as an effect, re-run when its input changes.
- Real-time devices in a sandboxed language, after a spike (FAUST is the
  candidate); the spec they are written to is Next
  ([devices anyone builds](features/custom-devices.md)).
- Plugins shared from the app: a `.zip` of a device dropped on the browser
  installs it, saying whether its code is sandboxed, and a plugin's row has
  Share… (the zip `daw plugin pack` makes), Show in Finder and Move to
  Trash (D156).
- `daw promote`: a render, slice or pattern into the library with its recipe.

### Library and perception

- The person's own tags and notes on a sample.
- References: several for one song, side by side; one attached to a project,
  so every `daw compare` uses it without being told; a clip of one written
  at the mix's loudness for the person to play beside it; sections paired by
  more than loudness when no IDs are shared; a reference's stems from `daw
  separate`, `daw reference add --stems`, so `daw compare` holds a mix's kick
  and bass to a record's (stage 4 of
  [stem separation](features/stem-separation.md)).
- Search by description with an audio-text model, only if a labeled test shows
  it beats names and measurements.
- Descriptors further: a loop's hits measured one at a time, so it has an
  attack, a decay and a punch; a rule or a trained classifier for snares,
  claps, 808s and the rest, where two rules name hats and kicks alone; a
  sweep told from noise, so a short kick does not read as noisy; the lines
  between the words set from the person's own library where it is unlike
  bought packs; a category read from more spellings of a name, `HH` and
  `_hat_` among them; a song's own samples listed with their words by
  `daw check` or `daw map`
  ([sound descriptors](features/sound-descriptors.md)).
- What the person declares about their right to use a sound, carried with it.
- Masking proper: a hearing model that spreads a loud band over its
  neighbors and tells a tone from noise, if it names different pairs from
  [overlap](features/masking.md); a stem covered by another, not contested,
  listed too; the bands under 89 Hz read through longer windows; a grouped
  track read after its group's effects; `daw check` naming the worst pair of
  the last render.
- Resonances further: a ring's own width and depth, read at a finer
  resolution where a track's notes do not get in the way, since a peak
  narrower than a twelfth of an octave reads as that wide; a resonance told
  from a melody's partials by following each note's harmonics; dips, a band
  a stem lacks against its neighbors; resonances of a file that is not a
  render, a sample before it is imported; a section's third octaves against
  a reference's.
- Translation further: the loss in mono by third octave, so a
  `mono_below_hz` is chosen from more than one band from 60 to 250 Hz; other
  systems beside the small speaker, a laptop, earbuds and a car, each a
  stated curve, and the phone's corners measured; how loud a bass seems on a
  small speaker through its harmonics, not only that they are there; a
  stem's losses by section; a reference's own losses, so a mix is held to
  what its reference keeps; `daw check` naming a mono loss of the last
  render.
- Dynamics further: reduction over time, a value a beat, for a picture of
  the limiter working under the energy; crest by band, to say that the low
  end is squashed and the top is not; hits found in an audio clip by its
  onsets, for a drum loop; a stem's loudness range; a reference's range and
  loudest seconds, so a master is held to what its reference does; the
  loudest 3 s and 400 ms drawn in the analyzer's window, which has them;
  one gated loudness, where pyloudnorm, the meter and a Synth's audition
  each have their own; `daw check` naming a limiter that works all through a
  section of the last render.
- A shorter `daw listen` by default, with the rest behind a flag, now that
  the report holds third octaves and resonances for every stem.
- An audio-capable model as an optional critic, with its own consent.
- A certified true-peak measurement; the engine's 4× oversampled peak is an
  estimate, which a limiter on a true peak holds and would have to read as
  the measurement does.
- Render caching per track.

### Export

- Sample rate conversion, metadata tags, loudness normalization, several files in
  one call.
- A level match that compares the kept parts, not whole files.
- Stems, MIDI and a project another DAW opens.

### Recording

- Monitoring the input through the track's chain while it is armed, `auto`,
  `on` or `off` as Ableton's, with the input's level in the header's meter;
  the input's frames fed to the renderer under the output's clock.
- A loopback measurement of the round trip, Settings › Audio playing a click
  into the chosen input and reading when it comes back, where a device's
  reported latency is wrong or missing.
- Punch in and out, loop recording into take lanes and comping; a take
  replacing what it covers when overdub is off.
- A take's end corrected for the drift between an input device's clock and
  the output's, read from the anchor as the take runs.
- The panel's recording from a track or the mix, to ask about a sound as it
  plays: `capture.start` from a tap, as a take is
  ([a sound recorded into the conversation](features/agent-record-audio.md)).
- A melody's pitch over time, `daw samples pitch FILE`, so a hummed line
  recorded into the conversation is read as notes rather than one pitch
  ([a sound recorded into the conversation](features/agent-record-audio.md)).

### The app

- Audio clips: a crossfade made by dragging one clip over another, where today
  both play; an edge that snaps to the beats of the file's beat map; a clip's
  lead shown and dragged; several clips trimmed at once.
- Positions in bars or `m:ss` for every command, beside beats.
- A beat map measured from the app when a song is dropped. It is drawn when
  `daw samples beats` has left one.
- A larger picture of a clip's file in the detail panel, with the part the clip
  plays marked.
- **Separate… in the app** (stage 3 of
  [stem separation](features/stem-separation.md)): a sample in the browser or
  an audio clip on the timeline taken apart from its menu, with the split,
  the quality and the model's size and license beside it, off the main thread
  with progress and a cancel, the parts laid on tracks under the source as
  `--tracks` lays them; the runtime made by the app on a Mac without this
  checkout, which must find `uv` or carry it (Shipping); whether the model
  should wait for the transport to stop, since it takes the GPU and every
  core.
- Save As… over a folder that is already there, after asking.
- The Python commands (`check`, `timeline`, `export`, `samples import`)
  following a project that was saved under another name, as the Rust ones do.
- A track's color saved in the song.
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
- The browser's samples sorted and filtered by what they measure as, with a
  sample's words on its row and Similar in its menu, as `daw samples search
  --sort` and `daw samples like` are for the agent (D104).
- The analyzer's spectrogram as a surface in three dimensions, as Insight
  draws it, and its window's arrangements of panes saved by name.
- A skills panel over the same list `/` opens for a skill
  ([skill commands](features/skill-commands.md)).
- The agent panel further: the commands of Claude Code's own that the `/`
  list hides shown, if wanted
  ([skill commands](features/skill-commands.md)); a video's sound or a
  frame read for the agent; a
  dropped sound imported at once rather than when it is placed; Apple's
  server recognition where the Mac cannot recognize the locale on the
  device, the song ducked while the person speaks, hold to talk,
  `SpeechTranscriber` on macOS 26 and spoken words sent as the person
  stops speaking ([attachments](features/agent-attachments.md),
  [speaking](features/voice-input.md)).
- Ghost clips: the agent proposes the next bars, drawn translucent, and Tab
  accepts.

### Long work on its own

- A definition of done as a skill, and a bounded, resumable loop that keeps the
  best so far.
- Craft skills shipped with the app: mix to a reference, finish a song, prepare
  stems. The person's own four are Next, and what they learn feeds these.
- Agent roles: a drummer and a mix engineer at once, each an origin.

### Shipping

Waiting since October 4, 2026, while the app is built and opened from this
checkout (D75):

- **An app that stands alone on this Mac.** Sample search and import in Rust, so
  the browser needs no Python. An install step that puts the app in
  Applications. The Python tools found through a setting, not a path compiled in.
- **Other Macs.** The rest of the Python ported or carried in the bundle,
  signing with a Developer ID and notarizing, the licenses of the Rust crates
  gathered into the bundle, and first launch creating the workspace.
- **The agent's engine for other people.** The panel on a Mac that is not
  the developer's, which may not drive a subscription login: Claude Code's
  engine with the person's own API key in the Keychain, each turn's cost
  shown and a budget; a harness of the app's own over a provider's API or a
  local model, perhaps a fork of an open agent framework such as LangChain's
  deep agents, with a sandbox for the code and files it runs and more of the
  audio work native to it, which the person wants explored once the panel's
  items are built; AAW as an MCP server for the agent the person already pays
  for; a sign-in button only if a program opens (D114).
  [features/agent-engine.md](features/agent-engine.md)
- **The `.aaw` type's identifier** on a domain the project will keep, in
  place of `local.aaw.project`, before the first `.aaw` leaves this Mac: an
  exported type identifier should not change once files carry it
  ([the `.aaw` project file](features/project-file.md)).
- A record of every dependency and model: version, license, whether it may be
  redistributed.
- A statement of what leaves the machine, tested with the network off.
- Skills, devices and presets packaged for other people.

## Ideas

- `daw effect add PROJECT OWNER --type T --help` printing the kind's fields,
  where `--help` after `--type` is now taken as a field and refused (found
  building the recipes, D152).
- Macros over a rack's fields, as Ableton's rack has: eight knobs each
  mapped to fields of the effects inside, automated as one.
- A rack in a rack, which a deeper address and a folded frame inside a
  frame would need; one level was what was asked for (D139).

One line each, newest first.

- Pitched music written as steps, as drums are: `daw note write CLIP "C2 . G2
  . | C2 . Bb1 ."`, a token a grid step, `.` a rest, `-` a hold, `|` a bar;
  and a chord by name, `--chord Am7 --at 8 --duration 4`, in place of its
  notes as JSON. Said by the agent on October 31, 2026, trying the CLI.
- `BAR:BEAT` accepted wherever a beat is taken (`--at 17:1`), and printed
  beside the beat in `note list`, `inspect` and `map`: the person says bars,
  the agent converts by hand with 1-based bars over 0-based beats. Said by
  the agent on October 31, 2026, trying the CLI.
- Relative edits of numbers: `daw set tracks.bass.gain_db -=3`, as "bring
  the bass down 3 dB" is said, without a read first. Said by the agent on
  October 31, 2026, trying the CLI.
- `daw key` and `daw chords` reading the key and the chord by bar from the
  note clips, later from audio by chroma: `note list` gives numbers and
  nothing names the harmony. The read half of the theory library in Next.
  Said by the agent on October 31, 2026, trying the CLI.
- Undo without a host, from a small journal beside the song, or at least
  `daw diff PROJECT --since SHA` from the shas every edit returns: alone at a
  terminal, every edit is written with no way back. Said by the agent on
  October 31, 2026, trying the CLI.
- `--brief` on `listen`, `inspect` and `get` (loudness, true peak, band
  balance, the three worst findings), and `daw render --listen` doing both
  in one call: a 5-second song's listen is 14 KB, most of it unread in a mix
  pass. Said by the agent on October 31, 2026, trying the CLI.
- `daw batch --dry-run` validating every op and reporting every error with
  nothing written; today the first bad op stops it. Said by the agent on
  October 31, 2026, trying the CLI.
- Fixes found trying the CLI on October 31, 2026: `daw effect add SONG bass`
  says "the song has no bass" where the owner must be `tracks.bass` while
  `clip add SONG bass` takes the bare name, so accept both everywhere and
  name the owners in the error; missing-field errors are raw pydantic
  (`tracks.0.effects.0.compressor.threshold_db Field required`) and the
  `set` typo error says "did you mean" then dumps pydantic anyway; `daw note
  set --help` marks `--pitch` and `--duration` required, copied from `add`;
  `note transpose` wants `--by` where a positional reads naturally; `daw
  render --help` has blank lines for `--output`, `--track`, `--section`;
  `daw samples` is argparse while the rest is clap, two help dialects and
  plain-text failures; `get` prints a pitch as a number where `note list`
  adds `name`; there is no `effect list` where other parts have one.
- The piano roll's note held while the mouse is down, as a key is, through
  `note.on` and `note.off` in place of the beat-long preview; and a key of
  the typing keyboard pressed while a note is selected setting its pitch.
  Said by the agent on October 30, 2026, building the typing keyboard.
- An effect moved to another row by dragging its title onto the header
  without Option, as Option copies it there; the host has no edit that moves
  an effect between chains, so it would be a copy and a remove as one step.
  Said by the agent on October 20, 2026, building effects reordered by
  dragging.
- Saving as it goes against Ableton's unsaved changes: the host writes the song
  after every edit, so nothing is lost and there is no Save, no dirty mark and
  no crash recovery; what is given up is a known good state to return to, which
  turns and A/B (Next), named versions (Later) and the undo history kept across
  closing a project would give back. Said out loud on October 19, 2026; worth
  thinking through once turns are built.
- The person's tables of every chord by its intervals, of scales and modes and
  of note frequencies by name: whether a model needs them as data or knows
  them. Taken into the theory library's item in Next.
- The Mac tests leave a defaults domain a run, `aaw.tests.export.UUID`, in
  `~/Library/Preferences`; a few dozen sit there. The export tests should
  take theirs away.
- A log of a project's sessions as a file of its own that the agent searches
  and no session reads unasked, only if `SONG.md` without history proves not
  to be enough.
- `daw render` printing the codes `daw check` would give on stderr, so a render
  made without a check still says what is stacked or crowded.
