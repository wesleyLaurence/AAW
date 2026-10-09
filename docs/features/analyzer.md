# The live analyzer (implemented October 7, 2026)

Status: built, in two pull requests on one day, decisions D95, D96 and D97:
the effect, the tap, the window and its levels, loudness, spectrum and stereo
field first, then the spectrogram and the scrolling waveform. What is said of
iZotope Insight 2 is from its product pages and a review; nobody ran it here.

## What

An effect that changes nothing and shows the sound that passes through it,
drawn as the song plays. It goes in any chain of a track, a group, a return or
the master, and where it sits says what it measures: before or after the
limiter, on the drum group or on the whole mix.

```sh
daw effect add projects/demo master --type analyzer
daw effect add projects/demo groups.drums --type analyzer --id kit
```

In the device panel it is a strip with the two channels' levels, the momentary
loudness and the spectrum, and under it Open Window. The window holds every
view, resizes, goes full screen with the green button and stays open whatever
is selected in the song's window; View › Analyzer Window (⌥⌘L) opens it too.

## Why

The person mixes by ear and by eye. Before this the app showed the sound in one
place: the spectrum under an equalizer's curve, for the row whose devices the
detail panel shows. Whether the low end is mono, how loud the drop is against
the verse, whether a limiter is flattening the mix and where a resonance sits
are read off a meter in every other DAW, and here they waited for a render and
`daw listen`.

A window of its own, because the detail panel shows one row's devices: an
analyzer on the master would go out of sight when a drum track is clicked,
which is when it is wanted. The panel is also 214 points tall at its least, too
short for a stereo field or a spectrogram.

## The effect

`analyzer` is an effect kind with no fields beyond the `id` and `bypass` every
effect has, so there is nothing to automate and the panel draws no controls.
Its output is its input, bit for bit, with no latency: a render of a song with
analyzers on a track, a group, a return and the master is the samples of the
song without them, and the chains report `latency_frames: 0`. Bypassed, it
measures nothing and its views are empty. It is added, moved, copied, saved in
a rack and removed as any effect is, by the person and by the agent. On the
master it reads the mix after `master_gain_db` and the effects before it, and
before the end fade. It is not offered inside a Synth's patch: the model refuses
one there, the patch's + menu does not list it, and the track's own chain takes
it. `daw describe effects` says all this under `analyzer`.

## The tap

An equalizer's tap is a ring of the last 4096 frames as mono, enough for one
spectrum and not for this: the stereo field needs both channels, and a loudness
that integrates needs every frame once, with none missed and none read twice.
The analyzer's ring (`aaw_dsp::meter::Ring`) holds the last 65 536 frames, both
channels packed into one atomic store a frame, with a count of frames written,
so a reader takes what has come since it last read and is told how many frames
it lost if it stayed away longer than the ring holds, about 1.4 s at 48 kHz.
The audio thread writes it after the device, one store a frame and no waiting,
and the rings live in the same `Taps` as the equalizers' taps, beside the
compiled program, so an edit elsewhere leaves a ring where it was and the
reader's count carries on. A bypassed or removed analyzer loses its ring at the
next compile.

## The measurements

They are worked out off the audio thread from the ring, in Rust
(`aaw_dsp::meter::Meter`), where a test can feed them a file; the app reads
them through `Song.analysis(effect)` once a frame drawn, however many views
show the same analyzer.

| View | Shows |
|---|---|
| Levels | Each channel's peak as a bar that falls at 20 dB a second, its RMS over 400 ms as a brighter bar inside it, the highest peak as a line held, and the held peak and true peak (four times oversampled, as a render's report and a limiter on a true peak read it) as numbers |
| Loudness | Momentary (400 ms), short-term (3 s) and integrated LUFS and the loudness range, by BS.1770-4 and EBU Tech 3342 as `daw listen` measures a render: the integrated loudness gated at −70 LUFS and 10 LU under the mean, the range the 10th to the 95th percentile of the short-term values gated at −70 and 20 LU under their mean; under the numbers the short-term loudness as a line over the last minute, with −14 LUFS marked and the integrated loudness as a line |
| Spectrum | The level in each bin of the last 4096 frames as mono, as under an equalizer's curve, filled to the floor, with the highest level of each bin held as a line over it |
| Stereo field | A vectorscope of the last 1024 frames, the left channel up the left diagonal and the right up the right so mono is a vertical line, drawn at the size of its loudest frame so the shape is seen at any level; the correlation over 400 ms from −1 to +1 and the balance of the channels' RMS in dB, each on a bar |
| Waveform | The last 20.48 s of each channel, the left over the right, newest at the right and scrolling left as the song plays: the lowest and highest sample of each hundredth of a second as a column, so a hit is a spike and a pad a band, with marks every five seconds back from now |
| Spectrogram | The same seconds with frequency up, 20 Hz to 20 kHz in equal steps of pitch as the spectrum is drawn across, and the level as the color from near black at −90 dB through blue and the modulation green to white at 0 dB: a spectrum of the last 4096 frames every fiftieth of a second as a column, each of 256 rows the loudest bin in its band or the level between the two bins around it where the bins are further apart than the rows; the frequencies the spectrum marks drawn across it |

Integrated loudness, the loudness range, the history and the held peaks start
again when the song is played from a stop, when the levels or the loudness are
clicked, and from the window's menu. When the song stops the peaks fall, the
stereo field stands still and the waveform and the spectrogram stop scrolling,
since no frames come; the integrated loudness holds.

The waveform's and the spectrogram's columns are made by the meter at their
fixed hops, not by the window at its frame rate, so time across them is honest
whatever the display does: a reading carries the count of columns made since
the app began watching and the last few of each, a view keeps the count it has
drawn and takes the columns past it, and frames the reader missed, because it
stayed away longer than the ring holds, pass as blank columns that are counted,
so a gap in the picture is a gap in time.

## The window

One window an analyzer, opened from its strip or the View menu, so two can sit
side by side. Its title names the row and the analyzer's id, `Analyzer · master
· mix`, with the song as its subtitle. The views are panes in three columns:
the levels over the loudness at the left, the spectrum over the stereo field in
the middle, and the waveform over the spectrogram at the right, the two that
scroll with time over each other so their seconds line up. The line between two
panes is dragged to size them, a column divider moving width between the two
columns either side of it and leaving the third; the mark at a pane's top right
fills the window with it and gives it back; a right click shows or hides each
pane, the last one staying, a column whose panes are all hidden giving its
width to the others. The window resizes freely from 480 by 300 points, goes
full screen with the green button as any Mac window does and can sit on another
display. It closes when its effect is removed or bypassed away, or its project
closes. Which panes show and how large they are belong to the app and are kept
between projects, as the detail panel's height is; they are not in the song, so
arranging them is not an edit, not an undo step and nothing the agent sees. An
arrangement saved by the first half, with two columns, reads as the standard
one.

## Drawing

The views are drawn at the display's rate while the window or the strip can be
seen, and nothing is measured for an analyzer nobody is looking at: the
measurement runs when a view asks for it, and the ring is all the audio thread
adds. The levels, loudness, spectrum and stereo field are Core Graphics paths
and rects; the vectorscope is one fill of 1024 small rects. The waveform and
the spectrogram are textures (`ScrollingTexture`): a bitmap of 2048 columns for
each channel of the waveform and 1024 for the spectrogram, a column written for
each the meter makes and the oldest overwritten, drawn each frame as one image
over the bitmap's own pixels in two slices so the newest column is at the right
edge, with no copy of the pixels and no path through the columns. The reading
itself is kept cheap too: the strip and the window drawing the same frame share
one reading, the labels and scales are laid out once and drawn each frame, and
the reading's long lists, the spectrum, its hold, the scope, the history and
the columns, cross from Rust as bytes in one copy rather than as lists the
generated bindings decode a value at a time, which had cost 6 ms a frame. On an
1100 by 580 window with all six panes a scripted `--measure` run drew a frame
in 2.0 ms and read the meter in 0.3 ms, a frame every 16.7 ms as the display has them.

## Scripted runs

`--key opt+cmd+l` opens the window of the analyzer the song's window shows,
`--analyzer-click X,Y` clicks in the first analyzer window that is open,
`--analyzer-drag X1,Y1,X2,Y2` drags in it, `--snapshot NAME.png` also writes
`NAME-analyzer.png` of each open analyzer window, and `--measure` adds
`analyzer` to its report, the window's draw times through the run, when one is
open.

## What it is not

No spectrogram in three dimensions and no saved arrangements of panes, which
are a Later line. No choice of the spectrogram's time span, its hop or its
color ramp, and no choice of the waveform's seconds: both show the last 20.48 s
at the meter's pace. No dialogue intelligibility and no surround, which Insight
has for film work. No analyzer the song does not hold: a meter for a row is an
effect on that row. The agent does not read the analyzer's live numbers; it
measures a saved render with `daw listen`.

## Open questions

- Whether the agent should read what the person is watching: a `daw` command
  that returns the loudness, peak and correlation of an analyzer as the song
  plays. The person said not now.
- Whether the arrangement of panes should be saved with the project, so a
  mastering song opens with its loudness history and a sound-design song with
  its spectrogram.
- Whether a new song's master should start with one, or the transport bar
  should carry a small level meter of the master that opens it.
- Whether the spectrum should offer the weighting and the averaging time
  other analyzers do, or one good setting is enough.
- Whether the loudness scale, −60 to 0 LUFS, and the level scale, −60 to +6
  dB, are the right ranges, or a mastering session wants a closer view.
- Whether twenty seconds is the right span for the waveform and the
  spectrogram, or a drag across them should zoom the time, and whether the
  spectrogram wants the range of its colors set, as Insight's does, for a
  quiet passage.
