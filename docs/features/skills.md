# Skills and reusable sounds — implemented October 1, 2026

A skill is a plain-English file that tells an agent how a kind of job is done. The
repository gives the agent the capabilities and says how each works
(`daw describe`); a skill says what to do with them, in whose words and to whose
taste. Personal preferences stay out of Git, like the songs; generic workflows
can be tracked.

## Task routing and discovery — implemented October 3, 2026

[AGENTS.md](../../AGENTS.md) routes a request to [music](../music.md) or
[development](../development.md). These guides are ordinary files read for the
applicable task; the root instructions do not import both. Command syntax and
semantics stay in CLI help and `daw describe`; skills hold repeatable workflows,
such as the sequence of decisions and checks for a finished-song edit.

The tracked `song-edit` source stays in `.claude/skills/song-edit/`.
`.agents/skills/song-edit` is a relative symlink to that directory for Codex
skill discovery, so both agents read the same instructions. Each skill's short
name and description support discovery; its body and references are read when
needed. The music guide also links directly to the skill for agents without
automatic discovery. No new workflow skills are added by this instruction split.

## Where a skill lives

- **A personal skill** goes in `.claude/skills/NAME/SKILL.md` in this checkout.
  Claude Code loads it by name there. Git ignores every folder under
  `.claude/skills/` except the generic skills this repository ships, which
  `.gitignore` names one by one, so `git add -A` cannot commit a personal skill.
- **Outside the checkout**, your own skills folder (`~/.claude/skills/NAME/`)
  works from any directory once `daw` is on the PATH, which the Mac app's menu sets
  up. In this checkout keep to `uv run daw`.
- **A generic skill** is tracked: `.claude/skills/song-edit/` is the first. To add
  another, add its folder to `.gitignore` as that one is, and keep it free of
  names, private sounds and local paths. For Codex, add a matching relative
  symlink under `.agents/skills/` and explicitly allow it in `.gitignore`.
- **Personal Codex skills** may live under `.agents/skills/NAME/` (ignored), or
  `~/.agents/skills/NAME/`. Personal links to `.claude/skills/` are ignored too.

## What goes in a personal skill

Only what is personal. The generic [`song-edit`](../../.claude/skills/song-edit/SKILL.md)
skill has the mechanics of editing a finished song from timecodes, and the
`daw describe` topics have how each step is done, so a personal skill reads like
this:

```markdown
---
name: my-edit
description: Make my kind of edit. Use when I paste a request with a song and
  timecodes from the person who sends them.
---

Follow the song-edit skill, with these particulars.

- Requests come as "edit from :41-1:43, pull the 8-count out around 1:20". An
  8-count is eight beats, two bars.
- Start with content/my-edit/open.wav and end with it again right after the song.
- content/my-edit/swell.wav ends where the song ends.
- The whole file is under 60 seconds.
- Export to projects/my-edit/SONG/exports/ as SONG_edit.m4a.
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
