# Mac app

The native macOS app of [docs/archive/Rust-Swift-Update.md](../../docs/archive/Rust-Swift-Update.md),
through its last milestone, M9, and what was built since: it opens on a blank
project or on one it is given, shows its arrangement with what
each clip plays as a waveform, plays it with its effects and automation, shows
each change as it lands, whoever makes it, and lets the person edit the mixer,
the clips, the tracks and returns, each row's effects and its automation lanes,
and each pattern's steps and events. An audio file dropped on the timeline is an
audio clip, which is moved, trimmed, faded and split there. A MIDI track's note
clips are made, moved, trimmed and copied, and their notes drawn and edited in a
piano roll; a MIDI file dropped on the timeline is a note clip, and a note clip
is exported as one. A browser finds
samples in the library's index and adds them to the song as audio clips and as
Samplers on MIDI tracks. Save As… gives a project a
name and a place. The bundle holds `daw` and
the libraries it needs, and a menu item puts `daw` on the PATH.

The app holds no model logic. Opening a project makes the app its session host
([engine/README.md](../../engine/README.md)): the Rust core runs inside the app,
and `daw` commands from a terminal reach that same host, so the person and the
agent share one song, one transport and one undo history. An edit in the window
is a command to that host, as an agent's is. `daw status` shows `"host": true`
while a song is open, and lists what the person has selected.

## Build and run

It needs Xcode 16.2 (Swift 6), Rust and libsndfile, as the engine does, and
targets macOS 14.

```sh
./build.sh                          # build/AAW.app
open -a build/AAW.app               # a new Untitled project
open -a build/AAW.app path/to/project   # its folder, or the song.yaml in it
./build.sh test                     # the Swift tests
./build.sh dist                     # build/AAW-0.1.0.zip, to share
```

`build.sh` builds the Rust core as a static library, generates the Swift
bindings from it with UniFFI, wraps both as an XCFramework under `Generated/`,
builds the Swift package and `daw`, and assembles and signs the bundle.
Bindings and build output stay out of Git. After a first `build.sh`, `swift
build` is enough to compile Swift-only changes, and Xcode opens `Package.swift`
directly; the app to run is the one `build.sh` assembles.

If the checkout lives in a synced folder, exclude `.build`, `build` and
`Generated` from syncing, as with the engine's `target`. For Dropbox: `xattr -w
com.dropbox.ignored 1 .build build Generated`.

## The bundle

| In `AAW.app/Contents` | Holds |
|---|---|
| `MacOS/AAW` | The app, with the Rust core linked into it |
| `Helpers/daw` | The `daw` binary, built as the tests build it |
| `Frameworks` | libsndfile and the seven libraries it links (Ogg, Vorbis and its encoder, FLAC, Opus, mpg123 and LAME), copied from Homebrew |
| `Resources/Licenses` | The licenses of those libraries |

The app and `daw` load the libraries from `Frameworks`, so the bundle runs from
any folder, and on a Mac without Homebrew. `build.sh` stops if anything in the
bundle still links outside it and the system, or was built for a newer macOS
than `Info.plist` says the app runs on, which Homebrew's libraries are when the
Mac that builds is newer than that. It then runs the bundle's `daw` once. The
bundle is for the kind of Mac that built it, Apple silicon or Intel, not both.

The sample library, sample analysis and perception are Python and are not in
the bundle. The bundle's `daw` and the app's browser run them in the `.venv` of
the checkout the bundle was built from, or the Python `AAW_PYTHON` names. On a
Mac with neither, `samples`, `listen`, `compare`, `check`, `timeline`, `joins`, `export` and the browser say
that Python was not found, and every other command works.

## The command line tool

**AAW › Install Command Line Tool…** makes `/usr/local/bin/daw` a link to the
bundle's `daw`. That folder is on the PATH of every shell, so a terminal or an
agent can then run `daw` from any folder, and its commands for a song open in
the app reach the app. The folder belongs to the system, so macOS asks for an
administrator's password. The app says what it will change first:

| At `/usr/local/bin/daw` | The menu item |
|---|---|
| Nothing | Offers to make the link |
| A link to this app's `daw` | Says so, and offers to remove it |
| A link to anything else, such as a copy of the app that has moved | Says where it points, and offers to point it here |
| A file that is no link | Leaves it, and says to move it away |

The link is to where the app is, so moving the app breaks it: choose the item
again from the app's new place. A downloaded app that has not been moved out of
its folder is run by macOS from a temporary copy, and asks to be moved first.
To make the link by hand, or in a folder of your own:

```sh
ln -s "$PWD/build/AAW.app/Contents/Helpers/daw" ~/bin/daw
```

In this checkout, keep to `uv run daw`. It runs the engine as it was last
built, where the link runs the copy made when the app was last built.

## Sharing the app

```sh
AAW_SIGN_IDENTITY="Developer ID Application: NAME (TEAM)" AAW_NOTARY_PROFILE=aaw ./build.sh dist
```

| Variable | Does |
|---|---|
| `AAW_SIGN_IDENTITY` | Signs the libraries, `daw` and the app with this certificate, the hardened runtime and a timestamp. Without it they are signed ad hoc |
| `AAW_NOTARY_PROFILE` | In `dist`, has Apple notarize the zip and staples the ticket to the app. It names a keychain profile, made once with `xcrun notarytool store-credentials aaw --apple-id ID --team-id TEAM` |

`dist` writes `build/AAW-VERSION.zip`. A notarized app opens on another Mac
as any downloaded app does. An ad hoc one is refused there until its user
allows it under Privacy & Security in System Settings. The app is not for the
Mac App Store, whose sandbox would keep `daw` from the app's socket and the app
from songs in folders of the person's choosing.

Signing with a certificate and notarizing have not been run: the Mac this was
built on has no Developer ID. What was tried in their place is in the plan's
[progress](../../docs/archive/Rust-Swift-Update.md#progress). Three things to know
before sharing a build:

- An ad hoc signature cannot use the hardened runtime: macOS then refuses the
  bundle's libraries, which have no team to match the app's. A certificate
  gives every part the same team.
- The binaries hold paths of the Mac that built them: the checkout, where
  `daw` looks for Python, and Cargo's folders, in the text of error messages.
- The libraries' licenses are in the bundle; those of the Rust crates the
  engine uses are not gathered, among them Signalsmith Stretch (MIT), which
  the engine links for time stretching. The `rubberband` program a song can
  name instead is not in the bundle.

## Projects

A project is a folder with `song.yaml` in it.
[docs/features/new-and-untitled-projects.md](../../docs/features/new-and-untitled-projects.md)
describes all of this; in short:

| | |
|---|---|
| Launch | Opens a blank project called Untitled, 120 BPM and 32 bars, kept in `Untitled/` of the app's data folder until it has a name. There is no welcome window, and closing the last window quits |
| File › New (⌘N) | Another Untitled project, in a window of its own |
| File › Open… (⌘O), Open Recent | A project's folder or its `song.yaml`. Open Recent is the app's own index of the projects it knows: a project whose folder was moved is found again, one that is gone is listed and cannot be chosen, and an Untitled project a crash left is there |
| File › Export Audio… (⇧⌘R) | Asks for a name and a place, with the format (WAV 24- or 16-bit, AAC, MP3), the level (as rendered, a true peak, a loudness or a gain) and the ceiling under the name, and writes the mix there with the bundle's `daw export`, rendering first when the song has changed; the panel opens in the project's `exports` folder with the song's title as the name, asks about a file that is there, and offers the last format and level next time. The transport bar says "Exporting NAME…" while it runs and the window stays usable; a banner then shows the length, loudness, true peak, gain and warnings with Show in Finder, or the command's reason when nothing was written. The song does not change ([export in the app](../../docs/features/export-in-app.md)) |
| File › Export MIDI Clip… (⇧⌘E) | With one note clip selected, asks for a name and a place and writes the notes that play as a MIDI file, as `daw midi export` does |
| File › Save As… (⇧⌘S) | Asks for a name and a place. An Untitled project moves there. A project that has a name is copied, and the window carries on in the copy. Undo, the selection and what is playing carry on, and `daw` commands sent to the old path still land in the window |
| Closing an Untitled project | One that holds nothing is deleted. One that holds something asks: Save…, Delete or Cancel. Quitting asks about each in turn |

The data folder is `~/Library/Application Support/AAW`, or what `AAW_DATA_DIR`
names. `daw projects` lists the projects open in the app, with the window in
front first.

## The window

| Part | Shows |
|---|---|
| Transport bar | Play or stop, loop, a metronome toggle, the position as bar.beat.sixteenth with the beat the time signature counts, tempo, the time signature, length in bars, the timeline's grid as a note value with its menu, "Exporting NAME…" while File › Export Audio… writes a file, and "Agent editing" while an agent's changes land |
| Ruler | The loop brace, section markers and bar numbers, with the start position as an orange marker |
| Headers | Each track's name, effect chain, mute, solo, volume and pan, and under a track unfolded with the mark by its name, its send to each return; a group as a row of the same kind above its tracks, which sit in from the edge under it, with a mark before its name that folds them away and shows them again; then the returns and the master. The A mark is orange when the row has automation |
| Lanes | Pattern clips as blocks named by their pattern, divided at each repeat, each with the waveform of what it plays; audio clips as blocks named by their sample, with the file's waveform, their fades and the beats of the file's beat map; a MIDI track's note clips as blocks named by their ID, with each note a bar from the lowest pitch to the highest; a looped note clip or audio clip with ↻ after its name, a mark at each wrap and its notes or its waveform drawn again from each. Clips of a muted track, or of a track in a muted group, are gray, and selected clips are outlined. A group's lane shows its tracks' clips small, a strip a track, folded or not |
| Automation | Under a row whose A mark is on: a lane for each automated parameter, with its name and range in the header and its points on the timeline joined as the song plays them, a shaped segment as its curve |
| Detail panel | Under the arrangement, one of two editors, which the two marks at its top left change between. A row's header shows Devices and a clip shows the clip: Pattern for a pattern clip, Audio Clip for an audio clip, Notes for a note clip |
| Devices | The effect chain of the row last selected: a panel for each effect with a control for each of its fields, and for a track its pads; an equalizer as one curve of its bands over the spectrum of what it puts out while the song plays, a numbered point for each band, and under the curve the selected band's fields. A MIDI track's chain starts with its instrument: the Sampler, with its sample's waveform, the part of it the keys play, its root note and the pad's fields; the Synth, with a header naming its patch, a column for its own fields and an octave of keys, one for each oscillator with its wave or its table's cycle drawn, the filter with its response, each envelope drawn with its handles, each LFO with its shape, the macros, the matrix's entries with their amounts, a line under each control an entry moves, and one for each of the patch's own effects; a sampler of several pads as a list of the pads and the notes that play each; or No instrument |
| Audio Clip | The audio clip last selected: its gain, fades, fade curve, the file's tempo and how it is stretched, and in words the part of the file it plays and the file's beat map |
| Pattern | The pattern of the clip last selected, with a row for each pad of the clip's track: steps as cells, events as bars, by note for a pad whose sample has a root note. Beside it the pattern's length, step and swing, and the selected event's velocity, beat, length and note, or the velocity of several. While the clip plays, a line shows where |
| Notes | The piano roll of the note clip last selected: a row for every MIDI note, 127 at the top, with each C named and a drum pad's note by its pad, darker where the instrument plays nothing; the notes as bars by velocity, gray outside the clip, before its start or past its end, where they are kept and do not play, and past the end of a looped clip's loop, which is shaded with a mark at the wrap. Under the notes, a lane of each note's velocity as a stalk at its start. Beside it the clip's ID, its length, its loop in beats or off, the grid, Preview, and the selected note's pitch, place, length and velocity, or the velocity of several. While the clip plays, a line shows where |
| Samples | Left of the arrangement, when shown: the library's samples by search, category and kind, each with its length and what is known of its tempo, key and pitch |
| Activity | Each change with who made it (agent, you, or an edit of the file), newest first. A drag is one entry |

| Input | Does |
|---|---|
| Click in the ruler or an empty lane | Sets the start position, on the grid; with ⌘, off it. While playing, playback jumps there |
| Click the metronome icon | Turns the beat click on or off. Yellow means on. It sounds during playback, including an empty project, follows BPM and loops, clicks on the note the time signature counts and accents the first beat of each bar. It starts off when a project opens and is excluded from renders and stems |
| Type in BPM | Sets the session tempo (20–400, decimals accepted). Return or leaving the field applies; Escape cancels. The edit saves and can be undone |
| Type a time signature | Sets the song's time signature, `3/4` or `6/8`: 1 to 32 beats over 1, 2, 4, 8 or 16. Return or leaving the field applies; Escape cancels. The bars, grid, rulers, position and metronome follow it; the edit saves and can be undone ([time signature](../../docs/features/time-signature.md)) |
| Space | Plays from the start position, or stops |
| Return | Jumps back to the start position while playing |
| Drag in the ruler's top strip | Sets the loop |
| L | Loops the selected clips; with none selected, turns the loop off, or on again |
| Scroll, pinch, Command-scroll, ⌘=, ⌘-, ⌘0 | Scroll and zoom; ⌘0 fits the song |
| The Grid menu in the transport bar, or View › Grid | Chooses the grid of the editor that has the keys, the timeline's from the transport bar: Follow Zoom, a size from 1 Bar to 1/64, ⌘1 finer, ⌘2 coarser, ⌘3 triplets, ⌘4 Snap to Grid. A chosen grid holds whatever the zoom; the transport bar names it, gray while it follows the zoom, `no snap` while snapping is off, when every click and drag lands where the pointer is and ⌘ snaps instead |
| Drag a volume, pan or send sideways | Changes it, heard as it moves; with Shift, ten times finer. Double-click sets volume to 0 dB and pan to center, and removes a send |
| Click M or S | Mutes or solos |
| Click a clip; Shift-click | Selects it and shows its pattern, or an audio clip's settings; adds it to the selection or takes it out. ⌘A selects every clip, Escape none |
| Double-click a clip | Opens its editor in the detail panel, shown if it was hidden: a pattern clip's pattern, an audio clip's settings or a note clip's piano roll |
| Right-click or Control-click a clip | Selects it, or keeps a selection it is part of, and offers its editor, Cut, Copy, Duplicate, Split at Start Position, Join, Loop Selection and Delete, each as the menus do them |
| Drag in the clear of the timeline | Selects the clips a rectangle touches, across tracks; with Shift, as well as those selected. The press sets the start position |
| Double-click an empty part of a track | Adds a clip there, in the grid step under the pointer, with a new pattern one bar long to fill in; on a MIDI track, an empty note clip a bar long |
| Drag a clip | Moves the selected clips by grid steps, and to other tracks; with ⌘, off the grid; with Option, copies them there, outlined on the way, and leaves them, whichever part of the clip was pressed. An audio clip can pass the song's end, which grows with it |
| Drag a pattern clip's end | Changes its repeats |
| Drag a note clip's start or end | Moves that edge, on the grid or with ⌘ off it. The notes stay where they are in the song: those an edge passes are kept outside the clip and do not play, and come back when it moves back. A looped clip's end sets how many times its loop plays |
| Drag an audio clip's edge | Trims it: the audio stays where it is and the clip shows more or less of it, as far as the file goes; with ⌘, off the grid |
| Drag the handle at an audio clip's top corner | Sets its fade in or its fade out. The clip still ends where it did: a longer fade out starts earlier |
| ⌘E | Splits the selected audio clips at the start position, or with a track selected and no clip, that track's, and selects the later halves |
| ⌘J | Joins the selected clips, two or more on one track, into one that plays what they played, and selects it: note clips into one note clip from the first's start to the last's end, with every note that played where it played and a looped clip laid out as notes; pattern clips and audio clips only where they meet and are one music, the same pattern or the same file played on as a split left it, else refused with the reason. The first clip is the one kept |
| Arrow keys | Move the selected clips a grid step, or to the next track |
| ⌘D, Delete | Copies the selected clips to right after them; deletes them, or else the selected track, group or return; a deleted group leaves its tracks. In the piano roll, the selected notes |
| ⌘C, ⌘X, ⌘V | Copies or cuts the selected clips, and pastes them at the start position, on the track whose lane was clicked last or else the tracks they came from; the start position moves to their end. In the piano roll, notes, pasted where the clip was last clicked in the clear, or else right after themselves. A copy is its own: changing it changes nothing else |
| Click a header; drag it up or down | Selects the track, group or return; moves it among the others. A track in a group stays beside the others: the host refuses a move out of the run |
| Click the mark before a group's name | Folds the group's tracks away under its row, or shows them again; with Option, every group. The fold is the window's, not the song's |
| ⌘G, ⇧⌘G | Groups the selected track, or the tracks of the selected clips, into a new group above them and asks for its name; takes the selected group, or the selected track's, away, leaving its tracks |
| Double-click a header | Selects the row and shows its devices in the detail panel, shown if it was hidden |
| Right-click or Control-click a header | Selects the row and offers Rename, Mute, Solo, Show Automation, Add Track, Add MIDI Track, Add Return, Group Tracks or Remove from Group on a track, Ungroup on a group, and Delete, as the row allows: the master is not renamed, muted or deleted, and a return has no solo |
| Rename in that menu, or ⌘R | Renames the track, group or return where its name is: Return keeps the name, Escape drops it |
| ⌘T, ⇧⌘T, ⌥⌘T | Adds a track under the selected one, a MIDI track with no instrument, or a return, and asks for its name |
| ⌘Z, ⇧⌘Z | Undo and redo, whoever made the change. The Edit menu names the step and whose it is |
| Click A in a header | Shows or hides the row's automation lanes; with Option, every row's |
| Double-click in a lane | Adds a point there, on the grid; with ⌘, off it |
| Click a point; Shift-click; drag in the clear of a lane | Selects it; adds it to the selection or takes it out; selects the points a rectangle holds, in every lane it crosses |
| Drag a point | Moves the selected points in time, no further than the points of their lanes that stay, and up or down by as much of each lane, heard as they move, one undo step. They stop together at a lane's edge. The value shows beside the one point selected |
| Double-click a point, or Delete | Removes it, or Delete the selected points; a lane's last point takes the lane with it. Option-click changes whether it holds its value until the next |
| Click × by a lane; + Lane | Removes the lane; offers the row's parameters that have none |
| Add Effect, in the device panel | Adds an effect to the end of the chain, with a place to start |
| Drag a bar in a device | Changes the field; with Shift, ten times finer. Double-click puts back the default. Levels and knobs are heard as they move; a field that reshapes the device, such as a delay's time or a reverb's decay, is sent when the drag ends |
| Click a bar's number | Opens a field over it with the number selected, as the tempo's: type a value, in the bar's unit or without it (`880`, `2.5k`, `-6 dB`), and Return or a click elsewhere applies it, held to the bar's range; Escape cancels. Every bar has it: an effect's fields, the Synth's knobs, a Sampler's pad, an audio clip's gain, a pattern's swing and a note's or an event's velocity |
| The marks in a device's title | Bypass, move earlier or later in the chain, remove; an equalizer also adds a band |
| Drag a point on an equalizer's curve | Moves the band's frequency across and its gain up and down, or for a highpass or lowpass band, whose point sits at its corner, its resonance; heard as it moves, one undo step. With Option, up narrows the band and down widens it, q doubling each 40 points. A click on a point or on a number under the curve selects the band, whose shape, frequency, q and gain or slope show there ([parametric EQ](../../docs/features/parametric-eq.md)) |
| Double-click an equalizer's curve | In the clear, adds a bell band at that frequency and gain, up to 16; on a point, removes its band, unless it is the last. The spectrum under the curve is the equalizer's output, read from the audio thread while the song plays and falling away once it stops |
| Click an effect's title | Selects the effect, outlined in the accent color, in place of the clips; the row stays selected. ⌘C, ⌘X, ⌘D and Delete then act on it while the devices show: a copy, a copy right after it with no id, or its removal. ⌘V pastes the copied effect on the selected row, or else the one whose devices show, after the selected effect of that row or last in its chain, and shows it. A copy has every field of the original, and its id where the chain has no effect of that name; a copy the host refuses, such as a compressor keyed from the track it lands on, is refused with the reason |
| Drag an effect's title with Option | Copies the effect onto the header it lands on, any track, return or the master, at the end of that chain, or between two devices where the strip lights up. Without Option it lands nowhere; the ◂ ▸ marks move an effect in its chain |
| Right-click or Control-click an effect's title | Selects it and offers Cut, Copy, Paste After, Duplicate, Bypass and Delete, each as the menus do them |
| The diamond by a field | Adds a lane for the field, or removes it; filled while a lane moves the field |
| ⌥⌘D, ⌥⌘P | Show the devices or the pattern in the detail panel, or hide the panel when it shows them already |
| Click a step | Turns it on or off; a drag along the row takes the steps it passes with it. A drag up or down on a step that is on sets how hard it plays, 1 to 9 |
| Double-click a row of held hits or of notes | Adds an event on the step under the pointer, at the note under it, held for a step on a pad that is held. With ⌘, a click adds one off the grid, in a row of steps too |
| Click an event; Shift-click; drag in the clear of a row of events | Selects it; adds it to the selection or takes it out; selects the events a rectangle touches, in any row. ⌘A selects every event, Escape none. A click in the clear sets where events are pasted |
| Drag an event; drag its end | Moves the selected events by steps of the pattern's grid, as far as each stays in the pattern, and those in rows of notes up and down by notes; with ⌘ off the grid; with Option, copies them there and leaves them. Its end ends that event alone on a line of the grid |
| Arrow keys, in the pattern | Move the selected events a step earlier or later, or a note up or down; with Shift, an octave |
| Double-click an event, or Delete | Removes it, or Delete the selected events |
| ⌘D, ⌘C, ⌘X, ⌘V, in the pattern | Copies the selected events to right after them; copies or cuts them, and pastes them into the pattern shown, where it was last clicked in the clear, or else right after themselves in their own pattern and at their own beats in another. A pad the pattern's tracks lack is refused with the reason |
| Type a length; choose a step; drag the swing | Change the pattern: step rows grow or shrink with its length, and are written on a new grid when their hits fall on it |
| Own Copy, by a pattern's name | Gives the clip a copy of the pattern, so that editing it leaves the other clips that play it as they are |
| Scroll, pinch, Command-scroll, in the pattern | Scroll and zoom the pattern |
| Double-click in the piano roll | Adds a note at the note under the pointer, on the step of the grid under it or with ⌘ where it is, a step long, and plays it. Double-click a note to remove it |
| Click a key, or drag along the keys | Plays the note through the track's instrument |
| Click a note; Shift-click; drag around notes | Selects it and plays it; adds it to the selection or takes it out; selects the notes the rectangle touches. ⌘A selects every note, Escape none. A click in the clear sets where notes are pasted |
| Drag a note; drag its end or its start | Moves the selected notes by whole steps of the grid and by notes, or with ⌘ by thousandths of a beat, playing the grabbed one at each pitch it reaches; with Option, copies them there and leaves them, as in Ableton. Its end makes them longer or shorter, ending the grabbed one on a line of the grid or with ⌘ anywhere; its start moves their starts the same way and leaves their ends. A note too short for both grips has its end only |
| Drag a stalk in the velocity lane | Selects its note, as a click on the note does, and changes the selected notes' velocities by as much, each within 1 to 127; the values show while it is dragged. The lane is under the notes when the panel is at least 150 points tall |
| Option-scroll, in the piano roll | Zooms up and down: the rows from 5 to 28 points, or as short as all 128 notes in view. Scroll, pinch and Command-scroll scroll and zoom across |
| The headphones beside the grid | Preview: whether a note is heard as it is drawn, clicked or moved, and as its key is pressed. On a Synth or a Sampler; a track with no instrument plays nothing |
| Arrow keys, in the piano roll | Move the selected notes a step earlier or later, or a note up or down, playing the first; with Shift, an octave |
| Type a note, a place or a length; drag Velocity | Sets the selected note's pitch (C4 or 60, kept as 60), place and length (1.975, 1/3), and how hard it plays; Velocity sets several at once |
| Choose a grid | The steps the piano roll adds and moves notes on, named as note values: 1 Bar, 1/2 and 1/4 to 1/64, with triplets such as 1/8T; ⌘1, ⌘2 and ⌘3 step through them while the piano roll has the keys. The pattern's Step menu names its steps the same way, a beat to 1/64, and the same keys set it while the pattern has the keys: its 1/16 is a quarter of a beat |
| ⌥⌘B, or the mark at the left of the transport bar | Shows or hides the browser |
| Type in Search; choose a category or a kind | Finds samples: every word must be in the sample's path |
| Click a sample | Plays it, as its file is, and gives the samples the keys; the speaker mark turns the playing off |
| Up and Down, in the samples | Move to the sample before or after and play it, as in the Finder, until a click elsewhere takes the keys back. The selected sample is drawn in the accent color while the samples have the keys, gray when they do not |
| Click + by a sample, or double-click it | Puts it where a drop on the selected row would: into a Sampler on the selected MIDI track, onto any other selected track as an audio clip at the start position, or with no track selected on a new MIDI track with a Sampler of it. The hint under the samples says which |
| Drag a sample, or an audio file from the Finder, onto the timeline | An audio clip at the grid line nearest the pointer (with ⌘, off the grid): on the track under it, or under the tracks on a new track. The clip it would make is outlined while the file is dragged |
| Drag a MIDI file from the Finder onto the timeline | A note clip of its notes at the grid line nearest the pointer (with ⌘, off the grid): on a MIDI track's lane, on that track, and anywhere else on a new MIDI track with no instrument, named after the file. The clip is outlined at its length while the file is dragged, and the song grows to hold it. The file is read where it is, and must be of one part, as `daw midi import` reads; one that is not is refused with the reason. Not in the headers |
| Drag one onto the headers | On a MIDI track's header: its instrument, a Sampler that plays the sample at every note's pitch, as it is at middle C (C4), in place of the instrument it had; × in its device panel takes it off, and the notes stay as they are. On another track's header: an audio clip on it at the start position. Under the tracks: a new MIDI track named after the sample, with a Sampler of it, drawn as its header with the name and Sampler while the file is dragged. A sample dropped on a MIDI track's lane goes to a new track as an audio clip. Never a pad. A sample the browser measured a pitch of, shown as a note by it, starts Held; any other plays to its end |
| Drag one onto the Sampler in the device panel | Loads it: the keys play it at every note's pitch, as it is at middle C. Over a sample the Sampler has, it takes its place and keeps the pad's mode, level, pan, transpose, attack, release and reverse; the start and the end go back to the whole file. A MIDI track with no instrument gets a Sampler of it. One undo step |
| Drag a marker on the Sampler's waveform; type Start or End | Moves where in the file the keys play from or to, no further than the file or the other marker, heard when the drag ends. An empty End plays to the file's end |
| Type in Root; click Measure | Sets the note the sample is at, as C4 or 60, so the keys play it from there; empty plays it as it is at middle C. Measure sets it to the pitch `daw samples analyze` finds, and says so when the file has no one pitch |
| Choose a mode; drag Transpose, Attack, Release, Level or Pan; tick Reverse | Sets the Sampler's pad, as `daw pad set` does; each is heard when the drag ends, after a 10 ms dip |
| Drag Synth, or one of its patches, from the browser onto a MIDI track's header or its device panel, or under the tracks; + or a double-click on it | Attaches the plain saw in place of the track's instrument, or loads the patch into the track's Synth, or attaches a Synth with it; under the tracks, a new MIDI track, `synth-1` or named after the patch. The notes are kept. One undo step, as `daw synth add --patch` and `daw patch load` make |
| Drag a knob, choose a wave, a table or a mode, type a number or tick a box in the Synth's panel; click a field's diamond; drag an entry's amount or click × on it | Sets that field of the patch, as `daw synth set` does: a knob is heard as it is dragged, and a wave, a table, a unison count, a filter mode or a routing when chosen, after a 10 ms dip; the Table menu lists the built-in tables and the project's samples; the diamond adds a lane on the field under the track, or removes it; an entry's amount is set in the target's unit, and × takes it out of the matrix |
| ◂ ▸ in the Synth's header; its menu | Loads the patch before or after the current one in the browser's list, Factory then Mine; the menu loads any of them, and Save… writes the patch over the one of Mine it came from, or asks for a name as Save As… does: a name, a description and tags, over a patch of Mine only when ticked. As `daw patch load` and `daw patch save` |
| Drag a rack from the browser's Audio Effects onto a header, the device panel or an insertion strip; + or a double-click on it | Adds the rack's effects to that row's chain, at the strip's place or the end, one undo step, as `daw rack load` makes; an effect whose id the chain has is numbered, and a compressor's sidechain is kept only where the song allows it. Racks are listed under the effect kinds and searched by name, tag and kind |
| Save Rack… in the device panel's header | Asks for a name, a description and tags and writes the row's chain to the library, ~/Music/AAW/library/racks, over a rack of the name only when ticked, as `daw rack save` does; gray while the chain is empty. The browser lists it at once |
| Drag the filter's corner; drag an envelope's handle | Moves the cutoff across and the resonance up and down, heard as it moves, one undo step; a press elsewhere on the curve takes the cutoff alone. An envelope's handles set the attack, the decay with the sustain, and the release |
| Drag an envelope's, an LFO's or a macro's tab, or velocity, note or random in the Matrix column, onto a control | Adds a matrix entry from that source to the control's target, an oscillator's Semitones standing for its pitch, with an amount enough to hear. A control an entry moves shows its reach as a line under the bar |
| + in the Synth column; × on a column or a macro | Adds an oscillator, an envelope, an LFO or a macro under the next free name, or from its Add Effect submenu an effect at the end of the patch's own chain; takes one off. The last oscillator, amp and a part the matrix names are refused with the reason |
| The marks in an effect column of the Synth; its rows | Bypass, move the effect in the patch's chain or remove it with its lanes, as `daw effect bypass\|move\|remove` on the synth's path; each row sets a field of the effect and its diamond adds a lane on `instrument.effects.REF.FIELD` under the track |
| Press a key under the Synth's fields; drag across the keys; ◂ ▸ by Keys | Plays the note now through the track, a beat long, softly at the top of the key and hard at the bottom, as `daw synth audition --play` does from a terminal; moves the octave |
| Drag the line above the detail panel | Makes the panel taller or shorter, 214 points the least, and it stays so between projects |
| Drag a bar or type a value in the Audio Clip panel | Sets the clip's gain, fades, fade curve, tempo or stretch |

The grid is the finest of bars, beats, eighths and sixteenths that the zoom has
room to draw, until one is chosen in the Grid menu, which then holds whatever
the zoom: its lines are drawn once they have room, every second or fourth
where they do not. Clicks and drags snap to the grid, drawn or not. A dragged
clip moves by whole grid steps, so one that starts off the grid stays as far
off it. [docs/features/grid.md](../../docs/features/grid.md) has the menu.

A device panel is drawn from what the host says of the effect: each field's
label, unit, range, default and choices. No panel is made by hand for an
effect type, so a new field or effect in the engine shows up here by itself;
the equalizer alone has a drawing of its own, its curve, over the same fields.

A pattern clip's waveform is what its track's sampler plays over the clip's
span, before the track's effects and fader, so a level or an effect does not
change it. The host works it out again for the tracks an edit changed; until it
has, a clip keeps the waveform it had, which moves with the clip, and a new clip
shows a line.

An audio clip's waveform is its file's, from where the clip starts in the file
and at the clip's gain, so it is there while an edge is dragged outward. The
host reads a file's peaks once. The clip is drawn to where its sound ends, with
its fade out inside it, and the ticks at its foot are the beats `daw samples
beats` measured, when its map is beside the file.
[docs/features/audio-clips-in-app.md](../../docs/features/audio-clips-in-app.md)
has the rest: what each drag writes to the song, and when the song grows.

A pad's row in the pattern editor is steps when the pad plays its sample
through at one pitch, bars of events when the pad is held (`mode: gate`), and
notes when the pad's sample has a root note. Steps and events of one pad both
play; steps are on the grid at one of ten levels, and events are anywhere, with
a velocity, a length and a note. A pattern is shared by every clip that plays
it, and they all light up when an agent changes it.

A sample added to the song is copied into the project's `samples` folder first,
as `daw samples import` copies it; the library is never changed. An `.m4a` or
`.mp3` file dropped from the Finder is decoded to WAV there instead. A Sampler's
pad and a new track are named after the sample's category, or its file. A
sample with a measured pitch, unless its name says it is a drum, an effect or a
loop, starts Held in a Sampler; the pitch is not written as its root note. The
sample, the Sampler or the audio clip, a new track and a longer song are one
undo step.

The browser has Samples, Instruments, Audio Effects and Folders, with search at
its top. Add Folder… registers one or more directories for every project and the
agent. Check folder names to search within one or several; All folders searches
all sources. Refresh scans for changes, and a folder's context menu removes its
registration without touching audio files. Offline folders remain listed.

The shared index is `library.sqlite` in the app data folder (`AAW_DATA_DIR`, or
`~/Library/Application Support/AAW`); `AAW_LIBRARY` overrides it. Existing local
indexes can still be used through that override, or their folders added again.
Search and indexing run through Python off the UI thread. WAV, AIFF, FLAC, MP3
and M4A are searchable. The agent's `daw samples folders` and repeatable
`daw samples search --folder DIRECTORY` use the same sources.

Drag an effect onto a track, return or master header to append it, or onto an
insertion strip between device panels to place it there. Instruments lists
Sampler and Synth, and under Synth its patches, Factory and Mine, which the
search field narrows by name and tag; Mine are the files `daw patch save`
writes to `~/Music/AAW/library/patches/`, read again when the window comes to
the front. Drag Sampler or Synth onto a MIDI header or instrument panel to
attach an empty sampler or the plain saw, or below the rows to make a MIDI
track; drag a patch onto a MIDI header or panel to load it into the track's
Synth, or attach a Synth with it, or below the rows for a track named after
it. Double-click or + adds to the selected row. Invalid destinations refuse
the drop. Each device addition is one undo step. See
[browser](../../docs/features/browser.md) and [the Synth](../../docs/features/synth.md).

An edit shows at once and goes to the host, whose song the window then follows.
A drag of a level is one undo step, however long, and the song is saved when it
pauses. An edit the host refuses, such as a clip dragged to a track without the
pads its pattern plays, is put back and the reason shown for a few seconds.

When a change lands, rows and clips ease to their new places and values. What a
change by the agent or an edit of the file touched lights up in that origin's
color for a moment. A clip also lights up when its pattern changed. A
`song.yaml` edited outside the app that does not load is reported in a banner
and the last valid song stays open. `daw close PROJECT` closes the window.

## Checking the app without the screen

The app takes scripted input on its command line, queued for the window as the
events a person's click or key press makes, and can write a picture of the
window and quit. Points are in the window's content, from its top left.

```sh
AAW_DATA_DIR=/tmp/aaw-data build/AAW.app/Contents/MacOS/AAW song.yaml --size 1280x560 \
    --click 800,250 --key space --snapshot /tmp/window.png --after 1.5
```

Set `AAW_DATA_DIR` to a scratch folder for such a run. The app keeps its index
of projects there, and with no project on the command line it makes its
Untitled project there, so neither touches the person's own. A run that takes a
picture or measures asks nothing when it quits: an Untitled project that holds
something is left in that folder.

`--measure JSON [--frames N]` in place of the picture, or before it, scrolls and
zooms the arrangement through the song for N frames (240 unless given) and
writes how long each took to draw, the time between frames, and how long the
waveforms trailed the song's opening and its last change.

These run in the order given, half a second apart; `--after` is the time
between the last of them and the picture:

| Argument | Does |
|---|---|
| `--click X,Y`, `--shift-click X,Y`, `--double-click X,Y` | Clicks |
| `--drag X1,Y1,X2,Y2` | Presses, moves in two steps and lets go; `--opt-drag` and `--cmd-drag` with Option or ⌘ held |
| `--key KEY` | Presses `space`, `return`, `delete`, `escape`, `left`, `right`, `up`, `down` or a letter, after any of `cmd+`, `shift+` and `opt+`: `--key shift+cmd+z` |
| `--type TEXT` | Types, as into a name |
| `--drop FILE,X,Y` | Lets an audio or MIDI file go at a point, as a drag from the Finder that ends there. In the detail panel, while it shows the devices of a MIDI track with a Sampler or no instrument, the file loads the Sampler |
| `--wait SECONDS` | Leaves time, such as for a `daw` command from a terminal |
| `--export PATH [--export-level LEVEL]` | After the actions, writes the mix as PATH, as File › Export Audio… does when its panel closes: the format is the extension's (`.wav` 24-bit, `.m4a`, `.mp3`) and the level `peak=-1`, `lufs=-14`, `gain=-3` or `rendered`, as rendered unless given. What the command reported is printed, then the picture is taken if one was asked for, with the banner in it, and the run quits; a file that is there is refused as the command refuses it, and a failed export says why on stderr and ends with status 1 |

Run `daw` commands against the song meanwhile to see them land in the picture.
A key that plays is heard on the speakers. Headers are 212 points wide and rows
start 93 points down: 46 for each track, more while its sends or lanes show
(46 for each lane). The detail panel is the bottom 214 points until the line
above it is dragged (`--drag 700,384,700,204` on a window 600 high makes it
394), its devices from x = 212. The pattern editor's rows start 16 points down the panel and 96 points
further right, at x = 308: a row of steps or hits is 24 points high, and rows
of notes share the rest. A pattern of a few bars fills the panel's width, and
a longer one scrolls. While the samples show, everything else is 361 points
further right.
A menu, such as Add Effect, a choice in a device, + Lane or a header's or a
clip's context menu, waits for a person
and cannot be scripted, and neither can a drag from the samples or the Finder:
`--drop` does what a drag from the Finder does when it ends, without the
outline shown on the way, and the + by a sample or a `daw` command does the
rest. The piano roll's notes start 96 points right of the panel's left, as
the pattern's rows do, under a 16-point ruler; a note is 10 points high, and the
clip opens fitted to the width with its notes in the middle of the height, so
read a picture to find a note's place. The velocity lane is the panel's bottom
56 points, a note's stalk a point right of its start. The selected note's fields are at the
panel's left: Note, At and Length, and Velocity, 21 points apart. A clip's title strip is 14 points high, with its waveform under it; an
audio clip's fade handles are in the top third of the waveform, at the corners
until it has fades. Install
Command Line Tool asks in an alert, which also waits for a person, and so do
Save As… and the question about an Untitled project: `daw move` and `daw copy`
do what Save As… does. A MIDI track's Sampler panel starts 8 points into the
devices, at x = 220, with its title strip 24 points high; its waveform is
204 by 92 points from 6 points inside the panel, so a marker at the file's
start is at x = 226 and one at its end at x = 430, and Root's Measure button
is at the panel's right, about x = 640 in the first row. The Synth's panel
starts at x = 220 too, its header 24 points high; its first column holds the
keys under six fields, from about y = 258 into the panel at its least
height, and a column's drawing is right under its title, 34 points high for a
wave or an LFO and 56 for the filter or an envelope. A key pressed by
`--click` plays through the speakers. An equalizer's panel is 400 points
wide; its curve starts 6 points in and 6 under its title strip and runs to
43 points above the panel's foot, 20 Hz at its left edge and 20 kHz at its
right in equal ratios, +30 dB at its top and −30 at its foot, so at the least
height a band's point at 1 kHz and 0 dB on the first device is about
(464, 483), and at +6 dB (464, 472): read a picture to find a point before
dragging it. A `--drag` from a point sets its band, as a person's drag does.

## Layout

| Path | Holds |
|---|---|
| `Sources/AAWCore` | The generated bindings (not in Git) |
| `Sources/AAWApp/SongModel.swift` | A song open in the app: what the host reports, the selection, and the edits and transport commands sent to the host |
| `Sources/AAWApp/ArrangementView.swift` | The timeline and headers, an AppKit view drawn with Core Graphics, the mouse and keys that edit them, and what a dropped sample lands on |
| `Sources/AAWApp/AudioClipLayout.swift` | An audio clip as it is drawn and dragged: its parts under the pointer, how far an edge or a fade goes, and the curve of a shaped automation segment, tested in `Tests` |
| `Sources/AAWApp/Waveform.swift` | The peaks the host sends for each track and each file, and the columns a clip draws from them, tested in `Tests` |
| `Sources/AAWApp/TextLines.swift` | Lines of text laid out once and drawn many times |
| `Sources/AAWApp/TimelineLayout.swift` | Zoom, scroll, the grid and what a click or drag means, tested in `Tests` |
| `Sources/AAWApp/Grid.swift` | The grid values, their names as note values and the steps the Grid menu takes through them, tested in `Tests` |
| `Sources/AAWApp/HeaderLayout.swift` | Where a header's controls and a row's lanes are, how a level reads a drag and how a lane or a knob maps its range, tested in `Tests` |
| `Sources/AAWApp/ContextMenus.swift` | What a right click on a header or a clip offers, as lists the arrangement makes menus from, tested in `Tests` |
| `Sources/AAWApp/DeviceView.swift` | The detail panel, its devices and an audio clip's settings: a row's chain, a panel for each effect drawn from its fields, the equalizer's curve as an AppKit view where its points are dragged and its spectrum read, a MIDI track's instrument, and the bar a number is dragged with |
| `Sources/AAWApp/EqLayout.swift` | Where the equalizer's panel puts its points and what a drag means: each band's response as the engine's sections give it, the curve they make, a point's band and what a point dragged to a place sets, and the spectrum's columns, tested in `Tests` |
| `Sources/AAWApp/SamplerPanel.swift` | The Sampler device: its waveform with the markers for the part the keys play, an AppKit view, the root note and Measure, the pad's fields, and what a drop on it loads |
| `Sources/AAWApp/SamplerLayout.swift` | Where the Sampler's markers are on its waveform, which one a point takes hold of and how far a drag goes, tested in `Tests` |
| `Sources/AAWApp/SynthPanel.swift` | The Synth: its header with the patch and the menu that saves and loads, a column of controls for each part of the patch drawn from its fields, the wave, filter, envelope and LFO drawings as AppKit views where they are dragged, the keys, the matrix's entries, the source tabs and their drops, and the lane mark beside a field |
| `Sources/AAWApp/SynthLayout.swift` | Where the Synth's drawings put things and what a drag means: the filter's response and its corner, an envelope's handles, a wave or an LFO over a cycle, the keys and an entry's reach, tested in `Tests` |
| `Sources/AAWApp/PatternEditor.swift` | The pattern editor, an AppKit view, and the fields beside it |
| `Sources/AAWApp/PatternLayout.swift` | Where a pattern's rows, steps and events are, and what a click or drag on them means, tested in `Tests` |
| `Sources/AAWApp/NoteEditor.swift` | The piano roll, an AppKit view, and the fields beside it |
| `Sources/AAWApp/PianoRollLayout.swift` | Where a note clip's notes are in the piano roll, and how the grid and a drag move them, tested in `Tests` |
| `Sources/AAWApp/BrowserView.swift` | The browser: shared folders, sample search, the device lists, the Synth's patches and the effect racks |
| `Sources/AAWApp/SongView.swift` | The window's SwiftUI: transport bar, banners, activity panel |
| `Sources/AAWApp/CommandLineTool.swift` | The bundle's `daw` and its link on the PATH: what is there now, the commands that make and remove it, and what the menu item asks, tested in `Tests` |
| `Sources/AAWApp/Export.swift` | File › Export Audio…: the panel's choices as the arguments of the bundle's `daw export`, the command's result as the banner reads it, and the process that runs it off the main thread, tested in `Tests` |
| `Sources/AAWApp/ExportPanel.swift` | The format, level and ceiling under the name in the Export Audio panel |
| `Sources/AAWApp/Projects.swift` | The index of the projects the app knows, and what closing a project does, tested in `Tests` |
| `Sources/AAWApp/App.swift` | The app delegate, menus, windows, launch, Save As…, closing and the command line |
| `Sources/AAW` | The executable's entry point |

## Limits

- The playhead is drawn where the output is: the audio thread's position less
  the song's latency and the delay the output device reports.
- Track colors are given out in the order tracks are first seen and are not
  saved; the song format has no color.
- A waveform shows a track's sampler before its effects, fader and sends, with
  full scale at the clip's height; louder is drawn as full scale.
- A pad's own settings, such as its level, tuning and whether it is held, are
  set with `daw pad set` on a track of patterns and in a kit of several pads
  on a MIDI track; the device panel lists those pads. The Sampler's one pad
  is set in its panel.
- A new track has no pads until a sample is added to it, and a sample added
  from the browser plays at its own tempo: a loop is fitted to the song with
  `daw pad set SONG TRACK PAD --source-bpm BPM`, and an audio clip with the
  Tempo in its panel.
- Audio clips that overlap on a track both play, and nothing crossfades them. A
  clip's lead is not shown. The window does not zoom out when the song grows.
- A trim or a fade out of a long file at another sample rate than the song's
  takes the engine a moment: about half a second for two minutes of audio.
- Steps are not selected or copied, and an event's transpose is set with
  `daw`. Events of two patterns are not moved or copied together.
- A sample on a MIDI track's header or its Sampler makes a sampler of one pad
  on every note, played as it is at middle C whatever its pitch until Root is
  set; a kit of pads on notes is made with `daw instrument map`. The Sampler
  has no note range, glide, sustain loop or filter, and is not played from a
  keyboard. Its root note is the sample's, so every pad of that file follows.
- The Synth's keys play a note a beat long, which cannot be held or let go
  early, and play nothing through a Sampler. The panel renames no part and
  no macro; `daw synth set` does. An effect dragged from the browser onto a
  MIDI track lands on its inserts; the patch's own chain takes effects from
  the + menu. The patches the header and the browser
  list are read when they are shown and when the window comes to the front,
  not while a terminal saves one in front of them.
- What Copy took is kept in the window, not on the system's clipboard, and is
  pasted into the same song. A copied effect comes without the lanes that
  automated it, and an effect of a Synth's own chain is not copied, dragged
  or duplicated; the patch's + menu adds one. An effect is not moved to
  another row by dragging.
- A row of notes shows the notes its events play with two more either side; a
  drag goes as far as that, and the arrow keys further.
- A sample is played from its file by the system, not through the song's
  engine, so it is heard at its own level, pitch and tempo.
- Names are IDs: letters, digits, `-` and `_`, starting with a letter.
- A drag in the clear of the timeline selects clips, not a range of time.
  Automation points are not copied.
- A lane shows levels from -60 to +6 dB (sends to 0), as the faders do; a point
  outside that is drawn at the edge, and a drag brings it inside.
- An effect's ID is set with `daw set`; the panel shows it.
- An effect added, removed, bypassed or moved while the song plays is heard
  after a 10 ms dip, and so is a change to a field that reshapes a device.
- Save As… does not replace a folder that is already there.
- Which groups are folded is kept by the window, not saved with the project,
  and a group is not dragged into another: a group is not in a group.
- The app has no icon, and the Python tools are not in its bundle, so Export
  Audio… needs the Python the bundle's `daw` finds, and says so where there is
  none. It exports the whole mix only, one file at a time, and the transport
  bar cannot tell the render from the writing: it says "Exporting" through
  both.
