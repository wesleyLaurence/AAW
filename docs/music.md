# Making music with AAW

Use `uv run daw ...` in this checkout, from its root or from a project's
folder under it. This guide is for operating existing capabilities; developing
them follows [development.md](development.md).

## Find the song and its state

A command's PROJECT is a project folder, its `song.yaml` or its `.aaw`. Use the supplied
path, or `uv run daw projects` to find open projects, the front window first.
For a new song, `uv run daw init projects/NAME` creates an empty project, in
4/4 unless `--time-signature 3/4` says otherwise;
`uv run daw describe start` is the dozen commands from there to a rendered song,
and the quickest way to learn the verbs.

Run `uv run daw map PROJECT` to see the arrangement bar by bar: where each
track plays, the same music under the same letter, and `#` where two clips of a
track sound at once. Run it again after laying out clips, before rendering.
Run `uv run daw inspect PROJECT` before editing and `uv run daw status PROJECT`
for transport, host state and the person's selection. Use `daw get PROJECT PATH`
for just the objects needed. If a Rust command reports a new `project` path after
Save As, use that path from then on, including for Python commands.

## What the song and the person ask for

Every project has a `SONG.md` at the top of its folder. It is to the project
what `AGENTS.md` is to a code repository: read at the start of every session
on the song, short, and about what holds, not what happened.

| Section | Whose | Holds |
|---|---|---|
| Vision | The person's: suggest a change, never rewrite it | The idea, the feel and style, the references, what the song is for |
| Instructions | The person's: never edit it | Rules for this song alone: what not to touch, who it is for, how it is delivered |
| Direction | Both: keep it current | Where the song is heading: what is wanted next, what is unresolved, what the person liked and disliked that still holds |
| Decisions | Both | Settled choices and why |

- **Have it before the first edit.** The project's own `AGENTS.md` imports it,
  and Claude Code then puts its text in the session: at launch when the
  session started in the project's folder, and at the first file read in the
  project when it started at the root. A `daw` command is not a file read, so
  read `SONG.md` yourself when its text is not in the session yet.
- **No history.** No log, no dated list of sessions, no revision notes. A
  decision that is replaced is rewritten where it stands. What happened is the
  host's history (`daw changes`) and the conversation.
- **Nothing the tool prints.** Tracks, clips, tempo, sections and levels come
  from `daw inspect`, `daw map` and `daw status`.
- **About a page,** since every session reads it whole. Past that, propose
  what to cut; start no second file.
- **Say what you changed in it** at the end of a turn that changed it.
- **A project without one** was made before `daw init` and the app wrote it.
  Offer to write it with the person: from a `HANDOFF.md` or `brief.md`, keep
  the vision and the rules and decisions that still hold, and drop the
  revision lists.

The person's instructions for all their music are
`.claude/rules/instructions.md` at the checkout's root, and what agents have
learned of their taste is `.claude/rules/taste.md` beside it. Claude Code loads
both into every session started at the root or below it; read them first
otherwise.

- **Never edit the instructions.** They are the person's.
- **Taste is written with the person.** Each entry has its evidence and dates:
  "Keeps lead vocals dry and upfront. Evidence: asked to remove vocal reverb in
  two songs (2026-10-02, 2026-10-15)." Say what you would add at the end of a
  session, and write it when the person agrees. An edit the person made by
  hand may be an experiment or a fix for one song: ask whether it holds beyond
  this song before writing it down. What is true of one song goes in its
  `SONG.md`.
- **Where they differ:** the person's instructions, then `SONG.md`, then
  taste, then defaults. Say which one decided when it matters.

A skill is a workflow the person keeps for a kind of job. Skills for every
project are in `.claude/skills/` at the root, and a project's own in
`.claude/skills/NAME/SKILL.md` in its folder. Where the session started
decides what it has:

| Started in | Has | Lacks |
|---|---|---|
| The project's folder, `cd projects/NAME && claude` | The root instructions, the person's two files and the root's skills from the folders above; `SONG.md` at launch; the project's skills as commands; a conversation list of this project alone (`claude --resume`, `claude --continue`); PROJECT is `.` | Other projects, which are outside the session's folder: reading one asks first, and `claude --add-dir` on the checkout or the other project grants it |
| The checkout's root | Every project in reach, to reuse a sound from another song or compare two mixes | A project's skills, which Claude Code does not load from a folder Git ignores: read the name and description at the top of each `SKILL.md` under the project's `.claude/skills/`, and the body of one that applies. One conversation list for every song |
| The app's agent panel (View › Agent) | What a session in the project's folder has, the checkout attached, and ahead of each message the start position, the loop, the selection and what the person changed in the window since the last turn; `daw` commands on the project run without asking, and the project's conversations are listed in the panel's menu ([the agent panel](features/agent-panel.md)) | Anything that is not `daw` on the project is a card the person answers, so say in words what is needed outside the project before asking for it |

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

The person leaves markers while they listen: M in the app drops one where the
song is playing, with a few words if they want, "too busy" or "love this". Run
`uv run daw marker list PROJECT` when they say "here", "this bar" or "my
markers", and when an edit's reply carries `new_markers`: each marker comes
with the bar it is in, the section over it and the clips sounding there, and
`daw map` shows them as a row. A marker dropped while the song played is a
moment after what was heard, so read what led into it too, and ask when it
could mean either. Answer a marker by changing the music or by saying why not,
and remove it in the same batch as the edit that answers it (`{"op":
"marker.remove", "markers": ["m1"]}`), so that one undo brings both back;
leave one that asks for nothing and keep what it marks. `daw marker add
PROJECT AT --text TEXT` leaves one of your own, to point the person at a bar.

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
| Find sounds | `daw describe samples`: `daw favorite list` first, what the person keeps at hand; `daw samples search` by name, sorted and filtered by what a sample measures as (`--sort punch`, `--short --bright`, `--max-decay 300`), and `daw samples like SAMPLE` for the nearest to one chosen; `daw samples analyze --all` measures the library once; filenames are hints, and a measurement is not a listen |
| Use a sound | `daw samples import` copies it into the project; import compressed audio before measuring it |
| A song's vocals, instrumental or stems | `daw describe separate`; `daw separate PROJECT SOURCE` for an a cappella and an instrumental, `--split stems` for drums, bass, vocals and the rest, each a sample in the project, `--tracks` to lay them under the source; `daw separate models` says what is downloaded, and a measurement is not a listen |
| A sound the library lacks | `daw describe generate`; `daw generate sound PROJECT "TEXT" --seconds S`, or `--bars N --loop` for a loop at the song's tempo, makes it with ElevenLabs at the person's own key and imports it as a sample, placing nothing; `--estimate` first, within the agent's daily budget; without a key the person enters one with `daw keys set elevenlabs`; a prompt names no artist, and a measurement is not a listen |
| Pitched or gated samples | `daw describe sampler`; confirm root octave with `samples analyze` or import `--root-note auto`; resolve `daw check` root warnings |
| MIDI notes and instruments | `daw describe midi` |
| A synthesized sound | `daw describe synth`; start from a factory patch with `daw synth add TRACK --patch NAME` (`daw patch list`), hear it with `daw synth audition`, keep it with `daw patch save` |
| Effects, groups or return buses | `daw describe effects`; a drum bus is `daw group add PROJECT drums --tracks kick,snare,hats`, with effects, a level and sends of its own; a chain worth keeping is `daw rack save PROJECT tracks.T NAME`, and `daw rack load` adds it to a chain in any song (`daw rack list`); track stems include inserts but exclude return contributions and master effects; groups and returns have separate stems, and a grouped track's stem is before its group |
| Automation | `daw describe automation`; give automated effects an `id` and ramp levels over a few milliseconds |
| Repeat, insert, remove or empty whole bars across the tracks | `daw range copy\|insert\|delete\|clear` and `daw section duplicate\|move\|remove --with-content`; `daw describe edit` under `ranges` |
| Finished-song edits from timecodes | `daw describe edit`, `beats`, `joins` and `export`, each before its step; follow the person's own skill for the kind of edit when one is listed, at the root or in the project |

## Check and hand off

Run `daw check PROJECT` before rendering and read each warning's code
(`daw describe check`); investigate edits with a short section or isolated
track render. Full mixes point from `renders/latest.json`; previews use
`renders/latest-preview.json`. `daw listen` and `daw compare` measure audio;
do not claim listening when only numerical analysis was performed
(`daw describe listen`). A render's `overlap` names the pairs of stems that put
their energy in the same band at the same moments; `daw listen RENDER --overlap
A B` reads one pair by band and section, and `daw compare` says what an edit did
to it. A pair is an observation, not a fault: say which pair and band before
choosing what gives way. `spectrum_db` is the mix and each stem in third
octaves, and a stem's `resonances` are its peaks that stay while the notes
move, each with the `freq_hz`, `prominence_db` and `q` an `eq`'s bell takes;
read `could_be_note` before cutting one, and `daw compare` afterwards for how
far it fell. `translation` is what the mix and each stem lose in mono and on a
small speaker, with the largest in `observations`: a low end that cancels, a
bass a phone will not play. `daw listen RENDER --write-translation` writes
`mono.wav` and `small-speaker.wav` for the person to play; a loss can be
meant, so say what was found before narrowing or driving anything.
Before setting a compressor, a limiter or a clipper, read `effects`, what each
one took off in each section, and a track's `hits`: `punch_db` is how hard its
hits land at its stem and `punch_in_mix_db` in the mix, so a kick that hits at
its stem and not in the mix is covered, and an attack that is too fast shows
as `punch_db` falling in `daw compare`. The mix's `loudness_range_lu`,
`max_short_term_lufs` and `max_momentary_lufs` are what a master is held to.
A master limiter that takes several decibels at the drums' hits turns the
whole mix down at each one; a `clipper` before it, its ceiling a few decibels
under the peaks that reach it, cuts those peaks off instead and leaves the
limiter less to do (`daw describe effects`). It distorts what it cuts, so read
its `fraction_over_1db_reduction`, which should stay near zero, and have the
person hear it.
A limiter holds the samples, and the mix's `estimated_true_peak_dbtp` passes
its ceiling. For a file that is to be encoded or streamed, set `true_peak:
true` on the master's last limiter with `ceiling_db` the true peak wanted,
−1 as a rule, instead of lowering the ceiling by guess or the whole export
with `--peak`; the render's report then reads at or under it.
When the person names a song as what the mix should sound like, keep it with
`daw reference add FILE --name NAME` and read `daw compare RENDER --reference
NAME` for how the mix differs from it, by section (`daw describe reference`);
its lines are differences, not faults, so say what was found before changing
the mix toward it.
For finished-song edits, check rendered joins with `daw joins` before export;
read `daw describe export` when delivering a named file.

Keep the editable project, selected source hashes, stems and render report with
demos. Keep song-specific automation inside the ignored project directory.
Report the result, artifact paths and uncertainties. Bring the project's
`SONG.md` up to date where the session changed where the song is heading or
what is decided, without copying state the CLI can read, and say what you
changed in it.
Audio, the sample database and environments stay out of Git; no upload is required.
