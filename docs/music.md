# Making music with AAW

Use `uv run daw ...` in this checkout. This guide is for operating existing
capabilities; developing them follows [development.md](development.md).

## Find the song and its state

A command's PROJECT is a project folder or its `song.yaml`. Use the supplied
path, or `uv run daw projects` to find open projects, the front window first.
For a new song, `uv run daw init projects/NAME` creates an empty project.
Before continuing a song, read its local `HANDOFF.md` if present.

Run `uv run daw inspect PROJECT` before editing and `uv run daw status PROJECT`
for transport, host state and the person's selection. Use `daw get PROJECT PATH`
for just the objects needed. If a Rust command reports a new `project` path after
Save As, use that path from then on, including for Python commands.

## Edit together

Prefer commands such as `set`, `clip move`, `note add` and `effect add`.
A running host (the Mac app, `daw host` or `daw play`; status reports `host: true`)
shows and plays edits as they land and keeps one undo history with their origin.
Without a host the commands edit the file, but undo/history require a host.
Read `daw changes PROJECT --since REV` for changes since a known host revision.
Give `daw batch PROJECT FILE` a `--label` describing the musical edit.

`inspect` returns `project_sha256`; use it with `daw apply PROJECT PATCH --expect SHA`
when a merge patch is needed. Objects merge, arrays replace, and null removes a
field. Raw file changes are possible, but a host records them as external edits.

Positions are zero-based quarter-note beats; use fractions such as `1/3` for
triplets. A grid of `1/4` means sixteenth notes.

## Load detail when needed

Use `uv run daw COMMAND --help` for syntax. The commands below also use the
`uv run` prefix; read only the topics the operation needs.

| Task | Read or do |
|---|---|
| Find sounds | `daw samples scan`, then `samples search`; filenames are hints, not measurements |
| Use a sound | `daw samples import` copies it into the project; import compressed audio before measuring it |
| Pitched or gated samples | `daw describe sampler`; confirm root octave with `samples analyze` or import `--root-note auto`; resolve `daw check` root warnings |
| MIDI notes and instruments | `daw describe midi` |
| Effects or return buses | `daw describe effects`; track stems include inserts but exclude return contributions and master effects; returns have separate stems |
| Automation | `daw describe automation`; give automated effects an `id` and ramp levels over a few milliseconds |
| Finished-song edits from timecodes | Follow [song-edit](../.claude/skills/song-edit/SKILL.md); load its CLI topics before the relevant step |

## Check and hand off

Run `daw check PROJECT` and investigate edits with a short section or isolated
track render. Full mixes point from `renders/latest.json`; previews use
`renders/latest-preview.json`. `daw listen` and `daw compare` measure audio;
do not claim listening when only numerical analysis was performed.
For finished-song edits, check rendered joins with `daw joins` before export;
read `daw describe export` when delivering a named file.

Keep the editable project, selected source hashes, stems and render report with
demos. Keep song-specific automation inside the ignored project directory.
Report the result, artifact paths and uncertainties. Update the project's
handoff when useful for continuing work, without copying state the CLI can read.
Audio, the sample database and environments stay out of Git; no upload is required.
