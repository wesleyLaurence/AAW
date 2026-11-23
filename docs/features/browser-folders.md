# Folders as a tree in the browser — implemented November 22, 2026

Status: implemented (D201). The person's note of November 22, 2026 (D194):
"in the sample viewer, it is not showing folders. For example i have a
folder on the side samples/ and in it there is one folder bass/ but i just
see all the audio files listed. Ideally I want to see each folder, and
folders within that folder each with an arrow that can be a collapse or
hide so i can drill down further cause the sample folders may be nested."

## What

The browser's Folders section lists the registered sample folders as a
tree, as the Finder's list view does: each subfolder a row with a
disclosure arrow, a folder icon, its name and how many samples are under
it at any depth, and under an open folder its subfolders and then the
samples directly in it, set in by depth. Folders and samples are ordered by
name as the Finder orders them, case aside and numbers by value, so
`kick 2` comes before `kick 10`. A folder with no samples under it is not
listed.

When one folder is the tree's root, because it is the only one registered
or the only one checked in the column at the left, its contents are listed
at the top; when there are several, each is a folder row of its own.

A search, a category or a kind lists every match under the checked folders
flat, as before, each with the folder it is in faint after its length:
`samples/drums/kicks`, named from the registered folder that holds it,
the nearest one when folders are registered inside each other. The Samples
section is unchanged and names a sample's pack.

## Why

The browser listed every sample under a folder flat, from the recursive
index in `library.sqlite` ([the browser](browser.md)): a library sorted
into `bass/`, `drums/kicks/` and `vox/` lost the sorting the person made.

## How it works

- **From the index.** `daw samples tree [DIR ...]` lists each folder's
  subfolders, with their counts, and the samples directly in it, each a
  search's row with what is measured of it; with no directory, the
  registered folders. A directory must be a registered folder or inside
  one. It reads the index only, no audio file, in about 0.15 s.
- **Only what is open is read.** The app reads the folders the tree shows
  open and has not read yet, in one run of `daw samples tree` off the main
  thread; an open folder not yet read shows "Reading…" under it. A
  subfolder left open inside a folder that was closed comes back open with
  it and is read then. What was read is kept until a folder is added,
  refreshed or removed, which reads what is open again.
- **Open folders are kept.** The paths of the open folders are the
  `browser.openFolders` default, so the tree comes back as it was left.
- **The same rows.** A sample in the tree plays on a click, adds with a
  double-click or its +, drags onto the timeline or a header, and has Add
  to Favorites in its menu, as in the flat list.
- **The keys.** A click on a row selects it and gives the tree the keys:
  Up and Down walk the rows and play a sample reached; Right opens a closed
  folder and goes into an open one; Left closes an open folder and
  otherwise goes to the folder the row is in. A click on the arrow, or a
  double-click on the row, opens or closes a folder.

## Limits

- A folder is not dragged: dropped on the timeline it is not taken.
- The project's own `samples/` is not in the tree; it is listed only when
  it is added as a folder.
- A folder registered inside another one is listed under both when all
  folders are shown, as each is a root of its own.
