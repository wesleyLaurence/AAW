# Learning the CLI quickly — proposed October 4, 2026

Status: proposed, not built. Backlog: Next, Learning the CLI quickly. The concept asks that "every error says how to fix it"
([concept.md](../concept.md#what-the-agent-works-with)).

## What

`daw describe` short enough to read whole, help that lists what a command
accepts, a recipe for a first song, and errors that say what to do next.

## Why

Every session starts with an agent that knows nothing about `daw` and learns
it from `describe` and `--help`. What it reads costs context for the rest of the
session, and every wrong guess costs a round trip.

Measured on October 4, 2026:

| `daw describe` topic | Bytes |
|---|---|
| `midi` | 89,339 |
| `project` (the default) | 65,871 |
| `synth` | 50,481 |
| `effects` | 38,261 |
| `sampler` | 10,440 |
| `automation` | 5,794 |

`midi` is about 25,000 tokens, nearly all of it JSON Schema; the guidance an
agent needs is its `semantics`, a few kilobytes. In a trial song made the same
day, an agent hit these:

- `daw set PROJECT bpm 90` answered `1 validation error for Project / bpm /
  Extra inputs are not permitted`. The field is `session.tempo`; nothing said so.
- `daw effect add PROJECT master limiter` answered `Expected --FIELD VALUE, got
  limiter`. Its help says `--type`, and lists neither the types nor their fields.
- A render that clipped answered `Unsafe PCM export: peak 2.76 dBFS; lower
  master_gain_db or add a master limiter`. It did not say which track was loud,
  though the engine has each stem's peak at that moment (`offline.rs`), and the
  field is `session.master_gain_db`.
- `daw describe midi | head` panicked with `failed printing to stdout: Broken
  pipe`.

## Design

**`describe` is short by default.** A topic prints its `semantics` and a field
list: each field's path, type, default and range on one line, generated from the
schema. `--schema` prints the JSON Schema as now. `daw describe` with no topic
lists the topics with a line each, rather than the whole song's schema.
AGENTS.md says the default prints the full schema; it changes with this.

**A first-song recipe.** `daw describe start`: the dozen commands from `init` to
`listen` that make an eight-bar song with a drum kit from samples, a bass and
chords from factory patches, a limiter on the master and a render, each with
one line on what it does, and one `daw batch` file of several commands as one
step. It is the shortest path to a working song, and the place to learn the
verbs by example. It names no personal sample, so it uses `daw samples search`
to find them. This was an Ideas line of the backlog: focused help, a concrete
batch example, short `describe` output and first-task recipes.

**Help lists what is accepted.** `effect add --help` lists the effect types and
each type's fields, `lane set --help` the parameters a lane can take, `set
--help` the commonest paths (`session.tempo`, `session.length_beats`,
`session.master_gain_db`, `tracks.T.gain_db`). Generated from the schema, so they
cannot drift from it.

**Errors that say what to do.**

- An unknown field names the nearest valid path: "no field `bpm`; did you mean
  `session.tempo`?" The validation message from the model stays after it.
- A positional argument where a flag was wanted names the flag: "`limiter` is an
  effect type; use `--type limiter`".
- A clipped render names the loudest stems with their peaks and the fix as a
  command: "peak +2.76 dBFS; drums +1.6, keys −5.4; lower `tracks.drums.gain_db`
  or `session.master_gain_db`, or `daw effect add PROJECT master --type
  limiter`."
- Every error keeps its JSON form on stderr and its nonzero exit status.

**A closed pipe ends quietly,** with the usual exit status for it, so
`daw … | head` is safe.

**`inspect` lists clips in the order they play,** with the order they were made
kept in the song. It lists them in the order they were made today, which hides a
clip placed earlier than another.

## Done when

- `daw describe midi` is under 8 KB and still says everything its semantics say
  today; `--schema` prints what it prints today.
- `daw describe start`, run as written in a fresh folder, makes a song that
  renders without an error.
- Each error above, reproduced in a test, gives the new message.
  `daw describe midi | head -1` exits without a panic.
- [music.md](../music.md) and AGENTS.md point to `daw describe start` and the
  short topics.

## Open questions

- Whether the field list should be YAML-like text inside the JSON result rather
  than JSON, for fewer tokens.
- Whether the Python commands (`samples`, `listen`, `check`) should have the same
  help treatment now, or after they move to Rust (Later, under Shipping).
