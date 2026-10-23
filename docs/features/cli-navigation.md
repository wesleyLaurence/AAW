# Finding one's way in the CLI — proposed October 23, 2026

Status: item 1, help in sections, built October 23, 2026 (D149, PR #143);
item 2, describe batch, built the same day (D150, PR #144); items 3–6
proposed, not built (D148). Backlog: Next, items 1–4. It follows
[learning the CLI quickly](cli-discovery.md) (D76), which made `describe` short,
wrote `describe start` and gave errors their fixes; this is what an agent still
trips on three weeks and some seventy decisions later.

## What

Six changes, each its own pull request, so that an agent that knows nothing
of `daw` at the start of a session finds the command, the batch op, the field
or the recipe it needs in one look:

1. [`daw --help` in sections](#help-in-sections)
2. [`daw describe batch`](#describe-batch): every op a batch takes
3. [A part of a topic](#a-part-of-a-topic): `daw describe effects.compressor`
4. [Recipes](#recipes), tested as `describe start` is
5. [Names that do not collide](#names-that-do-not-collide): `loop`, `device`
6. [No empty help lines](#no-empty-help-lines)

## Why

Asked on October 23, 2026 how it finds its way in `daw`, an agent described it
as one document with a path grammar and verbs around it, and named what is
hard. The person asked for these fixes. Measured that day:

| What | Then | Now |
|---|---|---|
| Top-level commands in `daw --help`, one flat list | — | 61 |
| Commands in the whole help tree (Rust) | — | 177, 12 with no description |
| `daw describe effects` | 8,809 bytes (October 4) | 27,416 bytes |
| `daw describe sampler` | 6,479 bytes (October 4) | 14,396 bytes, with all of `project`'s semantics repeated in it |
| Batch ops in the host's `Command` enum | — | 117, listed nowhere; one example in `describe start` and one in a line of prose |

The agent's own picture of the tool has layers the help does not show: the
song as a tree of paths, the typed verbs over it, the views that read it, the
transport and its host, rendering and measuring, and the libraries outside any
one song. A flat list of 61 makes every session rebuild that picture. Batch
ops are guessed from the pattern `noun.verb`, which breaks where the CLI nests
(`pattern event add` is `event.add`, `lane point add` is `point.add`). To
check one compressor field an agent reads every effect kind, roughly 7,000
tokens. And `daw device` means the Mac's audio hardware where every other
place in the tool, the app and `describe effects` means an instrument or an
effect by "device".

The person also asked whether skills of standard procedures would help. The
answer taken (D148) is that the mechanics stay in the CLI, where the code
generates them and a test runs them, and skills stay for the person's own
jobs; recipes (item 4) are the procedures, kept where they cannot go stale.

## Design

### Help in sections

Built October 23, 2026 (D149). `daw --help` lists the commands under headings
instead of one list. clap 4 puts every subcommand under one heading
(`subcommand_help_heading`), so `main` builds the command clap derives, makes
the root's `help_template` from it and parses with that: the usual usage and
options, and between them a list made from one table in `aaw-cli`,
`SECTIONS`, a heading and its commands, each with its line as clap has it. A
test fails when a command is under no heading or two, or a heading names one
that does not exist, so a new command cannot be left out. clap's own `help`
subcommand is under none; `-h, --help` is among the options. `daw COMMAND
--help` is unchanged.

| Heading | Commands |
|---|---|
| Start | `describe`, `init`, `projects`, `move`, `copy` |
| Read the song | `inspect`, `map`, `get`, `status`, `changes`, `timeline`, `schedule`, `check`, `model` |
| Edit any value | `set`, `toggle`, `remove`, `apply`, `batch`, `undo`, `redo`, `fmt` |
| Parts of the song | `track`, `group`, `return`, `clip`, `audio`, `note`, `pattern`, `pad`, `midi`, `instrument`, `synth`, `effect`, `send`, `lane`, `section`, `marker`, `range` |
| Play and record | `host`, `close`, `play`, `stop`, `locate`, `loop`, `record`, `device` |
| Render and measure | `render`, `listen`, `compare`, `joins`, `export` |
| Sounds and libraries | `samples`, `reference`, `separate`, `generate`, `patch`, `rack`, `favorite`, `plugin`, `keys` |

Above the list, two lines say what the sections imply: the song is one
document addressed by paths (`tracks.drums.clips.0`), which the commands under
Parts of the song edit with checks and defaults and those under Edit any value
edit directly; and a running host, `daw host` or the app with the project
open, adds `@N` references, undo and redo, and playback.

### describe batch

Built October 23, 2026 (D150). `daw describe batch` prints every op a batch
takes, the edits: its name, the command it is the same as (`clip.add` is
`daw clip add`), and its keys, each with what it takes, its default and
whether it is required, in the one-line form `describe` gives fields. The
lines are made from the `Command` enum, which derives a JSON Schema
(schemars), as `contract::listing` makes field lines from the model's
schema, so a new op is listed when it is added. Three things the enum does
not say are tables in `aaw-cli/src/batch.rs`, each held by a test: what a
key typed as any JSON value takes, by its name (`at` beats, `points` a list
of points); the models whose fields an op with fields takes besides its
keys (`clip.add` a Clip's or a NoteClip's, with the topic that lists them);
and the ops whose words are not the command's (`event.*` under `daw pattern
event`, `point.*` under `daw lane point`, `point.set` as `lane point
move`), every op held to a command that exists. Four lines of semantics say
how a batch file is written, how ops are spelled, what keys take and what is
refused; `--schema` adds the enum's schema.

A batch takes the ops whose kind is an edit, apply aside; `Kind::of` names
an op's kind and a test holds it to `Command::kind` over every op (open
question 2). `aaw_host::command::batch_commands` reads a batch file's list
before serde does: an op it does not know is refused with the nearest op
name (`pattern.event.add`: did you mean `event.add`?), as an unknown field is
refused with the nearest field; a transport, history or reading op is
refused as one a batch does not take; and a key an op without fields does
not have is refused with the nearest key, where serde ignored it. A key of
an op with fields goes to the object it adds, which the song's model checks
as before. `daw batch --help` and `describe start` point to `daw describe
batch`, and `describe start`'s batch file stays as the example.

### A part of a topic

`daw describe TOPIC.PART` prints one part of a topic: the topic's `semantics`
entry named PART and the `fields` lines whose path begins with it.
`daw describe effects.compressor` gives the compressor's paragraph and its
ten fields; `daw describe synth.filter` the filter's. `daw describe` with no
topic lists each topic's parts beside its line and its size in bytes, so an
agent knows before reading whether to read a part. An unknown part is refused
with the nearest one and the list. `--schema` takes a part too. A topic
without a part is unchanged.

`sampler` prints `project`'s semantics before its own today. With parts, it
prints its own and names `project` for the rest, as other topics name the
topic that lists a model.

### Recipes

`daw describe recipes` lists recipes, a line each; `daw describe
recipes.NAME` prints one in the form `describe start` has: steps of a
command and what it does, and a batch file beside them when one is used.
`start` becomes the first recipe, and `daw describe start` still prints it.
Each recipe runs in a test, as written, on a song the test makes from
generated audio, so a command that changes breaks the test rather than the
recipe. The first ones:

| Recipe | Steps |
|---|---|
| `start` | A first song, from `init` to `listen` (as today) |
| `drum-bus` | Drum tracks grouped, a compressor and a level on the group, a send to a reverb return |
| `phrase` | A note clip, a variation of it, the two duplicated in a row and one looped |
| `mix-pass` | `check`, a render, `listen`, a stem's overlap and resonances read, an EQ bell set from them, a render again and `compare` |
| `markers` | Markers read with what plays at each, an edit that answers one and the marker removed in the same batch |
| `finished-edit` | A song imported, its beats found, an audio clip cut at bars, the joins crossfaded and checked with `joins`, and an export |

The Synth's `recipes` are sounds, a patch built step by step, and stay in
`describe synth`. A recipe here is mechanics: which commands, in what order, and what each reply
says. It holds no taste. A skill of the person's names a recipe for a step
rather than repeating its commands, and `docs/music.md`'s table names the
recipe beside the topic.

### Names that do not collide

Two names say two things:

- **`daw loop`** is the transport's loop, and **`daw clip loop`** loops a
  clip's start. Different arguments keep them apart in practice; the
  transport's help names `clip loop` and the clip's names `loop`.
- **`daw device`** is the Mac's audio devices, while a device everywhere else
  is an instrument or an effect, as in Ableton, and a device of the person's
  own is `daw plugin`; `daw plugin`'s own help has to say so. It is renamed
  (open question 1). The old name is refused with the new one in the error,
  rather than kept as a hidden alias, so that an agent reading the help sees
  one name.

### No empty help lines

The twelve commands whose line in their parent's help is empty get one:
`track remove`, `track move`, `return move`, `group move`, `clip repeats`,
`clip remove`, `pattern duplicate`, `pattern event remove`, `pad remove`,
`effect bypass`, `lane remove`, `section add`. A test walks the help tree and
fails on a command with no line. The Python commands keep argparse's help, as
[learning the CLI quickly](cli-discovery.md#limits) records.

## Done when

1. `daw --help` lists every command under one of the headings, a test holds
   it, and the two lines above the list say what the sections mean. Done.
2. `daw describe batch` lists every op the host takes with its keys, made from
   the enum; an unknown op is refused with the nearest. Done.
3. `daw describe effects.compressor` prints the compressor alone; `daw
   describe` lists each topic's parts and size; `sampler` no longer repeats
   `project`.
4. `daw describe recipes` lists at least the six recipes above, each run in a
   test; `docs/music.md` names them.
5. The audio-devices command has its new name, the old one is refused with
   it, and every doc, test and `describe` text uses the new one.
6. No command in the Rust help tree has an empty line, and a test holds it.

Each item is done when an agent in a fresh session, told only the task, uses
it without reading source: a field found through a part, a batch written from
`describe batch` without a refused op.

## Open questions

1. **The audio-devices command's new name.** `daw audio-device`, as Settings
   › Audio and Ableton's Audio Device name it (recommended); `daw io`;
   or `daw interface`. `daw audio` is taken by audio clips.
2. **Whether a batch takes every op.** Answered while building item 2 (D150):
   a batch takes the edits, apply aside, as `Edit::run` already refused the
   rest one by one; `describe batch` lists those.
3. **Whether `loop` needs more than the cross-reference.** Renaming the
   transport's loop (`daw cycle`, as Logic has it) was weighed and not
   proposed, since every DAW the person knows calls it a loop.
