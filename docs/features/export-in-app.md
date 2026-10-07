# Export in the app (implemented 2026-10-07)

Status: built in the pull request that closed the first item of Next, decision
D94. Before it the app's only export was a MIDI clip: the mix left a project
through `daw render` and `daw export` in a terminal, so the person depended on
the agent or a shell for the last step of every song.

## What

**File › Export Audio…** (⇧⌘R, as in Ableton) writes the song's mix as a named
WAV, AAC or MP3 file at a stated level, as `daw export` does
([export.md](export.md)), and shows what it measured. A save panel asks for
the name and the place, with the format, the level and the ceiling under them:

| Control | Choices | `daw export` |
|---|---|---|
| Format | WAV 24-bit, WAV 16-bit, AAC, MP3 | `--to NAME.wav [--bits 16]`, `.m4a`, `.mp3` |
| Level | As rendered; True peak at … dBTP; Loudness … LUFS; Gain … dB | nothing, `--peak`, `--lufs`, `--gain` |
| Ceiling | A sample peak in dBFS, -0.1 unless changed; shown once a level is chosen | `--ceiling` |

The panel opens in the project's `exports/` folder, made if it is not there,
and offers the song's title as the name under the format's extension; the
extension follows the format when that changes. A project that has no name
yet opens the panel where the system last left it, since its folder is the
app's and goes when the project does. A file that is there already is asked
about, as every Mac app asks, and replaced with `--replace`. The last format,
level and ceiling chosen are offered next time. A level chosen in the menu
starts at -1 dBTP, -14 LUFS or 0 dB until a number is typed.

While the export runs the transport bar says "Exporting NAME.m4a…" with a
spinner, and the window stays usable: the song plays, edits land and the
agent's commands arrive. The menu item is gray until it is done, since one
export runs at a time. When it is done a banner under the transport bar shows
the file's length, its loudness, its true peak, and when a level was asked
for, the gain applied and how much the ceiling held back, with the warnings
`daw export` gives (a level that needed more than the ceiling allowed; a
compressed file's true peak above -1 dBTP), and a Show in Finder button. It
stays until its × is pressed. An export that wrote nothing shows the
command's reason the same way, in red.

## Why

The app could not finish a song. "Give it a finished song and timecodes and
get an edit … exported under a name" ([concept.md](../concept.md)) was
something only the agent could do. Of everything the review of October 6, 2026
found the app lacked beside `daw` (D81), this is the one the person meets on
every song.

## Design

**One export, through `daw`.** The app runs the bundle's `daw export PROJECT
--to PATH …` as a process off the main thread, on a queue of its own so that a
render never waits behind a fader, with the options the panel chose
(`Export.swift`). There is one export either way: the file, the level
policies, the dither, the ceiling and the `NAME.EXT.json` record are the ones
[export.md](export.md) describes, and a change to `daw export` shows up in the
app by itself. The app reads the command's JSON result for the banner and its
stderr, the `error` of its JSON or the text as it is, for the reason. Both
pipes are read at once, so a chatty command cannot stall the read.

**What is exported** is the song as the host last saved it, which is the song
in the window: the host saves after every edit. `daw export` renders when the
latest render is stale or missing, reading the file beside the running host,
and that render is the one the agent's `daw listen` and `daw compare` read
next, under `renders/`. The transport bar cannot tell the render from the
writing, since both are one command, so it says "Exporting" through both.

**Refusals are the command's.** A render that would clip names the loudest
stems and the paths to lower; a missing encoder names what to install; a
bundle on a Mac without Python says where it looked. Each is shown in the
banner as the message `daw` gives, and the song is not changed.

**The whole mix only**, as `daw export` writes: no track, section or stems.
Stems are a Later line under Export.

**Scripted runs.** `--export PATH [--export-level peak=-1|lufs=-14|gain=-3|
rendered]` on the app's command line does what the panel does when it closes,
after the other actions, since a save panel waits for a person and cannot be
scripted: the format is the extension's, 24-bit for a WAV. What the command
reported is printed on stdout, the picture is taken if one was asked for, with
the banner in it, and the run quits; a failed export says why on stderr and
ends with status 1. A file that is there is refused as the command refuses it,
since nobody is there to answer the panel. 16-bit WAV is the panel's alone.

**Not a change to the song.** An export changes nothing in the song, so it is
not an undo step and not in the change log, as `daw export` is not, and the
Activity panel does not list it.

## Checked

Scripted runs on a generated song exported a WAV, an AAC and an MP3 at
`peak=-1` through the bundle; the WAV and the MP3 are byte for byte what
`daw export` writes with the same options, and the AAC's record equals the
command's in everything but the file's hash, since `afconvert` writes a
different file each run, from the terminal too. A file already there and a
missing Python were refused with the command's message, the run ending with
status 1, and the green and red banners were seen in pictures at the window's
size. The Swift tests hold the mapping from the panel's choices to the
command's arguments, the level's spelling, the remembered options, the
report and the reason read from a process, with a stderr past a pipe's size.
Nothing was exported by hand or heard, and the panel with its format, level
and ceiling was never opened: a save panel waits for a person.

## Limits

- Export Audio… needs the Python the bundle's `daw` finds, as `daw export`
  does; the Python tools are not in the bundle ([Mac README](../../apps/mac/README.md)).
- One export at a time; a second asked for while one runs is refused with the
  first file's name.
- `--match SAMPLE`, `--bitrate` and `--render DIR` are the command's alone.
- The exports folder of an Untitled project is not offered, and one made there
  by hand makes the project ask about saving when it closes, since the folder
  then holds something.

## Open questions

- Whether a render should run inside the host, in Rust, so that a WAV export
  at the rendered level needs no Python and a bundle on another Mac can export;
  the level policies measure loudness in Python today.
- Export Stems…, over `daw render`'s stems, in the same panel or a second item.
- A loudness or true-peak preset the person keeps (-14 LUFS for streaming, -1
  dBTP), rather than retyping; the last values are offered, which is most of
  it.
