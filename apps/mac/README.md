# Mac app

The native macOS app of [docs/Rust-Swift-Update.md](../../docs/Rust-Swift-Update.md),
as far as milestone M5: it opens a song, shows its arrangement, plays it, shows
each change as it lands, whoever makes it, and lets the person edit the mixer,
the clips, and the tracks and returns.

The app holds no model logic. Opening a song makes the app its session host
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
open -a build/AAW.app path/to/song.yaml
./build.sh test                     # the Swift tests
```

`build.sh` builds the Rust core as a static library, generates the Swift
bindings from it with UniFFI, wraps both as an XCFramework under `Generated/`,
builds the Swift package and assembles the bundle. Bindings and build output
stay out of Git. After a first `build.sh`, `swift build` is enough for
Swift-only changes, and Xcode opens `Package.swift` directly.

If the checkout lives in a synced folder, exclude `.build`, `build` and
`Generated` from syncing, as with the engine's `target`. For Dropbox: `xattr -w
com.dropbox.ignored 1 .build build Generated`.

The bundle is signed ad hoc and links Homebrew's libsndfile, so it runs on the
machine that built it.

## The window

| Part | Shows |
|---|---|
| Transport bar | Play or stop, loop, the position as bar.beat.sixteenth, tempo, length, and "Agent editing" while an agent's changes land |
| Ruler | The loop brace, section markers and bar numbers, with the start position as an orange marker |
| Headers | Each track's name, effect chain, mute, solo, volume and pan, and under a track unfolded with the mark by its name, its send to each return; then the returns and the master |
| Lanes | Clips as blocks named by their pattern, divided at each repeat; clips of a muted track are gray, and selected clips are outlined |
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
| Click a clip; Shift-click | Selects it; adds it to the selection or takes it out. ⌘A selects every clip, Escape none |
| Drag a clip | Moves the selected clips by grid steps, and to other tracks; with Option, off the grid |
| Drag a clip's end | Changes its repeats |
| Arrow keys | Move the selected clips a grid step, or to the next track |
| ⌘D, Delete | Copies the selected clips to right after them; deletes them, or else the selected track or return |
| Click a header; drag it up or down | Selects the track or return; moves it among the others |
| Double-click a name, or ⌘R | Renames the track or return: Return keeps the name, Escape drops it |
| ⌘T, ⌥⌘T | Adds a track under the selected one, or a return, and asks for its name |
| ⌘Z, ⇧⌘Z | Undo and redo, whoever made the change. The Edit menu names the step and whose it is |

The grid is the finest of bars, beats, eighths and sixteenths that the zoom has
room to draw, and clicks and drags snap to the lines drawn. A dragged clip moves
by whole grid steps, so one that starts off the grid stays as far off it.

An edit shows at once and goes to the host, whose song the window then follows.
A drag of a level is one undo step, however long, and the song is saved when it
pauses. An edit the host refuses, such as a clip dragged to a track without the
pads its pattern plays, is put back and the reason shown for a few seconds.

When a change lands, rows and clips ease to their new places and values. What a
change by the agent or an edit of the file touched lights up in that origin's
color for a moment. A clip also lights up when its pattern changed. A
`song.yaml` edited outside the app that does not load is reported in a banner
and the last valid song stays open. `daw close SONG` closes the window.

## Checking the app without the screen

The app takes scripted input on its command line, sent to the window as the
events a person's click or key press makes, and can write a picture of the
window and quit. Points are in the window's content, from its top left.

```sh
build/AAW.app/Contents/MacOS/AAW song.yaml --size 1280x560 \
    --click 800,250 --key space --snapshot /tmp/window.png --after 1.5
```

These run in the order given, half a second apart; `--after` is the time
between the last of them and the picture:

| Argument | Does |
|---|---|
| `--click X,Y`, `--shift-click X,Y`, `--double-click X,Y` | Clicks |
| `--drag X1,Y1,X2,Y2` | Presses, moves in two steps and lets go |
| `--key KEY` | Presses `space`, `return`, `delete`, `escape`, `left`, `right`, `up`, `down` or a letter, after any of `cmd+`, `shift+` and `opt+`: `--key shift+cmd+z` |
| `--type TEXT` | Types, as into a name |
| `--wait SECONDS` | Leaves time, such as for a `daw` command from a terminal |

Run `daw` commands against the song meanwhile to see them land in the picture.
A key that plays is heard on the speakers. Headers are 212 points wide and rows
start 93 points down: 46 for each track, more while its sends show.

## Layout

| Path | Holds |
|---|---|
| `Sources/AAWCore` | The generated bindings (not in Git) |
| `Sources/AAWApp/SongModel.swift` | A song open in the app: what the host reports, the selection, and the edits and transport commands sent to the host |
| `Sources/AAWApp/ArrangementView.swift` | The timeline and headers, an AppKit view drawn with Core Graphics, and the mouse and keys that edit them |
| `Sources/AAWApp/TimelineLayout.swift` | Zoom, scroll, the grid and what a click or drag means, tested in `Tests` |
| `Sources/AAWApp/HeaderLayout.swift` | Where a header's controls are and how a level reads a drag, tested in `Tests` |
| `Sources/AAWApp/SongView.swift` | The window's SwiftUI: transport bar, banners, activity panel |
| `Sources/AAWApp/App.swift` | The app delegate, menus, windows and the command line |
| `Sources/AAW` | The executable's entry point |

## Limits

- The engine plays without effects, sends, returns and automation until M6; the
  info mark in the transport bar says what a song is played without.
- The playhead is drawn where the audio thread is, not yet delayed by the
  output's latency.
- Track colors are given out in the order tracks are first seen and are not
  saved; the song format has no color.
- No waveforms (M8), no automation lanes or device view (M6). Effects, pads
  and patterns are edited with `daw` commands.
- A new track has no pads, so no pattern plays on it until an agent or `daw
  pad add` gives it some.
- Names are IDs: letters, digits, `-` and `_`, starting with a letter.
- Several clips cannot be selected by dragging over them.
