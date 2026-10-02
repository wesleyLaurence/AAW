# New, Untitled and Save As — proposed October 2, 2026

Status: proposed, not built. Backlog item 1. What it should feel like is in
[concept.md](../concept.md#opening-the-app); this file is how.

## What

The app opens on a blank project called Untitled, a project can be made from the
File menu, an Untitled project is named and placed with Save As…, and both the
person and an agent can find any project the app knows.

## Why

Today the app can only open a `song.yaml` that already exists. It starts on a
welcome window with "Open Song…", and a new song is made from a terminal with
`daw init`. A person who opens the app to make something cannot begin.

## What is there to build on

- `daw init DIRECTORY` writes an empty song (it lives in the CLI, defaults to 144
  BPM and 16 bars, no tracks). A song's title already defaults to "Untitled".
- The host saves `song.yaml` after every edit, so there is no unsaved state to
  manage and no Save command.
- Everything about a project is found from its path: the host's socket is named
  from a hash of the canonical path, the lock is `.daw.lock` in its folder, and
  sample paths are relative to the folder, so a moved folder is still a valid
  project.
- A running host writes `{project, socket, pid}` under
  `~/Library/Application Support/AAW/hosts`.
- The app's `openSong` already takes a folder and finds `song.yaml` in it. The
  CLI takes only the file.

## Design

**A blank project.** Creating one moves out of the CLI into the core, so the app
makes it through `aaw-ffi` and `daw init` calls the same code. The app's default
is 120 BPM.

**Where Untitled lives.** `~/Library/Application Support/AAW/Untitled/Untitled N/`,
or the folder `AAW_UNTITLED_DIR` names, which the tests and the scripted app runs
set so they never write to the person's own. N is the lowest number not in use.

**Launch.** With no project on the command line the app makes an Untitled project
and opens it. The welcome window goes. Closing the last window quits the app.

**Save As… is the host's command.** `daw move PROJECT NEW_FOLDER` and the menu
item both send it. The host saves, renames the folder (or copies and deletes
across volumes), sets `session.title` from the name, registers under the new path
and records one change, "Saved as My Beat". Undo history, handles and playback
carry on, because the host never closes. The window's model takes the new URL from
the change.

**The old path keeps answering.** The host keeps its old registration as well
until it closes, and every reply carries the project's current path, so an agent
that holds the old path is told the new one on its next command. This is the
awkward part: a client finds a host by canonicalizing the path it was given, and
the old path no longer exists, so the lookup needs a second way in for a path that
is gone.

**Closing an Untitled project.** At revision 0, with nothing in its folder but
the song, the folder is deleted. Otherwise an alert offers Save…, Delete and
Cancel. Quitting asks for each open Untitled project in turn.

**The project index.** The app keeps `projects.json` in its data folder: for each
project its path, a macOS bookmark, its title, whether it is Untitled and when it
was last opened. It is written on New, Open, Save As and Close, and replaces
`NSDocumentController`'s recents as what Open Recent shows. At launch the app
resolves each bookmark and corrects a path that moved. A project whose bookmark
no longer resolves is kept and marked missing. Matching a moved project by an ID
in its file comes with the `.aaw` file type, backlog item 4.

**`daw projects`.** With no argument it lists the registered hosts that answer,
asking each for its title, its revision and whether its window is in front, which
the app tells its host as it tells it the selection. `--all` adds the index.

**A folder as PROJECT.** One function in the CLI turns a folder into the song
file in it, as the app's does, and every command uses it.

**Words.** "Open Song…" becomes "Open Project…", the open panel and the alert say
project, and File gains New (⌘N) and Save As… (⇧⌘S).

## Depends on

Nothing. Until backlog item 2 moves the sample index into the app's data folder,
an Untitled project's browser is empty unless `AAW_LIBRARY` names an index,
because the index is found by walking up from the project's folder.

## Done when

- Launching the app with no arguments shows an empty arrangement titled Untitled,
  and `daw projects` lists it with its path.
- ⌘N opens a second Untitled project in its own window.
- A track added by hand and a command from a terminal both land in an Untitled
  project and in one undo history.
- Save As… moves the project: the folder is at the new place with its samples,
  the title is the new name, Undo still undoes the edit made before the move, and
  a `daw set` sent to the old path lands and reports the new path.
- Closing an untouched Untitled project leaves no folder behind. Closing a changed
  one asks, and each of the three answers does what it says.
- A project moved in the Finder while the app is closed opens from Open Recent.
- `daw inspect FOLDER` works wherever `daw inspect FOLDER/song.yaml` does.
- `cargo test`, `uv run pytest -q` and `./build.sh test` pass, with tests of the
  move, the index, `daw projects` and the close decision.
- A person has done each of the above by hand at the Mac.

## Open questions

1. How long is a blank song? `session.length_beats` is required and clips must
   fit it. Either a generous default, or the app lengthens the song when a clip
   is placed past its end.
2. Do Untitled projects left by a crash appear in Open Recent, or in an offer at
   launch?
3. On a project that already has a name, is ⇧⌘S a rename or a copy?
