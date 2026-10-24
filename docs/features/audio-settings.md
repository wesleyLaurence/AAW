# Audio settings — implemented October 20, 2026

Asked for by the person on October 19, 2026 after an error: "my AirPods Pro
cannot play 44100 hz stereo audio. I think I made the song with my regular
speakers. I need to be able to support different audio sources", and "we need
an audio preferences settings panel that can change settings like the input
and output source, the sample rate, bit depth, all those kinds of things".
Built as D135; the proposal of October 19 is what this replaces.

## What

Settings › Audio (AAW › Settings…, ⌘,), the Audio pane: the output device, the input device,
the song's sample rate, the bit depth of exported files and the output
buffer, each a menu, with a line under the devices saying what plays through
what and at which rate. The song plays through whatever is chosen, at the
song's rate when the device offers it and at the device's own rate
otherwise, resampled in the player; the system's default is followed as it
changes, so AirPods connecting take the song over, and a chosen device that
is not connected gives way to the default and the tab says so. `daw audio-device
list` prints what the Mac has and `daw audio-device use` chooses for a running
host.

## How it works

- **The song's rate and the device's rate come apart.** `session.sample_rate`
  stays what the song is rendered and exported at. The output is opened at
  that rate when the device offers it, so what is heard is the render bit for
  bit, as before. A device that does not offer it, AirPods Pro at 48 kHz for
  a song at 44.1, is opened at the rate it runs at now, and the callback
  resamples the player's output to it with the engine's own polyphase sinc,
  `resample::Stream` in `aaw-dsp`, the same taps as the offline
  `resample_poly` run as phases over a ring of the input, allocating nothing
  after it is made. Its tests hold it to the offline resampler frame for
  frame at five rate pairs. The frames it holds ahead count in the output
  latency the playhead and the analyzer are corrected by, so the position
  stays on time. `status.audio` says `device_sample_rate` and `resampled`.
- **Which device.** The host keeps `Audio { output, input, buffer,
  device_rate }`: a device by part of its name, ignoring case, or None for the
  system's default. The app keeps the same three in its defaults
  (`audio.output`, `audio.input`, `audio.buffer`), gives them to each song's
  host as it opens and sends them to every open song when one changes; a
  terminal gives them to `daw play` and `daw host` with `--output`, and
  `AAW_OUTPUT_DEVICE` still stands in for a scripted run when nothing was
  chosen. `daw audio-device use PROJECT --output NAME --input NAME --buffer N` is
  the host's `audio` command, monitoring and never an edit; an open output is
  closed and opened again at the same beat, still playing if it was.
- **Following the Mac.** The engine listens, once a process, for the default
  output and input changing and for devices coming and going, and counts the
  changes. The host reads the count each tick: when the device the choice now
  means is not the one open, it opens that one at the same beat. A device
  chosen in the app that is not connected falls back to the default with a
  warning and `status.devices.fallback` naming it, and is taken up when it
  returns; one named on the terminal, by `--output` or `daw audio-device use`, or
  by `AAW_OUTPUT_DEVICE` is refused when it is not there, since a scripted
  run that must not be heard must never fall through to the speakers
  (`Audio.strict_output`).
  A stream the driver stops, as cpal does when a device changes its rate
  under it or goes away, is marked dead and opened again, at most once a
  second. A stream that moves its device to another rate is stopped by the
  late notification of that change, some 450 ms on, after a callback or two
  or none; the transport watches such a stream for 800 ms and builds it once
  more when it died, so a play or a take's tap is sent to the stream that
  lasts. A play the audio thread never took is still known to the control
  (`Control::play_unheard`), and the output opened again plays it from where
  it asked.
- **The input device** is kept and reported and opens nothing: recording
  ([recording](recording.md)) will open it.
- **Bit depth** is the export's: the menu sets the WAV depth File › Export
  Audio… offers, 24 or 16. Playback is 32-bit float; the device's own depth
  is the Mac's to set, in Audio MIDI Setup.
- **Sample rate** in the tab is the song's, 44.1 or 48 kHz, an edit of
  `session.sample_rate` of the front song, undoable; the line under the
  devices says whether the device plays it or resamples.
- **Buffer** is a menu of 64 to 2048 frames with its length in milliseconds
  at the device's rate; 128 to start, as the app and the terminal have
  always used, since nothing has shown it short.
- **The window** is AppKit holding a SwiftUI form, titled Settings with Audio
  as its subtitle; a second pane, the key's field and the budget of generated
  audio when built, makes it a toolbar of tabs.
  `--settings` on the app's command line opens it for a scripted run and
  `--snapshot NAME.png` then writes `NAME-settings.png`.

## Checked

- `aaw-dsp`: the stream resampler against `resample_poly` at 44.1↔48,
  48→96, 44.1↔192 kHz, pulled in random block sizes, within 1e-9.
- `aaw-engine`: through BlackHole, the virtual device on this Mac, a song at
  44.1 kHz opened at its own rate and the same song with the device opened
  at 48 kHz come back at 1000 Hz within 3 Hz and at the same level within
  3%; a song at 48 kHz opens at 48 kHz unchanged; the device list carries
  BlackHole's rates. Without BlackHole the tests say so and pass.
- `tests/test_host.py`: `daw audio-device list`; `daw audio-device use` on a running
  host, its refusals and that the revision stands; `daw play --output
  BlackHole --device-rate 48000` reporting `resampled` on a 44.1 kHz song,
  then at its own rate, skipped without the device. Each `daw` there has a
  two-minute limit, so a play that never starts fails the test instead of
  holding the suite.
- `aaw-engine/tests/player.rs`: a play sent and not yet taken is known with
  where it starts, and is not once taken or stopped.
- Swift tests: the defaults round trip, the menu keeping a device that is
  away, the words under the menus, the launch flag.
- Scripted pictures of the window only. Nothing was heard, no AirPods were
  connected, and no device was unplugged while a song played.

## Limits

- Open at the device's own rate, the output is not bit for bit the render:
  the resampler's passband is flat to 1e-3 and its stopband below -100 dB.
- cpal sets a device's nominal rate to the song's when it offers it, which
  moves the device for every app on the Mac, as before.
- The default input is followed in the count but nothing opens it yet.
