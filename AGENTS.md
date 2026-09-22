# Agent DAW development and composition

Read README.md and docs/mvp.md for the implemented MVP. Older design documents are
future scope, not a requirement to add effects, UI or broad infrastructure.

- Use `uv run daw ...`; all commands emit JSON. Run `uv run pytest -q` after engine changes.
- Index samples with `daw samples scan`; filename metadata is a hint, never guaranteed.
- Import selected samples into the project. Never modify the original Splice library.
- Read `daw inspect` before editing; use its SHA with `daw apply --expect` for revisions.
- Musical positions are zero-based quarter-note beats. Use fractions for triplets.
- Read `daw describe sampler` before assigning pitched/gated samples. Confirm root octave.
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
