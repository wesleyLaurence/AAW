# Learning the CLI quickly — implemented 2026-10-04

Status: implemented (D76). The concept asks that "every error says how to fix it"
([concept.md](../concept.md#what-the-agent-works-with)).

## What

`daw describe` is short enough to read whole, a command's help lists what it
accepts, `daw describe start` is a recipe for a first song, and errors say what
to do next.

## Why

Every session starts with an agent that knows nothing about `daw` and learns it
from `describe` and `--help`. What it reads costs context for the rest of the
session, and every wrong guess costs a round trip.

Measured on October 4, 2026, before and after:

| `daw describe` topic | Bytes before | Bytes now |
|---|---|---|
| `midi` | 89,339 | 7,142 |
| `project` (the default before) | 65,871 | 6,935 |
| `synth` | 50,481 | 16,242 |
| `effects` | 38,261 | 8,809 |
| `sampler` | 10,440 | 6,479 |
| `automation` | 5,794 | 4,305 |

Nearly all of what went was JSON Schema. In a trial song made the same day, an
agent had hit four errors that did not say what to do; each now does (below).

## Design

**`describe` is short by default.** A topic prints `semantics`, what its fields
mean, unchanged, and `fields`: each field's path in the song and a line saying
what it takes, its default and whether it is required, generated from the
schema (`contract::listing` in `aaw-model`).

```
"tracks[].clips[].notes[].velocity": "integer 1..127, default 100",
"tracks[].clips[].length_beats": "beats, required",
"tracks[].instrument.synth": "Synth (daw describe synth) or null, default null",
```

`beats` is a number or a fraction string. A topic lists its own models field by
field and names any other model with the topic that lists it. Paths start at
the song (`tracks[]`, `samples.ID`), except the Synth's, which are paths in the
patch (`filter.cutoff_hz`), as `daw synth set`, lanes and the matrix take them.
`synth` also prints `modulation` and `automatable`, and `automation` prints
`automatable`, which the schema cannot say. The lines are text inside the JSON
result, not JSON of their own, for fewer tokens.

`--schema` prints a topic as it was printed before: the JSON Schema of its
models with everything else. `daw describe` with no topic lists the topics with
a line each; `daw describe --schema` prints the whole song's schema, which was
the default.

**A first-song recipe.** `daw describe start` lists fifteen commands, each with
a line on what it does, that make an eight-bar song from `init` to `listen`: a
drum kit from samples found with `daw samples search` and imported, a pattern
of three rows played eight times, a bass line from the Sub Bass patch, chords
from Soft Pad written by one `daw batch` file, a limiter on the master, a render
and its measurements. The batch file is printed beside the steps. The sample
paths are placeholders (`KICK_PATH`) for what a search finds; everything else
runs as written in an empty folder, and a test does exactly that.

**Help lists what is accepted.** Commands that take `--FIELD VALUE` pairs list
the fields in `--help`, each with its range and default, from the schema:
`track add`, `return add`, `clip add` (a pattern clip's and a note clip's),
`note add|set`, `pattern add`, `pattern event add|set`, `pad add|set`,
`send set`, `audio add` and `lane point move`. `effect add --help` lists every
effect type with its fields and an equalizer's band; `lane set --help` and
`lane point add --help` list what a lane can move on a track, return, master and
synth, and each effect's movable fields; `set --help` lists the commonest paths.

**Errors that say what to do.**

- An edit naming a field the song does not have is refused with the nearest
  field first and the model's message after it:

  ```
  `bpm` is not a field; did you mean `session.tempo`?
  1 validation error for Project
  bpm
    Extra inputs are not permitted [type=extra_forbidden]
  ```

  The nearest is a synonym another tool uses (`bpm`, `volume`, `start`), the
  name with its unit left off (`cutoff` for `cutoff_hz`), or a spelling a letter
  or two away; at the top of the song a session field is tried too. Places are
  named as commands address them, a track by its ID (`tracks.drums.gain_db`),
  and an effect union's tag is left out. With nothing near, the hint lists the
  fields that are there. `aaw_model::hint` makes it from the schema; the host
  adds it wherever an edit or a merge patch is validated, so the app's socket
  replies have it too. The model's message itself is pydantic's and unchanged.
- A word where a `--FIELD VALUE` pair was wanted names the flag when it is a
  value one takes: "`limiter` is an effect type; use --type limiter", "Use
  --type midi for a MIDI track"; otherwise it says fields are flags and points
  to `--help`.
- A render that would clip names the mix's peak, the three loudest stems with
  theirs, and the fix: "Unsafe PCM export: the mix peaks at +5.15 dBFS; loudest
  stems kick +5.3, bass +4.1; lower `tracks.kick.gain_db` or
  `session.master_gain_db`, or add a limiter: daw effect add PROJECT master
  --type limiter". With a limiter already on the master, it points at what
  follows the limiter instead. Stem peaks are after the master gain and before
  the master effects, as the stems are written.
- Every error keeps its JSON form on stderr and its nonzero exit status.

**A closed pipe ends quietly.** When the reader of stdout stops, as `head`
does, `daw` exits with status 141, the status a shell gives a closed pipe, and
writes nothing to stderr; the Python commands do the same.

**`inspect` lists clips in the order they play:** pattern clips, audio clips and
note clips by their start, each with the reference of its place in the song,
which is the order they were made.

## Limits

- The Python commands (`samples`, `listen`, `check`, `timeline`, `joins`,
  `export`) keep argparse's help; they are to move to Rust (Later, under
  Shipping), and their help can follow then.
- A hint is given for a field the song does not have. A value out of range or of
  the wrong kind keeps the model's message alone, which names the range.
- A hand-edited `song.yaml` that fails to load gets the model's message without
  a hint.
