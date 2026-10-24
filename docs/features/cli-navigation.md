# Finding one's way in the CLI — implemented October 23–24, 2026

Status: item 1, help in sections, built October 23, 2026 (D149, PR #143);
item 2, describe batch, built the same day (D150, PR #144); item 3, a part
of a topic, built the same day (D151, PR #145); item 4, recipes, built the
same day (D152, PR #146); item 5, names that do not collide, built October
24, 2026 (D154, PR #148); item 6, no empty help lines, built the same day
(D155). Designed as D148. It follows
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

Built October 23, 2026 (D151). `daw describe TOPIC.PART` prints one part of
a topic: the `semantics` entry named by the part's first word, the `fields`
lines whose path is the part or within it, and a list of the topic's own by
that name (the Synth's `modulation`, `check`'s `codes`, `samples`' `lines`).
`daw describe effects.compressor` gives the compressor's paragraph and its
ten fields, 864 bytes of the topic's 27,416; `synth.filter` the filter's. A
part may go on past its first word, `effects.compressor.ratio` one field's
line, and a path's `[]` may be left out, `project.tracks.gain_db` for
`tracks[].gain_db`. For `batch` a part is the ops within it, `batch.clip`
every `clip.*` op. A topic's parts are its semantics entries, then the first
word of each field (or op), then its own lists, in that order; `start` has
none and is read whole.

`daw describe` with no topic gives each topic its line as `about`, its parts
as `parts` and as `bytes` how much `daw describe TOPIC` prints, so an agent
knows before reading whether to read a part. A part that names nothing is
refused with the nearest and the parts (`effects.compresor`: did you mean
`effects.compressor`?), through `hint::nearest`. With `--schema`, a part has
the schema of the topic's root models within it, by their path in the song
(`effects.compressor` the Compressor's, `sampler.tracks` the Pad's at
`tracks[].pads.ID`), or of the node it names inside one (`synth.filter` the
SynthFilter's), with the models it refers to; a part of `batch` the ops'
alternatives of the enum's schema; and the lists only the schema's form has
are parts there too (`synth.wavetables`). A topic without a part is
unchanged. The roots and the models a topic lists are one table, `layout`,
which the field lines and a part's schema both read.

`sampler` printed `project`'s semantics before its own, 14,396 bytes. It now
prints its own and a `project` entry naming the parts of `project` that say
what samples, pads and patterns mean (`project.steps`, `project.gate` and the
rest), 2,884 bytes; a test holds each name to a part.

### Recipes

Built October 23, 2026 (D152). `daw describe recipes` lists the recipes, a
line each, as `recipes`; `daw describe recipes.NAME` prints one: `given`,
the song it starts from, `steps`, each a command to run and what it does
and what its reply says, `batch`, a file to write before the step that runs
it, when it has one, and `semantics`. `start` is the first recipe, and `daw
describe start` prints the same. A word of two or more capitals in a
command, `KICK_PATH` or `PEAK_HZ`, comes from an earlier reply or from the
given, as its step says; a note's name, `A#1`, is not one. The recipes are
a table in `aaw_model::recipes`, and `daw describe` lists them as the parts
of `recipes`, each read whole: `recipes.phrase.steps` is refused, and a name
that is none is refused with the nearest (`recipes.drum_bus`: did you mean
`recipes.drum-bus`?).

`tests/test_recipes.py` runs every recipe as written, in a folder of its
own, on a song it makes from generated audio as the recipe's `given` says,
filling the capitals from the replies, and fails on a recipe it has no song
for. The recipes:

| Recipe | Given | Steps |
|---|---|---|
| `start` | An empty folder and a library of a kick, a snare and a hat | A first song, from `init` to `listen` |
| `drum-bus` | Tracks of pads `kick`, `snare` and `hat` | `group add`, a compressor and a level on the group, a return with a reverb, the group's send to it, a render and `listen`'s `effects` and `hits` |
| `phrase` | An empty song | A note clip, `clip duplicate` and two notes changed for its variation, the two copied on with `range copy`, a one-bar part with `clip loop` and `clip resize`, and `map` |
| `mix-pass` | The song `start` makes | `check`, a render, `listen` and one pair's `--overlap`, an eq bell from a stem's resonance, a render again and `compare` with the first |
| `markers` | The song `start` makes, with markers `bass too loud here` and `love this` | `marker list`, `map` around the first, a batch that answers it with a lane and removes it, and the second kept |
| `finished-edit` | A finished song's WAV and an empty song | `samples import`, `samples beats` on the copy, the session's tempo, rate and level, one audio clip from the downbeat, `audio cut` of four bars, `audio crossfade`, a fade out, a limiter, a render, `joins` and `export` |

The Synth's `recipes` are sounds, a patch built step by step, and stay in
`describe synth`. A recipe here is mechanics: which commands, in what
order, and what each reply says; its values are examples, and what to set
is the song's call. A skill of the person's names a recipe for a step
rather than repeating its commands, and `docs/music.md`'s table names the
recipe beside the task.

### Names that do not collide

Built October 24, 2026 (D154). Two names said two things:

- **`daw loop`** is the transport's loop, and **`daw clip loop`** loops a
  clip's start. Different arguments keep them apart in practice; `daw
  loop`'s line says it is the transport's and names `daw clip loop` for a
  clip's repeats, and `daw clip loop`'s ends naming `daw loop`.
- **`daw audio-device list|use`** is the Mac's audio devices, which was
  `daw device`, while a device everywhere else is an instrument or an
  effect, as in Ableton, and a device of the person's own is `daw plugin`,
  whose line says so. `daw device`, with anything after it and with
  `--origin` or `--expect` before it, is refused with exit status 1 and
  `{"error": ..., "command": "device"}` on stderr, the error naming `daw
  audio-device list`, `daw audio-device use PROJECT` and `daw plugin`,
  rather than kept as a hidden alias, so that an agent reading the help
  sees one name. The refusal is made where clap's parse fails on an
  unknown subcommand, from the word clap names, so a misspelling such as
  `devic` gets clap's own error. The errors that name an output or input
  the Mac does not have say `daw audio-device list names them`. A Rust
  test and a Python test hold the refusal.

### No empty help lines

Built October 24, 2026 (D155). Every command in the Rust help tree has a
line in its parent's help. Twelve had none: `track remove`, `track move`,
`return move`, `group move`, `clip repeats`, `clip remove`, `pattern
duplicate`, `pattern event remove`, `pad remove`, `effect bypass`, `lane
remove` and `section add`. Two had the line of the fields they take,
which clap gives a command with no line of its own that flattens
`FieldArgs`: `return add`, found building the recipes (D152), and `pattern
add`, found by the test. Each line says what the command does to the song
and what goes with it, as the host does it: a track is removed with its
clips, effects, sends and lanes, a note clip with its notes, a lane with
its points, and a `move`'s index counts from 0.

`every_command_has_a_help_line` in `aaw-cli` walks the tree from `Cli` and
fails on a command whose line is empty or is `FieldArgs`'s, naming each one.
The arguments' own lines are not held, and most are empty: an argument's
name beside the command's line says what it takes. The Python commands keep
argparse's help, as [learning the CLI quickly](cli-discovery.md#limits)
records.

## Done when

1. `daw --help` lists every command under one of the headings, a test holds
   it, and the two lines above the list say what the sections mean. Done.
2. `daw describe batch` lists every op the host takes with its keys, made from
   the enum; an unknown op is refused with the nearest. Done.
3. `daw describe effects.compressor` prints the compressor alone; `daw
   describe` lists each topic's parts and size; `sampler` no longer repeats
   `project`. Done.
4. `daw describe recipes` lists at least the six recipes above, each run in a
   test; `docs/music.md` names them. Done.
5. The audio-devices command has its new name, the old one is refused with
   it, and every doc, test and `describe` text uses the new one. Done.
6. No command in the Rust help tree has an empty line, and a test holds it.
   Done.

Each item is done when an agent in a fresh session, told only the task, uses
it without reading source: a field found through a part, a batch written from
`describe batch` without a refused op.

## Open questions

1. **The audio-devices command's new name.** Answered by the person on
   October 24, 2026 (D154): `daw audio-device`, as Settings › Audio and
   Ableton's Audio Device name it, over `daw io` and `daw interface`;
   `daw audio` is taken by audio clips.
2. **Whether a batch takes every op.** Answered while building item 2 (D150):
   a batch takes the edits, apply aside, as `Edit::run` already refused the
   rest one by one; `describe batch` lists those.
3. **Whether `loop` needs more than the cross-reference.** Renaming the
   transport's loop (`daw cycle`, as Logic has it) was weighed and not
   proposed, since every DAW the person knows calls it a loop.
