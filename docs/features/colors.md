# Colors for tracks and clips — implemented November 23, 2026

Backlog: Next 1, the person's note of November 22, 2026 (D194): "you should
be able to change the color of any track or clip in order to match it with
others for example making the drum clips white etc." It takes in the Later
line "a track's color saved in the song".

## What

A track, a group, a return and a clip each take a color, chosen in the
header's or the clip's right-click menu, Color ▸: a grid of 24 swatches, as
Ableton's, white among them. Several selected rows or clips are colored at
once, as one undo step. A clip with no color of its own takes its track's; a
track or a group with none takes one by its place in the song; a return with
none has none, as before. Colors are saved in the song, so they are the same
on every open and for the agent.

## In the song

`color` on `tracks[]`, `groups[]`, `returns[]`, a note clip and an audio
clip: one of 24 names, in the grid's order

    red coral orange amber yellow lime green teal cyan blue violet orchid
    rose pink peach sand olive mint sky lavender brown slate silver white

left out of the file when there is none. Any other value is refused with
the list. `aaw-model`'s `COLORS` holds each name with its RGB, and the app
reads them through the FFI's `palette()`, so the names and the colors have
one home. `DEFAULT_COLORS` are the ten the app gave out before, in the same
order: `coral amber lime teal blue violet orchid rose olive cyan`.
`Project::track_color(i)` is a track's own color or the default at its index
among the tracks; `group_color(i)` a group's own or the default counted back
from the last, so that the first group and the first track differ.

## The agent

- `daw set SONG tracks.drums.color white` colors a row, groups and returns
  alike; `daw remove SONG tracks.drums.color` takes it away.
- `daw clip color SONG CLIP... COLOR` colors note clips and audio clips
  (`tracks.T.audio.N`, `@N`), several as one step; `none` takes their own
  away. It is the host's `clip.color {clip, color}`, labelled "Color clip
  beat at 0 white".
- `daw inspect` gives a row's color after its ID and name, and a clip's,
  where they have one; `daw describe project` says what colors are.

## The app

- The FFI's views carry `color`, the own one, and `shown_color`, the one
  drawn, on tracks, groups, note clips and audio clips; a return has only
  `color`. `SongModel.color(of:)` reads `shown_color`, so a row and a clip
  are drawn the same on every open.
- The header's color bar, a clip's title strip and body, a folded group's
  overview, by each clip's own color, and the piano roll's notes, by its
  clip's, read the color. A clip's name and its notes are drawn dark over any
  color already, so white needs nothing of its own.
- Color ▸ is in every header's menu but the master's and in the clip's menu,
  before Delete: Automatic on a track or a group, No Color on a return and
  Track Color on a clip first, which take the own color away, then the grid
  (`SwatchGrid`), the color everything colored shares ringed, none for a
  mix, and the name of the one under the pointer as its tooltip. A click
  colors and closes the menu.
- A right click on a row among several selected keeps the selection, so
  Color ▸ colors them all, as a right click on a selected clip keeps the
  clips.
- `Edit::Color {rows, clips, color}` is one step: a `set` or `remove` of each
  row's `color`, only where it changes, and a `clip.color` of each clip.

## Decisions

- **Names, not indices or hex.** A name reads in the file and in a command,
  and the agent can say "white" without a table. A fixed palette keeps
  colors that look right on the dark window; hex was rejected for the same
  reason Ableton has no free picker.
- **24, in two rows of twelve.** The design asked for about 24: strong hues
  first in the order of the wheel, soft ones and neutrals after, white last.
  The ten the app gave out keep their RGB, under names, so a song no one has
  colored looks as it did.
- **A track without a color takes one by its place, not by when the app saw
  it.** Before, the app gave the next color to each track as it was first
  seen, so colors moved between opens. By place is stable on every open and
  the same for the agent; moving a track that has no color of its own
  changes its color, as the place changes. Giving every new track a color
  when it is made, as Ableton does, was rejected for now: it would write a
  field into every track the agent adds.
- **Groups count from the other end of the defaults,** so the first group,
  which sits over the first track, is not the same color.
- **A return without a color has none.** It had no color bar before; a
  return colored gets one.
- **One command for clips, a path for rows.** A clip is found by a reference
  or an audio clip's index, which `clip.color` resolves as `clip.active`
  does, and its undo step names the clip; a row is already a path.
- **The menu only.** Ableton has no key or main-menu item for color, and
  neither has this app.
- **The name and the notes stay dark.** They already were over every color;
  over white they read as on any light swatch.
