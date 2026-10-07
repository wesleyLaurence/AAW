# The live analyzer (implemented October 7, 2026)

Status: built in the pull request that closed the first of Next, decision D95
and D96, for the first of its two halves: the effect, the tap, the window and
its levels, loudness, spectrum and stereo field. The spectrogram and the
scrolling waveform are the next item in [backlog.md](../backlog.md). What is
said of iZotope Insight 2 is from its product pages and a review; nobody ran
it here.

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
short for a stereo field.

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
| Levels | Each channel's peak as a bar that falls at 20 dB a second, its RMS over 400 ms as a brighter bar inside it, the highest peak as a line held, and the held peak and true peak (four times oversampled, a 48-tap windowed sinc) as numbers |
| Loudness | Momentary (400 ms), short-term (3 s) and integrated LUFS and the loudness range, by BS.1770-4 and EBU Tech 3342 as `daw listen` measures a render: the integrated loudness gated at −70 LUFS and 10 LU under the mean, the range the 10th to the 95th percentile of the short-term values gated at −70 and 20 LU under their mean; under the numbers the short-term loudness as a line over the last minute, with −14 LUFS marked and the integrated loudness as a line |
| Spectrum | The level in each bin of the last 4096 frames as mono, as under an equalizer's curve, filled to the floor, with the highest level of each bin held as a line over it |
| Stereo field | A vectorscope of the last 1024 frames, the left channel up the left diagonal and the right up the right so mono is a vertical line, drawn at the size of its loudest frame so the shape is seen at any level; the correlation over 400 ms from −1 to +1 and the balance of the channels' RMS in dB, each on a bar |

Integrated loudness, the loudness range, the history and the held peaks start
again when the song is played from a stop, when the levels or the loudness are
clicked, and from the window's menu. When the song stops the peaks fall and the
stereo field stands still; the integrated loudness holds.

## The window

One window an analyzer, opened from its strip or the View menu, so two can sit
side by side. Its title names the row and the analyzer's id, `Analyzer · master
· mix`, with the song as its subtitle. The views are panes in two columns, the
levels over the loudness at the left and the spectrum over the stereo field at
the right: the line between two panes is dragged to size them, the mark at a
pane's top right fills the window with it and gives it back, and a right click
shows or hides each pane, the last one staying. The window resizes freely from
480 by 300 points, goes full screen with the green button as any Mac window does
and can sit on another display. It closes when its effect is removed or
bypassed away, or its project closes. Which panes show and how large they are
belong to the app and are kept between projects, as the detail panel's height
is; they are not in the song, so arranging them is not an edit, not an undo step
and nothing the agent sees.

## Drawing

The views are drawn at the display's rate while the window or the strip can be
seen, and nothing is measured for an analyzer nobody is looking at: the
measurement runs when a view asks for it, and the ring is all the audio thread
adds. Every pane is Core Graphics paths and rects; the vectorscope is one fill
of 1024 small rects.

## Scripted runs

`--key opt+cmd+l` opens the window of the analyzer the song's window shows,
`--analyzer-click X,Y` clicks in the first analyzer window that is open,
`--snapshot NAME.png` also writes `NAME-analyzer.png` of each open analyzer
window, and `--measure` adds `analyzer` to its report, the window's draw times
through the run, when one is open.

## What it is not

No spectrogram and no scrolling waveform yet, which are the next backlog item;
no spectrogram in three dimensions and no saved arrangements of panes, which
are a Later line. No dialogue intelligibility and no surround, which Insight has
for film work. No analyzer the song does not hold: a meter for a row is an
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
