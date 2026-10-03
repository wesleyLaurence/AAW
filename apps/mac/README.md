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
samples in the library's index and adds them to the song as pads, tracks and
audio clips. Save As… gives a project a
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
| File › Export MIDI Clip… (⇧⌘E) | With one note clip selected, asks for a name and a place and writes the notes that play as a MIDI file, as `daw midi export` does |
| File › Save As… (⇧⌘S) | Asks for a name and a place. An Untitled project moves there. A project that has a name is copied, and the window carries on in the copy. Undo, the selection and what is playing carry on, and `daw` commands sent to the old path still land in the window |
| Closing an Untitled project | One that holds nothing is deleted. One that holds something asks: Save…, Delete or Cancel. Quitting asks about each in turn |

The data folder is `~/Library/Application Support/AAW`, or what `AAW_DATA_DIR`
names. `daw projects` lists the projects open in the app, with the window in
front first.

## The window

| Part | Shows |
|---|---|
| Transport bar | Play or stop, loop, the position as bar.beat.sixteenth, tempo, length, and "Agent editing" while an agent's changes land |
| Ruler | The loop brace, section markers and bar numbers, with the start position as an orange marker |
| Headers | Each track's name, effect chain, mute, solo, volume and pan, and under a track unfolded with the mark by its name, its send to each return; then the returns and the master. The A mark is orange when the row has automation |
| Lanes | Pattern clips as blocks named by their pattern, divided at each repeat, each with the waveform of what it plays; audio clips as blocks named by their sample, with the file's waveform, their fades and the beats of the file's beat map; a MIDI track's note clips as blocks named by their ID, with each note a bar from the lowest pitch to the highest. Clips of a muted track are gray, and selected clips are outlined |
| Automation | Under a row whose A mark is on: a lane for each automated parameter, with its name and range in the header and its points on the timeline joined as the song plays them, a shaped segment as its curve |
| Detail panel | Under the arrangement, one of two editors, which the two marks at its top left change between. A row's header shows Devices and a clip shows the clip: Pattern for a pattern clip, Audio Clip for an audio clip, Notes for a note clip |
| Devices | The effect chain of the row last selected: a panel for each effect with a control for each of its fields, and for a track its pads. A MIDI track's chain starts with its instrument: the sampler's pads and the notes that play each, or No instrument |
| Audio Clip | The audio clip last selected: its gain, fades, fade curve, the file's tempo and how it is stretched, and in words the part of the file it plays and the file's beat map |
| Pattern | The pattern of the clip last selected, with a row for each pad of the clip's track: steps as cells, events as bars, by note for a pad whose sample has a root note. Beside it the pattern's length, step and swing, and the selected event's velocity, beat, length and note. While the clip plays, a line shows where |
| Notes | The piano roll of the note clip last selected: a row for every MIDI note, 127 at the top, with each C named and a drum pad's note by its pad, darker where the instrument plays nothing; the notes as bars by velocity, gray outside the clip, before its start or past its end, where they are kept and do not play. Beside it the clip's ID, its length, the grid, and the selected note's pitch, place, length and velocity, or the velocity of several. While the clip plays, a line shows where |
| Samples | Left of the arrangement, when shown: the library's samples by search, category and kind, each with its length and what is known of its tempo, key and pitch |
| Activity | Each change with who made it (agent, you, or an edit of the file), newest first. A drag is one entry |

| Input | Does |
|---|---|
| Click in the ruler or an empty lane | Sets the start position, on the grid; with Option, off it. While playing, playback jumps there |
| Space | Plays from the start position, or stops |
| Return | Jumps back to the start position while playing |
| Drag in the ruler's top strip | Sets the loop |
| L | Loops the selected clips; with none selected, turns the loop off, or on again |
| Scroll, pinch, Command-scroll, ⌘=, ⌘-, ⌘0 | Scroll and zoom; ⌘0 fits the song |
| Drag a volume, pan or send sideways | Changes it, heard as it moves; with Shift, ten times finer. Double-click sets volume to 0 dB and pan to center, and removes a send |
| Click M or S | Mutes or solos |
| Click a clip; Shift-click | Selects it and shows its pattern, or an audio clip's settings; adds it to the selection or takes it out. ⌘A selects every clip, Escape none |
| Double-click an empty part of a track | Adds a clip there, in the grid step under the pointer, with a new pattern one bar long to fill in; on a MIDI track, an empty note clip a bar long |
| Drag a clip | Moves the selected clips by grid steps, and to other tracks; with Option, off the grid. An audio clip can pass the song's end, which grows with it |
| Drag a pattern clip's end | Changes its repeats |
| Drag a note clip's start or end | Moves that edge, on the grid or with Option off it. The notes stay where they are in the song: those an edge passes are kept outside the clip and do not play, and come back when it moves back |
| Drag an audio clip's edge | Trims it: the audio stays where it is and the clip shows more or less of it, as far as the file goes; with Option, off the grid |
| Drag the handle at an audio clip's top corner | Sets its fade in or its fade out. The clip still ends where it did: a longer fade out starts earlier |
| ⌘E | Splits the selected audio clips at the start position, or with a track selected and no clip, that track's, and selects the later halves |
| Arrow keys | Move the selected clips a grid step, or to the next track |
| ⌘D, Delete | Copies the selected clips to right after them; deletes them, or else the selected track or return. In the piano roll, the selected notes |
| ⌘C, ⌘X, ⌘V | Copies or cuts the selected clips, and pastes them at the start position, on the track whose lane was clicked last or else the tracks they came from; the start position moves to their end. In the piano roll, notes, pasted where the clip was last clicked in the clear, or else right after themselves. A copy is its own: changing it changes nothing else |
| Click a header; drag it up or down | Selects the track or return; moves it among the others |
| Double-click a name, or ⌘R | Renames the track or return: Return keeps the name, Escape drops it |
| ⌘T, ⇧⌘T, ⌥⌘T | Adds a track under the selected one, a MIDI track with no instrument, or a return, and asks for its name |
| ⌘Z, ⇧⌘Z | Undo and redo, whoever made the change. The Edit menu names the step and whose it is |
| Click A in a header | Shows or hides the row's automation lanes; with Option, every row's |
| Double-click in a lane | Adds a point there, on the grid; with Option, off it |
| Drag a point | Moves it in time, no further than its neighbors, and in value, heard as it moves. Its value shows beside it |
| Double-click a point, or Delete | Removes it; a lane's last point takes the lane with it. Option-click changes whether it holds its value until the next |
| Click × by a lane; + Lane | Removes the lane; offers the row's parameters that have none |
| Add Effect, in the device panel | Adds an effect to the end of the chain, with a place to start |
| Drag a bar in a device | Changes the field; with Shift, ten times finer. Double-click puts back the default. Levels and knobs are heard as they move; a field that reshapes the device, such as a delay's time or a reverb's decay, is sent when the drag ends |
| The marks in a device's title | Bypass, move earlier or later in the chain, remove; an equalizer also adds a band |
| The diamond by a field | Adds a lane for the field, or removes it; filled while a lane moves the field |
| ⌥⌘D, ⌥⌘P | Show the devices or the pattern in the detail panel, or hide the panel when it shows them already |
| Click a step | Turns it on or off; a drag along the row takes the steps it passes with it. A drag up or down on a step that is on sets how hard it plays, 1 to 9 |
| Double-click a row of held hits or of notes | Adds an event on the step under the pointer, at the note under it, held for a step on a pad that is held. With Option, a click adds one off the grid, in a row of steps too |
| Click an event; drag it; drag its end | Selects it; moves it by steps of the pattern's grid, and in a row of notes up and down by notes; ends it on a line of the grid. With Option it moves off the grid |
| Arrow keys, in the pattern | Move the selected event a step earlier or later, or a note up or down; with Shift, an octave |
| Double-click an event, or Delete | Removes it |
| Type a length; choose a step; drag the swing | Change the pattern: step rows grow or shrink with its length, and are written on a new grid when their hits fall on it |
| Own Copy, by a pattern's name | Gives the clip a copy of the pattern, so that editing it leaves the other clips that play it as they are |
| Scroll, pinch, Command-scroll, in the pattern | Scroll and zoom the pattern |
| Double-click in the piano roll | Adds a note at the note under the pointer, on the step of the grid under it or with Option where it is, a step long. Double-click a note to remove it |
| Click a note; Shift-click; drag around notes | Selects it; adds it to the selection or takes it out; selects the notes the rectangle touches. ⌘A selects every note, Escape none. A click in the clear sets where notes are pasted |
| Drag a note; drag its end | Moves the selected notes by whole steps of the grid and by notes, or with Option by thousandths of a beat; makes them longer or shorter, ending the grabbed one on a line of the grid or with Option anywhere |
| Arrow keys, in the piano roll | Move the selected notes a step earlier or later, or a note up or down; with Shift, an octave |
| Type a note, a place or a length; drag Velocity | Sets the selected note's pitch (C4 or 60, kept as 60), place and length (1.975, 1/3), and how hard it plays; Velocity sets several at once |
| Choose a grid | The steps the piano roll adds and moves notes on: a beat to a sixteenth of one, triplets too |
| ⌥⌘B, or the mark at the left of the transport bar | Shows or hides the samples |
| Type in Search; choose a category or a kind | Finds samples: every word must be in the sample's path |
| Click a sample | Plays it, as its file is; the speaker mark turns that off |
| Click + by a sample | Adds it as a pad of the selected track, or with no track selected as a new track |
| Drag a sample, or an audio file from the Finder, onto the timeline | An audio clip at the grid line nearest the pointer (with Option, off the grid): on the track under it, or under the tracks on a new track. The clip it would make is outlined while the file is dragged |
| Drag a MIDI file from the Finder onto the timeline | A note clip of its notes at the grid line nearest the pointer (with Option, off the grid): on a MIDI track's lane, on that track, and anywhere else on a new MIDI track with no instrument, named after the file. The clip is outlined at its length while the file is dragged, and the song grows to hold it. The file is read where it is, and must be of one part, as `daw midi import` reads; one that is not is refused with the reason. Not in the headers |
| Drag one onto the headers | On a track's header: a pad of that track. On a MIDI track's header: its instrument, a sampler that plays the sample on every note, from its root note when it has one, in place of the instrument it had; × in its device panel takes it off, and the notes stay as they are. Under the tracks: a new track with a pad. A sample dropped on a MIDI track's lane goes to a new track as an audio clip |
| Drag a bar or type a value in the Audio Clip panel | Sets the clip's gain, fades, fade curve, tempo or stretch |

The grid is the finest of bars, beats, eighths and sixteenths that the zoom has
room to draw, and clicks and drags snap to the lines drawn. A dragged clip moves
by whole grid steps, so one that starts off the grid stays as far off it.

A device panel is drawn from what the host says of the effect: each field's
label, unit, range, default and choices. No panel is made by hand for an
effect type, so a new field or effect in the engine shows up here by itself.

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
`.mp3` file dropped from the Finder is decoded to WAV there instead. A pad and a
new track are named after the sample's category, or its file. A sample with a
measured pitch brings it along as its root note, unless its name says it is a
drum, an effect or a loop. The sample, the pad or the audio clip, a new track
and a longer song are one undo step.

The browser reads the index `daw samples scan` writes: `.daw/library.sqlite` in
the song's folder or the nearest folder above it, or the file `AAW_LIBRARY`
names. Searches run in Python, as `daw samples search` does.

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
| `--drag X1,Y1,X2,Y2` | Presses, moves in two steps and lets go |
| `--key KEY` | Presses `space`, `return`, `delete`, `escape`, `left`, `right`, `up`, `down` or a letter, after any of `cmd+`, `shift+` and `opt+`: `--key shift+cmd+z` |
| `--type TEXT` | Types, as into a name |
| `--drop FILE,X,Y` | Lets an audio or MIDI file go at a point, as a drag from the Finder that ends there |
| `--wait SECONDS` | Leaves time, such as for a `daw` command from a terminal |

Run `daw` commands against the song meanwhile to see them land in the picture.
A key that plays is heard on the speakers. Headers are 212 points wide and rows
start 93 points down: 46 for each track, more while its sends or lanes show
(46 for each lane). The detail panel is the bottom 214 points, its devices from
x = 212. The pattern editor's rows start 16 points down the panel and 96 points
further right, at x = 308: a row of steps or hits is 24 points high, and rows
of notes share the rest. A pattern of a few bars fills the panel's width, and
a longer one scrolls. While the samples show, everything else is 251 points
further right.
A menu, such as Add Effect, a choice in a device or + Lane, waits for a person
and cannot be scripted, and neither can a drag from the samples or the Finder:
`--drop` does what a drag from the Finder does when it ends, without the
outline shown on the way, and the + by a sample or a `daw` command does the
rest. The piano roll's notes start 96 points right of the panel's left, as
the pattern's rows do, under a 16-point ruler; a note is 10 points high, and the
clip opens fitted to the width with its notes in the middle of the height, so
read a picture to find a note's place. The selected note's fields are at the
panel's left: Note, At and Length, and Velocity, 21 points apart. A clip's title strip is 14 points high, with its waveform under it; an
audio clip's fade handles are in the top third of the waveform, at the corners
until it has fades. Install
Command Line Tool asks in an alert, which also waits for a person, and so do
Save As… and the question about an Untitled project: `daw move` and `daw copy`
do what Save As… does.

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
| `Sources/AAWApp/HeaderLayout.swift` | Where a header's controls and a row's lanes are, how a level reads a drag and how a lane or a knob maps its range, tested in `Tests` |
| `Sources/AAWApp/DeviceView.swift` | The detail panel, its devices and an audio clip's settings: a row's chain, a panel for each effect drawn from its fields, and the bar a number is dragged with |
| `Sources/AAWApp/PatternEditor.swift` | The pattern editor, an AppKit view, and the fields beside it |
| `Sources/AAWApp/PatternLayout.swift` | Where a pattern's rows, steps and events are, and what a click or drag on them means, tested in `Tests` |
| `Sources/AAWApp/NoteEditor.swift` | The piano roll, an AppKit view, and the fields beside it |
| `Sources/AAWApp/PianoRollLayout.swift` | Where a note clip's notes are in the piano roll, and how the grid and a drag move them, tested in `Tests` |
| `Sources/AAWApp/BrowserView.swift` | The samples: searches of the library's index, and the list |
| `Sources/AAWApp/SongView.swift` | The window's SwiftUI: transport bar, banners, activity panel |
| `Sources/AAWApp/CommandLineTool.swift` | The bundle's `daw` and its link on the PATH: what is there now, the commands that make and remove it, and what the menu item asks, tested in `Tests` |
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
  set with `daw pad set`; the device panel lists a track's pads.
- A new track has no pads until a sample is added to it, and a sample added
  from the browser plays at its own tempo: a loop is fitted to the song with
  `daw pad set SONG TRACK PAD --source-bpm BPM`, and an audio clip with the
  Tempo in its panel.
- Audio clips that overlap on a track both play, and nothing crossfades them. A
  clip's lead is not shown. The window does not zoom out when the song grows.
- A trim or a fade out of a long file at another sample rate than the song's
  takes the engine a moment: about half a second for two minutes of audio.
- One event is selected at a time, and events are not copied or pasted. An
  event's transpose is set with `daw`.
- A note is not heard as it is drawn or clicked, and velocity is set in the
  fields beside the piano roll, not in a lane under it. The piano roll zooms
  across and not up and down.
- A sample on a MIDI track's header makes a sampler of one pad on every note;
  a kit of pads on notes is made with `daw instrument map`. A file dropped from
  the Finder has no root note, so it plays as it is on every note.
- What Copy took is kept in the window, not on the system's clipboard, and is
  pasted into the same song.
- A row of notes shows the notes its events play with two more either side; a
  drag goes as far as that, and the arrow keys further.
- A sample is played from its file by the system, not through the song's
  engine, so it is heard at its own level, pitch and tempo.
- Names are IDs: letters, digits, `-` and `_`, starting with a letter.
- Several clips cannot be selected by dragging over them, and one automation
  point is selected at a time.
- A lane shows levels from -60 to +6 dB (sends to 0), as the faders do; a point
  outside that is drawn at the edge, and a drag brings it inside.
- An effect's ID is set with `daw set`; the panel shows it.
- An effect added, removed, bypassed or moved while the song plays is heard
  after a 10 ms dip, and so is a change to a field that reshapes a device.
- Save As… does not replace a folder that is already there, and a new
  project's browser is empty unless `AAW_LIBRARY` names an index.
- The app has no icon, and the Python tools are not in its bundle.
