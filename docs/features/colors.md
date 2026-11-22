# Colors for tracks and clips — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 8. The person's note of November
22, 2026 (D194): "you should be able to change the color of any track or
clip in order to match it with others for example making the drum clips
white etc." It takes in the Later line "a track's color saved in the song".

## What

A track, a group, a return and a clip each take a color, chosen from a
palette in the header's or the clip's menu, Color ▸, a grid of swatches as
Ableton's, white among them. Several selected rows or clips are colored at
once. A clip with no color of its own takes its track's; a track with none
takes the next of the palette, as today. Colors are saved in the song, so
they are the same on every open and for the agent.

## Why

The app gives each track the next of ten palette colors as it is first seen
(`SongModel.color(of:)`): the song has no color, so the colors move when
tracks are added in another order or the song is opened again, and a clip
cannot differ from its track.

## Design

- **In the song.** An optional `color` on `tracks[]`, `groups[]`, `returns[]`
  and every kind of clip: the index of a swatch in a fixed palette of about
  24, kept in the song as a name (`white`, `red`, `orange` ...) so it reads
  in text and the agent can set it. `daw set SONG tracks.drums.color white`
  and `daw clip color SONG CLIP... COLOR`.
- **The palette** is `Theme`'s, extended to the named swatches; a track with
  no color takes one by its index in the song, not by when the app saw it, so
  it is stable.
- **Drawing.** A clip's fill, the header's color bar and the group's band
  read the color. A light color such as white draws the clip's name and
  notes dark.
- **Undo** treats a color as any edit.

## Done when

- A track and a clip colored in the app keep their colors after closing and
  opening the song, and a clip colored differs from its track.
- Several selected clips colored at once; the agent sets one with `daw`.
