# Effect racks (implemented 2026-10-07)

Status: built in the pull request that closed the last of the person's notes
of October 6, 2026 that remained, decision D93. Before it, a chain of effects
lived in one song on one row: the drum bus's compressor, EQ and limiter that
took an afternoon to set were set again in the next song, one effect and one
field at a time, while a Synth's sound had been a file in the library since
October 4.

## What

A rack is a chain of effects saved under a name as a YAML file outside any
song, so that a chain built on one track is added to a track in any song, as a
Synth patch is loaded into any song. Around the chain are a name, a
description, tags, who saved it and when:

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
field named. A rack has at least one effect. There are no factory racks.

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
daw rack load SONG tracks.perc drum-glue [--index 0]    # by name, by slug, or a .yaml path
```

`rack save` takes the chain's owner as `effect add` does: `tracks.T`,
`groups.G`, `returns.R`, `master` or `tracks.T.instrument.synth`, so a
Synth patch's own chain is saved too. It writes the chain under the name, as
the file above, with `saved_by` the command's origin, user or agent; a name
already saved is written over only with `--replace`, which keeps the file's
description and tags unless new ones are given. The song does not change:
unlike a patch, a chain has no field that says where it came from, so the
reply says `changed: false`. An empty chain has nothing to save and is
refused.

`rack load` adds the rack's effects to the owner's chain at `--index` or its
end, in one undo step named for the rack, and the reply lists the effects it
made. The chain that was there stays: a rack dropped on a track with effects
goes after them, as a device dropped in Ableton goes where it is dropped. Two
things are adjusted on the way in, each said under `also`:

- **An `id` the chain already has** is given a number, `glue-2`, then
  `glue-3`, so that `daw set tracks.T.effects.glue.threshold_db` keeps
  meaning the effect it meant.
- **A compressor's `sidechain`** names a track of one song. It is kept when
  the chain is a track's, a group's or a return's and the song has a track of
  that name that is not the owner itself; otherwise it is dropped, and the
  reply says to set it. A rack saved from a chain keyed by the bass therefore
  loads onto another track of the same song keyed the same, and onto the
  master, into a Synth patch or into a song without a bass unkeyed.

Lanes on the owner's effects by index follow the effects a rack pushes along,
as they do when an effect is added, and go back with undo. `rack list` lists
every rack with its name, slug, description, tags, the kinds of its effects
in order, its file and who saved it when; `WORDS` keep those with every word
in the name, a tag or an effect's kind, so `daw rack list limiter` finds the
racks with a limiter in them. A file in the folder that is not a rack is
named under `problems`, not fatal.

**In the app.** The browser's Audio Effects lists the person's racks under a
Racks heading after the effect kinds, each with its name and the kinds of its
effects, searched by name, tag and kind along with the kinds. A rack is
dragged onto a track's, a group's, a return's or the master's header, where
its effects go at the end of that row's chain; onto the device panel, the
same for the row shown; or onto an insertion strip between two devices,
where they go in at that place. + and a double-click add it to the selected
row. A drop anywhere else is refused, as an effect's is. Save Rack… in the
device panel's header, beside Add Effect, asks for a name, a description and
tags, as the Synth's Save As… does, and writes the row's chain to the library
over a rack of the name only when ticked; the browser lists it at once. The
button is gray while the chain is empty. The list is read again when the
window comes to the front, so a rack saved from the terminal shows up.

## Why

The person's note of October 6, 2026 asked for effect chains saved and loaded
anywhere, as a Synth patch is. A mix engineer's work is largely a few chains
they trust, a drum bus, a vocal chain, a mastering chain, and in every DAW
they are presets: Ableton's Audio Effect Rack saved to the User Library and
dropped on a track. Here an agent and a person share the same library, so a
chain the agent tuned by ear with the person becomes a file the agent names
in the next song with one command and the person drops from the browser.

## Design

- **A list, not a device.** A rack is the effects it holds, added to a chain
  one after another, rather than a container device with a chain inside it,
  as Ableton's rack is. The song's chain stays flat: every path, lane and
  command that reaches `effects.REF.FIELD` works on a loaded rack's effects
  as on any, and the engine gains nothing. What is lost is the rack as a unit
  afterwards, its macros and its removal as one; both wait for a need.
- **The chain is added, not put in place of what is there.** A drop in
  Ableton adds; the agent that wants the rack alone runs `daw effect remove`
  first or adds the rack to an empty track.
- **The file is validated by the song's own code**, `Effect::parse_chain`,
  the list validator the song uses for `effects` with the uniqueness of IDs
  the song's rules require, so the error for a bad field reads as the song's
  does.
- **A sidechain stays in the file** and is judged at load, so a rack saved
  from a keyed compressor still carries the key when it is loaded next to the
  track it names, and loads everywhere else unkeyed with a note, rather than
  being refused by the song's validation.
- **Racks and patches share their mechanics**, `slug`, the workspace folder
  and the timestamp, from `aaw-host/src/patches.rs`; a rack is a second list
  of files, `aaw-host/src/racks.rs`, with no factory entries and no field in
  the song.

## Done when

- A chain saved from a track in one song and loaded onto a track in another
  gives the same effects, with their IDs, their bypasses and the fields they
  had; a lane on a loaded effect reaches it by its ID.
- A rack saved from a chain keyed by another track loads onto a third track
  of the same song keyed the same, and onto the master or into another song
  unkeyed, with the reply saying so.
- A rack loaded onto a chain that has its IDs numbers them, and lanes by
  index on the effects after the insertion point follow them.
- `daw rack list WORDS` finds a rack by a word of its name, a tag or a kind;
  a file that is not a rack is reported, not fatal.
- In the app a rack is listed under Audio Effects, dropped on a header, the
  device panel and an insertion strip, added with + and a double-click, and
  saved with Save Rack…, each one undo step.

## Open questions

- **A rack as a unit afterwards.** Whether a loaded rack should be shown as
  one device whose effects fold, with macros over them as Ableton's rack has,
  rather than its effects laid into the chain. This is Ableton's rack proper
  and would need a device kind of its own; it waits until a chain of five is
  found hard to read.
- **Which chain a rack is saved from in the app.** The device panel saves
  the row shown. Whether the Synth panel wants a Save Rack… of the patch's
  own effects too, when `daw rack save tracks.T.instrument.synth` has it.
- **Loading over, not after.** Whether a drop with a modifier should put the
  rack in place of the chain, as Save… over a patch does.
