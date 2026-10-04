# The browser — implemented October 3, 2026

Status: implemented in [#48](https://github.com/wesleyLaurence/AAW/pull/48).
The [Sampler device](sampler-device.md) followed in #51.

## What

The left panel is a browser:
a panel that opens and closes, with a column of categories and, beside it, what
the chosen category holds.

- **Samples:** the library's samples by search, category and kind, as the panel
  shows them today.
- **Folders:** sample directories shared across projects and with the agent; select one or several to scope sample search.
- **Instruments:** Sampler, and the instruments that come after it.
- **Audio Effects:** filter, equalizer, compressor, limiter, delay and reverb,
  and the effects that come after them.

Anything in it is dragged to where it should go, or added to the selected row
with a double-click or the + by its name.

## Why

The person makes a sound the way they would in Ableton: open the browser, find
an instrument or an effect, and drag it onto a track. Samples, instruments and
effects share one browser, and sample folders stay available when projects change.

## Design

**The panel.** It is where the Samples panel is, opened and closed by ⌥⌘B and
by the mark at the left of the transport bar, as now, and is 360 points wide to fit navigation and results. On
its left a narrow column lists the categories, Samples,
Instruments, Audio Effects and Folders. Clicking one shows its contents to the right. The category last
shown comes back when the panel is opened again.

**What is listed comes from the engine.** The effects are the kinds the host
knows, with the label `daw describe effects` gives them, so an effect added to
the engine appears without a change to the app. The app's fixed list of kinds
for Add Effect (`DeviceChain.kinds`) is replaced by that list for both. The
instruments are the kinds `instrument` takes: `sampler` for now.

**Folders shared across projects.** Add Folder… opens a picker that accepts one
or several directories. These are registered and recursively indexed in
`library.sqlite` in the app data folder (`AAW_DATA_DIR`, otherwise
`~/Library/Application Support/AAW`). Both the app and CLI use this default;
`AAW_LIBRARY` overrides it, and `samples --db PATH` overrides one CLI call.
Existing project-local indexes remain usable with an explicit override; they are
not migrated automatically. Their source folders can instead be added again.

All folders are searched by default. Check one or several folder names to narrow
the results; All folders clears the scope. Samples and Folders show the same
sample results and filters. Refresh rescans for new, changed and removed files.
Unavailable folders remain registered, marked offline. Remove Folder in the
context menu forgets a source without deleting source audio or project copies;
files covered by another registered source remain searchable.

WAV, AIFF, FLAC, MP3 and M4A are indexed. Compressed files are decoded temporarily
for metadata. Only a sample used in a project is copied into that project.
Scanning and search run off the UI thread; scan errors are shown. Folder changes
made in the browser refresh the source lists in other open project windows.

The agent uses `daw samples folders [list|add DIRECTORY|remove DIRECTORY|refresh]`
and `daw samples search QUERY [--folder DIRECTORY ...]`. `samples scan DIRECTORY`
also registers the source. Both interfaces use the same index and folder scopes.

**Search.** The search field at the top searches the category shown. Samples are searched
by the index as now. Instruments and effects are filtered by name as the person
types, since they are a few.

**Dragging an effect.**

| Dropped on | Does |
|---|---|
| A track's, return's or the master's header | Adds it to the end of that row's chain |
| The device panel outside an insertion strip | Adds it to the end of the shown row's chain |
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
with the instrument, in one batch, so a drop is one undo step labeled as a menu item would
be, including "Add Sampler track" and "Attach Sampler". These are `Edit`s in
`aaw-ffi`, as the other drops are, so the agent sees the same changes in
`daw changes`.

**Kept as it is.** Dropping a sample on a header or the timeline does what it
does now (D66). The Add Effect menu stays in the device panel, for a person who
does not want to drag.

## Verification

Generated fixtures cover shared defaults across project directories, multi-folder
search, exact path boundaries, overlapping roots, safe removal, file symlinks,
unavailable sources and compressed audio. Host and Mac tests cover creating a
Sampler track, attaching it, refusing incompatible rows, inserting effects and
undo. Scripted window snapshots show Samples, Instruments, Audio Effects and
Folders, including a folder selection that narrows results.

The native folder picker and actual mouse drags onto headers and device insertion
strips still need a hands-on check. Folder changes are refreshed explicitly;
there is no filesystem watcher. Loading and editing a sample in the Sampler's
panel is the [Sampler device](sampler-device.md); the track header accepts a
sample too.

## Open questions

- **Which categories after these four.** Ableton also lists Drums, Clips,
  MIDI Effects and Plug-ins. Clips would hold MIDI files and
  saved clips. Folders already registers sample directories.
- **The person's own presets.** The concept puts the person's devices in
  `devices/` in the workspace. Whether an effect saved with its settings shows
  in the browser beside the built-in ones belongs with the workspace.
- **A drop on an audio track.** Ableton turns an instrument dropped on an audio
  track into a new MIDI track next to it. Refusing it is simpler; whether the
  person expects Ableton's behavior is to be asked.
- **Hearing an effect before it is added,** as Ableton's preview does, is left
  out.
