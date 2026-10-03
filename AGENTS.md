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
prerequisites. When the person proposes future work, record a line under Ideas
in `docs/backlog.md`; do not create a separate notes or plan file.

## Tools and shared boundaries

- In this checkout use `uv run daw ...`. Discover verbs with `uv run daw --help`,
  syntax with a command's `--help`, and semantics with `uv run daw describe TOPIC`.
  Request the relevant topic; the default prints the full song schema.
- Command results are JSON; runtime errors use stderr and a nonzero exit status.
  Help and argument-parsing errors may be plain text.
- Preserve personal work. Songs, samples, briefs, handoffs, renders, exports and
  song-specific scripts belong under ignored `projects/` or `content/`, or outside
  this repository. Import selected audio; never modify the source library.
- Commit reusable code, tests, docs and generic examples only. Personal skills
  stay ignored or in a personal skills folder; never force-add personal content.
- Skills describe repeatable workflows. For a finished-song edit from timecodes,
  follow [song-edit](.claude/skills/song-edit/SKILL.md); read it only for that task.
  [Skill conventions](docs/features/skills.md) apply when adding or changing skills.
