# Export in the app — proposed October 6, 2026

Status: proposed, not built. Backlog: Next, Export in the app.

## What

**File › Export Audio…** (⇧⌘R, as in Ableton) writes the song's mix as a named
WAV, AAC or MP3 file at a stated level, as `daw export` does
([export.md](export.md)), and shows what it measured. A save panel asks for
the name and the place, with the format and the level beside them:

| Control | Choices | `daw export` |
|---|---|---|
| Format | WAV 24-bit, WAV 16-bit, AAC, MP3 | `--to NAME.wav [--bits 16]`, `.m4a`, `.mp3` |
| Level | As rendered; True peak at … dBTP; Loudness … LUFS; Gain … dB | nothing, `--peak`, `--lufs`, `--gain` |
| Ceiling | A sample peak in dBFS, -0.1 unless changed | `--ceiling` |

The panel opens in the project's `exports/` folder, made if it is not there,
and offers the song's title as the name. A file that is there already is asked
about, as every Mac app asks, and replaced with `--replace`. The last format and
level chosen are offered next time.

While the export runs the transport bar says what it is doing, "Rendering…"
then "Writing NAME.m4a…", and the window stays usable. When it is done a
banner shows the file's length, its loudness, its true peak, the gain applied
and how much the ceiling held back, with the warnings `daw export` gives
(a level that needed more than the ceiling allowed; a compressed file's true
peak above -1 dBTP), and a button that reveals the file in the Finder.

## Why

The app cannot finish a song. Its only export is Export MIDI Clip…; the mix
leaves the project only through `daw render` and `daw export` in a terminal,
so the person depends on the agent or a shell for the last step of every song,
and "give it a finished song and timecodes and get an edit … exported under a
name" ([concept.md](../concept.md)) is something only the agent can do. Of
everything the review of October 6, 2026 found the app lacked beside `daw`
(D81), this is the one the person meets on every song.

## Design

**One export, through `daw`.** The app runs the bundle's `daw export PROJECT
--to PATH …` as a process off the UI thread, as the browser runs the sample
search, with the options the panel chose. There is one export either way: the
file, the level policies, the dither, the ceiling and the `NAME.EXT.json`
record are the ones [export.md](export.md) describes, and a change to `daw
export` shows up in the app by itself. The app reads the command's JSON result
for the banner and its stderr for the error.

**What is exported** is the song as the host last saved it, which is the song
in the window: the host saves after every edit. `daw export` renders when the
latest render is stale or missing, and that render is the one the agent's
`daw listen` and `daw compare` read next, under `renders/`.

**Refusals are the command's.** A render that would clip names the loudest
stems and the paths to lower; a missing encoder names what to install; a
bundle on a Mac without Python says so. Each is shown in the banner as the
message `daw` gives, and the song is not changed.

**The whole mix only**, as `daw export` writes: no track, section or stems.
Stems are a Later line under Export.

**Scripted runs.** `--export PATH [--export-level peak=-1|lufs=-14|gain=-3]`
on the app's command line does what the panel does when it closes, since a
save panel waits for a person and cannot be scripted, so a test can run the
export in the app and read the file and its record.

**Not a change to the song.** An export changes nothing in the song, so it is
not an undo step and not in the change log, as `daw export` is not. Whether
the Activity panel should list it anyway is an open question.

## Done when

- File › Export Audio… writes a WAV, an AAC and an MP3 of a generated song
  that are byte for byte what `daw export` writes with the same options, with
  the record beside each.
- The banner shows the measurements of the record, and the clipping, missing
  encoder and existing file refusals each show their message and write
  nothing.
- A scripted `--export` run makes the file and quits, and the Swift tests cover
  the mapping from the panel's choices to the command's arguments.
- `daw describe export` and the app README name the menu item.

## Open questions

- Whether a render should run inside the host, in Rust, so that a WAV export
  at the rendered level needs no Python and a bundle on another Mac can export;
  the level policies measure loudness in Python today.
- Whether the Activity panel lists an export, so that a session's deliverables
  are in the window with the edits that made them.
- Export Stems…, over `daw render`'s stems, in the same panel or a second item.
- A loudness or true-peak preset the person keeps (-14 LUFS for streaming, -1
  dBTP), rather than retyping.
