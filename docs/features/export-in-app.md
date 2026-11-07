# Export in the app (implemented 2026-10-07; the sheet with a range 2026-11-07)

Status: built in the pull request that closed the first item of Next, decision
D94, as a save panel with the format, level and ceiling under the name; made a
sheet with a range, a sample rate and a bit depth that writes into `exports/`
on November 7, 2026, at the person's request, decision D176. Before D94 the
app's only export was a MIDI clip: the mix left a project through `daw render`
and `daw export` in a terminal, so the person depended on the agent or a shell
for the last step of every song.

## What

**File › Export Audio…** (⇧⌘R, as in Ableton) opens a sheet over the song
window that writes what is heard of the song, the whole of it or a range, as a
named WAV, AIFF, FLAC, AAC or MP3 file, as `daw export` does
([export.md](export.md)), and shows what it measured:

| Control | Choices | `daw export` |
|---|---|---|
| Name | The song's title, or the project folder's name while the title is still Untitled; the format's extension shown after it | `--to exports/NAME.EXT` |
| Start, End | A bar, a beat and a sixteenth each, as the transport writes a position: the beat the note the meter counts, an eighth in x/8 with two sixteenths; a decimal sixteenth is a place between sixteenths; End is where the file stops; Whole Song puts back the whole song | `--start BEATS --end BEATS`, or nothing for the whole song |
| Format | WAV, AIFF, FLAC, AAC, MP3 | the extension |
| Sample Rate | 44.1, 48, 88.2 or 96 kHz and the song's, marked; at most 48 kHz for AAC and MP3 | `--sample-rate`, when not the song's |
| Bit Depth | 16-bit, 24-bit, 32-bit float; FLAC to 24; shown for WAV, AIFF, FLAC | `--bits`, when not 24 |
| Bitrate | 128, 192, 256, 320 kb/s; shown for AAC and MP3 in place of the depth | `--bitrate`, once chosen |
| Level | As rendered; True peak at … dBTP; Loudness … LUFS; Gain … dB | nothing, `--peak`, `--lufs`, `--gain` |
| Ceiling | A sample peak in dBFS, -0.1 unless changed; shown once a level is chosen | `--ceiling` |

**The range** starts as what the selected clips sound over: drag a
rectangle over clips, or Shift-click them, and press ⇧⌘R, and Start is where
the first of them starts sounding, an audio clip's lead before its beat
included, and End where the last stops, its fade's tail included, cut at the
song's end. A clip off the grid gives the place it is at, not the nearest
sixteenth: a clip at beat 4.3 in 4/4 is Start 2 . 1 . 2.2. With no clip
selected it is the whole song. Under the fields a line says what the range is,
"Bars 2–35, the selected clips · 1:08" or "The whole song · 2:30"; a place off
the grid is written as the nearest sixteenth and how far from it, "1.4.4 +
55 ms", so it is never read as a sixteenth it is not. A range that ends after
the song does, a beat the bar does not have, a sixteenth the beat does not
have, an end not after the start or an empty name is said in orange beside
the buttons, and Export cannot be pressed until it is fixed.

**Export** writes into the project's `exports/` folder, made if it is not
there, with no save panel. A file there already is asked about, as every Mac
app asks — "“NAME” already exists in exports. Do you want to replace it?" —
and replaced with `--replace`. An Untitled project's folder is the app's and
goes when the project does, so its button reads Export… and a save panel asks
where, as before. The format, rate, depth, bitrate, level and ceiling chosen
are offered next time, and Settings › Audio's bit depth is the same choice; the
name and the range are not, since they belong to one export. A level chosen
in the menu starts at -1 dBTP, -14 LUFS or 0 dB until a number is typed.
Escape or Cancel closes the sheet and writes nothing.

While the export runs the transport bar says "Exporting NAME.m4a…" with a
spinner, and the window stays usable: the song plays, edits land and the
agent's commands arrive. The menu item is gray until it is done, since one
export runs at a time, and while the sheet is open. When it is done a banner
under the transport bar shows the range when it was not the whole song, written as the sheet writes a place, the
file's length, its loudness, its true peak, and when a level was asked for,
the gain applied and how much the ceiling held back, with the warnings
`daw export` gives, and a Show in Finder button. It stays until its × is
pressed. An export that wrote nothing shows the command's reason the same
way, in red.

## Why

The app could not finish a song. "Give it a finished song and timecodes and
get an edit … exported under a name" ([concept.md](../concept.md)) was
something only the agent could do. Of everything the review of October 6, 2026
found the app lacked beside `daw` (D81), this is the one the person meets on
every song.

## Design

**One export, through `daw`.** The app runs the bundle's `daw export PROJECT
--to PATH …` as a process off the main thread, on a queue of its own so that a
render never waits behind a fader, with the options the sheet chose
(`Export.swift`; the sheet is `ExportSheet.swift`). There is one export either way: the file, the level
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

**What is heard.** The range is cut from the full render, in which muted and
solo-muted tracks and deactivated clips are silent, so they are not in the file
even where they make the song longer than the range. The range is sent in
beats, the bar, beat and sixteenth fields read in the song's meter (`ExportRange`), so the
command and the app agree on where a bar starts in any time signature. No
track, section preview or stems: stems are a Later line under Export.

**Scripted runs.** `--export PATH [--export-level peak=-1|lufs=-14|gain=-3|
rendered] [--export-range START-END]` on the app's command line does what the
sheet does when Export is pressed, after the other actions, since a sheet
waits for a person: the format is the extension's at the song's rate, 24-bit
for WAV, AIFF and FLAC, and the range is the one ⇧⌘R would start with, the
selected clips' span or the whole song, unless `--export-range` gives one in
beats. `--key cmd+shift+r` opens the sheet, and a picture taken with it open is
written beside the window's as `NAME-sheet.png`. What the command
reported is printed on stdout, the picture is taken if one was asked for, with
the banner in it, and the run quits; a failed export says why on stderr and
ends with status 1. A file that is there is refused as the command refuses it,
since nobody is there to answer the question. A rate, a depth and a bitrate are
the sheet's alone.

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

The sheet of November 7, 2026 was checked by Swift tests of the mapping from
its choices to the command's arguments, the bar, beat and sixteenth of a range in 4/4,
6/8 and 7/8, the range from a selection of clips in a song the model opened,
a range starting at an off-grid clip and at an audio clip's lead, what the sheet says of a range and when Export is gray, the options
remembered and the older `wav16` read as a 16-bit WAV; and by scripted runs
of the bundle: the sheet opened by ⇧⌘R and pictured, and a range exported.
Nothing was pressed, typed or heard by hand.

## Limits

- Export Audio… needs the Python the bundle's `daw` finds, as `daw export`
  does; the Python tools are not in the bundle ([Mac README](../../apps/mac/README.md)).
- One export at a time; a second asked for while one runs is refused with the
  first file's name.
- `--match SAMPLE`, `--bars`, `--section` and `--render DIR` are the
  command's alone; the sheet's bar, beat and sixteenth fields reach every range those do.
- The range ends where End is: what rings past it is cut, as at the song's
  end. A range cannot reach past the song's end.
- An Untitled project exports where its save panel says, not into its own
  folder's `exports/`; one made there by hand makes the project ask about
  saving when it closes, since the folder then holds something.
- The time selection of the backlog is not built, so a range is set from
  clips or typed; the loop brace does not set it.

## Open questions

- Whether a render should run inside the host, in Rust, so that a WAV export
  at the rendered level needs no Python and a bundle on another Mac can export;
  the level policies measure loudness in Python today.
- Export Stems…, over `daw render`'s stems, in the same panel or a second item.
- A loudness or true-peak preset the person keeps (-14 LUFS for streaming, -1
  dBTP), rather than retyping; the last values are offered, which is most of
  it.
