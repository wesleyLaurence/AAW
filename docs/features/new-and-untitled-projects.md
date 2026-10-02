# New, Untitled and Save As — implemented October 2, 2026

What it should feel like is in [concept.md](../concept.md#opening-the-app); this
file is how it works. The choices are D57 and D58 in
[decisions.md](../decisions.md).

## What

The app opens on a blank project called Untitled. File › New makes another. An
Untitled project is given a name and a place with Save As…, and both the person
and an agent can find any project the app knows.

## A project is a folder

A project is a folder with `song.yaml` in it, its samples and its renders. Every
`daw` command takes the folder or the file as PROJECT, in Rust and in Python
alike, and so does the app. A path that is not there is taken as a folder unless
it is written as a `.yaml` file.

The app's data folder is `~/Library/Application Support/AAW`, or what
`AAW_DATA_DIR` names. It holds `Untitled/` and `projects.json`. The tests and the
scripted app runs set `AAW_DATA_DIR`, so that they never write to the person's
own. The registry of hosts stays where it was (`AAW_HOST_DIR`).

## A blank project

`aaw_host::project::create` writes one; `daw init` and the app both call it. The
app's is 120 BPM and 32 bars, 64 seconds, and `daw init` keeps its own defaults.

An Untitled project is a folder in `Untitled/` of the data folder, named
Untitled, Untitled 2, Untitled 3 and so on with the lowest number not in use,
and that name is the song's title. Making the folder is the claim on the name,
so two windows opened at once cannot take the same one.

## Launch

With no project on its command line the app makes an Untitled project and opens
it. There is no welcome window. A project opened while the launch's Untitled
project still holds nothing takes its place. Closing the last window quits.

Before that, launch tidies the Untitled folder: a project there that holds
nothing is deleted, and one that holds something is listed in Open Recent.
Nothing is asked. A project that a host has open, such as another copy of the
app, is left alone.

## Save As…

File › Save As… (⇧⌘S) asks for a name and a place, and the host saves the
project there. It is the host's command, so `daw move PROJECT NEW_FOLDER` and
`daw copy PROJECT NEW_FOLDER` do the same from a terminal.

| The project | Save As… | The command |
|---|---|---|
| Untitled | Moves the folder: a rename, or across volumes a copy and then the removal of the original | `project.move` |
| Has a name | Copies the folder. The original stays as it was, closed, and the window carries on in the copy | `project.copy` |

Either way the host saves first, sets `session.title` from the folder's name,
registers under the new path and records one change, "Saved as My Beat". The
undo history, the handles and what is playing carry on, because the host never
closes. The app's model takes the new path from the host, and from its own Save
As… at once.

The change is in the log and is not an undo step: Undo after Save As… undoes the
edit made before it. Every revision in the history is given the new title, so
undoing an earlier edit keeps the name, and a step that only changed the title
is dropped.

The new folder must not exist, and it cannot be inside the project. A refused
Save As… changes nothing: the new path is claimed in the registry before any
file moves. The lock file is not copied.

## The old path keeps answering

A host answers at each path its project has had, until it closes. A command sent
to an old path lands in the project in the window, and is told where that is:
its result carries `"project"` with the new path, and a line on stderr says
`{"notice": "OLD is now at NEW", "project": NEW}`. The reply to every request
carries the song's current path, which is how the command line knows.

After a move the old path is gone. A client finds a host by the canonical path
of the song, and a path that does not exist cannot be made canonical, so the
registry takes such a path as it is written, through the folders that do exist.

After a copy the old path is the original, which commands cannot reach while the
copy answers for it. `daw host` on the original is refused and says why. The
path answers for itself again when the copy is closed, or when the original is
opened in the app, which first asks the copy's host to release it
(`project.release`).

`daw render`, `daw schedule` and `daw play --benchmark` read the files
themselves, and follow a project that was saved elsewhere in the same way.

## Closing an Untitled project

| The project | Closing its window |
|---|---|
| Has a name | Closes. The host saved each edit |
| Untitled, holds nothing | Is deleted without a question |
| Untitled, holds something | Asks: Save…, Delete or Cancel |

"Holds nothing" is about what is there, not about what was done: the folder has
no file but the song, and the song is the blank one it began as. A track added
and then undone leaves nothing to ask about.

Save… runs Save As… and closes if the project was saved. Delete closes and
deletes the folder. Cancel leaves the window open. Quitting asks about each such
project in turn, and Cancel in any of them keeps the app open. `daw close` on an
Untitled project that holds something closes the window and keeps the project,
which Open Recent then lists. Only a folder inside `Untitled/` is ever deleted.

## The project index

The app keeps `projects.json` in the data folder: for each project its song
file, a macOS bookmark, its title, whether it is Untitled, when it was last
opened and whether it is missing. It is written on New, Open, Save As… and
Close, and is what Open Recent shows, fifteen at most, in place of
`NSDocumentController`'s list. A first launch without an index takes that list
over.

At launch each bookmark is resolved, which follows a folder that was moved or
renamed in the Finder, and the path is corrected. A project that is nowhere, or
in the Trash, is kept, marked missing and cannot be chosen. Clear Menu forgets
every project but the ones open and the Untitled ones, which nothing else leads
back to.

## `daw projects`

```sh
daw projects          # what a host has open
daw projects --all    # and what the index knows besides
```

It asks each registered host that answers for its title, its revision, whether
it is playing and whether its window is in front, which the app tells the host
as it tells it the selection. The window in front is listed first. Each row's
`project` is the folder, which every command takes. `--all` adds the index's
projects that no host has open, with `missing` and `opened`.

## Where it is

| Part | In |
|---|---|
| The song file a path names, a blank project, Untitled, moving and copying a folder, reading the index | `engine/crates/aaw-host/src/project.rs` |
| A session's move: the title through the history, the change | `Session::relocate` in `session.rs` |
| A host answering at several paths | `host.rs`, `registry.rs`, `client.rs` |
| `daw projects`, `move`, `copy`, and a folder as PROJECT | `engine/crates/aaw-cli/src/main.rs`, `src/agent_daw/cli.py` |
| What the app calls | `engine/crates/aaw-ffi/src/projects.rs` and `Song` in `lib.rs` |
| The index and the close decision | `apps/mac/Sources/AAWApp/Projects.swift` |
| Launch, the File menu, Save As…, closing and quitting | `apps/mac/Sources/AAWApp/App.swift` |

## Limits

- Save As… does not replace a folder that is already there. The save panel
  offers to, and the host then refuses.
- The Python commands (`check`, `timeline`, `export`, `samples import`) read the
  path they are given and do not follow a project that was saved elsewhere. The
  notice on any other command says where it is.
- Until the sample index moves into the app's data folder (backlog item 1), an
  Untitled project's browser is empty unless `AAW_LIBRARY` names an index,
  because the index is found by walking up from the project's folder.
- A moved project is matched by its bookmark. Matching by an ID in the project
  file comes with the `.aaw` file type (backlog item 3).
- A project made with `daw init` and never opened in the app is not in the
  index.
- A copy of a large project is made while the window waits. On one APFS volume
  the files are cloned, which is quick.

## Decided

Asked of the person on October 2, 2026, before building:

1. A blank song is 32 bars. The song growing when a clip is placed past its end
   is a backlog line of its own.
2. Untitled projects left by a crash are in Open Recent; launch asks nothing.
3. Save As… on a project that has a name makes a copy, as it does in every app,
   and an agent's commands for the old path follow the window to the copy.
