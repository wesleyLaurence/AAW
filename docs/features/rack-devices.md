# A rack as a device, and an instrument rack — proposed October 19, 2026

Status: proposed, not built. Backlog: Next, A rack as a device, and an
instrument rack. Asked for by the person on October 19, 2026: "An audio
effect rack that has a UI object. Like I should be able to create an audio fx
rack, then drag fx into it like a container and each effect should be
collapsable to a narrow view then able to expand back out", and "An
instrument rack where you can combine a midi instrument with FX into a savable
reusable instrument". Saved racks are built and lay their effects into the
chain ([effect racks](effect-racks.md), D93), whose open question "A rack as
a unit afterwards" this answers.

## What

A rack is a device in a chain that holds a chain: one title, one bypass, one
place in the row, and inside it its effects as columns, each folded to a
narrow strip with its name turned sideways or opened out to its fields, as
Ableton's rack folds. Effects are dragged into it from the browser or from the
chain around it, and out again; the rack is saved and loaded as racks are,
and a saved rack loaded from the browser arrives as a rack device, not as
loose effects.

An instrument rack is the same container on a MIDI track with an instrument
in it, a Synth or a Sampler, and effects after it, saved as one thing and
loaded onto any MIDI track: a bass with its compressor and saturation, a key
with its chorus and reverb.

```yaml
effects:
- type: rack
  id: glue
  name: Drum Glue
  effects:
  - {type: compressor, threshold_db: -18, ratio: 4}
  - {type: limiter, ceiling_db: -0.5}
```

```yaml
instrument:
  type: rack
  name: Warm Bass
  instrument: {type: synth, patch: ...}
  effects:
  - {type: saturation, drive_db: 6}
```

## Why

A chain of five reads as a wall of panels; a rack folds what is set and done
out of the way. More than that, a rack is the unit a sound is made of: the
person wants to save a bass as the instrument and its treatment together, and
drop it on a track in the next song as one object. Ableton's `.adg` is this.
Patches today carry a Synth's own effects ([synth](synth.md)) but nothing
carries a Sampler's, and a saved rack today dissolves into the chain the
moment it lands.

## Design

- **`rack` is an effect kind** with an `effects` list, so a chain is still a
  list of effects and every path still works: `effects.glue.effects.0.ratio`
  addresses inside it, lanes included. The engine flattens a rack into its
  effects at build time, so the sound of a rack equals the same effects laid
  in the chain, and its latency is their sum; bypass on the rack bypasses them
  all. Nesting a rack in a rack is allowed by the model and discouraged by
  the window.
- **The instrument rack is an instrument kind**, `rack`, with one
  `instrument` and `effects`; a MIDI track's `instrument` takes it as it
  takes a Synth or a Sampler. Its effects run before the track's chain, as a
  Synth's patch effects do, so a patch's effects and an instrument rack's are
  one idea with two homes; whether to fold the patch's effects into this is
  open question 1.
- **Saved files.** A rack file gains nothing: `library/racks/NAME.yaml`
  already holds a name and effects, and now loads as a rack device by default,
  with `--flat` to lay the effects in as today. Instrument racks save beside
  patches as `library/instruments/NAME.yaml`, with the instrument and its
  effects, listed in the browser's Instruments under a Racks heading as the
  effects' Racks are.
- **The window.** A rack's panel is a frame around its effects' panels, with
  the rack's title bar carrying name, bypass, fold-all and Save…; each inner
  panel has a fold mark that turns it into a strip of 28 points with the name
  along it; the folds are the window's and not saved. A drop on the rack's
  frame adds the effect to its end; a drop on an insertion strip inside it
  puts it there; an effect dragged out of the frame onto the chain's strip
  leaves the rack. Make Rack on a selection of effects wraps them; Unrack
  lays them out. Reordering by drag (Next) applies inside a rack as in the
  chain.
- **Commands.** `daw effect add --type rack`, `daw rack load` making one,
  `daw rack unwrap EFFECT` and `daw rack wrap EFFECT..`; `daw instrument rack
  save|load`.
- **Macros** over a rack's fields, which Ableton's rack has, are not in this
  stage.

## Done when

- A chain with a rack of two effects renders bit for bit as the chain with
  the two laid in, in a Rust test; bypass on the rack silences their effect;
  a lane on a field inside it moves it.
- A saved rack dragged from the browser lands as one device in a scripted
  run, its panel framed with two folded strips after the fold mark is clicked.
- An instrument rack saved from a MIDI track and loaded on another plays the
  same notes to the same stem.
- [effect-racks.md](effect-racks.md) rewritten as the reference with the
  device in it.

## Open questions

1. Whether a Synth patch's own effects should become an instrument rack's,
   leaving the patch to the synth alone. It would make one idea of two, and
   touch every factory patch.
2. Whether a rack's inner folds should be saved with the project, as the
   groups' fold is not.
3. Macros, and when.
