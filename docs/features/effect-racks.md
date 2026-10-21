# Effect racks and instrument racks (implemented 2026-10-07; the device and instrument racks 2026-10-21)

Status: saved racks were built on October 7, 2026 (D93), from the person's
note of October 6 asking for effect chains saved and loaded anywhere, as a
Synth patch is. Then a loaded rack dissolved into the chain the moment it
landed. The rack as a device, and the instrument rack, were built on October
21, 2026 (D139), from the person's notes of October 19: "An audio effect rack
that has a UI object. Like I should be able to create an audio fx rack, then
drag fx into it like a container and each effect should be collapsable to a
narrow view then able to expand back out", and "An instrument rack where you
can combine a midi instrument with FX into a savable reusable instrument".

## What

**A rack is a device.** `rack` is an effect kind: a device in a track's, a
group's, a return's or the master's chain that holds a chain of its own, with
one title, one `id`, one bypass and one place in the row:

```yaml
effects:
- type: rack
  id: drum-glue
  name: Drum Glue
  effects:
  - {type: compressor, id: glue, threshold_db: -18, ratio: 4}
  - type: eq
    bands:
    - {shape: highpass, freq_hz: 40}
  - {type: limiter, id: top, ceiling_db: -0.5}
```

The engine runs a rack's effects where the rack is, as if they were laid in
the chain: its sound is theirs, bit for bit, its latency is their sum, and
bypass on the rack bypasses them all. An effect inside is addressed through
the rack, `effects.RACK.effects.REF.FIELD`, `RACK` and `REF` each an id or an
index in its own list, for `daw set`, a lane (`effects.drum-glue.effects.glue.threshold_db`,
or `effects.0.effects.0.threshold_db`), `daw effect remove|move|bypass` and
`daw get` alike; `daw effect add`, `daw rack load` and `daw rack save` take
the rack as a chain's owner, `tracks.T.effects.RACK`. A rack's `name` is what
the window calls it; a rack loaded from a file is named for the file. A rack
holds no rack, and a Synth patch's chain holds none: the patch's chain is the
synth's own, and a rack goes on the track's chain. A chain and a rack each
hold up to 32 effects. The ids inside a rack are the rack's own: a chain may
have `glue` and a rack in it another `glue`.

**A saved rack** is a chain of effects as a YAML file outside any song, so
that a chain built on one track is added to a track in any song. Around the
chain are a name, a description, tags, who saved it and when:

```yaml
name: Drum Glue
description: A compressor over the kit, a highpass under it and a lid.
tags: [drums, bus]
saved_by: user
saved_at: '2026-10-07T20:12:40Z'
effects:
- {type: compressor, id: glue, threshold_db: -18, ratio: 4}
- type: eq
  bands:
  - {shape: highpass, freq_hz: 40}
- {type: limiter, id: top, ceiling_db: -0.5}
```

The chain is written as the song saves `effects`, fields at their defaults
left out, and validated as the song validates a chain, so a file with a wrong
kind, a value out of range or two effects of one `id` is refused with the
field named. A rack has at least one effect, and no rack in it: a chain with
racks is saved with each rack laid out into its effects, bypassed where the
rack was, since the file is one rack when it is loaded. There are no factory
racks.

**Where they live.** Saved racks are files in the workspace's library,
`~/Music/AAW/library/racks/`, or `library/racks/` under `AAW_WORKSPACE`,
beside `library/patches/`, made when the first rack is saved. A rack is named
by its file: the file's stem is the slug of the name, `drum-glue.yaml` for
Drum Glue, so a name may have spaces and capitals and the file has neither,
as a patch's is (D72). A `.yaml` file anywhere loads by its path too.

**Commands.**

```
daw rack list [WORDS]                                   # mine by name
daw rack show NAME                                      # the file: name, tags, who, when, effects
daw rack save SONG tracks.drums "Drum Glue" --description "…" --tags drums,bus [--replace]
daw rack save SONG tracks.drums.effects.drum-glue "Drum Glue" [--replace]   # a rack device's effects
daw rack load SONG tracks.perc drum-glue [--index 0] [--flat]   # by name, by slug, or a .yaml path
daw rack wrap SONG tracks.drums.effects.glue tracks.drums.effects.top [--name "Drum Glue"] [--id drum-glue]
daw rack unwrap SONG tracks.drums.effects.drum-glue
daw effect add SONG tracks.drums --type rack [--name "Drum Glue"] [--id drum-glue]
daw effect add SONG tracks.drums.effects.drum-glue --type saturation --drive-db 6
daw effect move SONG tracks.drums.effects.glue 0 --to tracks.drums.effects.drum-glue
daw effect move SONG tracks.drums.effects.drum-glue.effects.glue 9 --to tracks.drums
```

`rack save` takes the chain's owner as `effect add` does: `tracks.T`,
`groups.G`, `returns.R`, `master`, `tracks.T.instrument.synth` or a rack of
one of them, `tracks.T.effects.RACK`, whose effects alone it saves. It writes
the chain under the name, as the file above, with `saved_by` the command's
origin, user or agent; a name already saved is written over only with
`--replace`, which keeps the file's description and tags unless new ones are
given. The song does not change: unlike a patch, a chain has no field that
says where it came from, so the reply says `changed: false`. An empty chain
has nothing to save and is refused.

`rack load` adds one rack device to the owner's chain at `--index` or its end,
in one undo step named for the rack, with the file's effects inside it, its
`name` the rack's and its `id` the slug of the name, numbered when the chain
has it; the reply says `device: true` and the rack's handle. The chain that
was there stays: a rack dropped on a track with effects goes after them, as a
device dropped in Ableton goes where it is dropped. With `--flat`, and into a
rack or a Synth patch, where a rack cannot go, the file's effects are laid in
the chain instead, each keeping its ID unless the chain has it, when a number
is added, `glue-2`, then `glue-3`, so that `daw set tracks.T.effects.glue.threshold_db`
keeps meaning the effect it meant; the reply lists them under `handles` and
says `device: false`, and when the rack was laid in unasked, why. Either way
**a compressor's `sidechain`**, which names a track of one song, is kept when
the chain is a track's, a group's or a return's and the song has a track of
that name that is not the owner itself, and otherwise dropped with the reply
saying to set it. A rack saved from a chain keyed by the bass therefore loads
onto another track of the same song keyed the same, and onto the master, into
a Synth patch or into a song without a bass unkeyed.

`rack wrap` puts effects of one chain into a new rack where the first of them
was, in their order, with `--name` and `--id` the rack's; `rack unwrap` lays a
rack's effects out where it was, each bypassed if the rack was, an id the
chain has numbered. `effect move --to` moves an effect into a rack of its row,
or out of one into the row's chain, at the index given there; across rows an
effect is copied, not moved. In every case the lanes on the effects follow:
`effects.glue.threshold_db` becomes `effects.drum-glue.effects.glue.threshold_db`
on the way in and back again on the way out, a lane by index is renumbered,
and each change is said under `also`. Lanes on the owner's effects by index
follow the effects a rack pushes along, as they do when an effect is added,
and go back with undo. `rack list` lists every rack with its name, slug,
description, tags, the kinds of its effects in order, its file and who saved
it when; `WORDS` keep those with every word in the name, a tag or an effect's
kind, so `daw rack list limiter` finds the racks with a limiter in them. A
file in the folder that is not a rack is named under `problems`, not fatal.

**The render's report** lists a rack as an entry of its own, `type: rack` with
its id, name and bypass, and its effects under `effects` as the chain lists
them, each with its latency and, for a compressor, a limiter or a clipper,
what it took off; `daw listen` names such a device through its rack,
`tracks.drums.effects.drum-glue.effects.glue`.

**An instrument rack** is a MIDI track's instrument with its chain of effects
as a YAML file in `~/Music/AAW/library/instruments/`, or `library/instruments/`
under `AAW_WORKSPACE`, named by the slug of its name as a rack is:

```yaml
name: Warm Bass
tags: [bass]
saved_by: user
saved_at: '2026-10-21T20:12:40Z'
instrument:
  synth: {patch: Sub Bass, oscillators: {a: {wave: saw}}, …}
effects:
- {type: saturation, drive_db: 6}
- {type: compressor, threshold_db: -18}
```

A Sampler is saved with its pads and map as the song holds them, and under
`samples` the file each pad plays, by where the file was when the rack was
saved, with its hash, source and root note:

```yaml
instrument:
  sampler:
    pads: {t: {sample: tone, release_ms: 40}}
    map: [{notes: [0, 127], pad: t, pitched: true}]
samples:
  tone: {file: /Users/me/Music/AAW/projects/one/samples/tone.wav, sha256: …, root_note: A3}
```

```
daw instrument rack list [WORDS]                        # by name, tag, instrument or effect kind
daw instrument rack show NAME
daw instrument rack save SONG TRACK "Warm Bass" [--description …] [--tags bass] [--replace]
daw instrument rack load SONG TRACK warm-bass           # by name, by slug, or a .yaml path
```

`save` takes the track's instrument, required, and its whole chain, a rack in
it laid out into its effects; `load` is one undo step that brings each sample
the pads play into the project where the song does not have the file already,
by its hash, copied under `samples/` and listed as `samples.ID` with the
rack's name for it, numbered when the song has the name; puts the instrument
in place of the track's, the pads naming the song's samples; and adds the
effects as one rack device named for the file first in the track's chain,
when there are any. A Synth's own patch effects are the patch's and travel
inside the instrument; the rack's effects are the track's. The file's
`instrument` is validated as the song validates one, so a file with a wrong
field is refused with it named.

**In the app.** A rack is a frame around its effects' panels, in the device
panel where the rack is in the chain, under a title bar with the rack's name,
its bypass, a mark that folds every effect in it to a strip or opens them all
out, Save… for the rack's effects alone, Unrack, the moves and ×. Each
effect's panel, in a rack or in the chain, has a fold mark in its title that
turns it into a strip 28 points wide with its name along it, and a click on
the strip opens it out; the folds are the window's and are not saved. Between
a rack's effects are insertion strips as the chain has them: an effect or a
saved rack from the browser dropped on one goes in there, a saved rack laid
in as its effects; a drop on the rack's frame goes in at its end. An effect of
the chain dragged by its title onto a rack's strip moves into the rack, one
of the rack dragged onto the chain's strip leaves it, and a drag with Option
held copies it there, as between devices; a rack cannot be dragged into a
rack. Make Rack in an effect's menu wraps it in a new rack where it was, to
drag the others into, and Unrack in a rack's menu lays its effects out. Copy,
Paste, Duplicate and Delete act on an effect inside a rack as on any; Paste
and Duplicate land after the selected effect in its own list. A rack's
equalizer draws its curve and a rack's analyzer measures as on the chain.

The browser's Audio Effects lists Rack among the kinds, which adds an empty
rack, and the person's saved racks under a Racks heading after the kinds, each
with its name and the kinds of its effects, searched by name, tag and kind
along with the kinds. A saved rack is dragged onto a track's, a group's, a
return's or the master's header, where it lands as one device at the end of
that row's chain; onto the device panel, the same for the row shown; or onto
an insertion strip, where it goes in at that place. + and a double-click add
it to the selected row. Save Rack… in the device panel's header asks for a
name, a description and tags, as the Synth's Save As… does, and writes the
row's chain to the library, over a rack of the name only when ticked; the
browser lists it at once. The button is gray while the chain is empty.

The browser's Instruments lists the person's instrument racks under a Racks
heading after the patches, each with its name, its instrument and the kinds
of its effects. One is dragged onto a MIDI track's header or device panel,
where it loads as the track's instrument and one rack, or below the tracks,
where a new MIDI track named for it is made; + and a double-click load it
onto the selected MIDI track or a new one. Save Instrument… in the device
panel's header, on a MIDI track with an instrument, saves the instrument with
the chain the same way. The lists are read again when the window comes to
the front, so a rack saved from the terminal shows up.

## Why

The person's note of October 6, 2026 asked for effect chains saved and loaded
anywhere, as a Synth patch is. A mix engineer's work is largely a few chains
they trust, a drum bus, a vocal chain, a mastering chain, and in every DAW
they are presets: Ableton's Audio Effect Rack saved to the User Library and
dropped on a track. Here an agent and a person share the same library, so a
chain the agent tuned by ear with the person becomes a file the agent names
in the next song with one command and the person drops from the browser.

The notes of October 19 asked for the rack as a thing in the window: a chain
of five reads as a wall of panels, and a rack folds what is set and done out
of the way; more than that, a rack is the unit a sound is made of, and the
person wants to save a bass as the instrument and its treatment together and
drop it on a track in the next song as one object, as Ableton's `.adg` is.

## Design

- **`rack` is an effect kind with an `effects` list,** so a chain is still a
  list of effects and every path still works, lanes included; the engine
  lays a rack out into its effects at compile time (`Effect::flat`), so the
  sound of a rack equals the same effects laid in the chain and nothing in
  the audio path knows of racks. A lane's target gained the place inside a
  rack (`TargetKind::Effect { index, inner, band }`), and the engine keys a
  rack's lanes under the rack's index as `effects.J.FIELD`, as a patch's
  effects are keyed under the track's `instrument.` lanes.
- **One level.** A rack holds no rack, and a Synth patch's chain holds none.
  One level is what was asked for, keeps every address two deep and leaves
  the Synth's panel, whose chain is drawn by itself, as it is. Nesting would
  be a model change with a deeper address, and waits for a need.
- **A rack's ids are its own.** Uniqueness holds within each `effects` list,
  since an effect inside is always addressed through its rack; a saved rack
  loaded as a device therefore keeps every id it was saved with, and only a
  flat load numbers ids the chain has.
- **The chain's owner is a path,** so a rack is an owner like a track or a
  synth, `tracks.T.effects.RACK`, and `effect add`, `rack load` and `rack
  save` gained nothing but what the path reaches; the host finds the lanes
  of a rack's effects through the chain owner's automation with the rack's
  reference as a prefix, by its id and by its index.
- **The chain is added, not put in place of what is there.** A drop in
  Ableton adds; the agent that wants the rack alone runs `daw effect remove`
  first or adds the rack to an empty track.
- **The file is validated by the song's own code**, `Effect::parse_chain`,
  the list validator the song uses for `effects` with the uniqueness of IDs
  the song's rules require, so the error for a bad field reads as the song's
  does; an instrument rack's `instrument` by `Instrument::parse` the same way.
- **A sidechain stays in the file** and is judged at load, so a rack saved
  from a keyed compressor still carries the key when it is loaded next to the
  track it names, and loads everywhere else unkeyed with a note, rather than
  being refused by the song's validation.
- **An instrument rack is a file, not an instrument kind.** The instrument in
  the song stays `{synth}` or `{sampler}`, with every path and panel as it
  was, and the rack's effects land as a rack device in the track's chain,
  where every command and the window already reach them; what is one thing
  is the file, saved and loaded as one. A Synth patch's own effects stay the
  patch's: making them the rack's would touch every factory patch and the
  Synth's panel for a distinction nobody asked for. A Sampler's files travel
  by their place on disk and their hash, so a rack saved on this Mac loads
  into any project here, and a file the song already has, by its hash, is
  not copied twice.
- **Racks, instrument racks and patches share their mechanics**, `slug`, the
  workspace folder and the timestamp, from `aaw-host/src/patches.rs`; saved
  racks are `aaw-host/src/racks.rs` and instrument racks
  `aaw-host/src/instruments.rs`, with no factory entries.
- **The window keeps the folds,** not the song: a fold is how the person
  looks at a chain, as a group's fold is (D92), and the agent has no use for
  it.

## Done when

- A chain with a rack of three renders bit for bit as the chain with the
  three laid in, with a lane on a field inside it through the rack, and
  bypass on the rack renders as the chain without them, with their latency
  gone: `a_rack_renders_as_its_effects_laid_in_the_chain`.
- An effect inside a rack is set, automated by id and by index, added,
  bypassed, moved and removed through the rack; a rack is wrapped, unwrapped
  and moved into and out of with every lane following; a rack in a rack and a
  rack in a patch are refused: `aaw-host/tests/racks.rs`.
- A saved rack loads as one device named for it, and `--flat` as before; a
  chain with a rack saves with the rack laid out.
- An instrument rack saved from a Sampler track with its files and chain
  loads into another song with the file brought in once, or used where the
  song has it, the instrument playing it and one rack first in the chain.
- In the app a rack is a frame around its effects with strips between them,
  each folded to a strip and opened out, a saved rack from the browser lands
  as one device, Make Rack and Unrack are in the menus, and an instrument
  rack is listed under Instruments and loaded by a drop: scripted runs and
  the FFI's tests, `a_rack_is_one_device_with_its_effects_inside_it` and
  `an_instrument_rack_is_listed_and_loaded_from_the_browser`.

## Limits

- One level: a rack holds no rack, and a patch's chain holds none.
- No macros over a rack's fields, which Ableton's rack has; an Ideas line.
- A rack cannot be dragged onto another row's header; an effect is copied
  across rows with Option, as before.
- The Synth panel's own chain has no racks and no folds.
- The browser's Racks headings list what the library holds; there are no
  factory racks or instrument racks.

## Open questions

- **Which chain a rack is saved from in the app.** The device panel saves
  the row shown, or one rack of it. Whether the Synth panel wants a Save
  Rack… of the patch's own effects too, when `daw rack save
  tracks.T.instrument.synth` has it.
- **Loading over, not after.** Whether a drop with a modifier should put the
  rack in place of the chain, as Save… over a patch does.
- **Whether a rack's folds should be saved with the project,** as the groups'
  fold is not.
