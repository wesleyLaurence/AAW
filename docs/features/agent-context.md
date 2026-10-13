# What every session is given: SONG.md, skills and the person's instructions — implemented October 12, 2026

The end state is in
[concept.md](../concept.md#the-agent-one-home-and-each-project). This file is
what exists while the agent is a terminal started in this checkout, which stands
in for the workspace, and what was tried with Claude Code (version 2.1.296 on
this Mac). The choices made in building it are D110.

## What a session has

Every session that works on a song is given, without being told to look:

| Level | What | Where |
|---|---|---|
| Everything the person makes | How to use `daw` and the conventions | `AGENTS.md` at the checkout's root, and `docs/music.md` |
| | The person's instructions and taste | `.claude/rules/instructions.md` and `.claude/rules/taste.md` at the root, ignored by Git |
| | Skills usable in any project | `.claude/skills/` at the root, ignored by Git |
| This project | `SONG.md`: what the song is and is for, the person's instructions for it, where it is heading, what was decided | The top of the project folder |
| | Skills for this project only | `.claude/skills/` in the project folder |
| | This project's conversations | The agent's own list for the project folder, when a session is started there |

`SONG.md` takes the place of `HANDOFF.md` and `brief.md`. It is to a project
what `AGENTS.md` is to a code repository: read at the start of every session
there, short, and about what holds, not what happened.

## SONG.md

One file at the top of the project folder, written when the project is made,
with four headings and nothing under them:

```markdown
## Vision

## Instructions

## Direction

## Decisions
```

| Section | Owner | Holds |
|---|---|---|
| Vision | The person; the agent suggests edits and never rewrites | The idea, the feel and style, the references, what the song is for |
| Instructions | The person; the agent never edits it | Rules for this song alone: what not to touch, who it is for, how it is delivered |
| Direction | Both; the agent keeps it current | Where the song is heading: what is wanted next, what is unresolved, what the person liked and disliked that still holds |
| Decisions | Both | Settled choices and why |

- **No history.** There is no log, no dated list of sessions and no revision
  notes. A decision that is replaced is rewritten in place, not added under the
  old one. What happened is the host's history and the agent's own conversation
  list.
- **No state the tool can print.** Tracks, clips, tempo, sections and levels
  come from `daw inspect`, `daw map` and `daw status`.
- **Short enough to read every time.** It is loaded whole into every session, so
  it stays about a page. When it grows past that the agent proposes what to cut;
  it does not start a second file.
- **The agent says what it changed** in `SONG.md` at the end of a turn that
  changed it.

Nothing in the tool reads what is written in it or checks it: who writes which
section is an instruction to the agent, in [the music guide](../music.md) and in
`daw describe project` under `notes`.

### Where it is written

`aaw_host::project::create` writes it beside `song.yaml`, so `daw init` and the
app's Untitled projects both have it. `daw init` reports its path as `notes`
beside `project`.

- **A file already in the folder is kept.** `daw init` in a folder that has a
  `SONG.md` or an `AGENTS.md` leaves each as it is.
- **`daw move`, `daw copy` and Save As…** carry both files, as they carry the
  folder.
- **An Untitled project still holds nothing** while the two files are as they
  were written, so closing it deletes it without a question. One with something
  written in `SONG.md` holds something, and closing it asks.
- **A project made before October 12, 2026 has none.** No command adds one to a
  project that exists; the music guide tells the agent to offer to write it
  with the person.

## How SONG.md reaches the session

Each project also gets a two-line `AGENTS.md`, written with `SONG.md`:

```markdown
Read SONG.md before working on this project, and keep it current.

@SONG.md
```

Claude Code attaches the imported file, so the song's text is in the session
with no step by the agent: at launch when the session starts in the project
folder, and the first time the agent reads any file of the project with its
Read tool when the session starts in the checkout. An agent that does not expand
imports reads the first line and opens the file.

This works only because the root has no `CLAUDE.md`. With one, Claude Code reads
`CLAUDE.md` files alone and no `AGENTS.md` beside or below them, and a session
started in a project folder did not expand the root file's `@AGENTS.md` either.
So the root `CLAUDE.md`, whose whole text was that import, is gone, and Claude
Code reads the root `AGENTS.md` directly, as Codex does. That needs version
2.1.277 or later.

The music guide still says to read `SONG.md` first, for an agent that loads
neither file by itself and for a session that has run only `daw` commands, which
are not file reads.

## A project's skills

A skill for one project lives in the project at
`.claude/skills/NAME/SKILL.md`, in the form [skills](skills.md) describes, with
the link under `.agents/skills/` for Codex if wanted.

- **Started in the project folder,** the session lists the project's skills and
  the checkout's together, each as a slash command.
- **Started in the checkout,** the project's skills are not loaded: Claude Code
  does not look for skills in a folder Git ignores, and `projects/` is ignored.
  There the music guide tells the agent to read the name and description at the
  top of each `SKILL.md` under the project's `.claude/skills/`, and the body of
  one that applies. They are not slash commands in that session.

## The person's instructions, taste and skills

Until the workspace is a folder of its own, the checkout stands in for it. The
concept's `profile/instructions.md` and `profile/taste.md` are kept where Claude
Code loads a file into every session started anywhere under the checkout, which
is the root's `.claude/rules/`:

- **`.claude/rules/instructions.md`**: the person's rules for all their music.
  The agent never edits it.
- **`.claude/rules/taste.md`**: what the agent has learned of the person, each
  entry with its evidence and dates, as the concept describes. The agent says
  what it would add at the end of a session and writes it when the person
  agrees.
- **`.claude/skills/`** at the root.

`/.claude/rules/` is in `.gitignore` beside the skills folders, and nothing
creates the two files: the person makes them, or an agent with the person. An
import of the same files from the root `AGENTS.md` reached only a session
started in the root, which is why they are not kept in `profile/` and imported.
The root `AGENTS.md` says in words where the two files are, for an agent that
does not load them by itself. They are in every session in the checkout, a
development session too. Precedence is the concept's: the person's rules, then
`SONG.md`, then taste, then defaults.

## Where a session starts

Both ways work, and the person chooses by the job:

| Started in | Gets | Gives up |
|---|---|---|
| The project folder, `cd projects/NAME && claude` | The root instructions and skills from the folders above, the project's `SONG.md` at launch, its skills as slash commands, and a conversation list of this project alone (`claude --resume`, `claude --continue`) | Other projects are outside the session's folder, so reading one asks first; `--add-dir` on the checkout or on the other project grants it |
| The checkout's root | Every project in reach: reuse a sound from another song, compare two mixes | One conversation list for every song, and no slash commands for a project's skills |

`uv run daw` works from a project folder, since `uv` finds the checkout from
below it. All of this depends on the project being under the checkout, whose
folders above it are where the root instructions and skills are found. A project
kept elsewhere needs them attached, which is what
[the agent panel](agent-panel.md) is to do.

## What was tried

Each as one `claude -p` run against a scratch project under `projects/`, removed
afterwards. No interactive session was tried, and no other agent.

On October 12, 2026, with what is built: a project made by `daw init` in a
folder whose name has a space, one skill in its `.claude/skills/`, text under
each heading of its `SONG.md`, and a line in the root's
`.claude/rules/instructions.md`.

| Tried | Result |
|---|---|
| Started in the project folder, file tools switched off, asked what the song is for and what the person's rules are | Answered from `SONG.md` and from the instructions file, named the root `AGENTS.md` and its music guide, and listed as loaded the two rules files, the root `AGENTS.md`, the project's `AGENTS.md` and `SONG.md`, and no `CLAUDE.md` |
| The same, asked for its skills | The project's skill and the checkout's `roar-edit` were both listed |
| Where those runs were kept | In a list for the project's folder, apart from the checkout's |
| Started in the root, asked to add a track to the song and then to do what the project's skill is for | Read the music guide, then `SONG.md`, before its first edit; found the skill by reading the `SKILL.md` under the project's `.claude/skills/`, as the guide says, and followed it. The skill was not in its list |
| Started in the root, one Read of the project's `song.yaml` and nothing else | The project's `AGENTS.md` and the whole of `SONG.md` were attached after the read, unasked, the import's own line left as written with the file after it |

On October 11, 2026, before any of it was built:

| Tried | Result |
|---|---|
| Started in the project folder, with the root `CLAUDE.md` in place | The root `CLAUDE.md` was loaded and its `@AGENTS.md` was not expanded: the session had none of `AGENTS.md`. Claude Code's documentation calls an import that resolves outside the session's folder external, and asks to approve it once in an interactive session; a `-p` run cannot ask |
| The same, with the root `CLAUDE.md` moved aside for the run | The root `AGENTS.md` was read directly and its text was in the session |
| Started in the root, the agent reading a file of the project, which had one skill | The project's skill was not listed |
| The same skill in an untracked folder under `docs/`, which Git does not ignore | The skill was listed after the read |
| The root `AGENTS.md` importing a file under `profile/` and a file that does not exist | Started in the root, the file's text was in the session and the missing one was passed over without a word. Started in the project folder, neither was: only the two import lines |
| `uv run daw --help` in a project folder | Ran |
| The memory a project-folder session was given | The checkout's, the same index as a session in the root |

From the documentation and not tried: `claude --resume` and `claude --continue`
list by the folder a session started in; `--add-dir` loads that folder's
`.claude/skills/` and, only with
`CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`, its instruction files.

## Limits

- A session started in the checkout has a project's skills only by reading
  them, and has one conversation list for every song. The agent panel and the
  workspace as a folder of its own are the backlog's answers.
- A project kept outside the checkout has its own `SONG.md` and skills and none
  of the root's instructions, taste or skills.
- The person's two files are in a folder Finder hides, and under names that are
  Claude Code's.
- Nothing stops an agent from editing Vision or Instructions: the rule is
  written, not enforced.
- Whether Codex reads the `AGENTS.md` of a project folder below where it
  started, and a project's skills, was not looked up.

## Open questions

1. **A song's Instructions against the person's rules.** The concept puts the
   person's rules first, and so does the music guide. A rule written for one
   song is the person's too and the more exact of the two, so it should
   probably win where they differ.
2. **A visible `skills/` in the project.** The concept draws one; Claude Code
   finds `.claude/skills/`, which Finder hides. Whether `skills/` is the real
   folder with a link to it is the panel's question too.
3. **Where the person's two files are seen.** A `profile/` folder at the root
   whose two files are links to them would show them under the concept's names;
   whether another agent then reads them was not looked up.
4. **A log.** If a project's history proves wanted as something to search, it is
   a separate file that no session reads unasked. Not planned.
