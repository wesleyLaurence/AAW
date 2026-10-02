# Agent DAW development and composition

Read README.md and docs/mvp.md for the implemented MVP. Older design documents are
future scope, not a requirement to add effects, UI or broad infrastructure.

- Use `uv run daw ...`; all commands emit JSON. `daw` is the Rust binary, which `uv run daw`
  runs from `engine/target/release/daw`; `samples`, `listen`, `compare`, `check` and `export` run in
  Python, and either entry point passes the other its commands.
- The engine, the song model and the session host are Rust, in `engine/`
  (docs/Rust-Swift-Update.md). After Rust changes run `cargo test` there, then
  `uv run pytest -q`, which rebuilds the release `daw` and the package's song model
  (`agent_daw.aaw_py`) and drives both. Python reads songs only through that model.
- The Mac app in `apps/mac/` builds with `./build.sh`; run `./build.sh test` after Swift changes
  and see its README for checking the window with `--snapshot` and timing its drawing with
  `--measure`. Its bundle holds a copy of `daw`, which the app's menu can link onto the PATH;
  in this checkout keep to `uv run daw`, which runs the engine as last built.
- Edit a song with commands (`set`, `clip move`, `effect add`, `undo`; see engine/README.md).
  While a session host runs for the song (it is open in the Mac app, or `daw host` or
  `daw play` is running; `daw status` shows `"host": true`), prefer them to `apply`: the
  person sees and hears each edit, and it lands in one undo history with your origin. Read
  `daw changes --since REV` for what the person changed, and `daw status` for what they have
  selected in the app. Give a `daw batch` a `--label` that says what it does; the person
  sees it in the activity panel and the Undo menu. `apply` and raw file edits still work;
  a host loads a file changed from outside as an external edit.
- Index samples with `daw samples scan`; filename metadata is a hint, never guaranteed.
- Import selected samples into the project. Never modify the original Splice library.
- Read `daw inspect` before editing; use its SHA with `daw apply --expect` for revisions.
- Musical positions are zero-based quarter-note beats. Use fractions for triplets.
- Read `daw describe sampler` before assigning pitched/gated samples. Confirm root octave
  with `daw samples analyze` or `--root-note auto`; resolve `daw check` root warnings.
- Read `daw describe effects` before adding effects or returns. Stems exclude master
  effects; track stems are dry and each return has its own stem.
- Read `daw describe automation` before writing automation lanes. Give automated
  effects an `id`; ramp levels over a few milliseconds rather than jumping them.
- Render a short section or isolated track to investigate an edit. Full mix pointer is
  `renders/latest.json`; previews use `renders/latest-preview.json`.
- Keep demo composition separate from reusable engine code. Do not hardcode creative
  patterns, sample names or a user's absolute library path in the core package.
- Preserve editable project, selected source hashes, stems and render report with demos.
- Do not claim a render was listened to when only numerical analysis was performed.
- Audio files, sample database and environment stay out of Git; no uploads are required.
- Before continuing an existing song, read its local `HANDOFF.md` if present.

## Git scope

- Commit reusable tooling, tests, documentation, and generic instructional examples.
- Keep personal music out of this repository: arrangements, samples, source manifests,
  creative briefs, handoff notes, revisions, renders, exports, and song-specific scripts.
- Store creative work under ignored `projects/` or `content/`, or outside the repository.
- Put song-specific automation inside its ignored project directory. Never force-add
  ignored content or embed personal compositions in tooling, tests, or documentation.
- Generic examples and test fixtures must be independent of personal songs and private
  libraries; prefer generated test audio.
- Review staged changes before committing for creative content and local source paths.
- Keep local project files intact when changing tracking. Song history, if wanted,
  belongs in separate private versioning, not the tooling repository.
