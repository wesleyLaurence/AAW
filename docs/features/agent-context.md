# What every session is given: SONG.md, skills and the person's instructions — proposed October 11, 2026

Status: proposed, not built. Backlog: Next, What every session is given. The end
state is in
[concept.md](../concept.md#the-agent-one-home-and-each-project); this file is how
to have it now, while the agent is a terminal started in this checkout, and what
was tried with Claude Code on October 11, 2026 (version 2.1.296 on this Mac).

## What

Every session that works on a song is given, without being told to look:

| Level | What | Where, for now |
|---|---|---|
| Everything the person makes | How to use `daw` and the conventions | `AGENTS.md` at the checkout's root, and `docs/music.md` |
| | The person's instructions and taste | `.claude/rules/instructions.md` and `.claude/rules/taste.md` at the root, ignored by Git |
| | Skills usable in any project | `.claude/skills/` at the root, ignored by Git, as today |
| This project | `SONG.md`: what the song is and is for, the person's instructions for it, where it is heading, what was decided | The top of the project folder |
| | Skills for this project only | `.claude/skills/` in the project folder |
| | This project's conversations | The agent's own list for the project folder, when a session is started there |

`SONG.md` replaces `HANDOFF.md` and `brief.md`. It is to a project what
`AGENTS.md` is to a code repository: read at the start of every session there,
short, and about what holds, not what happened.

## Why

The person asked for it on October 11, 2026, and put it ahead of everything else
in Next (D109). Today a project's intent is a `HANDOFF.md`, named for one chat
handing over to the next and mostly a list of revisions; the person's own rules
and taste have no file at all, and what an agent has learned of them sits in one
agent's private memory; a skill can only be for every project; and every
conversation is filed under the checkout, whichever song it was about.

## Design

### SONG.md

One file at the top of the project folder, written by the tool when the project
is made, with four headings and nothing under them:

| Section | Owner | Holds |
|---|---|---|
| Vision | The person; the agent suggests edits and never rewrites | The idea, the feel and style, the references, what the song is for |
| Instructions | The person; the agent never edits it | Rules for this song alone: what not to touch, who it is for, how it is delivered |
| Direction | Both; the agent keeps it current | Where the song is heading: what is wanted next, what is unresolved, what the person liked and disliked that still holds |
| Decisions | Both | Settled choices and why |

- **No history.** There is no log, no dated list of sessions and no revision
  notes. A decision that is replaced is rewritten in place, not added under the
  old one. What happened is the host's history, the project's versions and the
  agent's own conversation list.
- **No state the tool can print.** Tracks, clips, tempo, sections and levels
  come from `daw inspect`, `daw map` and `daw status`, as the music guide
  already says of a handoff.
- **Short enough to read every time.** It is loaded whole into every session, so
  it stays about a page. When it grows past that the agent proposes what to cut;
  it does not start a second file.
- **The agent says what it changed** in `SONG.md` at the end of a turn that
  changed it, as it says what it learned of the person's taste.

### How SONG.md reaches the session

Each project also gets a two-line `AGENTS.md`, written with `SONG.md` and never
edited:

```markdown
Read SONG.md before working on this project, and keep it current.

@SONG.md
```

Claude Code expands the import, so the song's text is in the session with no
step by the agent: at launch when the session starts in the project folder, and
the first time the agent reads any file of the project with its Read tool when
the session starts in the checkout (both tried, below). An agent that does not
expand imports reads the first line and opens the file.

This works only when the root has no `CLAUDE.md`. With one, Claude Code's
documentation says it reads `CLAUDE.md` files alone and no `AGENTS.md` beside or
below them, and a session started in a project folder did not expand the root
file's `@AGENTS.md` either (tried, below). So the
root `CLAUDE.md`, whose whole text is that import, is removed, and Claude Code
reads the root `AGENTS.md` directly, as Codex does.

The music guide still says to read `SONG.md` first, for an agent that loads
neither file by itself and for a session that has run only `daw` commands, which
are not file reads.

### A project's skills

A skill for one project lives in the project at
`.claude/skills/NAME/SKILL.md`, in the form [skills](skills.md) describes, with
the link under `.agents/skills/` for Codex if wanted.

- **Started in the project folder,** the session lists the project's skills and
  the checkout's together, each as a slash command (tried).
- **Started in the checkout,** the project's skills are not loaded: Claude Code
  does not look for skills in a folder Git ignores, and `projects/` is ignored
  (tried). There the music guide tells the agent to read the name and
  description at the top of each `SKILL.md` under the project's
  `.claude/skills/`, and the body of one that applies. They are not slash
  commands in that session.

### The person's instructions, taste and skills

Until the workspace is a folder of its own, the checkout stands in for it. The
concept's `profile/instructions.md` and `profile/taste.md` are kept, for now,
where Claude Code loads a file into every session started anywhere under the
checkout, which is the root's `.claude/rules/`:

- **`.claude/rules/instructions.md`**: the person's rules for all their music.
  The agent never edits it.
- **`.claude/rules/taste.md`**: what the agent has learned of the person, each
  entry with its evidence and dates, as the concept describes. The agent says
  what it would add at the end of a session and writes it when the person
  agrees. What an agent has kept of the person's taste in its own memory moves
  here once, so another agent has it.
- **`.claude/skills/`** at the root, as today.

`/.claude/rules/` joins `.gitignore` beside the skills folders. A file there
reached a session started in the root and one started in a project folder
(tried). An import of the same file from the root `AGENTS.md` reached only the
session started in the root, which is why the files are not kept in `profile/`
and imported. The root `AGENTS.md` says in words where the two files are, for
an agent that does not load them by itself. They are in every session in the
checkout, a development session too. Precedence is the concept's: the person's
rules, then `SONG.md`, then taste, then defaults.

### Where a session starts

Both ways work, and the person chooses by the job:

| Started in | Gets | Gives up |
|---|---|---|
| The project folder, `cd projects/NAME && claude` | The root instructions and skills from the folders above, the project's `SONG.md` at launch, its skills as slash commands, and a conversation list of this project alone (`claude --resume`, `claude --continue`) | Other projects are outside the session's folder, so reading one asks first; `--add-dir` on the checkout or on the other project grants it |
| The checkout's root | Every project in reach: reuse a sound from another song, compare two mixes | One conversation list for every song, and no slash commands for a project's skills |

`uv run daw` works from a project folder, since `uv` finds the checkout from
below it (tried). All of this depends on the project being under the checkout,
whose folders above it are where the root instructions and skills are found. A
project kept elsewhere needs them attached, which is what
[the agent panel](agent-panel.md) does.

### Retiring HANDOFF.md

- `docs/music.md` names `SONG.md` where it names `HANDOFF.md`, and says to keep
  it current where it says to update the handoff. `AGENTS.md`,
  `docs/development.md` and [skills](skills.md) follow.
- The person's projects are theirs and ignored by Git, so no command rewrites
  them. Once, with the person, an agent makes a `SONG.md` from each project's
  `HANDOFF.md` and `brief.md`: the vision and the standing rules and decisions
  are kept, the revision lists are dropped, and the old files are removed when
  the person has read the new one.
- A skill of the person's that names `HANDOFF.md` is changed with them.

### What is built

1. `daw init`, and the host where the app makes a project, write `SONG.md` and
   the project's `AGENTS.md`. `daw copy` and Save As… carry them, as they carry
   the folder.
2. The root `CLAUDE.md` is removed, `/.claude/rules/` is ignored, and the root
   `AGENTS.md` says where the person's two files and a project's skills are.
3. The guides are changed as above, and `daw describe project` says what
   `SONG.md` is.
4. The person's projects are moved over, with the person.

Nothing here needs the app's window, the workspace outside the checkout or the
project index.

## What was tried

Each as one `claude -p` run on October 11, 2026, against a scratch project,
`projects/_context-test/`, removed afterwards. No interactive session was tried,
and no other agent.

| Tried | Result |
|---|---|
| Started in the project folder, which had one skill | The project's skill and the checkout's `roar-edit` were both listed |
| The same, asking for the root instructions | The root `CLAUDE.md` was loaded and its `@AGENTS.md` was not expanded: the session had none of `AGENTS.md`. Claude Code's documentation calls an import that resolves outside the session's folder external, and asks to approve it once in an interactive session; a `-p` run cannot ask |
| The same, with the root `CLAUDE.md` moved aside for the run | The root `AGENTS.md` was read directly and its text was in the session |
| Root `CLAUDE.md` aside, the project holding an `AGENTS.md` that imports `SONG.md`, started in the project folder | The root `AGENTS.md`, the project's and the text of `SONG.md` were all in the session at launch |
| The same files, started in the root, the agent reading another file of the project | The project's `AGENTS.md` and `SONG.md` were attached after the read, unasked |
| Started in the root, the agent reading a file of the project, which had one skill | The project's skill was not listed |
| The same skill in an untracked folder under `docs/`, which Git does not ignore | The skill was listed after the read |
| Root `CLAUDE.md` aside, a file in the root's `.claude/rules/`, started in the project folder and in the root | The file's text was in both sessions |
| Root `CLAUDE.md` aside, the root `AGENTS.md` importing a file under `profile/` and a file that does not exist | Started in the root, the file's text was in the session and the missing one was passed over without a word. Started in the project folder, neither was: only the two import lines |
| `uv run daw --help` in a project folder | Ran |
| The memory a project-folder session was given | The checkout's, the same index as a session in the root |

From the documentation and not tried: conversations are stored by the folder a
session started in; `--add-dir` loads that folder's `.claude/skills/` and, only
with `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`, its instruction files;
reading `AGENTS.md` directly needs version 2.1.277 or later.

## Done when

- A project made with `daw init` or in the app has a `SONG.md` with its four
  headings.
- A session started in a project folder, asked what the song is for and what the
  person's rules are, answers from `SONG.md` and the person's instructions
  without reading a file, and lists the project's skills and the checkout's.
- A session started in the checkout and asked to continue a song has its
  `SONG.md` before its first edit, and uses a skill of the project's that
  applies.
- `claude --resume` in a project folder lists that project's conversations and
  no other's.
- No guide names `HANDOFF.md`, and each of the person's projects that had one
  has a `SONG.md` the person has read.
- An agent that changed `SONG.md` said so, and changed nothing under
  Instructions.
- A person has done each of the above by hand, in an interactive session.

## Open questions

1. **Removing the root `CLAUDE.md`.** In the runs above it is what lets a
   project-folder session have the root instructions and lets a project's
   `AGENTS.md` load. Whether an interactive session in a project folder, with
   `CLAUDE.md` kept, asks once and then expands the import was not tried; even
   so the project's `AGENTS.md` would not load beside it.
2. **A song's Instructions against the person's rules.** The concept puts the
   person's rules first. A rule written for one song is the person's too and
   the more exact of the two, so it should probably win where they differ.
3. **A visible `skills/` in the project.** The concept draws one; Claude Code
   finds `.claude/skills/`, which Finder hides. Whether `skills/` is the real
   folder with a link to it is the panel's question too.
4. **Where the person's two files are seen.** `.claude/rules/` is hidden in
   Finder and is Claude Code's own place. A `profile/` folder at the root whose
   two files are links to them would show them under the concept's names;
   whether another agent then reads them was not looked up.
5. **A log.** If a project's history proves wanted as something to search, it is
   a separate file that no session reads unasked. Not planned.
6. **Codex.** Whether it reads the `AGENTS.md` of a project folder below where
   it started, and a project's skills, was not looked up.
7. **A project's name with a space,** as `groove box` has: nothing above was
   tried in one.
