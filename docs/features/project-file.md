# The `.aaw` project file — implemented October 22, 2026

Every project has a file named for its folder at the top of it, `My Beat/My
Beat.aaw`, beside `song.yaml`. It is the way into the project from outside the
app, as an Ableton project's `.als` is: double-clicked in the Finder it opens
the app on that song, or brings the song's window forward if it is open; the
Finder shows it as an AAW Project with the app's icon, and draws the project's
folder with the app's icon too; File › Open… offers it; and every `daw`
command takes it as PROJECT, as it takes the folder or `song.yaml`. Decided as
D142; backlog: Next, The `.aaw` project and its folder, from the person's notes
of October 19, 2026 (D123).

## What is in it

The song stays in `song.yaml`. The `.aaw` is small, readable text that names
the format, the project's ID and the song file:

```yaml
# An AAW project. Open it with AAW; the song is in song.yaml beside it.
format: 1
id: 3f0c2a5e-8d7b-4c1e-9a52-0b6f1d2e4c7a
song: song.yaml
```

- `format`, an integer, 1. A file that says a later number was written by a
  newer AAW and is refused with that, rather than guessed at.
- `id`, a random UUID written once, when the project is made. A copy gets its
  own. It is about the folder, which it tells apart after a move.
- `song`, the song's file name in the same folder. Only a bare file name is
  accepted, never a path, so a `.aaw` sent from elsewhere cannot point the app
  or `daw` at a file outside its folder.
- A comment line saying what the file is, for a person or an agent who opens
  it in an editor.

The title is not in it: `session.title` is the song's, and the file's own name
is what the Finder shows.

## Why beside the song, not the song renamed

The concept had the song itself as `My Beat.aaw`. The file is beside the song
instead, as the person suggested, because `song.yaml` is named in about 170
places in the code, the tests, the docs, `daw describe` and the person's
skills, so a file beside it touches the few places that find a project's song;
because Save As… and a rename in the Finder would otherwise rename the song
file itself, and the host's registry, the index and every open handle key on
that path; because an ID in `session` would be part of the model, its
fingerprint and the pinned outcomes, undone, copied with `range copy` and
diffed like music, where in the `.aaw` it is about the folder; and because
agents and editors keep reading YAML under its own extension. What it costs is
two files where there was one, and a `.aaw` that can be missing, stale or
duplicated; each case is handled below, and a project without one keeps
working as every project did.

## Its name and where it is found

- It is named for the folder: `My Beat/My Beat.aaw`, `Untitled 3/Untitled
  3.aaw`.
- A folder's `.aaw` is the one `*.aaw` file at the folder's top. With several,
  the one named for the folder is the project's, else the first by name.
- A folder renamed in the Finder keeps its old `.aaw` name, as Ableton leaves
  an `.als`. Nothing renames it behind the person's back; it still opens, and
  Save As… names it anew.

## Who writes it

| When | What happens | Where |
|---|---|---|
| A project is made: `daw init`, File › New, an Untitled project | Written with a new ID; `daw init`'s reply names it as `file` | `aaw_host::project::create` |
| Save As… on an Untitled project, `daw move` | Named for the new folder, same ID | `project::move_folder` |
| Save As… on a named project, `daw copy` | Named for the new folder, a new ID | `project::copy_folder` |
| A host opens a project with none, one made before this | Written with a new ID; a folder that cannot be written is hosted all the same, with a warning on stderr | `host::open` |
| The app opens a project whose ID another project the app knows still carries, a folder duplicated in the Finder | The one being opened is given a new ID | `AppDelegate.openSong`, `project::renew_id` |
| Anything else, the Python commands included | Read only | |

The "holds nothing" test for an Untitled project (`untouched` in `project.rs`)
leaves the `.aaw` out, as it does the lock file and the folder's icon, so a
blank Untitled project is still deleted without a question.

## Opening a project by its `.aaw`

- **`daw`:** `song_file` in `aaw-host/src/project.rs` and in
  `src/agent_daw/cli.py` read a `.aaw` path's `song` field and return that
  file, so a command given the `.aaw`, the folder or `song.yaml` reaches the
  same host. A `.aaw` that is gone, as the project's old one is after a move,
  names its folder's `song.yaml`, which a host may still answer for. A `.aaw`
  that does not read is refused as a bad argument, in plain text with exit
  status 2, saying what is wrong and the song file to open instead, in Rust and
  in Python alike.
- **The app:** `AppDelegate.songFile` takes a `.aaw` URL through the core's
  `project_song_file`, so the Finder's double-click (`application(_:open:)`),
  Open…, Open Recent and the command line go through one rule. A project
  already open brings its window forward. With the app closed, the open event
  comes before `applicationDidFinishLaunching`, which then makes no Untitled
  project, so none is shown on the way.
- **From a terminal:** `open "My Beat.aaw"` opens the song in the app, which
  an agent can use to show the person a song.

## The file type

In `apps/mac/Info.plist`, `UTExportedTypeDeclarations` declares
`local.aaw.project`, "AAW Project", with the extension `aaw`, conforming to
`public.yaml` so Quick Look and Open With show it as text, with the app's icon
as its `UTTypeIconFile`; `CFBundleDocumentTypes` claims that type with the role
Editor and the rank Owner, in place of the earlier claim on `public.yaml` at
the rank Alternate, which offered AAW for every YAML file on the Mac. The open
panel's `allowedContentTypes` is the project type and folders.

`build.sh` runs `plutil -lint` on the Info.plist and registers the bundle it
builds with `lsregister -f`, so a build from this checkout owns `.aaw` without
being launched first. Another copy of AAW.app on the Mac can win the
double-click; the Mac README says how to see which with `lsregister -dump`.

The identifier is under the bundle's `local.aaw` because nothing has been
shipped; an exported type identifier should not change once files carry it,
so it wants a reverse-DNS name on a domain the project will keep before the
first `.aaw` leaves this Mac (a line under Later › Shipping).

## The folder's icon

A folder is not a document type, so Launch Services cannot draw it with the
app's icon; the Finder draws a custom icon on a folder that holds an `Icon\r`
file with its icon and has the custom-icon bit set, which
`NSWorkspace.setIcon(_:forFile:options:)` writes in one call. The app sets it
when it opens or saves a named project whose folder has no `Icon\r` yet; `daw
init` leaves it to the app, and an Untitled project in the app's data folder
gets none. The `Icon\r` file is hidden and is passed over by the "holds
nothing" test. On another Mac without the icon file the folder is plain, as
Ableton's is.

## The ID and the project index

`projects.json` gains each project's `id`, written when the app opens, closes
or saves it. When a project is opened whose ID another entry carries at a path
where no song is any longer, that entry goes, so a project moved while the app
was closed and opened by double-click takes its old place in Open Recent with
no missing row left. Bookmarks still find a moved folder at launch; the ID
covers what a bookmark cannot, a folder copied to another disk and the original
deleted, or a project restored from a backup. When a project is opened whose ID
another entry carries at a path where a song still is, a folder duplicated in
the Finder, the one being opened is given a new ID.

`daw projects` and `daw projects --all` say each project's `id`, read from its
folder, or as the index last recorded it for a project that is gone, and null
without one.

## Limits

- A folder renamed in the Finder keeps its old `.aaw` name until Save As….
- `daw init` does not add the project to the app's index; that is Next, A
  project made with `daw init` in the app's index.
- The `Icon\r` file is a file in the folder that Dropbox and Git see; the
  person asked for the icon, so it stays unless it proves a nuisance.
- Whether `.aaw` is used by another Mac app was not searched.

## Tried

Tests, the `daw` commands by hand on a scratch project (`daw init`, `daw map`
on the `.aaw`, `daw move` and `daw copy` renaming it with the ID kept and
renewed, a file of format 3 refused with the reason, a `.aaw` that is gone
reaching the folder's song), and a scripted run of the built app given a
`.aaw` on its command line: it opened on the song with no Untitled project
made, wrote the ID into `projects.json` and `Icon\r` into the folder; after
`build.sh` the file's Kind read "AAW Project" to `mdls`. The Finder's
double-click, the icons on the file and the folder, Open… and Open Recent
after a move were not tried by hand.
