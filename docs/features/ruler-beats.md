# Beats and sixteenths in the ruler — implemented November 22, 2026

Built from the person's note of November 22, 2026 (D194): "on the top bar
that shows the bar numbers i dont see any labels for the beats so if I zoom
in and want to select the 3rd 16th note I don't see it. I see sub divisions
of time but not on the actually bar/beat. As I zoom in it should reveal more
of that detail." Decided in D197.

## What

The bar strip of the timeline's ruler labels finer places as the zoom
leaves room, as Ableton's does: bars when zoomed out, every 8, 4, 2 or 1 of
them, then `5.2`, `5.3`, `5.4` at each beat, then `5.1.3` at each eighth,
then `5.2.3` at each sixteenth. A bar's number is brighter than a beat's,
and a beat's than an eighth's or a sixteenth's; a label's tick runs the
strip's height, and short ticks fill between the labels. The piano roll's
ruler labels the same steps with the same text, counted from the clip's
start, so the two read alike.

A click in the bar strip puts the start position on the grid when the grid
falls on the labels, and else on the ticks, so that a label shown can be
clicked whatever the grid: with a grid of a bar, zoomed in to sixteenths, a
click lands on the sixteenth under it. ⌘ still clicks off the grid. Clicks
elsewhere in the ruler and in the lanes keep the grid.

## Design

- **The steps.** `TimelineLayout.rulerSteps` gives the label step, the tick
  step and what a label's third number counts. Labels take the finest of a
  sixteenth, an eighth, the meter's beat and whole bars doubling from one
  ([time signature](time-signature.md)) whose labels are at least 36 points
  apart and wider than the text the song's last bar would have at that
  step, about 6.2 points a character and 8 more, so that `128.4.3` does not
  run into the next label. Only steps that divide the meter's beat and the
  bar are used: 6/8 labels eighths, its beat, and sixteenths; 7/8 the same,
  up to `2.7`. Ticks take the finest step of the same ladder at least 6
  points apart that divides the label step, whatever the grid.
- **Triplets.** On a triplet grid the ladder below the beat is thirds of
  the beat, and sixths when the grid is finer than a third, and the third
  number counts them: on a 1/8T grid, `5.2.3` is the beat's last triplet
  eighth; on 1/16T, `5.1.2` is its second sixth.
- **The text.** `TimelineLayout.rulerLabel` writes the bar alone on a bar,
  `bar.beat` on a beat and `bar.beat.count` between, each from one, the
  trailing `.1`s dropped as Ableton drops them. Before a clip's start in the
  piano roll the bar is 0 and less.
- **The click.** `TimelineLayout.rulerTarget` snaps to the grid when it
  divides the label step, else to the tick step.

## Limits

- Labels stop at a sixteenth: a 1/32 grid is drawn in the lanes but not
  labelled in the ruler, since the position is written bar.beat.sixteenth.
- A label's width is estimated from its characters, not measured in the
  font.
