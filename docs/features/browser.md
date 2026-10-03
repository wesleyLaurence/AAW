# The browser — proposed October 3, 2026

Status: proposed, not built. Backlog item 1. The [Sampler device](sampler-device.md),
item 2, is dragged out of it.

## What

The Samples panel on the left of the window becomes a browser, as in Ableton:
a panel that opens and closes, with a column of categories and, beside it, what
the chosen category holds.

- **Samples:** the library's samples by search, category and kind, as the panel
  shows them today.
- **Instruments:** Sampler, and the instruments that come after it.
- **Audio Effects:** filter, equalizer, compressor, limiter, delay and reverb,
  and the effects that come after them.

Anything in it is dragged to where it should go, or added to the selected row
with a double-click or the + by its name.

## Why

The person makes a sound the way they would in Ableton: open the browser, find
an instrument or an effect, and drag it onto a track. Today a sample is the only
thing that can be dragged in. An instrument is attached only by dropping a
sample on a MIDI track's header, and an effect only from the Add Effect menu in
the device panel, one row at a time.

## Design

**The panel.** It is where the Samples panel is, opened and closed by ⌥⌘B and
by the mark at the left of the transport bar, as now, and keeps its width. On
its left a narrow column lists the categories, each with an icon: Samples,
Instruments, Audio Effects. Clicking one shows its contents to the right. The category last
shown comes back when the panel is opened again.

**What is listed comes from the engine.** The effects are the kinds the host
knows, with the label `daw describe effects` gives them, so an effect added to
the engine appears without a change to the app. The app's fixed list of kinds
for Add Effect (`DeviceChain.kinds`) is replaced by that list for both. The
instruments are the kinds `instrument` takes: `sampler` for now.

**Search.** The search field searches the category shown. Samples are searched
by the index as now. Instruments and effects are filtered by name as the person
types, since they are a few.

**Dragging an effect.**

| Dropped on | Does |
|---|---|
| A track's, return's or the master's header | Adds it to the end of that row's chain |
| The device panel, between two devices | Adds it to the shown row's chain at that place, with a line where it will go while it is dragged |
| Anywhere else | Nothing; the pointer shows that it will not be taken |

**Dragging an instrument.**

| Dropped on | Does |
|---|---|
| A MIDI track's header, or its device panel | Makes it the track's instrument, in place of the one it had; the notes are kept |
| Under the tracks | A new MIDI track with that instrument, named after it |
| A track that is not MIDI, a return or the master | Nothing |

A Sampler dropped this way is empty: `{sampler: {pads: {}, map: []}}`, which
the model already takes, and which plays nothing until a sample is dropped on
it ([Sampler device](sampler-device.md)).

**Double-click and +.** Add the item to the row selected, as a drop on its
header would. With no row selected, an instrument makes a new MIDI track and an
effect does nothing.

**Edits.** An effect is the host's `effect.add` with an index, as Add Effect
sends now. An instrument is `instrument.set`, and on a new track `track.add`
with it, in one batch, so a drop is one undo step labeled as a menu item would
be: "Add Reverb to drums", "Attach a Sampler to keys". These are `Edit`s in
`aaw-ffi`, as the other drops are, so the agent sees the same changes in
`daw changes`.

**Kept as it is.** Dropping a sample on a header or the timeline does what it
does now (D66). The Add Effect menu stays in the device panel, for a person who
does not want to drag.

## Done when

- The browser opens and closes, shows each category, and remembers the last.
- An effect dragged onto each kind of header and between two devices lands
  there, as one undo step; an instrument dragged onto a MIDI track's header,
  its device panel and under the tracks attaches or makes a track; drops on
  the wrong rows are refused while dragging.
- Double-click and + add to the selected row.
- `./build.sh test` covers the edits each drop makes, and the window is seen in
  a picture with each category shown. A drag by hand is a line under Verify,
  since scripted input cannot start one.
- `apps/mac/README.md` describes the browser, and this file is rewritten as its
  reference.

## Open questions

- **Which categories after these three.** Ableton also lists Drums, Clips,
  MIDI Effects, Plug-ins and Places (folders). Clips would hold MIDI files and
  saved clips; Places would browse a folder from the Finder.
- **The person's own presets.** The concept puts the person's devices in
  `devices/` in the workspace. Whether an effect saved with its settings shows
  in the browser beside the built-in ones belongs with the workspace (item 5).
- **A drop on an audio track.** Ableton turns an instrument dropped on an audio
  track into a new MIDI track next to it. Refusing it is simpler; whether the
  person expects Ableton's behavior is to be asked.
- **Hearing an effect before it is added,** as Ableton's preview does, is left
  out.
