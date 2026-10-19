# Devices anyone builds, to a spec — proposed October 19, 2026

Status: proposed as a direction, not designed in full and not built. Backlog:
Next, Devices anyone builds, to a spec. Asked for by the person on October 19,
2026: "we need some kind of standard almost programming spec where it's like
you can code a brand new plugin it just has to meet this spec and then it can
run ... the person should literally be able to build and design a new part to
the system, with a UI elements and usable CLI interface for the agent. This
DAW needs to be completely flexible and programmable ... similar concept to
Max for Live where people can build things into Ableton, but I want it to be
completely agent driven". The concept already holds three tiers of tools the
agent writes, and the backlog a spike on a sandboxed DSP language (Later ›
Tools and devices the agent writes); this names what the spec has to be for
those tiers to become shareable parts of the system.

## What

A device is a folder, as a skill is: a manifest that names it and declares
its fields, the code that makes its sound, and nothing the app has to be
rebuilt for. Dropped into the workspace's `devices/`, it appears in the
browser under Audio Effects or Instruments, on a track it is a panel whose
controls come from its fields, the agent has `daw effect add --type NAME`
and `daw set` on its fields as on any effect, lanes move its continuous
fields, it renders in playback and export alike, and a zip of the folder is
how it is shared. The agent writes one from a sentence: "a phaser into
saturation into a rhythmic chopper" becomes a folder in `devices/`, tried on
the track in the same conversation.

The primitives such a device is built from and sits among: samples, clips,
tracks, groups, returns, devices (instruments and effects, built-in and
these), racks, patches, lanes, sections, markers, references and skills.

## Why

The DAW is not to be limited to what ships in it, and the agent is what makes
the wall between user and plugin developer fall: anyone who can describe a
sound can have the device that makes it, if there is a spec to write it to
and the engine runs what meets it. Everything the agent makes that way should
be a file a person keeps and hands on, as patches and racks are. Without a
spec, each device the agent writes is a one-off script whose sound is a sample
(the first tier) and nothing accumulates.

## Design, as far as it goes

- **The manifest is the contract.** `device.yaml`: `kind` (effect or
  instrument), `name`, `description`, `version`, `author`, `license`, and
  `fields`, each with a type (`db`, `hz`, `percent`, `beats`, `ms`, `switch`,
  `choice`), a range, a default, an interpolation (linear or log) and whether
  it can be a lane. `daw describe effects` lists the device from it, the
  panel is generated from it, and `daw set` validates against it, so a device
  needs no Swift and no Rust to appear everywhere a built-in does.
- **The code is in one of two tiers**, as the concept has them. An
  **offline** device is a script in a known Python environment, `process(in,
  sample_rate, fields) -> out`, run when its input or fields change and
  cached; heard from cache and exact in export. A **real-time** device is DSP
  in a sandboxed language the engine compiles and runs on the audio thread,
  the spike's candidate being FAUST, whose compiler emits code with the
  block-process signature the engine's effects already have, no allocation
  in the process call, and whose language cannot reach the file system or the
  network. Both tiers share the manifest, so a device can begin offline and
  be rewritten real-time with the same fields.
- **The panel** is the generic effect panel: a knob or a bar for each field
  in manifest order, a menu for a choice, the diamond for a lane, the bypass
  and the id, which every built-in effect without a drawing already gets.
  A device may add a drawing later through a small set of named widgets
  (a curve over a spectrum, a meter, a sequencer row), not arbitrary UI code
  at first.
- **Where devices live.** `~/Music/AAW/devices/NAME/`, and a project's own
  under its `devices/` for one song; the browser lists both. A song that uses
  a device names it by name and version, and `daw check` says when a device
  the song names is not installed. Sharing is the folder zipped; the license
  travels in the manifest and the record of dependencies (Shipping) lists
  what a device pulls in.
- **The agent's part** is a skill, not code in the app: how to write a device
  to the spec, try it on the track, listen with `daw listen` and iterate,
  with `daw device new NAME --kind effect` scaffolding the folder and `daw
  device check NAME` validating the manifest and compiling the code.
- **What stays out.** Third-party plugin hosting (AU, VST) remains out, as the
  concept says; this is the system's own device format. Devices do not reach
  the arrangement; a device that wants to change notes is a MIDI effect
  (Next), which this spec grows to cover after audio effects and instruments.

## Plan

1. The spike the backlog already names: FAUST compiled to a block-process
   function loaded by the engine, one gain device end to end, its cost in
   latency and in the audio thread measured, and whether the sandbox holds.
2. The manifest, the generic panel from it, and `daw effect add --type NAME`
   for an offline Python device; one shipped example, generic, in the
   repository as a test fixture.
3. The real-time tier on the spike's result; the browser's listing; `daw
   device new|check`.
4. Instruments: note input to the device, the piano roll's preview through it.
5. MIDI effects, then the spec's version 1 written as the reference, with how
   a device is shared.

## Done when

- A device folder with a manifest and a FAUST or Python file, dropped into
  `devices/`, appears in the browser, is added to a track from it and from
  `daw`, shows a panel of its fields, takes a lane on one, and renders in
  export as it plays, in tests over a generated device.
- The agent, given a description and the device skill, writes a working
  device in one session on a scratch project.

## Open questions

1. The language for real-time devices: FAUST, a WebAssembly module from any
   language with a frozen ABI, or both with one manifest. The spike answers
   for FAUST; WebAssembly is the alternative if FAUST's reach is too short.
2. How much UI a device may own beyond the generic panel, and when.
3. Versioning a device a song already uses: whether a song pins a version and
   carries a copy under its own `devices/`, as it copies samples.
4. Whether an offline device may read the arrangement (the tempo, the clip's
   position) or only audio and fields.
