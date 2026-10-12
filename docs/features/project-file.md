# The `.aaw` project file — proposed October 11, 2026

Status: proposed, not built. Backlog: Later › Shipping, The `.aaw` file type and
the project ID. What is said here about the code was read from this checkout on
October 11, 2026; what is said about macOS (Uniform Type Identifiers, Launch
Services) is from Apple's documentation as remembered, and nothing was built or
tried on a Mac.

## What

Every project has a file named for it, `My Beat.aaw`, at the top of its folder,
beside `song.yaml`. It is the way into the project from outside the app, as an
Ableton project's `.als` is: double-clicked in the Finder it opens the app on
that song, or brings the song's window to the front if it is open; Finder shows
it as an AAW project; File › Open… offers it; and every `daw` command takes it
as PROJECT, as it takes the folder or `song.yaml` today.

The song stays in `song.yaml`. The `.aaw` file is small, readable text that
names the format, the project's ID and the song file:

```yaml
# An AAW project. Open it with AAW; the song is in song.yaml beside it.
format: 1
id: 3f0c2a5e-8d7b-4c1e-9a52-0b6f1d2e4c7a
song: song.yaml
```

## Why

The person asked for it on October 11, 2026: a project made today is a folder
with a YAML file in it, and once the app is closed there is no file to find and
double-click that opens the session. A folder cannot be double-clicked into an
app, and `song.yaml` is a YAML file like any other, which the Finder opens in a
text editor unless AAW has been chosen for every YAML file.

[concept.md](../concept.md#a-project-on-disk) already wants `.aaw` as the app's
own file type and an ID in the project that recognizes it after it moves; the
backlog has carried both as one line under Shipping since D75.

## Design

### A file beside the song, not the song renamed

The concept has the song itself as `My Beat.aaw`. This design keeps
`song.yaml` and adds the `.aaw` beside it, as the person suggested. The reasons:

- **The song file stays where everything expects it.** `song.yaml` is named in
  about 170 places in the Rust, Python and Swift code and tests, in the docs, in
  `daw describe`, in the person's skills and in what agents have learned. A file
  beside it touches the few places that find a project's song, not all of them.
- **The song's name does not follow its title.** If the song were `My
  Beat.aaw`, Save As… and a rename in the Finder would rename the song file
  itself, and the host's registry, the index and every open handle key on that
  path. A `.aaw` that is renamed changes none of them.
- **The ID stays out of the song.** An `id` in `session` would be part of the
  model, its fingerprint and the pinned outcomes in `tests/test_model.py`, and
  would be undone, copied with `range copy` and diffed like music. In the `.aaw`
  it is about the folder, which is what it identifies.
- **Agents and editors keep reading YAML** under its own extension, with no
  editor told that `.aaw` is YAML.

What it costs: two files where there was one, and a `.aaw` that can be missing,
stale or duplicated. Each case is handled below, and a project without a `.aaw`
keeps working as every project does today.

When this is built, the concept's project layout and its first open question
(whether the project file is named for the project) change to this design, with
a decision recorded in [decisions.md](../decisions.md).

### What is in it

- `format`, an integer, 1. A later version that reads a newer number says so
  rather than guessing.
- `id`, a random UUID written once, when the project is made.
- `song`, the song's file name in the same folder, `song.yaml`. Only a bare file
  name is accepted, never a path, so a `.aaw` sent from elsewhere cannot point the
  app or `daw` at a file outside its folder.
- A comment line saying what the file is, for a person or an agent who opens it
  in an editor.

The title is not in it: `session.title` is the song's, and the file's own name is
what the Finder shows.

### Its name and where it is found

- It is named for the folder: `My Beat/My Beat.aaw`, `Untitled 3/Untitled 3.aaw`.
- A folder's `.aaw` is the one `*.aaw` file at the folder's top. With several,
  the one named for the folder is used, else the first by name, and `daw check`
  says that there are several.
- A folder renamed in the Finder keeps its old `.aaw` name. Nothing renames it
  behind the person's back; it still opens, and Save As… names it anew.

### Who writes it

| When | What happens | Where |
|---|---|---|
| A project is made: `daw init`, File › New, an Untitled project | Written with a new ID | `aaw_host::project::create` |
| Save As… on an Untitled project, `daw move` | Renamed for the new folder, same ID | `project::move_folder` |
| Save As… on a named project, `daw copy` | Renamed for the new folder, a new ID | `project::copy_folder` |
| A host opens a project with no `.aaw`, one made before this | Written with a new ID | where the host opens a project |
| Anything else, the Python commands included | Read only | |

The "holds nothing" test for an Untitled project (`IGNORED` and `untouched` in
`project.rs`) leaves the `.aaw` out, as it does the lock file, so a blank
Untitled project is still deleted without a question.

### Opening a project by its `.aaw`

- **`daw`:** `song_file` in `aaw-host/src/project.rs` and in
  `src/agent_daw/cli.py` read a `.aaw` path's `song` field and return that file.
  The host's registry keys on the song's canonical path, as now, so a command
  given the `.aaw`, the folder or `song.yaml` reaches the same host. A broken
  `.aaw` is refused with what is wrong and the song file to use instead.
- **The app:** `AppDelegate.songFile` takes a `.aaw` URL the same way, so
  `application(_:open:)`, which the Finder's double-click calls, Open…, Open
  Recent and the command line all go through one rule. A project already open
  brings its window forward, as `openSong` does today.
- **Launch:** a double-click with the app closed launches it with an open event.
  The app must open that project without first making an Untitled window and
  then closing it; today an opened project replaces the launch's Untitled one
  after it is shown.
- **From a terminal:** once the type is registered, `open "My Beat.aaw"` opens
  the song in the app, which an agent can use to show the person a song.

### The file type

In `apps/mac/Info.plist`:

- `UTExportedTypeDeclarations`: an identifier for an AAW project, the extension
  `aaw`, conforming to `public.yaml` (so Quick Look and Open With show text),
  and a description, "AAW Project".
- `CFBundleDocumentTypes`: that type with the role Editor and the rank Owner, in
  place of today's claim on `public.yaml` at the rank Alternate, which offers AAW
  for every YAML file on the Mac.
- An icon for the document once the app has one (An app that stands alone, under
  Shipping). Until then the Finder draws its generic document.

The open panel's `allowedContentTypes` becomes the project type and folders, in
place of YAML and folders.

Launch Services learns an app's types when the bundle is registered: when it is
first launched or copied into Applications. `build.sh` registers the bundle it
builds with `lsregister -f`, so a build from this checkout owns `.aaw` without
being launched first. Another copy of AAW.app on the Mac can win the
double-click; the Mac README will say how to see which with `lsregister -dump`.

### The ID and the project index

`projects.json` gains each project's `id`. When a project is opened at a path
the index does not know, an entry with the same ID that is marked missing is
moved to the new path, rather than leaving a missing row beside a new one: a
project moved while the app was closed and opened by double-click keeps its
place in Open Recent. Bookmarks still find a moved folder at launch, as now; the
ID covers what a bookmark cannot, a folder copied to another disk and the
original deleted, or a project restored from a backup.

Two folders can share an ID when one is duplicated in the Finder. When a project
is opened whose ID belongs to another entry whose project still exists, the one
being opened is given a new ID. The ID is only for matching, so which of two
copies keeps it does not matter.

`daw projects --all` shows each project's ID.

## Plan

One pull request, in this order, each step with its tests:

1. **The file in Rust.** Read and write a `.aaw` in `aaw-host/src/project.rs`;
   `create`, `move_folder` and `copy_folder` as in the table; `song_file`
   following a `.aaw`; the `IGNORED` and `untouched` change; a host writing one
   for a project that lacks one. Tests in `aaw-host/tests/project.rs`: made,
   moved, copied with a new ID, a folder with none, two, a broken one, a `song`
   field that is a path, a blank Untitled project still holding nothing.
2. **`daw` and Python.** `cli.py`'s `song_file`, so the Python commands take a
   `.aaw` too; `daw projects --all` with the ID. A Python test that runs a Rust
   command and a Python command on the same `.aaw`.
3. **The app.** `songFile`, the open panel, launch without the passing Untitled
   window, the index's `id` and its matching, and the Info.plist. Swift tests in
   `ProjectsTests.swift` for the index; a scripted run that opens a `.aaw` given
   on the command line.
4. **The build.** `lsregister -f` in `build.sh`; `plutil -lint` on the Info.plist.
5. **Docs.** This file rewritten as the reference; concept, a decision,
   [new and untitled projects](new-and-untitled-projects.md) (its limits on
   matching by ID and on `.yaml` paths), architecture, the engine and Mac
   READMEs, [music.md](../music.md) and `daw describe` where they say what a
   PROJECT is; the backlog line to [completed.md](../completed.md), and a Verify
   line for the double-click by hand.

The work is a few hundred lines, most of it in `project.rs` and the tests.

## Done when

- `daw init my-beat` makes `my-beat/song.yaml` and `my-beat/my-beat.aaw`, and
  `daw map my-beat/my-beat.aaw`, `daw listen` and `daw export` on that path work.
- Save As… and `daw move` rename the `.aaw` and keep its ID; `daw copy` renames
  it and gives a new ID.
- A blank Untitled project closes without a question, as before.
- A project made before this gets a `.aaw` the first time the app opens it.
- On the Mac, a `.aaw` double-clicked in the Finder opens the song with the app
  closed, with no Untitled window shown on the way, and brings its window
  forward with the app open; the Finder's Kind reads "AAW Project"; Open… offers
  `.aaw` files and folders; a YAML file elsewhere no longer lists AAW first.
- A project moved in the Finder while the app is closed, then double-clicked,
  takes its old place in Open Recent with no missing row left.
- `cargo test`, `uv run pytest -q` and `./build.sh test` pass.

## Open questions

1. **A file beside the song, or the song renamed?** This design recommends
   beside, as the person suggested; the concept says the song itself. The
   person decides, and the concept and a decision follow.
2. **The type's identifier.** The bundle is `local.aaw.mac`. An exported type
   identifier should not change once files carry it, so it wants a reverse-DNS
   name on a domain the project will keep before the first `.aaw` leaves this
   Mac.
3. **Does a folder renamed in the Finder get its `.aaw` renamed** the next time
   it opens? This design leaves it, as Ableton leaves an `.als`.
4. **Should `daw init` add the project to the app's index** now that the
   project has an ID? That is Next, item 7, and could ride with this.
5. **Is `.aaw` taken?** No common Mac app is known to use it, but that was not
   searched.
