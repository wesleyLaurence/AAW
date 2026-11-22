# Beats and sixteenths in the ruler — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 2. The person's note of November
22, 2026 (D194): "on the top bar that shows the bar numbers i dont see any
labels for the beats so if I zoom in and want to select the 3rd 16th note I
don't see it. I see sub divisions of time but not on the actually bar/beat.
As I zoom in it should reveal more of that detail." What is said here about
the ruler was read from the checkout on November 22, 2026.

## What

The bar strip of the timeline's ruler labels finer positions as the zoom
allows, as Ableton's does: bars only when zoomed out, then `5.2`, `5.3`,
`5.4` at each beat, then `5.2.3` at each sixteenth, each with its tick, the
bar's own number brighter than the beats' and the beats' than the
sixteenths'. Ticks finer than the labels fill between them down to the grid.
A click on a label puts the start position there, as a click on the strip
does today.

## Why

The arrangement's ruler draws a bar number every `barLabelStep` bars and a
tick at each bar, and nothing inside a bar (`ArrangementView`, the bar
labels after the markers). The finer divisions the person sees are the time
strip's clock ([time ruler](time-ruler.md)), which counts seconds, not beats.
The piano roll's ruler already labels beats as `bar.beat`
(`NoteEditor`), so the two rulers disagree.

## Design

- **The step.** One label step from bars down: 4, 2 and 1 bars, a beat, an
  eighth, a sixteenth, the finest whose labels are 36 points apart at the
  zoom, in the meter's own beats ([time signature](time-signature.md)); a
  triplet grid labels its own divisions. `TimelineLayout` gains
  `positionLabelStep` beside `barLabelStep`, and the label text is the
  position's own, as `TimelineLayout`'s `bar.beat.sixteenth` already formats
  it.
- **Ticks.** The finest of the grid's lines at least 6 points apart, as the
  lanes draw them, shorter than a labelled tick.
- **The piano roll** uses the same step and text, so both rulers read alike.

## Done when

- Zooming in from the whole song to one beat shows bar numbers, then beats,
  then sixteenths labelled, with no labels crowding at any zoom, in 4/4,
  3/4 and 6/8.
