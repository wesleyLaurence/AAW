# Developing AAW

Read this when changing the software, tests, build, or repository documentation.
For operating a song, use [music.md](music.md). Read the relevant backlog item
and its feature file before building; keep one item to a branch and pull request.

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
not already rebuilt it. Initial dependencies and installation are in
[README setup](../README.md#setup).

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
  build, Verify, Later and Ideas. When the person thinks out loud, put the idea
  under Ideas in a line. Do not start a new notes or plan file.
- `features/NAME.md` is one feature. Write it when a backlog item is picked up and
  needs a design: status proposed, then What, Why, Design, Done when and Open
  questions. When the feature is built, rewrite the file in the present tense as
  its reference, with the date it was implemented in the title.
- `architecture.md`, `engine/README.md`, `apps/mac/README.md` and `daw describe` say
  what exists. Change them in the pull request that changes the code.
- `decisions.md` is append-only. A choice gets a new numbered entry, and the entry
  it replaces gets "Revised by Dn".
- `completed.md` is what was built, newest first.
- `archive/` holds superseded documents. Do not update them or build from them.
- Before building, read the backlog item and its feature file. One item to a
  branch and a pull request.
- The pull request that finishes an item also moves its line from `backlog.md` to
  `completed.md` with the date, the pull request and what a person tried; adds a
  line under Verify for anything nobody heard or tried by hand; and renumbers Next.
- A limit that should be lifted gets a backlog line. A limit that is simply true
  stays in the feature's file.

## Git scope

- Commit reusable tooling, tests, documentation, and generic instructional examples.
- Keep demo composition separate from reusable engine code. Do not hardcode
  creative patterns, sample names or a person's absolute library path in it.
- Keep personal music out of this repository: arrangements, samples, source manifests,
  creative briefs, handoff notes, revisions, renders, exports, and song-specific scripts.
- Store creative work under ignored `projects/` or `content/`, or outside the repository.
- A skill is personal content like a song: who asks, which sounds, the limits, the
  file names. Keep it in ignored `.claude/skills/NAME/` or `.agents/skills/NAME/`, or
  in your own skills folder; this repository tracks none. A skill names `daw`
  commands and paths under `projects/` or `content/`, never an absolute library
  path. See [skills](features/skills.md).
- Put song-specific automation inside its ignored project directory. Never force-add
  ignored content or embed personal compositions in tooling, tests, or documentation.
- Generic examples and test fixtures must be independent of personal songs and private
  libraries; prefer generated test audio.
- Review staged changes before committing for creative content and local source paths.
- Keep local project files intact when changing tracking. Song history, if wanted,
  belongs in separate private versioning, not the tooling repository.
