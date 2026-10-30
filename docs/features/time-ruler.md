# Time ruler (implemented 2026-10-30)

Status: built, decision D166. The person's request after using the app on
October 30, 2026: the ruler showed bars and nothing in time, so how long a
song is could not be read off the window.

## What

The ruler's fifth strip, under the bar numbers and nearest the lanes, is the
time from the song's start as a clock: `0:00`, `0:15`, `1:00`, and past an
hour `1:00:00`. It follows the zoom as the bar numbers do: zoomed out a label
every few minutes, zoomed in every second, then every fifth of a second, with
ticks between the labels. It follows the session tempo: at 96 BPM bar 25
stands over `1:00`, and the strip redraws when the tempo changes.

## Design

`TimelineLayout` holds the strip's geometry and the song's `tempo`, which
`ArrangementView` sets from the arrangement with the meter and the length.
`timeStrip` is 16 points; `rulerHeight` is now 84, with `barTop` and `timeTop`
named, so the start position's flag and the playhead's head keep drawing from
the bar strip and the lanes begin under the time.

- **Labels.** `timeLabelStep` is the finest of the clock's spacings, 0.1, 0.2,
  0.5, 1, 2, 5, 10, 15, 30 seconds, 1, 2, 5, 10, 15, 30 minutes and an hour,
  whose labels are at least 56 points apart at the zoom's `pixelsPerSecond`;
  past an hour the step doubles. Labels fall at multiples of the step, from
  `0:00`.
- **Ticks.** `timeTickStep` is the finest spacing, from hundredths of a second
  up, that divides the label step whole and keeps the ticks 10 points apart,
  else the labels' own: ten ticks under labels five seconds apart, twelve
  under labels a minute apart.
- **Writing.** `TimelineLayout.clock(seconds, step:)` writes minutes and
  seconds, hours before them past an hour, and after the seconds the tenths
  when the labels are under a second apart and the hundredths under a tenth.
  The time is rounded in the finest place shown, so a float's noise never
  reads as `0:00.299`.
- **Drawing.** `drawRuler` draws a hairline between the bars and the time, a
  tick at each tick step and the clock at each label, in the bar numbers'
  font and color, so the two rows read as one ruler. The bar strip's ticks
  stop at the time strip.

The time is a reading of the ruler, not a place a click lands: the grid,
snapping and the start position are in beats as before, and `daw` has no
time ruler to show. The transport bar's position stays bar.beat.sixteenth.

## Done when

- The ruler shows the time under the bars, right at the tempo, at every zoom:
  tested in `TimelineLayoutTests` for the step at four zooms and two tempos,
  and for the clock's writing; seen in scripted pictures of a 96 BPM song
  fitted, zoomed in and zoomed out.

## Open questions

- Whether the song's length in time should also stand in the transport bar,
  beside the position, so that it is read without scrolling to the end.
