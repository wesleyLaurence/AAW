# Working in AAW

AAW is a local music workspace: a person and an agent edit the same song through
`daw` and the Mac app. This repository also contains the software that builds it.

## Choose instructions for the task

- **Make or edit music, find sounds, render or export:** read
  [docs/music.md](docs/music.md). Start from the intended project's state.
- **Change software, tests, builds, or repository documentation:** read
  [docs/development.md](docs/development.md). It routes to the relevant code,
  build checks and documentation rules.
- **Both:** read both guides when each part becomes relevant. Infer the route
  from the request; no mode-selection question is needed. A musical request
  does not authorize developing a missing feature: explain the limitation and
  use available tools, or ask when a software change is needed.

Read only the applicable guide and references needed for the current operation.
The README is an introduction and setup guide, not required reading every session.
Architecture, backlog and feature designs are development references, not music
prerequisites. `docs/verify.md` is the person's list of what to try by hand;
an agent does not read it. When the person proposes future work, record a line
under Ideas in `docs/backlog.md`; do not create a separate notes or plan file.

## What a music session is given

- **The person's instructions and taste for all their music** are
  `.claude/rules/instructions.md` and `.claude/rules/taste.md` at this root,
  ignored by Git. Claude Code loads both into every session started at or
  below this root; another agent reads them before music work. Never edit the
  instructions.
- **A project's `SONG.md`,** at the top of its folder, is what the song is and
  is for, the person's instructions for it, where it is heading and what was
  decided. The project's own `AGENTS.md` imports it. Have it before the first
  edit, and keep it current.
- **Skills** for every project are in `.claude/skills/` at this root, and a
  project's own in its `.claude/skills/`.

[docs/music.md](docs/music.md) says who writes what in each, which wins where
they differ, and what a session started here or in a project's folder has.

## Tools and shared boundaries

- In this checkout use `uv run daw ...`. Discover verbs with `uv run daw --help`,
  syntax and accepted fields with a command's `--help`, and semantics with
  `uv run daw describe TOPIC`; `daw describe` lists the topics, and
  `daw describe start` makes a first song. `--schema` adds a topic's JSON Schema.
- Command results are JSON; runtime errors use stderr and a nonzero exit status.
  Help and argument-parsing errors may be plain text.
- Preserve personal work. Songs, their `SONG.md`, samples, renders, exports and
  song-specific scripts belong under ignored `projects/` or `content/`, or outside
  this repository. Import selected audio; never modify the source library.
- Commit reusable code, tests, docs and generic examples only. Personal skills,
  instructions and taste stay ignored or in a personal folder; never force-add
  personal content.
- A skill describes a repeatable workflow and is personal: this repository ships
  none. [Skill conventions](docs/features/skills.md) apply when adding or changing one.
