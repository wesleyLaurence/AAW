# The live analyzer — proposed October 7, 2026

Status: proposed, not built. Backlog: Next, A live analyzer (D95). What is
said of iZotope Insight 2 is from its product pages and a review, read on
October 7, 2026; nobody ran it here.

## What

An effect that changes nothing and shows the sound that passes through it:
levels, loudness, a spectrum, a spectrogram, the stereo field and a waveform,
drawn as the song plays. It goes in any chain of a track, a group, a return
or the master, and where it sits says what it measures: before or after the
limiter, on the drum group or on the whole mix.

In the device panel it is a strip with the levels and the spectrum. A button
on the strip opens the analyzer's window, which holds every view, resizes,
goes full screen and stays open whatever is selected in the song's window.

```sh
daw effect add projects/demo master --type analyzer
```

## Why

The person mixes by ear and by eye. The app shows the sound in one place
today: the spectrum under an equalizer's curve, 400 points wide, for the row
whose devices the detail panel shows. Whether the low end is mono, how loud
the drop is against the verse, whether a limiter is flattening the mix and
where a resonance sits are read off a meter in every other DAW, and here they
wait for a render and `daw listen`.

A window of its own, because the detail panel shows one row's devices: an
analyzer on the master would go out of sight when a drum track is clicked,
which is when it is wanted. The panel is also 214 points tall at its least,
too short for a spectrogram or a stereo field.

## Design

**The effect.** `analyzer` is an effect kind with no fields beyond the `id`
and `bypass` every effect has. Its output is its input, bit for bit, with no
latency, so a render of a song with one is the samples of the song without
it; bypassed, it measures nothing and its views are empty. It is added, moved, copied, saved
in a rack and removed as any effect is, by the person and by the agent. On the
master it reads the mix after `master_gain_db` and the effects before it, and
before the end fade. It is not offered inside a Synth's patch, as an
equalizer's spectrum is not drawn there.

**The tap.** An equalizer's tap is a ring of the last 4096 frames as mono,
which is enough for one spectrum and not for this: the stereo field needs both
channels, and a waveform, a spectrogram and a loudness that scroll need every
frame once, with none missed and none read twice. The analyzer's tap is a
stereo ring of about a second with a count of the frames written, so a reader
takes what has come since it last read. It is written on the audio thread as
the equalizer's is, one store a frame and no waiting, and kept beside the
compiled program so an edit does not empty it.

**The measurements** are worked out off the audio thread from the tap, in
Rust, where a test can feed them a file:

| View | Shows |
|---|---|
| Levels | Each channel's peak, RMS and true peak, with the highest peak held until it is clicked |
| Loudness | Momentary (400 ms), short-term (3 s) and integrated LUFS and the loudness range, by BS.1770-4 as `daw listen` measures a render, with the short-term loudness as a line over the last minutes |
| Spectrum | The level in each band, as under an equalizer's curve, with the highest held as a second line |
| Spectrogram | Frequency up, time across, level as color, scrolling |
| Stereo field | A vectorscope of the two channels, the correlation from −1 to +1 and the balance |
| Waveform | The last seconds of each channel, scrolling |

Integrated loudness, the loudness range and the held peaks start again when
the song is played from a stop and when the view is clicked. When the song
stops the levels fall and the scrolling views stand still.

**The window.** One window an analyzer, opened from its strip, so two can sit
side by side. Its title names the row and the song. The views are panes in
it: a line between two panes is dragged to size them, a + on a pane makes it
fill the window and gives it back, and a menu shows or hides each pane, as
Insight lays out its meters. The window resizes freely, goes full screen with
the green button as any Mac window does, and can sit on another display. It
closes when its effect is removed or its project closes. Which panes show and
how large they are belong to the app and are kept between projects, as the
detail panel's height is; they are not in the song, so arranging them is not
an edit, not an undo step and nothing the agent sees.

**Drawing.** The views are drawn at the display's rate while the window or the
strip can be seen, and nothing is measured for an analyzer nobody is looking
at. The spectrogram and the vectorscope are the costly ones; they are drawn
into a texture, not as paths, and `--measure` holds them to the display's
rate.

**The agent.** It adds one with `daw effect add`, and `daw describe effects`
says that it changes nothing and what the window shows. `--snapshot` takes a
picture of the analyzer's window for a scripted run.

**What it is not, at first.** No spectrogram in three dimensions and no saved
arrangements of panes, which are a Later line. No dialogue intelligibility and
no surround, which Insight has for film work. No analyzer the song does not
hold: a meter for a row is an effect on that row.

## Done when

- A song with an analyzer on a track, a group, a return and the master
  renders to the same samples as the song without them, and its chains report
  no latency.
- Every frame played reaches a reader of the tap once, in order, through an
  edit of the song, held by an engine test.
- Fed a generated song's render, the integrated loudness is within 0.1 LU of
  `daw listen`'s, and the true peak within the engine's estimate; a mono
  signal reads a correlation of +1, the same with one channel inverted −1 and
  two unrelated noises near 0; a sine reads its level and its bin.
- The strip shows in the device panel, and its button opens the window; the
  window keeps showing the master's analyzer while a track is selected, shows
  six panes, one pane filling it, and its panes at two sizes, each seen in a
  scripted picture.
- With all six panes showing on a playing song, `--measure` reports drawing
  at the display's rate, and the audio thread's cost is the equalizer tap's.
- `daw effect add` makes one, and `daw describe effects` says what it is.

## Open questions

- Whether it is built as one pull request or two: the effect, the tap, the
  window, levels, loudness, spectrum and stereo field first, then the
  spectrogram and the waveform.
- Whether the agent should read what the person is watching: a `daw` command
  that returns the loudness, peak and correlation of an analyzer as the song
  plays. The agent measures only a saved render today.
- Whether the views should keep scrolling, empty, while the song is stopped,
  or stand still as proposed.
- Whether the arrangement of panes should be saved with the project, so a
  mastering song opens with its loudness history and a sound-design song with
  its spectrogram.
- Whether a new song's master should start with one, or the transport bar
  should carry a small level meter of the master that opens it.
- Whether the spectrum should offer the weighting and the averaging time
  other analyzers do, or one good setting is enough.
