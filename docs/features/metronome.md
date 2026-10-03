# Playback metronome — implemented 2026-10-03

Status: implemented.

## What

A metronome icon in the transport bar toggles a click during playback, including
an empty project. It starts off each time a project is opened.

## Why

The person needs to hear the beat while starting a song and setting its tempo.

## Design

The host owns a per-open-project transport setting, outside the song and undo
history. The app sends a transport command and follows the host's reported state.
The audio player mixes a short synthesized click after the song's processing,
using the audible timeline and current tempo, with a higher click on each 4/4
downbeat. The setting survives stop, locate, looping and live edits. It does not
start playback itself. The offline renderer and stems never include the click.
Toggling fades its level over 5 ms; the audio callback allocates nothing.

## Done when

Engine tests verify timing at fractional tempos, loops, locate, stop, tempo
changes, block independence and allocation safety. Host tests verify the setting
does not edit the song. The app's icon and on/off state are checked on a scratch
project and the release build succeeds.

## Open questions

None. This is a playback click, with no count-in or adjustable volume.
