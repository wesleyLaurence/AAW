# Devices anyone builds, to a spec — stages 1 and 2 implemented October 22, 2026, stages 3 and 4 October 23

Asked for by the person on October 19, 2026 (D123): "we need some kind of
standard almost programming spec where it's like you can code a brand new
plugin it just has to meet this spec and then it can run ... the person
should literally be able to build and design a new part to the system, with
a UI elements and usable CLI interface for the agent. This DAW needs to be
completely flexible and programmable ... similar concept to Max for Live
where people can build things into Ableton, but I want it to be completely
agent driven". Backlog: Next, Devices anyone builds, to a spec. The first
stage, the manifest and the offline tier, is built (D144), the second, the
real-time tier (D145), the third, instruments (D146), and the fourth, MIDI
effects (D147); the spec's version 1 and FAUST are the stages below.

## What

A device is a folder, as a skill is: `device.yaml`, the manifest that names
it and declares its fields, and the code that makes its sound: `device.py`,
run offline over the whole song, or `device.wasm`, built from `device.rs`
and run on the audio thread.
Dropped into the workspace's `devices/` (`~/Music/AAW/devices/`, or under
`AAW_WORKSPACE`) or a project's own `devices/`, named by its slug, it is in
the browser under Audio Effects as a plugin, on a track it is a panel whose
controls come from its fields, the agent has `daw effect add --type SLUG`
and `daw set` on its fields as on any effect, a lane moves a field the
manifest allows one on, it renders in playback and export alike, and the
folder is how it is shared. The agent writes one from a sentence: `daw
plugin new NAME [--realtime]` scaffolds a working device, the code is
edited, `daw plugin check` runs it once (`daw plugin build` compiles a
real-time one first), and it is tried on the track in the same
conversation.

An instrument is the same folder with `kind: instrument`: a MIDI track's
notes play it in place of a Sampler or the Synth, `daw instrument set
PROJECT TRACK SLUG` puts it there, the browser lists it under Instruments,
and `daw plugin new NAME --instrument [--realtime]` scaffolds a working
synth of a wave, an envelope and a tone.

A MIDI effect is the same folder with `kind: midi`: it sits among a MIDI
track's `midi_effects` with the arpeggiator and the chord, its code is
given the notes and returns the notes to play, `daw effect add PROJECT
tracks.T --type SLUG` puts it there, the browser lists it under MIDI
Effects, and `daw plugin new NAME --midi` scaffolds one that transposes
each note and echoes it.

## How it works

- **The manifest is the contract.** `device.yaml` has `format: 1`, `kind`
  (`effect` unless said, `instrument` or `midi`), `name` (the title),
  `description`, `version`, `author`, `license`, `run` (`offline` unless
  said, or `realtime`), `entry` (`device.py` offline and `device.wasm` in
  real time unless said, a file in the folder), `source` (a real-time
  device's `.rs` file, which its entry is built from) and `fields`, each
  named in lowercase letters, digits and underscores, with a `type` among
  `db`, `hz`, `percent`, `ms`, `beats`, `number`, `switch` and `choice`. A
  numeric type takes `min`, `max` and `default`, a `scale` (`linear`, or
  `log`, which `hz` is unless said) and `lane` (whether a lane may move it,
  yes unless said); a `switch` a `default` of `true` or `false`; a `choice`
  its `choices` and a `default` among them; `label` and `unit` are optional.
  The manifest is read by the host (`aaw-host/src/devices.rs`), which
  refuses one that does not read in words naming the part.
- **In the song** a device is `{type: device, name: SLUG, version: V, fields:
  {F: value}}`, a fourteenth effect kind of the model (`aaw_model::Device`),
  in a track's, a group's, a return's or the master's chain and not in a rack
  or a patch. The song checks the shape, a slug, a version as text, and a
  map of numbers, booleans and choices by field names of the right form, so
  a song whose device is not installed still loads, plays dry and renders. A
  host checks the values against the manifest after every edit
  (`devices::check_song`): a field the device has not, a number outside its
  range, a choice it lacks, a switch given a number, a lane on a field the
  manifest gives none, each refused in words that name the field and what
  it takes. `daw effect add OWNER --type SLUG [--F VALUE]` makes the entry
  from the manifest's defaults under the fields given, with the manifest's
  version; `--type device --name SLUG` is the same. The address of a field
  is `effects.REF.fields.F`, for `daw set` and a lane alike.
- **The offline tier.** `device.py` defines `process(audio, sample_rate,
  fields)`: `audio` a float64 array of shape (frames, channels), the
  device's input as the song makes it over the whole song; `fields` each
  name to a float, a bool or a string, or to a (frames, 1) float64 array of
  one value a frame where a lane moves it, which broadcasts against the
  audio; the result an array of shape (frames, channels), (frames, 1) or
  (frames,), longer than the input if it has a tail. The host runs it in the
  checkout's Python (`AAW_PYTHON`, or the `.venv`) through
  `agent_daw.device_runner`, which loads the module from its file, reads the
  input, hands the fields and the lanes over, and writes the output as
  float32 WAV at the song's rate; what it raises is the device's error.
- **What the device is given** is rendered by the engine
  (`aaw-engine/src/custom.rs`): the song with the owner's chain cut before
  the device, the device and the effects after it gone with the lanes on
  them, compiled for the owner's stem with the tracks that key its
  sidechains and the devices before it playing along, and tapped before the
  owner's fader (`Renderer::tap_before_faders`, now on groups and returns
  too), each block laid by the timeline frame it starts at, so frame N of
  the input is the device's input at timeline frame N whatever the latency
  before it. The master's device is given the mix before the end fade.
- **The cache.** The result is `.daw/devices/IDENTITY.wav` under the
  project, stereo float32 at the song's rate, where the identity is a
  SHA-256 over the cut song's canonical text, the device's own entry and the
  lanes on it, the hash of its folder's files, the hashes of every device's
  code in the song (a device fed by another whose code changed is made
  again) and the sample rate. A compile reads the caches from
  `Cache::set_devices`, by owner and index; the device's `Plan` carries one
  and its signature counts it, so a new cache swaps in through the dip. In
  the chain the device is `aaw_dsp::device::Offline`: rolling, it plays the
  cache by the timeline frame of its input, silence past the cache's end;
  without a cache, or while the transport stands still (a previewed note,
  a ringing tail), the input passes through.
- **The host's work** (`devices::Work`): after every compile it reads the
  song's devices in the order their inputs can be made (tracks, groups each
  after the groups in it, returns, the master, within a chain in order),
  loads the caches that are there, drops those of devices changed or gone,
  and runs one device's code at a time off its thread with the caches so
  far; a run that lands is loaded, the song compiled again and played with
  it, old cache files swept. What each device is doing is `custom_devices`
  in `daw status` (`ready`, `computing`, `waiting`, `failed` with the
  reason, `missing` with the reason, `off` for one bypassed or on a frozen
  track), a `custom_devices` line on the host's feed, and
  `SongObserver::devices` for the app. A failure is kept by identity until
  something changes. `daw render` and a freeze settle the caches first,
  synchronously (`devices::settle`), running what is not yet made; a device
  whose code fails refuses the render and the freeze with the reason, so an
  export is never silently dry.
- **The terminal.** `daw plugin list [--project P] [WORDS]`, `daw plugin
  show SLUG`, `daw plugin new NAME [--project P]` (the scaffold: a gain, a
  tone, a warmth, a mode and a switch, with the contract in its docstring),
  `daw plugin check SLUG` (the manifest read and the code run once on noise
  at the defaults) and `daw plugin status PROJECT`. The group is `plugin`
  because `daw audio-device` is the Mac's audio devices. `daw describe effects`
  has the `device` entry; `daw effect add --help` names `--type SLUG`.
- **In the app** a device's panel is the generic effect panel from the
  manifest: a bar for each numeric field with its range and unit, a menu
  for a choice, a switch, the diamond for a field a lane may move, titled
  as the manifest names it, with a line under the fields for what the host
  is doing with it or why it is not installed. The browser lists the
  plugins installed under Audio Effects, searched with the effect kinds,
  each dragged by its slug onto a header, the device panel or an insertion
  strip and added with + or a double-click; the Add Effect menu lists them
  too.
- **The real-time tier** (`run: realtime`). `device.wasm` is a
  WebAssembly module meeting the device ABI, version 1
  (`aaw-engine/src/wasm.rs`): it imports nothing and exports `memory`,
  `aaw_abi() -> i32` returning 1, `aaw_init(sample_rate: f64, tempo: f64)`,
  `aaw_fields() -> i32`, where the engine writes an `f64` for each field in
  the manifest's order before every call (a number as itself, a switch as 1
  or 0, a choice as the index of its choice), `aaw_left() -> i32` and
  `aaw_right() -> i32`, 128 `f32` frames each, and `aaw_process(frames:
  i32, frame: i64, rolling: i32)`, which processes 1 to 128 frames in
  place; `frame` is the timeline frame of the first, and `rolling` 0 while
  the transport stands still and a tail rings out. An optional
  `aaw_latency() -> i32`, read after `aaw_init`, declares a latency of up
  to ten seconds, which the chain compensates as a built-in device's. The
  host makes the code ready with `Installed::code`, a few milliseconds:
  wasmtime compiles the module with Cranelift (once for the same bytes),
  the exports are checked, and it is instantiated and run once on silence
  at the song's rate and tempo, each failure refused in words. The engine
  gives each place the device is in a chain an instance of its own, made
  off the audio thread (`aaw_dsp::device::Live`, `Running`); a block is
  given in pieces of 128 frames, each piece with the fields at its first
  frame, a lane on a log field moving on a log scale. An instance replacing
  another of the same code takes its state, so a tail rings on through an
  edit. Every call has a deadline: a thread advances the engine's epoch
  every 50 ms and a call past four ticks is stopped. A call that traps,
  passes its deadline or gives samples that are not numbers ends the
  instance, which passes its input through delayed by its latency; the
  code keeps the reason, every later instance passes through too, and
  `daw status` says `failed` with it until the device's folder changes. An
  instance's memory is capped at 256 MB.
- **The scaffold and the build.** `daw plugin new NAME --realtime` writes
  the manifest with `run: realtime`, `entry: device.wasm` and `source:
  device.rs`, and `device.rs`: Rust with the standard library, a `Device`
  with `new(sample_rate, tempo)`, `latency()` and `process(left, right,
  fields, frame, rolling)` to edit, the same gain, tone, warmth, mode and
  switch as the Python scaffold, and the ABI's glue under a line that says
  to leave it. `daw plugin build SLUG` compiles the source with rustc
  (`AAW_RUSTC`, else rustup's under `~/.cargo/bin`, else the path's) as
  `--edition 2024 --crate-type cdylib --target wasm32-unknown-unknown -C
  opt-level=3 -C panic=abort -C strip=symbols`, a single file with no
  Cargo project, then checks it; a missing target is said with `rustup
  target add wasm32-unknown-unknown`. `new --realtime` builds at once when
  it can and says why not when it cannot. `daw plugin check` on a
  real-time device runs a second of noise at 48 kHz through an instance at
  the defaults and replies its latency and its load, the share of real
  time one instance takes on this Mac, and `stale` when the source was
  changed after the module was built.
- **The host's work on a real-time device** loads its code where an
  offline device's run would start, keyed by the folder's hash, the rate
  and the tempo, and hands it to the engine with the offline caches
  (`Made::Code` beside `Made::Audio`, by owner and index), so an offline
  device fed by a real-time one is given its sound. `daw render` and a
  freeze load it too and refuse on code that does not load. Once a second
  the host reads the song's device folders for a change, so a build, or an
  edit of an offline device's code, is heard without an edit of the song.
- **Instruments** (`kind: instrument`, D146). On a MIDI track the
  instrument is `{device: {name: SLUG, version: V, fields: {F: value}}}`
  (`aaw_model::Instrument::Device`, the same `Device` with no ID and no
  bypass), a third kind beside the sampler and the Synth. `daw instrument
  set PROJECT TRACK SLUG` (`instrument.set` with a string,
  `devices::instrument_node`) makes it from the manifest's defaults; `daw
  set tracks.T.instrument.device.fields.F` sets a field and a lane on
  `instrument.fields.F` moves one, each held to the manifest as an
  effect's are (`check_instrument`). An instrument in a chain is refused
  with `daw instrument set` named, and an effect as an instrument with
  `daw effect add`; replacing the instrument keeps only the lanes on the
  fields the new one lets a lane move. The track's notes as they play,
  after its MIDI effects (the Synth's cached `track_notes`), are its input;
  its sound goes to the track's inserts as the Synth's does, its latency
  counted before them.
  - **Offline**, `device.py` defines `play(notes, frames, sample_rate,
    fields)`: `notes` a numpy structured array of `start`, `end`, `pitch`
    and `velocity`, a row a note in the order they start, `start` and `end`
    in frames of the song; `frames` the song's length; `fields` as an
    effect's. It returns the sound over the whole song, cached under
    `.daw/devices/` by an identity of the notes in frames, its entry and
    the lanes on its fields, its code, the song's length, tempo and rate,
    and added to the track by the timeline while the transport rolls. The
    host gives it the notes and no input (`compute_instrument`), and it
    comes first among its track's devices, so an effect after it is given
    its sound.
  - **In real time**, `device.wasm` also exports `aaw_note(at: i32, key:
    i32, velocity: i32)`, which the engine calls before `aaw_process` for
    each note starting or ending within the frames given, in order, `at`
    the frame among them and a velocity of 0 an end; the buffers hold
    silence, and the module writes its sound there. Code that lacks it is
    refused for an instrument, and `Code::new` plays it a note to check it.
    `aaw_dsp::instrument::Feed` makes the events of each block from the
    notes: a note ends at its end or the song's, an end comes before a
    start at the same frame, a seek ends what is held and starts again the
    notes sounding there, a stop ends what is held, and at most 256 notes
    are held, the longest held ending first past that. A live edit keeps
    the instance and its voices: a field changed carries a held note on, and
    notes changed end what was held where the edit lands.
  - **Previews.** `note.preview`, the piano roll's notes, plays through a
    real-time instrument whether or not the song plays: a start now, an end
    after its length. An offline one plays only from the timeline; the
    reply says `sounds: false` with `why`, as it does while the code is
    not ready.
  - **The terminal and the app.** `daw plugin list` says each plugin's
    `kind`; `daw plugin check` plays an instrument a chord held half a
    second and a note above it over a second at 48 kHz and replies its
    peak, and a real-time one's load; `daw plugin status` lists it as
    `tracks.T.instrument`. In the app its panel is first in the track's
    device panel, the manifest's fields as a device's in a chain with a
    diamond for a lane, titled as the manifest names it, with a line for
    what the host is doing (`TrackView.plugin`); the browser lists
    instruments under Instruments › Plugins and effects under Audio
    Effects › Plugins, an instrument dragged onto a MIDI track's header or
    device panel, or below the tracks for a new MIDI track named after it.
    `daw map` names it `plugin SLUG`, and `daw check` takes it to sound
    each note at its pitch.
- **MIDI effects** (`kind: midi`, D147). Among a MIDI track's
  `midi_effects` the entry is `{type: device, name: SLUG, version: V,
  fields: {F: value}}` with an `id` and a `bypass`
  (`aaw_model::MidiEffect::Device`), beside the arpeggiator, the chord,
  the scale and the velocity. `daw effect add PROJECT tracks.T --type
  SLUG`, or `--type device --name SLUG`, puts one there from the
  manifest's defaults when the slug names a MIDI effect, and refuses it
  off a MIDI track; `daw effect remove|move|bypass` and `daw set
  tracks.T.midi_effects.REF.fields.F` work as on any MIDI effect, each
  value held to the manifest (`check_midi`). A MIDI effect in a chain or
  as an instrument is refused in words, as an effect or an instrument
  among the MIDI effects is. It runs offline only (`run: realtime` is
  refused in the manifest), and no lane moves its fields (`lane: true` is
  refused), as with the built-in kinds, since it runs on the notes before
  they become frames.
  - **The contract.** `device.py` defines `transform(notes, fields,
    tempo)`: `notes` a numpy structured array of `start` and `end` in
    beats from the song's start (float64), `pitch` and `velocity`, a row a
    note in the order they start, as the clips play them through the MIDI
    effects before it (`schedule::midi_effect_input`); `fields` each name
    to a float, a bool or a string; `tempo` in beats a minute. It returns
    the notes to play in any order, a structured array of those fields or
    rows of four numbers. The runner and the host hold every note to a
    start at 0 or after, an end after it, a whole pitch 0 to 127 and a
    whole velocity 1 to 127, at most 200,000 notes, and keep the times to
    1/960 of a beat (`TICKS_PER_BEAT`), a note at least one tick long.
  - **The cache.** The notes are kept under `.daw/devices/IDENTITY.json`
    as ticks, the identity a SHA-256 over the notes it is given in exact
    beats, its entry without its ID, its code and the tempo, so an edit
    elsewhere in the song runs nothing. What it made reaches the engine as
    `Made::Notes` among the caches, by `(tracks.T.midi_effects, I)`
    (`custom::midi_owner`); `custom::attach` sets them on the copy of the
    song a compile plays (`Midi::made`, never written), and
    `midi_effects::apply` plays them in the device's place only while
    their key, of the slug, the version, the fields and the very notes the
    device is given, matches. Until then, when the run fails, when it is
    bypassed or not installed, the notes pass through as written. So the
    sampler, the Synth, an instrument of the person's own, a freeze and a
    render play what it made, and an offline device after the instrument
    is given it: every offline identity counts the notes the MIDI effects
    made.
  - **The host's work** reads a track's MIDI effects of the person's own
    first, before its instrument and its chain, and reads the song's
    devices again with what each made while that changes
    (`devices::walk` for a render, a freeze and the headless status,
    `Work::refresh` for a host), so a device after it is judged by what it
    is now given. `daw plugin status` lists one as
    `tracks.T.midi_effects.REF`. `daw plugin check` runs it on a chord
    held two beats and a note above it at 120 BPM and replies the notes in
    and out, their range of pitches and the first eight in beats.
  - **The terminal and the app.** `daw plugin new NAME --midi` writes the
    scaffold, a transpose, echoes of each note at a spacing in beats, each
    softer by a decay, moved by an octave or not, and the note itself or
    not, with the contract in its docstring; `--realtime` is refused with
    it. `daw map` names the MIDI effects a track's notes pass through, a
    plugin by its slug. In the app its panel stands among the MIDI effects
    before the instrument, titled as the manifest names it, its fields
    from the manifest with no diamond, and a line for what the host is
    doing; the browser lists MIDI effect plugins under MIDI Effects ›
    Plugins, dropped on a MIDI track's header or device panel and added
    with + or a double-click, and Add Effect lists them on a MIDI track.
- **`daw check`** warns `device-missing` of a device the song names that is
  not installed and `device-version` of one installed at another version
  than the song was written with, in a chain, among a MIDI track's MIDI
  effects or as its instrument; a project's own folder shadows the
  workspace's of the same slug.

## Tested

- `aaw-model`: the kind validated, dumped and in the schema `daw describe`
  prints; a device refused in a rack and in a patch; a lane on
  `effects.REF.fields.F`.
- `aaw-dsp/src/device.rs`: the cache played by the timeline, silence past
  its end and before zero, pass-through stopped and without a cache, the
  signature telling a cache from none.
- `aaw-engine/tests/wasm.rs`, over modules written as WebAssembly text: a
  module played in blocks of any size without allocating, a choice's
  index, a default and a value clamped to its range; a wrong ABI, an
  import, a missing export, a buffer outside the memory, a latency outside
  its range and a module that traps at once each refused in words; a trap,
  a loop stopped at its deadline and samples that are not numbers ending
  the instance with its input passing through, delayed by its latency;
  an instance taking over another's state; and in a song, the device at
  its gain in place of its input and a lane moving its field.
- `aaw-engine/tests/custom.rs`: the cut keeps the chain before the device
  and its lanes; the input equals the cut chain's freeze at two block
  sizes; pass-through without a cache; a cache played in the device's place;
  a bypassed device not run; a cache keyed elsewhere not heard; the
  master's input the mix before the fade and a silent master cache a silent
  mix; a group's input its track's stem.
- `aaw-host/src/devices.rs`: the manifest's every refusal; a project's
  device before the workspace's; the code hash; the effect node from the
  manifest. `aaw-host/tests/devices.rs`: added by slug and by `device` with
  a name; every refusal of a field, a lane, a rack and a missing device in
  words; the saved form; the headless states; and, with the checkout's
  Python, the check, the cache equal to the input at the gain, the freeze
  equal to the cache, a changed field made again by `Work` off the thread
  with the old cache swept, a bypass turning it off, and code that raises
  failing the strict settle and said in the lenient one.
- `aaw-host/tests/devices.rs`, with rustc for WebAssembly: the real-time
  scaffold built and checked, added by slug, settled as code, frozen at its
  gain, loaded by `Work` as ready, a rebuild seen in its folder and its
  check refusing code that traps once playing, and the trap said as
  `failed` once the engine ran it.
- `tests/test_devices.py`: the scaffold made, listed, shown and checked, and
  a broken manifest refused; a device in a chain rendering at its gain, the
  same bytes from the cache, a lane on its gain, the warmth's harmonics,
  values refused, and failing code refusing the render; added by slug from
  the terminal with fields and a lane; `daw plugin status`; the check's two
  warnings; the project's folder shadowing the workspace's; a real-time
  plugin made and built from the terminal, rendering at its gain with no
  cache, lanes on its gain and on its tone, `ready` in `daw plugin status`,
  `stale` after its source changed, a source that does not compile refused
  with rustc's words, and an offline plugin with nothing to build.
- Swift: the panel's line for each state, an instrument's included, and a
  plugin matched by name, slug, description or field.
- Instruments. `aaw-dsp/src/instrument.rs`: notes starting and ending at
  their frames whatever the blocks, an end before a start at one frame, a
  seek ending what is held and starting what sounds, a stop, a preview
  standing still and let go, a take-over ending the old notes before the
  new, and an offline instrument's cache added by the timeline and
  playing no preview. `aaw-engine/tests/wasm.rs`: code without `aaw_note`
  refused for an instrument; notes given at their frames across pieces of
  128 without allocating; in a song, the note at its gain from its beat to
  its end, a lane on its gain, a preview standing still; a note carried
  through a field's edit and let go when it is removed.
  `aaw-host/tests/devices.rs`: set by slug, its fields and lanes held to
  the manifest, refused in a chain and an effect refused as one, its lanes
  dropped when the Synth replaces it, first among its track's devices; an
  offline one checked, settled to a cache silent before its notes and
  sounding through them and fed to the effect after it, made again when a
  note changes, its failure refusing the render; a real-time one built
  from the scaffold, checked with sound, settled as code, frozen silent
  before its notes and sounding through them, ready in the work.
  `tests/test_devices.py`: both made from the terminal, set by slug, and
  rendered with the pitches of their notes (A4 then E5), the offline one
  silent after its release and the real-time one with no cache.
- MIDI effects. `aaw-model/src/midi_effects.rs`: a device passing the notes
  through with nothing made, playing what was made of its very input,
  passing through for another input, another field or a bypass.
  `aaw-host/src/devices.rs`: `kind: midi` read, `run: realtime` and a lane
  refused. `aaw-host/tests/devices.rs`: the scaffold made and `--realtime`
  refused; added by slug and by `device` with a name among the MIDI
  effects; its fields held to the manifest; refused off a MIDI track, in
  a chain, as an instrument, and an effect and an instrument refused among
  the MIDI effects; first on its track among the instances by its own
  owner, a bypass not heard; saved and its headless state; with the
  checkout's Python, the check, the written notes before it is made, the
  notes made an octave up and echoed in the schedule and the compile's
  Synth, the cache a `.json`, the tilt after the Synth judged by them, a
  note's change made again by `Work` with the old notes swept, code that
  raises refusing the strict settle with the notes passing through in the
  lenient one, and a pitch out of range refused. `tests/test_devices.py`:
  made, checked and listed from the terminal, added by slug, a render's
  stem at 880 Hz for an A4 an octave up and again softer two beats later,
  `ready` in `daw plugin status`, refused off a MIDI track and as an
  instrument, named by `daw map`, and once its folder is gone warned of by
  `daw check` and passing the note through.

## Limits

- An offline device's code runs after an edit, not on the audio thread, so
  a change is heard when the run lands (a second or more on a long song)
  and a previewed note or a ringing tail passes through dry; a real-time
  device has neither limit.
- A real-time device's source is Rust, one file, built by rustc; a module
  built from any other language that meets the ABI plays, but nothing
  builds it for the person. Its fields change every 128 frames, so the
  code glides what would click. Growing its memory in `process` is not
  free of allocation on the audio thread; the scaffold makes everything in
  `new`. The deadline stops a stall only after the audio has broken for
  150 to 200 ms. A failure is kept until the device's folder changes; the
  host does not try the code again on its own.
- The app's entitlements ask for unsigned executable memory, which
  wasmtime's compiled code needs under the hardened runtime a Developer ID
  signs with; nothing has been signed with one to see it run.
- What feeds a device is judged by the whole song: an edit anywhere, a
  fader on another track included, renders the device's input again, and
  the code runs again only when the input's identity changed. A finer
  dependency would spare the renders.
- An offline device has no latency; neither tier has a sidechain, and a
  device reads only its input and its fields, and knows the tempo but not
  the arrangement.
- One device's code runs at a time, in order; a failure is retried only
  when its identity changes (an edit of the device's code or fields, or of
  what feeds it), and the host does not retry a transient one on its own.
- A device is not offered in a rack or a Synth patch; a saved rack holding
  one loads only with `--flat`. An instrument of the person's own is not
  saved in an instrument rack's file as anything but its entry: the
  folder is not carried with it.
- An instrument is given notes, their velocities and its fields: no pitch
  bend, controllers, aftertouch or the arrangement. An offline one plays no
  preview and is computed again on any change of its notes. Two notes of
  one key overlapping are given as they come, two starts and two ends, and
  what the code makes of that is its own. `daw synth audition` is for the
  Synth alone.
- No timeout on a run: code that never returns holds the host's work on
  devices, though not the host.
- A MIDI effect runs offline: an edit of the notes it is given is heard
  with its result once the run lands, the notes as written meanwhile, and
  a note played now (the piano roll's previews, the Synth's keys) does not
  pass through it, as through no MIDI effect. Its fields take no lane. It
  is given the notes, their velocities, its fields and the tempo: no
  controllers, and not the sections or the clips the notes came from.
- `daw check` and `daw listen`'s `sounded` read a track's notes through the
  built-in MIDI effects only: a MIDI effect of the person's own passes the
  notes through for them, since they read the song without the host's
  caches.

## Remaining stages

5. The spec's version 1 written as the reference, with how a device is
   shared.
6. FAUST as a way to make a real-time device: a `.dsp` source built to a
   module of the same ABI, where the FAUST compiler is installed.

## Open questions

1. Answered (D145): WebAssembly, under the engine's own ABI, built from
   Rust; FAUST later as another source for the same ABI.
2. How much UI a device may own beyond the generic panel, and when: a curve
   over a spectrum, a meter, a sequencer row, as named widgets.
3. Versioning a device a song already uses: whether a song pins a version
   and carries a copy under its own `devices/`, as it copies samples. The
   song names the version it was written with and `daw check` says when
   the installed one differs; nothing copies yet.
4. Whether an offline device may read the arrangement (the clip's position,
   the sections) or only audio and fields; the tempo is given.
