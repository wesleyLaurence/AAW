# Audio settings — proposed October 19, 2026

Status: proposed, not built. Backlog: Next, Audio settings. Asked for by the
person on October 19, 2026 after an error: "my AirPods Pro cannot play 44100 hz
stereo audio. I think I made the song with my regular speakers. I need to be
able to support different audio sources", and "we need an audio preferences
settings panel that can change settings like the input and output source, the
sample rate, bit depth, all those kinds of things". What is said here about
the engine was read from the checkout on October 19, 2026.

## What

A Settings window, Audio tab: the output device, the input device, the sample
rate and the bit depth, each a menu of what the Mac offers, kept by the app
and read by the engine when it opens the device. The song plays through
whatever is chosen, at the device's rate, whatever rate the song was made at.
A device unplugged or a pair of AirPods connected mid-session is followed
without an error.

## Why

The engine plays through the system's default output device, or the one
`AAW_OUTPUT_DEVICE` names for scripted runs, and opens it at the song's
`sample_rate`, 44100 or 48000 ([architecture](../architecture.md)). AirPods
Pro take 48 kHz, so a song at 44.1 kHz was refused with the error the person
saw; the same will meet any interface whose rate is fixed. There is no place in
the window to choose a device, and recording ([recording](recording.md)) needs
an input chosen before anything is armed.

## Design

- **The song's rate and the device's rate come apart.** `session.sample_rate`
  stays the rate the song is rendered and exported at, the rate its samples are
  prepared at. The player resamples its output to the device's rate when they
  differ, with the engine's own resampler (the `resample_poly` port in
  `aaw-dsp`), a block at a time, as the sampler already resamples sources.
  The latency the resampler adds is reported with the device's, so the
  position and the analyzer stay on time. A device whose rate matches runs
  as now, bit for bit.
- **Which device.** The output and the input are kept by name in the app's
  defaults, "System Default" the first value, and passed to the engine as
  `AAW_OUTPUT_DEVICE` is today, through the host rather than the environment.
  A chosen device that is not there at launch falls back to the default and
  the tab says so. CoreAudio's default-device change notification is listened
  to, so AirPods connecting switches the output as every Mac app does when the
  default is chosen.
- **Bit depth.** The device is opened at float where CoreAudio allows it,
  which is every Mac device, so the choice is about the export: the tab shows
  the export's bit depth as File › Export Audio… already asks it, and the
  device's own is shown and not chosen.
- **Buffer size.** A menu of 64 to 2048 frames, 256 to start, the latency
  shown beside it in milliseconds; a smaller buffer for playing a MIDI
  keyboard (Next) and recording, a larger one for a heavy song.
- **The window.** Settings… under the app menu (⌘,), a tabbed window; the
  Audio tab here, the key's field and the budget of generated audio (Next) in
  another once built. Built in SwiftUI with AppKit where a tab needs it, as the
  panels are.
- **The engine.** A `Device` message on the host: set output, set input, set
  buffer size; `daw device list` prints what the Mac offers with each one's
  rates and channels, so the agent can say what to pick.

## Done when

- A song at 44.1 kHz plays through a device opened at 48 kHz, in a test that
  runs the player into a virtual device at 48 kHz and reads a tone's frequency
  and level back, and at 44.1 kHz into one at 44.1 kHz unchanged.
- Settings › Audio lists this Mac's devices, chooses one, and the choice is
  kept across launches and read by the engine.
- The default output followed when the system's changes, in a scripted run
  that changes the default with `SwitchAudioSource` or the like.
- `daw device list` prints the devices.

## Open questions

1. Whether to resample in the player, as proposed, or to re-prepare the song
   at the device's rate, which is what a change of `session.sample_rate` does
   today and would be slower and exact.
2. Whether the Settings window belongs to this item or to generated audio's
   stage 3, which also wants it; whichever is built first makes it.
3. Whether the input device belongs here before recording exists, or arrives
   with it. Here, so the tab is whole.
