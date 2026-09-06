# The idea: an agentic DAW

## In one paragraph

A digital audio workstation built from first principles for an AI agent to operate. It has everything Ableton has that matters: sessions, tracks on an arrangement timeline, audio and MIDI clips, samplers and synthesizers, effect chains, sends and returns, per-track volume and pan, and automation of any parameter over time. It has none of the things Ableton has that only exist for a human with a mouse. A person describes a song, a sound, a beat built from a sample, or a direction, and an agent such as Claude Code or Codex builds it using the same primitives a producer uses, listens to the result through analysis, and iterates. It can work as a collaborator taking direction, or run on its own until a song is finished.

This is not a generative music model. The composition intelligence lives in the language model. The DAW gives it hands, a workspace, and a way to perceive what it made.

## Why not Suno, why not Ableton

Generative music models produce a finished waveform from a prompt. There are no tracks to adjust, no sample you chose, no bassline to rewrite, no automation to tweak. That is a vending machine, not an instrument.

Ableton has exactly the right primitives and the right mental model. But it was designed to be operated by hand. Its session file is an opaque blob, its plugins expose parameters as normalized floats with no meaning, its only feedback channel is a pair of ears, and every action is a click. An agent can be bolted onto it, but it will always be a tourist.

The goal is to give an agent its own Ableton: the same power, exposed in the way an agent works best, which is reading and writing text, running commands, and looping on feedback over long time horizons.

## First principles

These are the reframes the whole design rests on. Each one came from asking what a DAW is when the operator is an agent instead of a person.

**The session is a document, not a UI.** Ableton is a data model plus a realtime engine plus a UI. Only the data model matters to an agent. The project is a plain text document the agent edits like code. The format is the API. Every command is sugar over editing the document, plus the verbs that compute. When a visual workspace arrives, it is a second client of the same document: it reads the file and writes the file exactly as the agent does, and never holds state the agent cannot see. That is what lets a person and an agent work in one project without an integration layer between them.

**Rendering is a compiler, not an instrument.** Nearly all of the difficulty in a DAW is realtime audio: threads, latency, buffers, plugin GUIs. An agent does not play live. It edits, renders, analyzes, and edits again. So the renderer is offline and deterministic: the same document always produces the same audio. Unchanged tracks are cached. Bouncing is the fundamental operation, not playing. The person hears the project through a player over rendered audio, and editing feels responsive because small regions re-render quickly against the cache. A true realtime engine is a possible end-state addition and never constrains the core.

**Perception is designed for the reader.** A language model cannot hear, but it can read numbers and look at images. Mixing engineers already work heavily from meters and analyzers. The perception layer produces compact reports of loudness, spectral balance, stereo width, masking between tracks, transient character, and section structure, plus spectrograms drawn with bar gridlines and log frequency for a multimodal model to inspect. The most useful question is "what changed between these two renders," and numbers answer it more precisely than ears. Taste and feel come from the human. Audio-input models can be added later as an optional critic, not as the foundation.

**Devices are built in-house and AI-native.** Old VST plugins were designed for a screen. Their parameters are normalized floats, their presets are binary blobs, and their state is invisible. Instruments and effects here are built from scratch with real units (Hz, dB, ms, semitones, beats), text patches, self-describing parameter references the agent reads like a function signature, and modulation any parameter can receive. Existing open source is adopted only where it is clearly optimal and necessary: time stretching, stem separation, FFTs, resampling. Everything above that layer is ours.

**Theory is a library, not a constraint.** The session does not declare a key. Chords sit outside keys, songs change keys, modes exist, and the best music breaks rules. Scales, chords, modes, voicings, and key inference are tools the agent consults. Intent about tonality goes in the brief as prose.

**Every step leaves a listenable artifact.** The agent never works in silence. Each pattern, sound, section, stem, and mix lands as a named audio file the moment it exists, so the person can listen while the agent keeps going and interrupt with direction at any point. Stems are free because the track render cache is the stem export.

**Outputs are inputs.** Anything rendered can be reimported as a sample. A kick designed on the synth becomes a sample in a drum rack. A loop the person loves gets promoted to the library with the recipe that made it. Resampling is a core electronic technique and it is a one-liner here.

**Intent is layered memory.** Rules the person writes and the agent never edits. Preferences the agent learns with evidence and the person can prune. A per-song brief holding the idea, references, and what the person has said they like and dislike. The agent can always say which layer drove a decision.

## The four layers

1. **The document.** A project directory of text files: session globals, tracks, routing, arrangement, patterns, patches, automation. Small enough to hold in context. Diffs cleanly. Git is the undo history and the branching model for song versions.
2. **The renderer.** Turns the document into audio. Renders a clip, a track, a section, all stems, or the mix. Deterministic, seeded, cached per track, with draft and final quality.
3. **The perception layer.** Turns audio back into reports and images the agent can reason about, and compares renders against each other and against reference tracks.
4. **The agent harness.** Claude Code or Codex, with an instruction file defining the framework, skills encoding reusable craft (sidechain the bass to the kick, master to a loudness target, slice a break into a rack, finish a song), and the CLI as the toolset.

A fifth layer, the **workspace**, is the end state and is described next. It sits beside the harness as a second client of the document and the renderer, not on top of them.

## The end state: a shared workspace

The agentic loop is the priority and the point. DAW UIs already exist and another one is not groundbreaking. An agent that can work for hours with a DAW toolkit to make music or do audio engineering does not exist, and that is what this project is.

The end state, though, is a workspace where both operate side by side. A visual DAW in the style of Ableton, Logic, or Pro Tools where a person adjusts tracks, volume, panning, devices, and knobs and hears the result as they do it. The agent works in the same project at the same time. The person watches it build in real time, plays back what it has made as it goes, reaches in to fix something small with a few clicks instead of a prompt, then hands the work back. Small things by hand, large things by delegation, one project underneath.

This is designed in from the start rather than strapped on later, and the first principles above make it cheap. The document is the only source of truth, so the workspace is a viewer and editor of the same file. Renders are cached per track, so the workspace's player has stems to play for free. Devices describe themselves, so the workspace can generate their panels. The perception layer produces reports and images, so the workspace has better mix analysis than most DAWs ship with. Git is the history, so "go back to before you changed the drums" is a click.

The requirements the end state places on the early build are few and specific, and they are recorded in the spec and the decision log: the document has one canonical form so two writers never fight over formatting; every device is implemented exactly once so the person and the agent always hear identical audio; the CLI and the workspace's long-lived service are both thin shells over one core library; and the renderer can render a region around a playhead. None of these slow the agentic build down, and each is worth having even if the workspace never ships.

## Workflows this must support on day one

- Point the agent at a folder of samples. It indexes them, so "find me a dark punchy kick" works without listening to a thousand files.
- Give it a song or a sample and say "make a beat with this." It detects tempo and downbeat, separates stems if useful, slices, loads slices into a drum rack, programs MIDI, and builds an arrangement.
- Ask for a sound. It designs it on the sampler or synth, renders it, and hands you the file. If you like it, it is promoted to the library.
- Program drums, bass, chords, and melodies as MIDI over samples and synths, with groove and swing.
- Build effect chains, route sends to returns, sidechain, automate any parameter across the song.
- Render stems for every track and a mixdown at a loudness target.
- Say "make a song and don't stop until it is done," and have it plan an arrangement, build section by section, render, self-critique against a definition of done, and finish.
- Say "I don't like this part" or "keep that sound" at any moment, and have it remembered.

## What it is not

- No recording of new audio. Input is samples, songs, and files the person provides, plus what the agent synthesizes.
- No clip launching or session view. The mental model is the arrangement view: tracks, a timeline, clips.
- No realtime audio engine in version one. The person hears the project through playback of rendered audio and, in the workspace, through fast re-rendering of small regions. A callback-driven engine for playing instruments live is an end-state question, settled before effects are built.
- No hosting of third-party VST or AU plugins in version one. Reconsider only if a specific need cannot be met in-house.
- No generative audio model at the core. One may appear later as an instrument for texture, never as the product.

## The working rhythm

The person drives: they bring the idea, the source material, the references, and the taste. The agent plans, builds, renders, and critiques, and leaves a short journal of what it is doing and why. The person listens to renders as they appear and interrupts with direction. Once the workspace exists, the person can also reach into the project directly, and those hand edits are signals the agent reads. Every meaningful step is a git checkpoint, so "go back to before you changed the drums" is a real command and "try a version with a different bassline" is a branch. When the person walks away, the agent keeps working against the brief, the rules, and the definition of done until the song is finished.
