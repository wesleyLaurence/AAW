# The agent-native DAW: ideas for later

Status: ideas collected September 30, 2026, from a design conversation. Nothing here is scheduled or decided. [Rust-Swift-Update.md](Rust-Swift-Update.md) remains the build plan and [mvp.md](mvp.md) describes what exists. When an idea is picked up, it gets a plan and its choices are recorded in [decisions.md](decisions.md). The earlier review, [design-notes-2026-09-07.md](design-notes-2026-09-07.md), covers some of the same ground; the sections below link to it where it does.

## The picture

A native macOS DAW built from scratch in Rust and Swift. It feels like a normal DAW: an arrangement of tracks and clips, a mixer, device chains, and space to play. On the right sits a collapsible agent panel, as in the Cursor code editor. The person makes music directly, or opens the panel and asks the agent to do something. As the agent works, its changes appear in the arrangement: clips move, faders animate, tracks appear. The person can reach in at any moment, and the agent sees what they changed.

Both operate on one data model, with one undo history and one sound. The session host and command model from the rebuild (D36, D38) already provide that shared model. This document collects what goes on top of it: the app experience, the collaboration primitives, the workspace and project structure, memory and intent, skills, custom tools and devices, and the question underneath all of them: what kind of DAW would an agent most want to use?

## 1. What interface does an agent want?

### What an agent is like

Design from the operator's traits, as Ableton was designed from a person's hands, eyes and ears.

| Trait of an agent | What the DAW should do |
|---|---|
| Works in text and code, and already knows existing vocabularies | Keep state as readable text, use Ableton's terms and real units (D2, D5, D9) |
| Has a limited context window | Offer compact views and queries so it never has to load the whole song to answer a question |
| Acts through tool calls, each with a round trip | Make each call expressive; let one program issue many edits |
| Cannot reliably hear | Translate sound into numbers and images, and make the person's reactions easy to capture (D10) |
| Makes mistakes | Validate before applying, make everything undoable, keep renders deterministic, keep checks fast |
| Thinks in structure: sections, motifs, variations | Keep that structure in the document instead of flattening it to events |
| Works for long stretches and resumes cold | Keep memory in files it reads first: the song's brief, the person's taste |
| Can work in parallel | Commands that commute, and branches for variations |

### Is the answer a CLI?

A CLI is the right floor, and it stays: it works with any agent harness, composes in a shell, and describes itself through `--help` and `daw describe`. It is not the ceiling.

In practice, an agent given the CLI and a text song file composes by writing a program that generates the whole arrangement and applies it as one patch. That suggests code is the agent's native medium for composition, with single verbs for targeted edits. Each interface has its place:

| Interface | Good for | Weak at |
|---|---|---|
| The document (`song.yaml`) | Reading the whole state, diffs, review | Large songs overflow context; repetition is stored as copies |
| Verbs (`daw set`, `daw clip move`) | Small targeted edits, live collaboration, undo, discovery | Generative work that needs hundreds of calls |
| Programs (a client library over the host) | Generation, variation, transformations across a song | Drift from the song if they write files directly; opaque to the person |
| Queries | Answering a question without loading everything | Not built yet |
| Perception (`listen`, `compare`, checks) | Verifying what an edit did | Numbers are not ears |
| MCP tools | Typed schemas | Schemas cost context and do not compose in a shell; worth an adapter only |

The principle that ties them together: **every write, from any interface, becomes commands in the host.** Each write is visible in the app, undoable, and attributed to the request that caused it. A program is just a fast way to issue many commands. A sketch of what a client library could look like:

```python
from aaw import connect

song = connect("projects/new-song/song.yaml")   # the running host, or headless
with song.turn("double-time hats in the second hook"):
    hats = song.pattern("hats-a").duplicate("hats-double")
    hats.steps["closed"] = "x.x.x.x.x.x.x.x." * 2
    song.clips("hats", section="hook-2").set(pattern="hats-double")
```

Every call sends host commands, and the `with` block is one turn and one undo step. The person watches the edits land and can revert the whole request at once. The program is kept, if at all, as a record of how the edit was made, never as a second source of the song.

This extends the earlier review's hybrid mutation interface ([design-notes-2026-09-07.md §11.2](design-notes-2026-09-07.md#112-semantic-operations-add-a-hybrid-mutation-interface)): typed operations for concurrent, repetitive and reference-changing edits, with text authoring kept where it reads best. The host's command catalog (D38) is that typed layer; a client library makes it programmable.

### A representation that keeps musical structure

Ableton stores a chorus that repeats with a small change as two independent clips. For an agent, a representation that says what something *is* can be better: "clip B is clip A transposed up a fifth, with the last bar replaced by a fill." Then "change the hook everywhere" is one edit, and the relationship is visible in the document. Programmers get this from functions instead of copy and paste.

The cost falls on the person: the app has to show such links and let them be broken ("make this one independent"). How much structure the document should carry, versus flat events plus agent programs, is an open question.

Other representation ideas:

- **A notation for each kind of material:** step strings for drums (exists), note events for melody, chord symbols as a layer the agent can read and write, named motifs.
- **Every object addressable:** a path or a handle for every object (exists while a host runs), stable across edits.
- **Errors that teach:** each error says what is wrong and how to fix it, as validation errors do today.

### Views the agent reads

The agent needs to perceive the song without reading all of it and without hearing it.

- **Arrangement map:** tracks by bars, with a glyph for density, in a few hundred tokens:

  ```
  bar         1       9       17      25      33
  drums       ........████████████████....████████
  bass        ........▆▆▆▆▆▆▆▆████████....████████
  keys        ▂▂▂▂▂▂▂▂▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▆▆▆▆▆▆▆▆▆▆▆▆
  vox         ................▃.▃.▃.▃.........▃.▃.
  section     intro   verse   hook    break   hook
  ```

- **Text piano roll** for one clip or a few bars.
- **Queries** (syntax to be designed): which bars the bass plays in, every snare hit that is off the backbeat, what sounds between beats 128 and 160.
- **Musical checks** in `daw check`, as a linter for music: a snare off the backbeat in a section declared half-time, two tracks sitting in the same register, a note cut off by the end of its clip, a root note that disagrees with the audio (exists).
- **Images:** spectrograms and energy plots on a bar grid (exist).

The Rust engine renders about 500 times faster than real time, so checks that render a section can run after every edit.

## 2. The app: a DAW with an agent panel

### Layout

```
┌──────────────────────────────────────────────────┬─────────────────┐
│ transport · position · tempo · agent activity    │  Agent panel    │
├──────────────────────────────────────────────────┤                 │
│ ruler and sections                               │  chat           │
├──────────────┬───────────────────────────────────┤  turns          │
│ track headers│ clip lanes                        │  tool activity  │
│              │                                   │  keep · revert  │
│              │                                   │  A/B            │
├──────────────┴───────────────────────────────────┤                 │
│ detail: device chain, pads, pattern editor       │  (collapsible)  │
└──────────────────────────────────────────────────┴─────────────────┘
```

The main area is the Arrangement View from the rebuild plan. The panel opens and closes with a shortcut and never blocks the person from working.

### Borrowing from Cursor

| Cursor | The DAW |
|---|---|
| Chat panel beside the editor | Agent panel beside the arrangement |
| Agent edits appear in the editor | Changes animate in the arrangement, highlighted briefly and listed in the activity panel |
| Accept or reject each change | Keep or revert each turn; A/B it while the loop plays |
| @-mentions of files and symbols | @track, @clip, @section, @skill, @reference; the current selection is included automatically |
| Inline edit on a selection | Select clips or a region, then type a request scoped to them |
| Checkpoints: restore to before a message | Restore the song to before any turn |
| Tab completion | Ghost clips: the agent proposes a fill or the next four bars, drawn translucent; Tab accepts (speculative) |
| Rules files | `profile/instructions.md` and the song's `SONG.md` |
| Background agents | The agent builds variations on a branch while the person keeps playing |

### Embedding Claude Code

In stages, each usable on its own:

1. **Terminal beside the app.** Works today with the host: the agent's edits are heard live.
2. **A terminal pane inside the app** running `claude` in the workspace.
3. **A native panel** over the Claude Agent SDK or `claude -p` with streaming JSON. The app starts the agent in the workspace, shows its messages and tool calls, puts permission prompts in the panel, and knows where each turn begins and ends.

With each message the app adds context the agent would otherwise have to ask for: the open song, the selection, the playhead and loop region, and what the person changed since the agent's last turn (`daw changes --since`). "Make *this* darker" then works, and the agent treats the person's edits as direction instead of overwriting them.

The agent's tools stay the `daw` CLI and the host socket, so the panel is agent-agnostic: Claude Code first, a small adapter for others such as Codex. How people authenticate, and what Anthropic's terms allow for an app that embeds Claude Code, must be checked before stage 3.

### Collaboration primitives

- **Turns.** An agent request ("make the hats busier") is often a dozen commands. The host groups them into a turn labeled with the request, which can be kept, reverted or compared as one. When the app starts the agent, it marks turn boundaries itself; in terminal mode the agent opens and closes turns.
- **A/B while looping.** The engine compiles the song into an immutable program and already crossfades between programs at the same beat over 5 ms. Keeping the program from before a turn lets one key flip between before and after without stopping the loop. Producers decide by comparing, and a conventional DAW has no unit for "what the assistant just did."
- **Variations as branches.** "Give me three hat patterns" makes three branches; the person flips through them on the loop and keeps one.
- **Locks.** "Don't touch the drums" as a rule the host enforces: agent commands on locked tracks or parameters are refused.
- **Listening markers.** During playback, a key drops a note at the playhead: "too busy", "love this", "snare feels late". The agent receives feedback tied to beats instead of prose it has to locate. This is the person's ears as the agent's perception.
- **Visibility.** Animated changes, highlights, the activity panel and undo labeled by origin, as the rebuild plan describes.
- **Autonomy modes.**

  | Mode | What the agent does |
  |---|---|
  | Ask | Reads and answers; makes no edits |
  | Edit | Edits land live as turns the person can revert |
  | Propose | Edits go to a branch the person auditions and merges |
  | Background | Works through a task list against the brief while the person is away |

## 3. Workspace and projects

### The workspace

When someone installs the DAW, they get a workspace: a folder of their music that the agent opens at its root, as Claude Code opens a code repository. The DAW is the installed tool; the workspace is the person's, and lives outside the tooling repository.

```
~/Music/AAW/                    anywhere the person chooses, including a synced folder
  CLAUDE.md / AGENTS.md         managed by the DAW: how to use daw, the conventions
  profile/
    instructions.md             the person's rules; the agent never edits them
    taste.md                    what the agent has learned about the person, with evidence
  skills/                       the person's workflows in plain English
  devices/                      custom effects and instruments usable in any song
  library/                      sounds the person keeps: bounces, recordings, drag-ins, saved chains
  projects/
    a-song/
  .daw/                         sample index and caches
```

Opened at the root, the agent can work across songs: reuse a sound from another song, apply a saved master chain, compare two songs' mixes.

This splits today's instructions cleanly. The tooling repository's AGENTS.md covers developing the DAW. The workspace's managed instructions cover making music, and the DAW updates them when it upgrades.

### A song

```
a-song/
  SONG.md        vision, feedback, decisions, status
  song.yaml      the arrangement, the only source of musical truth
  audio/         every sound the song uses: imports, drag-ins, generated sounds
  references/    tracks to match or learn from; never played in the song
  scripts/       one-off tools written for this song
  renders/       working renders the DAW manages; safe to delete
  exports/       deliverables the person keeps, such as A-Song_v3_master.wav
  versions/      named snapshots, such as 003-halftime-snare/
  .daw/          host state, analysis output, scratch
```

### Strict structure, free contents

Songs should look the same from one to the next without limiting what the agent can do. The rule is fixed places, free contents: nobody invents new top-level folders, but anything can go inside `scripts/`. The tooling enforces it:

- **`daw new` creates the layout,** so every song starts identical.
- **`daw check` flags files outside the layout.** Command output goes to `.daw/`, never into the song folder. With the MVP, song folders fill up with saved JSON outputs and numbered revision files.
- **Person-owned versus DAW-managed.** `SONG.md`, `song.yaml`, `audio/`, `references/` and `exports/` belong to the person. `renders/` and `.daw/` are caches that can be deleted.
- **Versions through a command.** `daw version save "halftime snare"` takes a named snapshot. That replaces the per-song revision scripts and folders agents invent today. The snapshots are also what A/B and branches compare.
- **Drag and drop.** Audio dropped on the app is copied into `audio/` with its hash and registered in the song, as `daw samples import` does for a file path today. The agent finds it in `song.yaml` like any other sample.

## 4. Intent: SONG.md, taste.md and instructions.md

These implement the three tiers of intent in D15 and the DAW-owned memory in D16, which were designed but never built.

| File | Tier | Written by | Scope |
|---|---|---|---|
| `profile/instructions.md` | Rules | The person only | Every song |
| `profile/taste.md` | Preferences | The agent, with evidence; the person prunes | Every song, optionally by genre |
| `SONG.md` | Brief | Both, by section | One song |

Precedence stays as D15 sets it: rules, then the brief, then preferences, then defaults. The agent can say which one drove a decision.

### SONG.md

The song's README: the first thing the agent reads before working on a song, and the file it keeps current as the song develops. Every section has an owner, so the agent can update it without rewriting the person's words:

| Section | Owner | Contents |
|---|---|---|
| Vision | The person; the agent suggests edits | The feel, style, references, what the song is for |
| Feedback | The agent, quoting the person, dated | What the person liked and disliked |
| Decisions | Both | Settled musical choices and why |
| Now | The agent keeps it current | Where the song stands and what is next |
| Log | The agent | One line per session or version |

### taste.md

Entries carry evidence and dates, and fade unless reconfirmed:

```markdown
- Keeps lead vocals dry and upfront. Evidence: asked to remove vocal reverb in two
  songs (2026-10-02, 2026-10-15).
- Prefers a repeating motif to constant variation in melodic parts. Evidence: asked
  to simplify busy synth parts (2026-10-08).
```

At the end of a session the agent says what it learned rather than adapting silently. The files belong to the DAW, not to any agent harness's own memory, so they work under any harness and the app can show them.

The earlier review's memory design ([design-notes-2026-09-07.md §13](design-notes-2026-09-07.md#13-memory-and-collaboration-design)) refines this in two ways worth keeping:

- **A hand edit is candidate evidence, not a preference.** It may be an experiment or a correction that belongs to one song. The agent asks whether it applies beyond this song before writing it to `taste.md`.
- **Structured evidence, readable summary.** Keep/reject and feedback events are stored as structured records under `.daw/`, and `taste.md` is the bounded summary generated from them, loaded under a token budget rather than growing without limit.

## 5. Skills: programming music in plain English

A skill is a `SKILL.md` file: a name, when to use it, and the steps, in the Agent Skills format that Claude Code loads natively. It lets a person write down their process once and run it by name:

```markdown
---
name: my-mix-pass
description: My mixing process. Use when asked to mix a song or to "do my mix pass".
---
1. Gain-stage every track to peak around -12 dBFS before effects.
2. High-pass everything except kick and bass.
3. Set the kick-to-bass sidechain before any bus compression.
4. Render the drop, compare it against the song's reference and report the differences.
```

- **Captured from a session.** The easiest way to write one is to walk the agent through a process once, then say "save that as a skill". The agent drafts the file from the conversation and the person edits it.
- **Chained.** "Run my mix pass, then my master chain" composes skills the way a script composes functions.
- **Bundled scripts.** A skill can carry scripts, so a custom tool can live inside the skill that uses it.
- **Built-in and personal.** The DAW ships craft skills, such as mixing to a reference, finishing a song or preparing stems, and updates them with each release. The person's skills live in the workspace and take precedence.
- **Visible in the app.** A skills panel lists them, and `@skill` in the chat invokes one.

## 6. Custom tools and devices

An agent can write almost any program. The DAW should let what it writes become part of the song, with a record of how it was made (D14, outputs are inputs). There are three tiers, cheapest first:

| Tier | What it is | Heard live | When |
|---|---|---|---|
| One-off scripts | The agent writes a script in `scripts/` when no tool exists, runs it, and imports the result into `audio/` with the recipe that made it: the script, its inputs and parameters | As a sample | Conventions only; possible now |
| Offline devices | A script used as an effect on a track. It re-runs in the background when its input changes, and playback uses the cached result, like freezing a track | From cache | Later |
| Real-time devices | DSP the agent writes in a sandboxed language that the engine compiles and runs on the audio thread, in playback and export alike (D22) | Yes | After M6, following a spike |

- **A known environment.** Scripts need a Python environment the DAW provides, with numpy, scipy and soundfile, so they run the same way every time.
- **Scripts go through the host.** A script that changes the arrangement issues commands through the host, never by writing `song.yaml`. Its edits then land live and can be undone, and the song never drifts from the script that made it.
- **Promotion.** A tool worth reusing moves from a song's `scripts/` into the workspace's `skills/` or `devices/`.
- **Candidate language for real-time devices: FAUST.** It is a DSP language designed for this, compiles to Rust or WebAssembly, and declares each parameter with a unit and range. The app could generate device panels from those declarations, as the rebuild plan does for built-in effects. WebAssembly isolates a faulty device from the engine; native code is faster. The spike decides. The earlier review already named FAUST as a serious DSP option and flagged a licensing audit of its generated code and libraries ([design-notes-2026-09-07.md §11.3](design-notes-2026-09-07.md#113-dsp-language-decide-by-product-commitment-and-benchmark)). Its worker-isolation rules (§11.9) apply to scripts and offline devices: time, memory and output limits, captured errors, and outputs published only after validation.
- **Presets.** Effect chains, pad setups and whole tracks save as named text presets in `library/` and load into any song.

## 7. Engine gaps seen in practice

Songs made with the MVP show where the engine falls short:

- **Sounds made outside the engine.** Agents repeatedly wrote scripts to synthesize sounds: sine sub-basses, pitch glides baked into a sample, saturated bass, synthesized pads and brass. The DAW could not see those sounds or adjust them afterwards.
- **No saturation or clipper.** Matching a loud reference mix stopped at the limiter.
- **No pitch glide, no time-stretching, no synthesizer.**

The rebuild defers synthesizers and time-stretching. This records the demand: saturation and a clipper, a simple synth with text patches, and glide are the devices agents reached for most.

## 8. Later and bigger

- **MIDI keyboard capture.** The person plays a riff and the agent builds around it: harmony, drums, arrangement. Much cheaper than audio recording.
- **Agent roles.** A drummer and a mix engineer working at once on different tracks. The host already serializes commands and records their origin, so a role can be an origin.
- **Audio-capable models as a critic,** when available: an optional listener beside the numbers, never the foundation (D10).
- **A reference library.** Tracks the person points to as "this is what good sounds like," analyzed once and compared against every mix.
- **Sharing.** Skills, devices and presets packaged for other people to install.

## Relation to the rebuild plan

None of this changes milestones M4 to M9. Where the ideas would fit:

- **Turns, locks and A/B:** host features, small enough to land between M3 and M5. The app's activity panel, undo labels and comparison build on them.
- **Workspace, song layout, versions, SONG.md, taste.md and instructions.md:** independent of the app, buildable at any time. Doing them before M4 lets the app be designed around the workspace.
- **Agent panel:** a new milestone after M5, embedding Claude Code in the stages above.
- **Custom real-time devices:** after M6, when the engine's device model is settled.

## Open questions

1. How much musical structure should the document carry, such as derived clips, versus flat events plus agent programs?
2. Which language should the client library use: Python, where the analysis already lives, or TypeScript, the Agent SDK's other language?
3. How do people authenticate an embedded Claude Code, and what do the terms allow?
4. Should versions and branches be snapshots in `versions/` or commits in a git repository per workspace?
5. Which format should real-time custom devices use: FAUST to WebAssembly, FAUST to Rust, or something else?
6. Where does the workspace live by default, and how does the app choose one?
7. In terminal mode, with no app to mark them, how are turn boundaries set?
