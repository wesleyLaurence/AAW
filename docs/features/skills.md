# Skills and reusable sounds — implemented October 1, 2026

A skill is a plain-English file that tells an agent how a kind of job is done. The
repository gives the agent the capabilities and says how each works
(`daw describe`); a skill says what to do with them, in whose words and to whose
taste. A skill is personal and stays out of Git, like the songs: this repository
ships none (D108).

## Task routing and discovery — implemented October 3, 2026

[AGENTS.md](../../AGENTS.md) routes a request to [music](../music.md) or
[development](../development.md). These guides are ordinary files read for the
applicable task; the root instructions do not import both. Command syntax and
semantics stay in CLI help and `daw describe`; a skill holds a repeatable
workflow, such as the sequence of decisions and checks for one person's kind of
edit of a finished song.

An agent finds a skill by its short name and description; its body and references
are read when needed. Without a skill for the job, the music guide's table names
the `daw describe` topics to read.

## Where a skill lives

- **In this checkout** a skill goes in `.claude/skills/NAME/SKILL.md`. Claude Code
  loads it by name there. Git ignores everything under `.claude/skills/` and
  `.agents/skills/`, so `git add -A` cannot commit a skill.
- **Outside the checkout**, your own skills folder (`~/.claude/skills/NAME/`)
  works from any directory once `daw` is on the PATH, which the Mac app's menu sets
  up. In this checkout keep to `uv run daw`.
- **For Codex** a skill may live under `.agents/skills/NAME/`, or
  `~/.agents/skills/NAME/`. A relative symlink from `.agents/skills/NAME` to
  `../../.claude/skills/NAME` gives both agents the same instructions.

## What goes in a skill

The whole workflow of the job: what a request means, the steps in order, who asks,
which sounds, the limits, the file names, what to check and what to tell the
person. How each step is done stays in the `daw describe` topics, which the skill
names where a step needs one, so a skill reads like this:

```markdown
---
name: my-edit
description: Make my kind of edit. Use when I paste a request with a song and
  timecodes from the person who sends them.
---

Requests come as "edit from :41-1:43, pull the 8-count out around 1:20". A time
means the downbeat nearest it. An 8-count is eight beats, two bars.

1. Import the song and find its beat (`daw describe beats`).
2. Tell me the cuts, as times and bar numbers. Then build the edit: one audio clip
   with the parts cut out (`daw describe edit`).
3. Start with content/my-edit/open.wav and end with it again right after the song.
   content/my-edit/swell.wav ends where the song ends.
4. Render and check every join (`daw describe joins`). The whole file is under
   60 seconds.
5. Export to projects/my-edit/SONG/exports/ as SONG_edit.m4a
   (`daw describe export`).

Tell me the cuts made, the final length and what was measured at each join.
```

A skill names `daw` commands and paths under `projects/` or `content/`. It never
holds an absolute path to a sample library, and it leaves the arithmetic to
`daw describe edit`.

## Sounds a skill reuses

The sounds an edit always uses, such as a one-shot at the start and the end, live
in a folder under `content/`, which Git ignores: `content/NAME/sound.wav`. Each
project imports them by path, which copies them in and leaves the originals alone:

```sh
uv run daw samples import content/my-edit/open.wav --project projects/my-edit/SONG/song.yaml --id open
```

`daw samples scan content/NAME` indexes the folder as a small library if searching
it is useful.

## The describe topics

| Topic | What it covers |
|---|---|
| `daw describe edit` | Building an edit of a song from audio clips: the `daw audio` commands, the lead before a beat, fades at a join, level, speed, and `daw timeline` for times, end alignment and length |
| `daw describe beats` | `daw samples beats`: tempo, downbeats, the beats near a time, the click audition |
| `daw describe joins` | `daw joins`: what each measurement of a join means and what the flags say |
| `daw describe export` | `daw export`: formats, the level policy, the record beside the file |
| `daw describe project`, `sampler`, `effects`, `automation` | The song's schema and what its fields mean |

Importing a compressed song is under `samples` in `daw describe project`.
