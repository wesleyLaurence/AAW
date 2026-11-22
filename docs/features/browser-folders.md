# Folders as a tree in the browser — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 4. The person's note of November
22, 2026 (D194): "in the sample viewer, it is not showing folders. For
example i have a folder on the side samples/ and in it there is one folder
bass/ but i just see all the audio files listed. Ideally I want to see each
folder, and folders within that folder each with an arrow that can be a
collapse or hide so i can drill down further cause the sample folders may be
nested."

## What

A shared folder in the browser's Folders category, and the project's own
`samples/`, list as a tree, as the Finder's list view and Ableton's browser
do: each subfolder a row with a disclosure arrow, opened and closed by a
click on the arrow or → and ←, and the samples directly in a folder listed
under it after its subfolders. Each folder's open or closed state is kept
across launches. A search still lists every matching sample under the
scope, flat, each with its folder's path faint beside its name.

## Why

The browser lists every sample under a folder flat, from the recursive index
in `library.sqlite` ([the browser](browser.md)): a library sorted into
`bass/`, `drums/kicks/` and `vox/` loses the sorting the person made.

## Design

- **From the index, not the disk.** The index already holds each sample's
  path under its registered folder; the tree is built from those paths, so
  opening a folder reads nothing from the disk and a large library stays
  quick. Empty subfolders are not shown.
- **Lazy rows.** Only the open folders' rows are made, so a folder of
  thousands of samples draws when opened, not when the category is shown.
- **The same rows.** A sample in the tree drags, previews, favorites and
  shows its key and tempo as it does in the flat list today.
- **A folder dragged** onto the timeline is not a drop target in this item.

## Done when

- A folder with nested subfolders shows them as a tree whose arrows open and
  close, with its samples in their own folders, and the state is kept.
- Search lists matches flat with their folders.
