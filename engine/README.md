# Rust engine

The Cargo workspace of the native build planned in
[docs/archive/Rust-Swift-Update.md](../docs/archive/Rust-Swift-Update.md): the song model, the
engine that plays and renders, the session host and the `daw` command. It
replaced the Python engine at that plan's cutover (M7). The Python package in
`src/agent_daw` keeps the sample library, sample analysis and perception, and
reads songs through `aaw-py`.

| Crate | Responsibility |
|---|---|
| `aaw-model` | Schema types (version 1, and 2 with MIDI tracks), validation, exact beats, canonical YAML, fingerprints, the event schedule, the schema `daw describe` prints, the warnings `daw check` gives |
| `aaw-dsp` | Resampler (a port of `scipy.signal.resample_poly`), automation envelopes, the eight effects, wavetables and the Synth |
| `aaw-engine` | Song compilation, routing, latency alignment, mixing, the transport, offline and real-time drivers, waveform peaks |
| `aaw-host` | The session host: commands, handles, undo, change log, saving, external edits, socket; a project as a folder, made, moved and copied |
| `aaw-cli` | The `daw` binary |
| `aaw-ffi` | What the Mac app calls, through UniFFI: a hosted song's arrangement, devices, lanes, patterns, waveforms, changes and transport, its project's Save As…, Untitled projects, and the sample library |
| `aaw-py` | The model for Python, through PyO3: the `agent_daw.aaw_py` module |

## Build and test

Install Rust stable with [rustup](https://rustup.rs) and libsndfile (`brew install
libsndfile`; set `SNDFILE_LIB_DIR` to use another copy), then from this directory:

```sh
cargo test
cargo build --release   # target/release/daw
```

If the checkout lives in a synced folder, exclude `target/` from syncing. For
Dropbox: `mkdir -p target && xattr -w com.dropbox.ignored 1 target`, repeated
after `cargo clean`, which deletes the directory.

`aaw-py` is a Python extension module, so plain `cargo` commands leave it out
(the workspace's `default-members`). The Python package builds it for its own
interpreter with setuptools-rust: `uv sync`, and `uv run` after a change to the
model, as [pyproject.toml](../pyproject.toml) sets up. An Intel Python on Apple
silicon needs `rustup target add x86_64-apple-darwin`. To check it alone:
`PYO3_PYTHON=../.venv/bin/python cargo check -p aaw-py`.

`daw` is not on the PATH. Run it as `target/release/daw`, or as `uv run daw`,
which runs that binary (or the one `AAW_DAW` names). The Mac app's bundle holds
a copy, which the app's menu links onto the PATH
([apps/mac/README.md](../apps/mac/README.md)). A command's PROJECT is the
project's folder or the song file in it. It implements:

| Command | Does |
|---|---|
| `daw init DIRECTORY [--tempo T] [--bars N]` | Creates `DIRECTORY/song.yaml`, an empty song |
| `daw projects [--all]` | The projects a host has open, with each one's title, revision and whether its window is in front in the app; `--all` adds the projects the app knows that are not open |
| `daw move PROJECT NEW_FOLDER`, `daw copy PROJECT NEW_FOLDER` | Saves the project under another name: moves its folder, or copies it and leaves the original, and names the song after the folder. A running host carries on there; see below |
| `daw describe [start\|project\|sampler\|synth\|midi\|effects\|automation\|edit\|beats\|joins\|export] [--schema]` | The authoring contract: without a topic, the topics a line each; with one, what its fields mean and a line for each field (its path, what it takes, its default), generated from the schema, with the Synth's modulation and what lanes can move; `start` is the commands from `init` to `listen` that make a first song, with a `batch` file. `--schema` adds the topic's JSON Schema, and with no topic prints the whole song's |
| `daw fmt PROJECT` | Rewrites the song in canonical form |
| `daw apply PROJECT PATCH --expect SHA [--label TEXT]` | Replaces fields from a JSON merge patch, unless the song changed since SHA; a label names the edit in the change log and for undo |
| `daw samples ...`, `daw listen`, `daw compare`, `daw check`, `daw timeline`, `daw joins`, `daw export` | Run in Python, with the same arguments and output: the sample library, perception, `inspect` with measured root notes and the model's warnings, the timeline in beats and seconds, the checks of an edit's joins, and a named deliverable from a render. The binary uses the checkout's `.venv/bin/python`, or `AAW_PYTHON` |
| `daw audio add\|move\|cut\|split\|trim\|crossfade` | Audio clips on a track: parts of a sample file placed on beats and moved to another beat or track, a range removed with the gap closed, and the fades of a join |
| `daw model PATH...` | Canonical YAML and fingerprints, or validation errors |
| `daw schedule PROJECT` | Every hit's start frame, track, pad and release frame |
| `daw render PROJECT [--output DIR] [--track T] [--section S]` | Mix, stems, snapshot and `report.json`, which `daw listen` and `daw compare` read |
| `daw host PROJECT` | Runs a session host until interrupted or `daw close` |
| `daw play PROJECT [--from BEAT] [--seconds S] [--buffer FRAMES]` | Plays through the default output; see below |
| `daw play PROJECT --benchmark` | Times the playback path in buffer-sized blocks without a device, and a compile, a recompile, building a renderer and each track's waveform peaks |
| `daw stop`, `daw locate PROJECT BEAT`, `daw loop PROJECT START LENGTH`, `daw loop PROJECT off` | Transport of a running host |
| `daw inspect`, `daw get PROJECT [PATH]`, `daw status`, `daw changes PROJECT --since REV` | Reading: summary, part of the song, host state, change log |
| `daw map PROJECT [--per bar\|beat\|BARS] [--from BEAT] [--to BEAT] [--track T,...] [--lanes]` | The song as a grid of tracks by bars: a letter where a clip plays, the same letter for the same music, `#` where two clips of a track sound at once, `:` where a clip holds and nothing starts; the bars and sections above, a row a lane with `--lanes`, a legend of each letter and a line a track. `map` is the text as a list of lines and `clips` each letter's clips by reference. See [the arrangement map](../docs/features/arrangement-map.md) |
| `daw set PROJECT PATH VALUE`, `daw toggle`, `daw remove` | Any value by path, e.g. `tracks.drums.gain_db -4.5` |
| `daw track`, `return`, `clip`, `pattern`, `pattern event`, `pad`, `effect`, `send`, `lane`, `lane point`, `section` | The command catalog of the rebuild plan; `--help` lists each group's verbs |
| `daw range copy PROJECT START LENGTH --to AT [--insert] [--track T]…`, `daw range insert PROJECT AT LENGTH`, `daw range delete PROJECT START LENGTH`, `daw range clear PROJECT START LENGTH`, `daw section duplicate PROJECT SECTION [--to AT] [--id ID]`, `daw section move\|remove PROJECT SECTION … --with-content` | A range of beats across every track, or the tracks named: its pattern clips, note clips, audio clips, lanes and sections copied over what is at AT or into time opened for it, empty beats opened, beats removed with the gap closed and the clips that meet there made one again, or the range emptied; a clip across an edge is cut there, a pattern clip only between repeats; a section with what is under it duplicated after itself, moved over what is at AT, or deleted. One undo step each; the reply lists what was made, split, moved and removed. See [editing a range of bars](../docs/features/bar-ranges.md) and `daw describe edit` |
| `daw track add PROJECT ID --type midi`, `daw clip add PROJECT TRACK --length-beats L`, `daw clip resize`, `daw clip trim --start\|--end`, `daw note add\|set\|move\|transpose\|remove\|list`, `daw instrument set\|remove\|map` | MIDI tracks: note clips that own their notes, the notes read with their names and song beats, and the instrument that plays them; `daw pad` edits a MIDI track's sampler. See `daw describe midi` |
| `daw clip loop PROJECT CLIP BEATS\|off [--length BEATS]` | A note clip or an audio clip looped: its first so many beats play again and again until its end, which `clip resize` or `audio trim --end` sets, the last repetition cut off; `off` plays it once again. The schedule unrolls the loop, each audio repetition a copy with the clip's fades at the wrap; a looped clip is split only at a wrap. See [looping clips](../docs/features/looping-clips.md) |
| `daw clip join PROJECT CLIP CLIP...` | One clip of two or more clips of one track that plays what they played, the first kept with its ID and reference: note clips into one note clip from the first's start to the last's end, with every note that played where it played and a loop laid out as notes; pattern clips and audio clips (`tracks.T.audio.N`) only where they meet and are one music, the same pattern with the repeats summed, or the same file played on from where the one before leaves, as `audio split` left them; else refused with the reason |
| `daw synth add PROJECT TRACK [--patch NAME]`, `daw synth show PROJECT TRACK`, `daw synth set PROJECT TRACK PATH VALUE...`, `daw synth mod PROJECT TRACK SOURCE TARGET AMOUNT [--remove]`, `daw synth audition PROJECT TRACK [--notes C2,C3] [--velocity V] [--length-beats B] [--track-chain] [--output FILE] [--play]` | The Synth on a MIDI track: the plain saw or a patch attached, or a new MIDI track with it; the patch read; fields set by their paths in the patch as one undo step, a null removing a part, an oscillator's `unison`, `unison_detune_cents`, `unison_width_percent` and `table` among them; a matrix entry added, changed or removed; and notes rendered through the patch and its own effects to a WAV under `renders/auditions` with its peak, loudness and spectral centroid in the reply, and with `--play` also played now, one after another, through the running host (`note.preview`). The patch's effects are edited with `daw effect add\|remove\|move\|bypass` on `tracks.T.instrument.synth`. See `daw describe synth` |
| `daw patch list [WORDS]`, `daw patch show NAME`, `daw patch save PROJECT TRACK NAME [--description TEXT] [--tags a,b] [--replace]`, `daw patch load PROJECT TRACK NAME` | Patches: a Synth's sound as a YAML file, the twelve factory patches built into `daw` and the saved ones in the workspace library, `~/Music/AAW/library/patches/` or `library/patches/` under `AAW_WORKSPACE`, each named by the slug of its name; a track's synth saved there, over a saved name only with `--replace`, naming the song's patch after it; and a patch, by name or as a `.yaml` path, put in place of a MIDI track's whole synth in one undo step, the notes staying. See `daw describe synth` |
| `daw midi import PROJECT FILE [--track T] [--at BEAT]`, `daw midi export PROJECT CLIP FILE` | A Standard MIDI file of one part made into a note clip, on a MIDI track or a new one, with what the song cannot hold counted in the reply; a note clip's notes that play written as a type 0 file at 960 ticks a beat. The file's tempo is not taken. See `daw describe midi` |
| `daw undo`, `daw redo`, `daw batch PROJECT FILE [--label TEXT]` | History of a running host; a JSON list of commands as one step, which a label names in the change log and for undo |

The engine covers the whole song: the sampler (scheduling, choke groups, gates,
repitch, trim, reverse, downmix, pan laws), the Synth on MIDI tracks, the eight effects on tracks, returns
and the master, sidechains, pre- and post-fader sends, automation lanes, track
gain, pan, mute and solo, master gain and the end fade. `render` writes the
mix, a stem for each track and return, the snapshot and `report.json` with what
each effect did; `--track` renders a track or a return as its stem, with only
the tracks that key or feed it.

## The engine

A song is compiled into a program (`program.rs`): each track's voices, each
channel's effect chain with the envelopes of its lanes, the routing, and the
delays that align latency. A `Renderer` (`render.rs`) plays a program as one
stream, for export and for playback alike. Output does not depend on how the
stream is cut into blocks, and processing never allocates.

- **Devices** (`aaw-dsp`) were ported from the Python engine operation for operation:
  Butterworth filters designed as `scipy.signal.butter` designs them, RBJ
  equalizer bands, the state-variable filter that automation moves, the
  compressor, the look-ahead limiter and the tempo-synced delay. A lane whose
  points share one value is that static value.
- **The reverb** has the Python engine's impulse response except for its noise,
  which comes from a generator of its own (D40, D44), so a tail is statistically
  the same and not sample-identical. Its convolution is non-uniformly
  partitioned and adds no latency.
- **Latency.** Only the limiter delays its input. A track keyed by a source
  with latency renders its voices that much later, tracks are delayed to the
  slowest before their faders and sends, and the output trails the transport by
  the total; `daw status` reports it as `latency_frames`. An offline render
  runs that much longer and places every stem on the timeline.
- **A compile redoes only what changed.** Each track's voices, prepared pad
  audio and reverb kernels are kept from the last compile, so after a level or
  knob edit it takes under a millisecond. Pad audio a first compile lacks is
  repitched on several threads.
- **The Synth** (`aaw-dsp/src/synth.rs`) plays a MIDI track's notes from its
  patch. A program holds the track's notes, shared across patch edits, and
  the patch compiled with the lanes on its fields; the renderer holds the
  synth's state, 16 voices and 16 ringing out, allocated when it is built.
  Oscillators and the filter run every frame, and the envelopes, LFOs, matrix
  and tuning every 16 frames of a voice's own time, so the output does not
  depend on blocks and a render is the same bytes twice. A take-over carries
  the voices and glides the patch's values from the old patch's over 5 ms; a
  change of wave, filter mode or routing changes the program's structure and
  swaps through the dip. A locate chases the notes sounding there with their
  envelopes and free LFOs where time would have brought them. A track with a
  synth has no sample voices and no peaks of its own. Unison copies are
  spread in detune and width inside the voice and summed at the level of
  one; a wavetable (`aaw-dsp/src/wavetable.rs`) is one cycle as a stack of
  bandlimited levels, built in or read from a sample through the compile's
  cache; the patch's own effects (`chorus.rs`, `saturation.rs` and the rest)
  are a chain run on the voices before the inserts, whose latency the
  inserts count as upstream, and the render report lists them under the
  track's `instrument_effects`.
- **Patches** (`aaw-host/src/patches.rs`) are the synth mapping as a YAML
  file with a name, description, tags, `saved_by` and `saved_at` around it,
  validated as a song's synth is. The factory patches are `engine/patches/`,
  built into the binary; saved ones are files in the workspace library,
  named by the slug of the name, and one of a factory name shadows it.
  `patch.load` and `synth.add` with a patch are `instrument.set` of the
  mapping led by `patch: NAME`; `patch.save` writes the file and sets the
  song's `patch`.
- **Peaks** (`peaks.rs`) are what a track's voices sum to, before its inserts
  and fader, as the least and greatest sample of every 64 frames and of
  coarser stretches four times as long each, for a display to draw at any
  zoom. A track's program carries an identity of what its voices are made
  from, so peaks are worked out again only for the tracks an edit changed.
  `file_peaks` gives the same of a sample file as it is, read in blocks so
  that a whole song is never held, for an audio clip to draw.

## Session host

A host holds a song in memory as the authority for it, applies commands one at
a time, keeps an undo history and a change log, and plays the song with each
edit heard as it lands. `daw host PROJECT` runs one until interrupted or `daw
close PROJECT`; `daw play PROJECT` without a running host hosts the project for
as long as it plays. Each change prints to the host's stderr as a JSON line.

Every other command reaches the running host of its project through a Unix
socket registered under `~/Library/Application Support/AAW/hosts` (or
`AAW_HOST_DIR`), outside the project because projects may be synced. With no
host, commands run headless: load, apply, save, exit, under the project's
`.daw.lock`. Edits behave the same either way; undo, redo, the change
log, handles and the transport need a host.

- **Commands** validate the whole resulting song before they apply and carry an
  origin, `agent` by default or `--origin user`. `--expect SHA` refuses an edit
  over an unseen change. A host saves canonical `song.yaml` before it replies.
- **Values** are JSON, or text when they do not parse as JSON: `-4.5`, `true`,
  `1/3`, `C3`, `'{"sample": "kick"}'`. Fields of objects a command adds or
  changes are `--name value` pairs, such as `daw pad set PROJECT drums kick
  --gain-db -3 --release-ms 40`.
- **Paths** are dot-separated. A list item is an index, its `id` (a send: its
  `to`), or a handle: while a host runs, clips, effects, events and points have
  handles such as `@12` that keep naming the same object as other edits land.
  `daw inspect` lists each clip's reference, in the order the clips play, and
  `daw get` puts one on every list item.
- **Help and errors say what is accepted.** A command that takes `--FIELD VALUE`
  pairs lists them in its `--help` with each one's range and default, from the
  schema: `effect add --help` every effect type and its fields, `lane set
  --help` what a lane can move, `set --help` the commonest paths. An edit that
  names a field the song does not have is refused with the nearest one first,
  `` `bpm` is not a field; did you mean `session.tempo`? ``, then the model's
  message (`aaw_model::hint`). A word where a flag was wanted names the flag
  (`--type limiter`), and a render that would clip names the loudest stems and
  the paths to lower. Output to a pipe that closes, as `| head` does, ends with
  status 141 and nothing on stderr.
- **An agent that fell silent is hinted.** A host whose agent sent nothing for
  five minutes (`host::PACE`, counted from its last command of any kind) adds
  `hint` to the reply of the agent's next edit: work in steps the person can
  watch, a sound, part or clip a command as soon as it is decided, a batch one
  musical edit. Reads, undo and the person's edits carry none, and the hint is
  in no change. See [the feature's file](../docs/features/watchable-steps.md).
- **A batch builds on itself.** A track, return or pattern a batch adds can be
  added to by its later commands: a pad and a clip on a new track, steps and
  events in a new pattern, notes in a new note clip. A note clip or a note is
  given its ID when the command makes it (the next `clipN` in the song, the
  next `nN` in its clip), so a later command can name it; a command that makes
  several replies with each one's path.
- **References stay valid.** Renaming a track renames the sidechains naming it;
  renaming or removing a return updates or removes its sends and their lanes;
  inserting, moving or removing an effect rewrites or removes the lanes that
  address effects by index; a lane's last point takes the lane with it. Removing
  something still referenced, such as a pad a pattern plays, is refused.
- **Gestures.** A request may carry a `gesture` ID, as the app's drags do.
  Edits of one gesture and origin that land one after another are one undo
  step and one change-log entry, named for the whole move, and are saved once
  they pause for 250 ms instead of after each.
- **Selection.** `daw status` lists what the person has selected in the app as
  `{"ref", "path"}` pairs, so a request about "the selected clip" can be
  answered with `daw get PATH`. The app sets it with the `select` command.
- **Saved under another name.** `daw move` and `daw copy`, and Save As… in the
  app, are commands to the host (`project.move`, `project.copy`). It saves,
  moves or copies the folder to a path where nothing is yet, names the song
  after it, registers under the new path and records one change, which is not
  an undo step. History, handles and playback carry on. The host keeps
  answering at the path it had: a command sent there lands, its result's
  `project` is the new path and a `notice` on stderr says so. After a copy the
  original cannot be reached by its path until the host closes or is asked to
  release it (`project.release`). Without a host the two commands do the same
  to the files. [The feature's file](../docs/features/new-and-untitled-projects.md)
  has the rest.
- **The window in front.** The app tells its host which project's window is
  in front, with the `front` command, and `daw status` and `daw projects`
  report it.
- **External edits**, such as a text edit or `git checkout`, load as one
  undoable `external` change. A file that does not validate
  pauses edits, with the error in `daw status`, until it is fixed; `daw fmt`
  writes the host's song over it instead.
- **Playback** recompiles the song after each edit and the audio thread swaps
  to the new program between blocks, at the same beat; a tempo change keeps the
  beat. A program with the same channels and devices takes over in place:
  devices keep their state, so tails ring on; levels, pans, sends, mutes and
  the knobs automation can move glide to their new values over 5 ms; a filter
  or equalizer crosses over to its new setting; and where a track's voices
  changed, the old ones ring out under the new. A program with another
  structure, such as an added, removed or bypassed effect, another delay time
  or reverb response, or an added or removed track, is swapped in at the bottom
  of a 5 ms fade out and back in, and the devices still there keep their state.
- **The transport** moves only the voices. Stop, locate and loop jumps let the
  old position ring out for 5 ms without new hits and fade in voices picked up
  mid-sample; hits at the new position play in full. Effects run on, so a
  reverb rings through a locate and after a stop; the stream rests once it has
  been silent for a second. The audio thread never allocates, locks or blocks;
  replaced programs return to the host to be freed.

The host also accepts `{"op":"note.preview","track":"lead","pitch":"C4",
"velocity":100,"length_beats":1}`: a note played now through a MIDI track's
instrument, its Synth or its Sampler, and the track's chain, from where the
stream stands, whether or not the song plays, outside the timeline and the
undo history and in no render. The app's Synth panel sends it from its keys
and the piano roll as notes are drawn, clicked and moved (`Song.preview_note`),
and `daw synth audition --play` from the terminal. It opens the output if
nothing has played yet, and refuses a track with no instrument and a note
outside 0 to 127. A Sampler plays it as the same note in a clip would: the
host prepares the voice (`program::preview_voice`) and the player plays up to
16 such notes at once, the oldest giving its place; a pitch no map entry
plays is silent, and the reply's `sounds` is false. The player keeps
previewed notes apart from the song's: they survive a program swap, voices it
has finished with go back to the host to be freed, and while the transport
stands still the stream is cut by nothing, the end fade and the gate at the
song's end applying only while it rolls.

The host also accepts `{"op":"metronome","enabled":true}` (or false), sent by
`Song.set_metronome` from the app. `daw status` and transport observer updates
report `metronome`. This is per-open-project monitoring state, initially off:
it changes neither revision nor undo history. The real-time player synthesizes
the click from absolute quarter-note positions at the program's current BPM,
aligned with its output latency, after the song's master processing. The click
works with an empty song, follows stop, locate and loop, and is never rendered
into the mix or stems. There is no separate CLI metronome subcommand.

A process can embed a host instead of running `daw host`: `host::spawn` runs one
on its own thread and calls an observer with the opened song, each change with
the song after it, each revision's program once it is compiled, transport
changes, warnings and the close, while `Clock` gives the playhead straight from
the audio thread. The Mac app does this through `aaw-ffi`, whose `Song` turns
each revision into the arrangement the app draws (`view.rs`), with every
effect's fields as a control needs them, every lane's points and every
pattern's steps and events, names the tracks, clips, returns and sections a
change touched, and turns the person's edits into commands (`edits.rs`),
working out exact beats from the song for a clip moved by so many beats or
copied after itself, and for an event moved by steps of its pattern's grid.
A MIDI track is in the arrangement with its instrument's kind, its sampler's
pads and map, and its note clips, each with its notes and their handles. The
person's note edits work out exact beats from the piano roll's grid as typed,
so a note added or moved on a grid of thirds is on its third; Copy takes clips
or notes as they are, so that Paste works after the originals are gone. A
sample added in the app is an audio clip or the instrument of a MIDI track, a
Sampler, never a pad (D85). A sampler that is empty
or one pad on every note is also in the arrangement as the Sampler device
(`SamplerView`): its pad's fields as controls need them, from
`aaw_model::describe::PAD`, the sample's root note, and its file's identity,
length and the seconds played; the file's peaks are sent as an audio clip's
are. The edits load a sample into it as one batch, carrying the pad's
settings over, set a field of its pad, and set or take off the sample's root
note as typed, `C4` or `60`; `library_pitch` measures a file's pitch for it
through `daw samples analyze`. A track's audio clips are in the arrangement beside its pattern clips, each to
where it leaves and with how long it sounds after that, and so are the files
they play (`files.rs`): each file's length, and the beats of the map `daw
samples beats` left beside it, when that map is of the sample the song lists.
The person's edits of an audio clip keep its sound ending where it is drawn: an
end is trimmed to the fade out's length before the beat, and a longer fade out
starts earlier. An edit that would end an audio clip past the song's end by
more than the end fade lengthens the song to the end of that bar, in the same
step; `daw` commands leave the length to whoever sends them.
What a device panel shows of an effect type, or of the Sampler's pad, comes
from `aaw_model::describe`.
Such a host is reached through its socket like any other, and compiles each
revision as it lands so that play starts at once.

Waveforms follow each compile (`waveform.rs`): a thread of its own works out
the peaks of the tracks whose identity it has not seen, a few at a time, and
tells the app which audio each track has at the revision and the peaks it has
not been sent. Only the latest revision is worked on, and peaks are kept for a
while, so undo and redo find theirs. An audio clip draws its file, not its
track: each file's peaks are read once, in blocks, and sent by the file's
identity, and a track of audio clips alone has no peaks of its own to work
out.

A project as the app needs it is in `projects.rs`: a new Untitled project in
the app's data folder (`AAW_DATA_DIR`, or `~/Library/Application Support/AAW`),
the ones a crash left there, and whether one holds nothing and can be deleted
unasked. `Song` saves its project under another name and reports the new path
to its observer.

The browser's shared index is `library.sqlite` in `AAW_DATA_DIR` (otherwise
`~/Library/Application Support/AAW`), overridden by `AAW_LIBRARY` or an explicit
`samples --db PATH`. `daw samples folders` lists registered directories;
`folders add DIRECTORY` scans and registers one, `folders remove DIRECTORY`
forgets it without touching source files, and `folders refresh` rescans.
`samples scan DIRECTORY` also registers a source. `samples search QUERY
--folder DIRECTORY` scopes results, with `--folder` repeatable. This replaces
the project-relative default index; existing indexes remain usable with an
explicit override.

The app's sample browser asks Python (`library.rs`, `aaw_host::python`):
`daw samples search` for what it lists, and `daw samples import --copy-only`
to copy a chosen file into the project, which the song then takes as one edit
with the audio clip or the Sampler, and the track if it is new, that plays it.

`crates/aaw-engine/tests/synth.rs` holds a synth's render to playback, to
every block size and to itself, hears a lane on its filter, chases a locate,
hears a previewed note while stopped, across a take-over and past the song's
end, and plays under allocation checking; `crates/aaw-dsp/src/synth.rs` measures a
sine's pitch and level, the envelope, a glide, stealing and a take-over.
`crates/aaw-host/tests/patches.rs` renders every factory patch under full
scale and holds a patch saved from one song and loaded into another to the
same audition byte for byte.
`crates/aaw-host/tests` cover every command, handles, undo, batches, gestures,
the selection, external edits, concurrent clients, an embedded host, a project
moved and copied under a running host with its old path still answering,
Untitled projects, and a
real-time stress run: random edits compiled and
swapped in while a thread renders 128-frame blocks on schedule under allocation
checking. `tests/test_host.py` checks `inspect` and command results against
the model as Python reads it, and drives a `daw host` process from outside,
including a sample import that reaches the song through the host.
`tests/test_projects.py` does the same for a folder as PROJECT, `daw projects`,
`daw move` and `daw copy`.
`crates/aaw-ffi/tests` open a song as the app does and check the arrangement,
what each kind of change touches, the person's edits and their place in the
shared history, the fields a device panel is drawn from, edits of effects,
equalizer bands, lanes and points, the transport shared with an agent, the
waveforms that follow each kind of edit, patterns as the editor draws them,
edits of steps, events, lengths and grids, a sample added as a pad or a track,
and, where the checkout's Python is there, a search of a generated library and
a copy from it.

## The model

`aaw-model` is the one implementation of the document. It began as an exact port
of the Python model (pydantic and PyYAML): it accepts and rejects the same
documents, `save` writes the same bytes, and `project_sha256` and the legacy
fingerprints are the same, so existing projects, render reports and `--expect`
SHAs carried over. It reads YAML with PyYAML's YAML 1.1 rules (`010` is 8, `1e5`
is a string, `yes` is true, merge keys apply), applies pydantic's coercions, and
writes with a port of PyYAML's emitter.

`daw model PATH...` prints each document's canonical YAML and fingerprints, or its
validation errors as `{loc, type, msg}`, which are pydantic's. Before the Python
model was deleted the two agreed on a generated corpus of 6,133 documents: valid
songs, the same songs with one field broken, and YAML edge cases. What the Python
model did with 533 of them, and with 120 generated `fmt` and `apply` edits, is
pinned in `tests/golden_model.json`, and `tests/test_model.py` holds the Rust model
to it. `AAW_UPDATE_GOLDEN=1 uv run pytest tests/test_model.py` accepts a
deliberate change. `crates/aaw-model/tests` pin the fingerprints each earlier
schema wrote, and walk a song with one of every model beside the schema `daw
describe` prints: every field, default, limit and choice in it is what validation
enforces.

Differences from the Python model, all in input no tool writes:

- An escaped UTF-16 surrogate pair in a double-quoted string, such as
  `"\ud83c\udfb5"`, is rejected. PyYAML kept it as two lone surrogates; libyaml
  and the YAML spec reject it. JSON written with `ensure_ascii` produces these.
- Beats and note octaves accept ASCII digits only; Python also accepted other
  Unicode decimal digits.
- YAML syntax errors are reported in libyaml's words rather than PyYAML's.
- Error messages quote values with an approximation of Python's `repr` for
  unprintable characters.

## What holds the engine to the Python engine's sound

The engine was ported against the Python engine and compared with it until the
cutover. The last comparison, on October 1, 2026:

- **Without effects**, renders of 60 generated songs, with generated audio in
  every format the library reads (8- to 32-bit PCM, float and double WAV, AIFF
  and FLAC at 22.05 to 96 kHz), were byte-identical: `mix.wav`, every stem and
  the snapshot, with the same schedules.
- **With effects, sends, returns and automation**, over 60 generated songs the
  largest difference in any of 257 mixes and stems was 5.8e-11, and the reports
  agreed on each effect's latency, lanes and gain reduction.
- **Reverb** tails come from another noise generator (D40), so they were
  compared on what a tail is made of: the frame it starts on, its energy and
  width, and over several seeds its decay time and the level of each octave.
- **Copies of five local songs**: one was refused by both engines for clipping,
  with the same message; one without effects was byte-identical in all 11 files;
  in the other three every stem without reverb was within 6e-8, reverb returns
  within 0.45 dB and mixes within 0.02 dB in level.

Two details keep effect-free bytes equal to the Python engine's. The repitch is a
port of `resample_poly`, including `firwin`'s Kaiser window, Cephes `i0` and
numpy's pairwise sum; it matches scipy to a few units in the last place. Decoding
uses libsndfile, but the 24-bit mix is written here: Homebrew's libsndfile rounds
24-bit PCM differently from the build that Python's `soundfile` bundles, and
the engine writes `lrint(x * 2**31) >> 8` as the latter does.

With the reference gone, tests hold the engine to its own promises. The engine's
tests prove that output does not depend on block partition, with and without
effects, that playing from the middle equals the same frames of a render from
the start, that stems sum to the mix before master effects, that a preview is
its channel's stem, that float stems keep the bytes of the first renders, and
that processing never allocates. Each device has tests of what it does
(`crates/aaw-dsp`). `crates/aaw-engine/tests/player.rs` plays through the
transport: a song with latency against its render, levels and knobs stepped
thirty times a second without a click, a tail ringing through an edit, a locate
and a stop, a change of structure fading through silence, and a stopped stream
coming to rest. The Python suite drives `daw render`: `tests/test_renders.py`
renders generated songs whole, in other block sizes, as sections and as single
channels and holds the results to each other and to the song, and the effect,
routing and automation tests run chains and songs with generated audio.
