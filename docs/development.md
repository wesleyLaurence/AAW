# Developing AAW

Read this when changing the software, tests, build, or repository documentation.
For operating a song, use [music.md](music.md). Read the relevant backlog item
and its feature file before building; keep one item to a branch and pull request.

## Set up

Install as [the README](../README.md#install) says, then add the test
dependencies and run both suites:

```sh
uv sync --extra dev --locked
uv run pytest -q && (cd engine && cargo test)
```

Dependencies are pinned in `engine/Cargo.lock` and `uv.lock`. If uv's Python is
an Intel build on Apple silicon, add its Rust target first: `rustup target add
x86_64-apple-darwin`. `daw` is one command: the Rust binary and `uv run daw`
each pass the other the commands it does not own, and
[engine/README.md](../engine/README.md) says which, and how `AAW_DAW` and
`AAW_PYTHON` choose the binary and the Python.

## Find the implementation

Read [architecture.md](architecture.md) when changing runtime behavior. It says
what exists. [concept.md](concept.md) is the end state and [backlog.md](backlog.md)
is work toward it; neither expands the scope of the requested change.
For documentation-only work, read the affected references and the rules below.

| Area | Code and reference | Required checks after code changes |
|---|---|---|
| Song model, engine, session host, CLI | `engine/`; [engine README](../engine/README.md) | `cargo test` in `engine/`, then `uv run pytest -q` at the root |
| Sample library and analysis | `src/agent_daw/`; relevant module and tests | Relevant pytest tests; full suite for changes crossing the Python/Rust boundary |
| Mac app | `apps/mac/`; [Mac README](../apps/mac/README.md) | `./build.sh test` in `apps/mac/` |

The runtime song model is Rust. Python reads songs through `agent_daw.aaw_py`.
`uv run pytest -q` rebuilds the release `daw` and the package's Rust model.
`uv run daw` itself uses the release binary as last built; after Rust changes,
run `cargo build --release` in `engine/` before manual CLI checks if tests have
not already rebuilt it.

The Mac app builds with `./build.sh` in `apps/mac/`. Its README explains
`--snapshot` for checking the window and `--measure` for timing its drawing.
Run it with `AAW_DATA_DIR` set to a scratch folder: without a project it creates
an Untitled project and keeps its project index there, outside the person's data.
Its bundle holds a copy of `daw`; in this checkout keep using `uv run daw`.

## Documentation maintenance

Each kind of statement has one home in `docs/`; [README.md](README.md) is the index.

- `concept.md` is the end state: what the app should be and why. It never says what
  is built. Change it when the direction changes, and record the reason in
  `decisions.md`.
- `backlog.md` is everything not built, one line an item: Next in the order to
  build, Later and Ideas. When the person thinks out loud, put the idea
  under Ideas in a line. Do not start a new notes or plan file.
- `features/NAME.md` is one feature. Write it when a backlog item is picked up and
  needs a design: status proposed, then What, Why, Design, Done when and Open
  questions. When the feature is built, rewrite the file in the present tense as
  its reference, with the date it was implemented in the title. The choices made
  in building it, with what was rejected and why, go under its Decisions heading,
  not in `decisions.md` (D203).
- The root `README.md` is for a person, a musician or a developer: what AAW is,
  why it exists, what it does at a high level, its status, how to install it and
  a first song. It holds no commands beyond install, no song format and no
  feature reference; a feature changes it only when it changes what AAW does at
  that level (D193).
- `architecture.md`, `engine/README.md`, `apps/mac/README.md` and `daw describe` say
  what exists. Change them in the pull request that changes the code.
- `decisions.md` is append-only and holds only choices for the whole project:
  direction, priorities, process, and rules that cut across features (D203). An
  entry is a few sentences: what was chosen, what was rejected, why. A choice
  within one feature goes in that feature's file. A choice that replaces an
  entry gets a new numbered entry, and the old one gets "Revised by Dn". It is
  long: never read it whole. Find an entry by its number (`grep -n '^\*\*D59\.'`)
  or a word, and the last number with `tail`.
- `completed.md` is what was built, newest first, one short line a pull request:
  the date, what in a sentence, the pull request, and whether a person tried it
  or only tests and scripted runs did.
- `archive/` holds superseded documents. Do not update them or build from them.
- Before building, read the backlog item and its feature file. One item to a
  branch and a pull request.
- The pull request that finishes an item also moves its line from `backlog.md` to
  `completed.md` with the date, the pull request and what a person tried, and renumbers Next.
  There is no list of things to try by hand (D202): the person finds what does
  not work by using the app, and it comes back as a backlog item.
- A limit that should be lifted gets a backlog line. A limit that is simply true
  stays in the feature's file.

## Git scope

- Commit reusable tooling, tests, documentation, and generic instructional examples.
- Keep demo composition separate from reusable engine code. Do not hardcode
  creative patterns, sample names or a person's absolute library path in it.
- Keep personal music out of this repository: arrangements, samples, source manifests,
  a song's `SONG.md`, revisions, renders, exports, and song-specific scripts.
- Store creative work under ignored `projects/` or `content/`, or outside the repository.
- A skill is personal content like a song: who asks, which sounds, the limits, the
  file names. Keep it in ignored `.claude/skills/NAME/` or `.agents/skills/NAME/`, in
  a project's own `.claude/skills/NAME/`, or in your own skills folder; this
  repository tracks none. A skill names `daw` commands and paths under `projects/`
  or `content/`, never an absolute library path. See [skills](features/skills.md).
- The person's instructions and taste for all their music are personal too: ignored
  `.claude/rules/instructions.md` and `.claude/rules/taste.md`. Claude Code loads
  them into a development session as well; they say nothing about the software.
  See [what every session is given](features/agent-context.md).
- The root has no `CLAUDE.md`: Claude Code reads `AGENTS.md` directly, and beside a
  `CLAUDE.md` it would read neither a project's `AGENTS.md` nor, in a session
  started in a project's folder, the root's.
- Put song-specific automation inside its ignored project directory. Never force-add
  ignored content or embed personal compositions in tooling, tests, or documentation.
- Generic examples and test fixtures must be independent of personal songs and private
  libraries; prefer generated test audio.
- Review staged changes before committing for creative content and local source paths.
- A commit message is a short subject and, if needed, a few lines of body. The
  detail goes in the pull request and the feature's file, not the commit (D203).
- Keep local project files intact when changing tracking. Song history, if wanted,
  belongs in separate private versioning, not the tooling repository.
