# Names with spaces — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 3. The person's note of November
22, 2026 (D194): "it seems that I cant name a track something with a space
in it? i had to use a - instad of a space to make it work. But I should be
able to name a track with a space in it." It was a Later line, "names that
are not IDs", and moves up. What is said here about the song was read from
the checkout on November 22, 2026.

## What

A track, a group or a return is named anything in the app: "Lead Vox",
"808 (sub)", "Drums – Room". The header shows the name as typed. The agent
still addresses the row by a short ID, and sees both.

## Why

A row's name today is its `id`, and an ID is letters, digits, `-` and `_`
starting with a letter (`aaw-model/src/schema.rs`), because sends, groups,
inputs, lanes and every `daw` command name a row by it. Rename in the app
sends `track.rename`, which changes the ID, so a space is refused.

## Design

- **A `name` beside the `id`.** `tracks[]`, `groups[]` and `returns[]` take an
  optional `name`, any text up to 64 characters. The app shows the `name` when
  there is one and the `id` otherwise. Rename in the app sets the `name`.
- **The ID follows a new row's name.** A row made in the app takes its ID
  from its name, lower-cased with spaces as `-` and other characters left
  out (`lead-vox`), made unique as IDs are now; renamed later, the ID stays,
  so the song's references do not move under the agent.
- **The agent.** `daw inspect`, `daw map` and `daw describe` give both; a
  command that takes a row's ID also takes its exact name, so "Lead Vox"
  quoted works. `daw track rename` keeps changing the ID; `daw set SONG
  tracks.ID.name "Lead Vox"` sets the name.
- **Clips** keep their names as they are; this is the rows only.

## Done when

- A track, a group and a return renamed in the app with spaces and
  punctuation show it, save it, and keep playing through their sends,
  groups and lanes.
- `daw` finds the row by that name.

## Open questions

1. Whether a rename in the app should also move the ID when the row is new
   and nothing refers to it yet.
