# What makes the person wait — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 5. The person's note of November
22, 2026 (D194): "a system wide scan for inefficiencies, anything that could
be slowing down actions. I want to optimize performance to make sure this
system can run as fast as possible and never see the spinning mac wheel when
loading or processing something."

## What

A sweep of the app, the host and the `daw` commands the app runs, measured
before anything is changed: every action a person takes timed from the
input to the window showing its result, every stall of the main thread over
100 ms found and named, and each fixed so that the spinning wheel never
shows. What is slow and cannot be made quick runs off the main thread with
progress and a cancel. The result is a table in this file of each action,
its time before and after, and what was changed, and a test that holds the
worst of them.

## Why

The person has seen the spinning wheel loading and processing. Work done on
the main thread blocks the window; the app was built a feature at a time and
nothing has measured it as a whole.

## Design

- **Measure first.** Signposts (`os_signpost`) around each edit's round trip
  to the host, each redraw, each file read and each process the app runs;
  a hang detector on the main thread that logs any stall over 100 ms with
  its stack; a scripted session that opens a large song, plays, zooms,
  scrolls, drags clips, edits notes, opens the browser on a large library and
  saves, run with `./build.sh --measure`, so that before and after are the
  same steps. The host's commands timed the same way, and `daw` commands by
  their wall time.
- **Places to measure first,** read from the checkout on November 22, 2026,
  none yet found slow: the commands the app runs synchronously on the main
  thread (`SongModel`'s `commands.sync` for Save As, release and close); the
  processes the app waits on (`Export`, `Generate`, `CommandLineTool`'s
  `waitUntilExit`); the agent's thread files read whole (`AgentThreads`);
  the arrangement sent after every edit and what the window rebuilds from
  it; the waveform's columns recomputed on every draw (`Waveform.columns`);
  the browser's lists of a large library; a song opened with many audio
  files whose peaks are made; the engine's render and the full graph past
  the end (Next, item "Full DSP runs forever past the end").
- **Fix the measured.** Off the main thread what reads the disk or waits on
  a process; cached what is recomputed unchanged; incremental what is rebuilt
  whole; each fix its own commit with its numbers.

## Done when

- The scripted session runs with no main-thread stall over 100 ms, on a song
  of 30 tracks and a library of 50,000 samples.
- The table of actions before and after is in this file, and a test fails
  when the worst of them regresses past its budget.
