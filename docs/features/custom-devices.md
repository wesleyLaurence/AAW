# Devices anyone builds, to a spec — stage 1 implemented October 22, 2026

Asked for by the person on October 19, 2026 (D123): "we need some kind of
standard almost programming spec where it's like you can code a brand new
plugin it just has to meet this spec and then it can run ... the person
should literally be able to build and design a new part to the system, with
a UI elements and usable CLI interface for the agent. This DAW needs to be
completely flexible and programmable ... similar concept to Max for Live
where people can build things into Ableton, but I want it to be completely
agent driven". Backlog: Next, Devices anyone builds, to a spec. The first
stage, the manifest and the offline tier, is built (D144); the real-time
tier, instruments and MIDI effects are the stages below.

## What

A device is a folder, as a skill is: `device.yaml`, the manifest that names
it and declares its fields, and `device.py`, the code that makes its sound.
Dropped into the workspace's `devices/` (`~/Music/AAW/devices/`, or under
`AAW_WORKSPACE`) or a project's own `devices/`, named by its slug, it is in
the browser under Audio Effects as a plugin, on a track it is a panel whose
controls come from its fields, the agent has `daw effect add --type SLUG`
and `daw set` on its fields as on any effect, a lane moves a field the
manifest allows one on, it renders in playback and export alike, and the
folder is how it is shared. The agent writes one from a sentence: `daw
plugin new NAME` scaffolds a working device, the code is edited, `daw plugin
check` runs it once, and it is tried on the track in the same conversation.

## How it works

- **The manifest is the contract.** `device.yaml` has `format: 1`, `kind`
  (`effect`; `instrument` and `midi` are later stages), `name` (the title),
  `description`, `version`, `author`, `license`, `run` (`offline`; `realtime`
  is a later stage), `entry` (`device.py` unless said) and `fields`, each
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
  because `daw device` is the Mac's audio devices. `daw describe effects`
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
- **`daw check`** warns `device-missing` of a device the song names that is
  not installed and `device-version` of one installed at another version
  than the song was written with; a project's own folder shadows the
  workspace's of the same slug.

## Tested

- `aaw-model`: the kind validated, dumped and in the schema `daw describe`
  prints; a device refused in a rack and in a patch; a lane on
  `effects.REF.fields.F`.
- `aaw-dsp/src/device.rs`: the cache played by the timeline, silence past
  its end and before zero, pass-through stopped and without a cache, the
  signature telling a cache from none.
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
- `tests/test_devices.py`: the scaffold made, listed, shown and checked, and
  a broken manifest refused; a device in a chain rendering at its gain, the
  same bytes from the cache, a lane on its gain, the warmth's harmonics,
  values refused, and failing code refusing the render; added by slug from
  the terminal with fields and a lane; `daw plugin status`; the check's two
  warnings; the project's folder shadowing the workspace's.
- Swift: the panel's line for each state, and a plugin matched by name,
  slug, description or field.

## Limits

- Offline only: the device's code runs after an edit, not on the audio
  thread, so a change is heard when the run lands (a second or more on a
  long song) and a previewed note or a ringing tail passes through dry; the
  real-time tier is the next stage.
- What feeds a device is judged by the whole song: an edit anywhere, a
  fader on another track included, renders the device's input again, and
  the code runs again only when the input's identity changed. A finer
  dependency would spare the renders.
- A device has no latency and no sidechain, reads only its input and its
  fields, and knows the tempo but not the arrangement.
- One device's code runs at a time, in order; a failure is retried only
  when its identity changes (an edit of the device's code or fields, or of
  what feeds it), and the host does not retry a transient one on its own.
- A device is not offered in a rack or a Synth patch; a saved rack holding
  one loads only with `--flat`.
- No timeout on a run: code that never returns holds the host's work on
  devices, though not the host.

## Remaining stages

2. The real-time tier: DSP in a sandboxed language the engine compiles and
   runs on the audio thread, under the same manifest, after the spike the
   backlog names (FAUST, or a WebAssembly module from any language with a
   frozen ABI; no FAUST compiler is on this Mac, which is why the offline
   tier came first).
3. Instruments: note input to the device, the piano roll's preview through
   it.
4. MIDI effects, then the spec's version 1 written as the reference, with
   how a device is shared.

## Open questions

1. The language for real-time devices: FAUST, WebAssembly, or both with one
   manifest.
2. How much UI a device may own beyond the generic panel, and when: a curve
   over a spectrum, a meter, a sequencer row, as named widgets.
3. Versioning a device a song already uses: whether a song pins a version
   and carries a copy under its own `devices/`, as it copies samples. The
   song names the version it was written with and `daw check` says when
   the installed one differs; nothing copies yet.
4. Whether an offline device may read the arrangement (the clip's position,
   the sections) or only audio and fields; the tempo is given.
