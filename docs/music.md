# Making music with AAW

Use `uv run daw ...` in this checkout. This guide is for operating existing
capabilities; developing them follows [development.md](development.md).

## Find the song and its state

A command's PROJECT is a project folder or its `song.yaml`. Use the supplied
path, or `uv run daw projects` to find open projects, the front window first.
For a new song, `uv run daw init projects/NAME` creates an empty project, in
4/4 unless `--time-signature 3/4` says otherwise;
`uv run daw describe start` is the dozen commands from there to a rendered song,
and the quickest way to learn the verbs.
Before continuing a song, read its local `HANDOFF.md` if present.

Run `uv run daw map PROJECT` to see the arrangement bar by bar: where each
track plays, the same music under the same letter, and `#` where two clips of a
track sound at once. Run it again after laying out clips, before rendering.
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

With a host running, work in steps the person can watch. Make the song a part
at a time: a sound found and imported, a track added, a clip and its notes,
each a command as soon as it is decided, so it appears on the timeline and in
the activity as it is made and the person can say what they think before the
next. Read the song once, then start placing; plan the next part while the
last one plays. Do not plan the whole song and then run everything at once. A
batch is one musical edit, a phrase with its variations, not the song held
back until it is finished. A host whose agent sent nothing for five minutes
says so in the next edit's reply, as `hint`.

`inspect` returns `project_sha256`; use it with `daw apply PROJECT PATCH --expect SHA`
when a merge patch is needed. Objects merge, arrays replace, and null removes a
field. Raw file changes are possible, but a host records them as external edits.

Positions are zero-based quarter-note beats; use fractions such as `1/3` for
triplets. A grid of `1/4` means sixteenth notes.

## Load detail when needed

Use `uv run daw COMMAND --help` for syntax and the fields a command takes.
`daw describe TOPIC` says what a topic's fields mean and gives each field's path,
range and default on one line; `--schema` adds the JSON Schema when it is
needed. The commands below also use the `uv run` prefix; read only the topics
the operation needs. A refused edit names the field it meant and the fix.

| Task | Read or do |
|---|---|
| Find sounds | `daw samples scan`, then `samples search`; filenames are hints, not measurements |
| Use a sound | `daw samples import` copies it into the project; import compressed audio before measuring it |
| Pitched or gated samples | `daw describe sampler`; confirm root octave with `samples analyze` or import `--root-note auto`; resolve `daw check` root warnings |
| MIDI notes and instruments | `daw describe midi` |
| A synthesized sound | `daw describe synth`; start from a factory patch with `daw synth add TRACK --patch NAME` (`daw patch list`), hear it with `daw synth audition`, keep it with `daw patch save` |
| Effects, groups or return buses | `daw describe effects`; a drum bus is `daw group add PROJECT drums --tracks kick,snare,hats`, with effects, a level and sends of its own; a chain worth keeping is `daw rack save PROJECT tracks.T NAME`, and `daw rack load` adds it to a chain in any song (`daw rack list`); track stems include inserts but exclude return contributions and master effects; groups and returns have separate stems, and a grouped track's stem is before its group |
| Automation | `daw describe automation`; give automated effects an `id` and ramp levels over a few milliseconds |
| Repeat, insert, remove or empty whole bars across the tracks | `daw range copy\|insert\|delete\|clear` and `daw section duplicate\|move\|remove --with-content`; `daw describe edit` under `ranges` |
| Finished-song edits from timecodes | Follow [song-edit](../.claude/skills/song-edit/SKILL.md); load its CLI topics before the relevant step |

## Check and hand off

Run `daw check PROJECT` before rendering and read each warning's code
(`daw describe check`); investigate edits with a short section or isolated
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
