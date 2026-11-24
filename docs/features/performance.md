# What makes the person wait — implemented November 23, 2026

Status: implemented. The person's note of November 22, 2026 (D194): "a
system wide scan for inefficiencies, anything that could be slowing down
actions. I want to optimize performance to make sure this system can run as
fast as possible and never see the spinning mac wheel when loading or
processing something."

## What

The app watches its own main thread. Any piece of work that holds it longer
than a tenth of a second, past which a person feels the window stop
answering and after a couple of seconds sees the spinning wheel, is a stall:
it is logged with the main thread's stack, read every 50 ms while it lasts,
and `--measure` reports every stall of a run. A scripted session on a large
song measured the app before anything changed; what it found is fixed, and
a test fails when opening a large song holds the main thread again.

## How it works

- **The watch.** `Stalls` (`apps/mac/Sources/AAWApp/Stalls.swift`) puts an
  observer on the main run loop, which reports each time the loop moves on;
  the time between two reports while the loop is awake is one piece of
  work. A watchdog thread sleeps while the main thread does; once a piece of
  work has run past the threshold it suspends the main thread for a moment
  and follows its frame pointers, and again every 50 ms until the work ends. When it ends the frames are
  named off the main thread, `dladdr` for the system's libraries and `atos`
  for the app's own code when `--measure` reports, and the stall is logged:

  ```sh
  log stream --predicate 'subsystem == "local.aaw.mac" && category == "stall"'
  ```

  `AAW_STALL_MS` sets the threshold, 100 unless given, lower to look into
  shorter work. The watch starts as the app launches, costs nothing while
  the main thread sleeps, and keeps the latest 200 stalls for the report.
- **The report.** `--measure` adds `stalls` to its JSON: how many, the
  longest, their total, and each stall's length, when it started and its
  stacks, each told once with how many times it was read.
- **The session.** `apps/mac/measure.sh OUT.json [APP]` generates, once, a
  song of 30 tracks and five minutes (20 audio tracks of 160 clips from
  40-second files, 10 Synth tracks of 10,240 notes), a library of 50,000
  samples in nested folders, indexed, and a conversation of 2,000 messages,
  about 8 MB, as the song's newest thread. Each run opens a fresh copy in a
  window of 1440 by 900 and opens the browser, searches the library, opens
  Folders, drags a clip and undoes it, plays four seconds, saves, takes
  sixteen edits from `daw` in a terminal, and scrolls and zooms for 240
  frames, then prints the stalls.
- **The test.** `PerformanceTests` opens a generated 30-track song as the app
  does, with an 8 MB conversation, waits for its waveforms and history and
  reads the browser's lists, all under the watch, and fails on any piece of
  main-thread work over 100 ms, naming its stack. It also gives a field the
  keys in a window of 300 SwiftUI rows, which held the main thread 3.9 s
  while password AutoFill was on, and fails the same way.

## What was measured and changed

On an M2, the release build, the session above unless said. A
time is the longest single piece of main-thread work; ranges are over
repeated runs.

| Action | Before | After | What changed |
|---|---|---|---|
| Open the 30-track song | 794–848 ms | 101–138 ms | The host opens the song off the main thread (`SongModel.open`); the window is made when it is ready, and a panel with Cancel shows if that takes over half a second. What the host reports meanwhile waits for the window. What is left is SwiftUI building the window the first time |
| … of which the host loading the song | ~300 ms | 0 | As above |
| … the folder's icon, the first time a project is opened | ~200 ms | 0 | Written off the main thread |
| … asking whether the microphone can listen | ~150 ms | 0 | Asked once off the main thread, again when the app comes to the front; it was asked twice on every drawing of the microphone button |
| Open, the newest conversation 17 MB | 489 ms (with the open above already fixed) | 117 ms | The record is read and parsed off the main thread and put before what the panel shows |
| First click after opening, with that conversation | 796 ms | 155 ms | The conversation's rows are a lazy stack, laid out as they scroll into view, not all of them on every change of the window |
| The browser's lists of patches, racks, instrument racks, favorites and plugins, as it shows | 50 ms | 0 | Read off the main thread |
| Naming a stall's frames | 33 ms | 0 | The watch's own cost, moved off the main thread |
| A field taking the keys: a new MIDI track's name (⌘⇧T), a new group's (⌘G), a marker's (⇧M), the tempo, the browser's search; a copy of a person's 37-track song in a window of 1440 by 900 | 78–493 ms; 3.9 s in a window of 300 SwiftUI rows of a button and a toggle | 0 | macOS's password AutoFill is off (`NSAutoFillHeuristicControllerEnabled`, registered as the app launches). As a field took the keys it looked for the fields before and after it, and with none set it walked every view in the window to find them, so it grew with what the window shows: half as long with the conversation panel empty. The generated session's song never showed it |
| Drag a clip and undo; play; save; search 50,000 samples; open Folders; 16 edits from `daw`; scroll and zoom 240 frames | no stall | no stall | A frame draws in 1.9 ms on average and 7 ms at most |

Two stalls came once each and not in six later runs: 435 ms for the first
key typed in a field, in macOS making its spell checker
(`NSSpellChecker init`, waiting on the services daemon), and 108 ms for the
first click in a field. Both are the system's first use and outside the app.

## Limits

- The first window takes 100–140 ms to build and first draw, and the browser's first
  showing 85–155 ms, all of it SwiftUI making and laying out its views; no
  frame of the app's own code stands out in the stacks. Both are once per
  launch. A line in the backlog.
- A host can not be stopped while it loads the song: Cancel leaves the
  project unopened, and its song is closed as soon as the host has it.
- The watch reads stacks on Apple silicon only; on an Intel Mac stalls are
  timed and logged without frames.

## Decisions

- **A run-loop observer and a watchdog, not a ping.** A ping to the main
  queue measures only how late the queue answers, and misses work done in
  event tracking or a nested loop; the observer sees each piece of work in
  every common mode, and the watchdog reads the stack while it is still
  running, which the work's end can not.
- **Stacks read every 50 ms, not once.** The first measurement read the
  stack once, at 100 ms, and named the opening `Song.open` alone; read on,
  the same stall was five things in a row.
- **The open off the main thread, with a panel only when slow.** What every
  Mac app does: the window comes when the project is ready, and a long open
  says so with a way out. A panel for every open would flash on a small
  song.
- **Password AutoFill off for the whole app, not a field at a time.** A
  name field of its own with no neighbours (`nextValidKeyView` nil) took the
  walk out of the arrangement's fields, but SwiftUI's fields, the tempo and
  the browser's search, walked the window too. The app has no login to fill;
  its one secure field is an API key, pasted.
- **The lists read again as the browser shows, not cached.** The
  person's patches and racks change from a terminal; the browser reads them
  again as it shows, now off the main thread, and shows what it had
  meanwhile.
- **A budget over the work, not a time for each action.** The test fails on
  any piece of main-thread work over 100 ms while a large song opens, not
  on how long the opening takes in all, which depends on the machine and is
  off the main thread now.
