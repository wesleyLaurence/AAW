# Song edits from timecodes: what is missing — October 1, 2026

An analysis of the codebase against one use case, and the features the repository
needs before an agent can carry it out end to end. It lists what to add; it is not
a build plan with milestones. Each feature is built on its own branch, and the
table under [What is missing](#what-is-missing) says which are done.

## The use case

A person is sent a finished song and a short request such as "edit from 0:41 to
1:43, take out the eight beats around 1:20 to 1:28, keep it under a minute". The
agent should:

1. Import the song, which arrives as a purchased `.m4a` (AAC).
2. Find the strong beat, usually a downbeat, nearest each timecode. The times in a
   request are approximate, often a second off.
3. Cut the song at those beats and join the pieces so the beat never slips and the
   joins cannot be heard: short crossfades, no clicks, no audible fade.
4. When the result is a few seconds too long, speed it up slightly without changing
   its pitch.
5. Put a one-shot at the very start and the very end, a swell into the ending, and
   sometimes a highpass sweep over the last beats.
6. Check the result (length, beat across each join, clicks, level) and export a
   named file to hand over.

The process itself is personal: who sends the request, which sounds mark the start
and end, the length limit, the file naming. That belongs in a skill file the person
writes in plain English and keeps out of Git (see [Skills](#7-personal-skills-and-assets)).
The repository's part is to give the agent every capability such a skill needs.

## What works today

Checked by building this edit from generated audio: a 48-second, 120 BPM test song
at 44.1 kHz whose first downbeat is at 0.137 s, two regions of it joined at a
downbeat, a noise burst at each end, a swell, a highpass lane and a master limiter.

| Step | Today | How |
|---|---|---|
| Play part of a long file | Works | A pad with `start_seconds` and `end_seconds`, played by a pattern event |
| Place it to the sample | Works | An event's `at` takes decimals and fractions; frames are rounded once from exact beats |
| Join two regions | Works, by hand | Two pads of the same sample, overlapped by the fade length, with `release_ms` on the first and `attack_ms` on the second |
| One-shots at the start and end | Works | `daw samples import PATH` takes a file path as well as an index ID |
| Highpass sweep | Works | A `filter` in `highpass` mode with a `cutoff_hz` lane; frequencies move in equal ratios per beat, which is an exponential sweep in hertz |
| Level safety | Works | A master `limiter`; a render refuses to clip |
| Length in seconds | Works | `daw inspect` and the render report give `duration_seconds` |
| Mix and stems | Works | `daw render` |

In the rendered test every beat of the stitched track was within 0.16 ms of the
grid, including the beats either side of the join, and the join had no larger
sample-to-sample step than the audio around it. So the engine can already produce
the edit. What it lacks is everything that tells the agent where to cut, the two
kinds of audio processing the use case needs, and a way to express the edit that a
person can see and adjust.

## What is missing

| # | Feature | Without it | Needed for | Status |
|---|---|---|---|---|
| 1 | Decode `.m4a` and `.mp3` on import | The song cannot be loaded at all | Every edit | Done, October 1, 2026 |
| 2 | Beat and downbeat map of a whole song | The agent cannot find where to cut | Every edit | Done, October 1, 2026 |
| 3 | Pitch-preserving time stretch | "Speed it up slightly" changes the pitch | Edits that run long | Done, October 1, 2026 |
| 4 | Audio regions and crossfades as first-class objects | Each cut is hand arithmetic across pads and events; the person cannot adjust it in the app | Reliability, and the app | Schema, engine and commands done, October 1, 2026; the app is not |
| 5 | Join and length checks | Nothing measures whether a join is on the beat or clicks | Trusting the result | Done, October 1, 2026 |
| 6 | Export of a named deliverable | Only `renders/<id>/mix.wav`, 24-bit WAV | Handing the file over | Done, October 1, 2026 |
| 7 | Personal skills and assets | No place for a skill that Git ignores; nothing documents how to write one | Running the process by name | Done, October 1, 2026 |
| 8 | Smaller gaps | See below | Convenience | Three of six done, October 1, 2026 |

Features 1, 2, 5, 6 and 7 are enough for a request that needs no speed change,
using today's pads for the regions. Feature 3 adds the speed change. Feature 4
replaces the hand arithmetic and gives the Mac app something to draw and drag.

### 1. Decoding compressed audio on import

**Done, October 1, 2026.** `daw samples import` decodes `.m4a` and `.mp3` into the
project as 32-bit float WAV, with `afconvert` or `ffmpeg`, and records the
original's hash as `source_sha256`. It refuses a file the engine cannot play, and
`daw check` warns of one already in a song. The Mac app takes the same files
dropped from the Finder. See "Format v1" in [mvp.md](mvp.md) and D48 in
[decisions.md](decisions.md). Checked on generated audio encoded here; no
purchased file and no drop onto the app's window was tried. What follows is the
analysis as it was written.

Today:

- The library indexes `.wav`, `.aif`, `.aiff` and `.flac` only (`EXTENSIONS` in
  `src/agent_daw/library.py`).
- The engine decodes with libsndfile (`aaw-engine/src/sndfile.rs`), which has no AAC
  decoder. `daw samples inspect song.m4a` fails with "Format not recognised".
- `daw samples import song.m4a` nevertheless succeeds: `import_asset` copies any
  file without opening it. `daw check` then reports the song valid with no
  warnings, and the failure appears only at `daw render`.

Needed:

- Import decodes a compressed file once into a lossless asset in the project
  (`samples/HASH_name.wav` or FLAC) and records the original's path and hash as the
  source. Everything after that reads PCM, so positions are exact and the audio is
  lossy-encoded only as many times as it already was.
- A decoder. On macOS, AudioToolbox (the `afconvert` tool ships with the system)
  reads AAC and removes the encoder's padding. `ffmpeg` is the portable choice
  where it is installed. Either runs as a separate process, as Python does now.
- Import refuses a file the engine cannot decode, so the error arrives at import
  and not at render.
- `.mp3` handled the same way. The libsndfile build in use lists MP3, but decoding
  to PCM at import keeps one path.

Limits to state in the documentation: purchased iTunes Store files are plain AAC
and decode; files downloaded through an Apple Music subscription are
copy-protected and cannot be read. A decoded AAC file can exceed full scale by a
fraction of a decibel, which matters for feature 6.

### 2. Beat and downbeat map of a whole song

**Done, October 1, 2026.** `daw samples beats FILE` measures the whole file: the
tempo and how far the song strays from it, every beat with its bar and beat
number, the downbeat with a confidence for each of the four places, and phrase
changes. `--near 0:41` lists the beats around a time, `--click` writes an audition
with a click on each beat, and `--bpm` and `--downbeat` correct the map, which is
kept beside the audio as `NAME.beats.json`. It is a numpy tracker; see
[beat-map.md](beat-map.md) and D49 in [decisions.md](decisions.md). On generated
songs every beat is within 0.1 ms, and on five local renders within 0.4 ms on four
and 8 ms on one. No commercially released song was measured, and nobody listened
to a click audition. The beat map is not yet drawn in the app, which is part of
feature 4. Open questions 1 and 4 were answered by default: steady tempo first,
with slow drift followed, and numpy first. What follows is the analysis as it was
written.

Today (`src/agent_daw/analysis.py`, [sample-analysis.md](sample-analysis.md)):

- Only the first 120 seconds are analysed (`RHYTHM_SECONDS`).
- A tempo is reported only when the file is judged a loop, as one number for the
  whole file, anchored on the first onset. No beat times are returned, and the
  first 64 onsets are all that is listed.
- Onset times fall on a 512-frame hop, about 12 ms at 44.1 kHz. On the test song
  the first beat was reported 9 ms early and the tempo as 119.98 against a true
  120.00, an error that grows to 10 ms over a minute. An edit for dancing needs
  about 1 ms.
- Downbeats, bars and pickups are not detected; 4/4 and a steady tempo are assumed.
  `docs/mvp.md` lists downbeat detection as deferred.

Needed:

- A command, such as `daw samples beats FILE`, that analyses the whole file and
  returns every beat's time in seconds, its bar and beat number, the tempo, and how
  far the tempo drifts.
- Beat times refined to the transient, to about a millisecond, by looking at the
  audio around each tracked beat at a fine hop.
- Downbeats with a confidence and the alternatives. Which of four beats is "one"
  is the least certain part of any tracker, so the report gives each candidate's
  score and the agent can ask when they are close.
- A lookup for a timecode: `--near 0:41` returns the beats and downbeats within a
  few seconds of it, each with its strength. This is the question a request
  actually poses.
- Phrase hints: where the energy or texture changes, so that among nearby
  downbeats the one that starts a phrase can be preferred, and so that "take out
  eight beats" removes a whole phrase.
- The map stored with the sample (the spec's clip `warp` block recorded source
  tempo and downbeat for the same reason), so the engine, the checks and the app
  share one grid, and cached as other analysis is.
- A way to hear it: an audition of the song with a click on each beat and an
  accent on each downbeat, so the person can confirm the grid in a few seconds.
  The agent measures; it does not listen (AGENTS.md).

Choices to make: a tracker written with numpy and scipy keeps the present rule
that analysis needs no model, and is likely good enough for music made to a click.
A learned tracker finds downbeats more reliably on harder material but brings a
large dependency, and some published models are licensed for non-commercial use
only. Songs with a drifting tempo need per-beat times rather than one tempo, which
the output above already gives.

### 3. Pitch-preserving time stretch

**Done, October 1, 2026.** A pad has `stretch: repitch` (the default) or
`preserve_pitch`; with `source_bpm` set to the song's tempo, raising
`session.tempo` plays the song faster at its own pitch and leaves one-shots alone.
It is done where repitching is, when a pad's audio is prepared. Open question 2
was answered by the person: both stretchers, with Signalsmith Stretch (MIT) linked
in as the default and Rubber Band run as the installed program when
`session.stretcher` names it. Each region is stretched with a margin either side.
`daw timeline --tempo-for SECONDS` turns a target length into a tempo, and
`daw check` warns past about 8%. A render's report names the stretcher and its
version when the song stretches. See [time-stretch.md](time-stretch.md) and D53 in
[decisions.md](decisions.md). Timing was measured for each candidate; nobody has
listened to a stretched render, which is what the choice between the two is for.
What follows is the analysis as it was written.

Today: the only way to change a sample's speed is bandlimited repitch. A pad's
`source_bpm` multiplies its playback speed by session tempo over source tempo
(`Sampler::job` in `aaw-engine/src/program.rs`), and pitch moves with it. The
render report says so under `pitch_policy`, and `docs/mvp.md` lists time stretching
as deferred.

Needed:

- A stretch mode on the pad or region: `repitch` (today's behavior, the default)
  or `preserve_pitch`. With it, `source_bpm` set to the song's tempo, and the
  session tempo raised by a few percent, the song plays faster at its own pitch
  while one-shots without `source_bpm` are untouched. "Five seconds shorter"
  becomes a change of session tempo.
- The stretch belongs where repitch is now: `Job::make` prepares each pad's audio
  once, off the audio thread, and caches it by a key that already includes the
  speed. Stretching there needs no real-time stretcher and keeps playback equal to
  the render.
- A helper that turns a target length into a tempo, and a `daw check` warning when
  the ratio is large enough to be heard. A few percent is the normal range.

Choices to make:

- **The stretcher.** D4 names Rubber Band, whose finer engine is the open-source
  reference for full mixes. It is GPL with a commercial license available, and this
  repository is MIT, so linking it into `daw` would change what the app can be
  distributed under. Running the `rubberband` program as a separate process avoids
  linking and adds an install step. Signalsmith Stretch (MIT) and Bungee (MPL) can
  be linked. The choice should be made by ear on real songs.
- **What is stretched.** Stretching each region with some audio either side keeps
  artifacts away from the joins. Stretching the whole file once is simpler but the
  beat map must then be scaled or measured again, since a stretcher does not
  promise that each beat lands exactly where the ratio says.
- **Determinism.** Render identities include dependencies; an external stretcher's
  version has to be part of that.

### 4. Audio regions and crossfades

**Schema, engine and commands done, October 1, 2026; the Mac app's part is not.**
The person answered open question 3: audio clips in the schema now, the app
later. A track has `audio`, a list of audio clips: the sample, the beat, the
source range, a lead before the beat, gain, fades with an equal-power or linear
curve, and the tempo and stretch mode of feature 3. `daw audio add`, `cut`,
`split`, `trim` and `crossfade` work on them; `cut` removes a range, closes the
gap and crossfades the join. A clip that ends where another begins leaves from
where that one starts, so a join is at one place before the beat. `daw joins`,
`daw timeline` and `daw inspect` read them, and `daw describe edit` and the
`song-edit` skill build edits from them. They are `tracks[].audio` and not
inside `clips`, as the sketch below had them. See
[audio-clips.md](audio-clips.md) and D54 in [decisions.md](decisions.md). Not
built: region edges and fades that drag in the app, and the beat map on the
waveform; the app plays audio clips and does not draw them. Nobody has listened
to an edit. What follows is the analysis as it was written.

Today a region of a file is a pad, and a pad is placed by a pattern event inside a
clip. For this use case that means:

- One pad for each region, since `start_seconds` and `end_seconds` belong to the
  pad and an event has no offset into the sample, plus a pattern and a clip to
  carry the events.
- A region's source position is in seconds on the pad and its place on the
  timeline is in beats on the event, so every cut is arithmetic across both.
- A crossfade is two regions overlapped by hand: the first extended past the cut
  by the fade length, the second started that much early and its event moved
  earlier to match.
- The voice envelope is linear in and out (`Voice::envelope`). Two linear fades sum
  to constant level for material that is alike across the join, and dip by up to
  3 dB where it is not. There is no equal-power fade.
- No command splits, trims or removes a range and closes the gap.
- The Mac app draws the waveform but cannot move a region's edges or fades:
  dragging a clip's end changes its repeats, and a pad's trim is set only with
  `daw pad set`.

Needed:

- An audio clip on a track, as [spec.md](spec.md) described before the sampler MVP
  narrowed clips to patterns: the sample, its place on the timeline, the source
  range, gain, fade in and out with a curve, and the stretch mode of feature 3.
  A sketch, not a schema:

  ```yaml
  clips:
  - {audio: song, at: 4, source_start_seconds: 41.213, source_end_seconds: 79.840,
     fade_in_ms: 5, fade_out_ms: 12, fade_curve: equal_power}
  ```

- Commands over it: split at a beat, trim an edge, crossfade two neighbors, and
  remove a range and close the gap. The last is the request "take out the eight
  beats around 1:20".
- A place for the cut a few milliseconds before the beat's transient rather than
  on it, so the incoming hit is whole and the fade happens in the quiet before it.
- In the app: region edges and fades that drag, and the beat map of feature 2
  drawn on the waveform.

A smaller first step, if the schema should not change yet, is a command that
writes today's pads and events for a list of regions and fades. It removes the
arithmetic but gives the app nothing to show, so it is a stopgap.

### 5. Join and length checks

**Done, October 1, 2026.** `daw joins RENDER` reports every join of a rendered
edit and the file's length against `--limit`. For each join: whether the beat
carries across it, from the song's beat map placed through both parts and again
from the transients in the track's stem; how many beats of the song it skips; the
step in the waveform at the splice; the level either side; flags in words; and an
excerpt of the mix to hear. It is a command of its own and not part of
`daw listen`. A join is found from today's pads (two parts of one sample meeting
on a track) and will be read from audio clips when feature 4 adds them. See
[join-checks.md](join-checks.md) and D51 in [decisions.md](decisions.md). Checked
on edits of a generated song; nobody listened to an excerpt. What follows is the
analysis as it was written.

Today `daw listen` measures loudness, peaks, spectrum, stereo and energy per beat.
[perception.md](perception.md) says it does not infer tempo or transients. The
figures quoted under "What works today" came from a script written for the test.

Needed, as part of `daw listen` or beside it, for a render whose song has regions:

- For each join: the interval between the beats either side against the tempo, in
  milliseconds; a click measure (the largest step at the join against the level
  around it); and the short-term level before and after.
- The total length against a target, such as under 60 seconds, with everything
  included.
- A short audio excerpt around each join, written for the person to hear, since
  whether a join is natural is a listening judgment.

### 6. Export of a deliverable

**Done, October 1, 2026.** `daw export PROJECT --to exports/NAME.EXT` writes the
latest full render, rendering first if the song has changed, as 16- or 24-bit WAV,
AAC (`afconvert` or `ffmpeg`) or MP3 (`ffmpeg` or `lame`). The level is as
rendered, or one gain: `--gain`, `--peak`, `--lufs` or `--match SAMPLE`, held
under `--ceiling`, and the result says which applied and by how much. A limiter is
the song's master effect, not the export's. `NAME.EXT.json` beside the file
records the render it came from. See [export.md](export.md) and D50 in
[decisions.md](decisions.md). Open question 6 was answered by default with those
four formats. Nobody listened to an export. What follows is the analysis as it was
written.

Today `daw render` writes `renders/<id>/mix.wav` as 24-bit PCM with stems, a
snapshot and a report. The directory is named by a hash, nothing is normalized, and
a mix that would clip is refused.

Needed:

- `daw export PROJECT --to exports/NAME.EXT`: a named file in a folder the person
  keeps (`exports/` in [design-notes-2026-09-30.md](design-notes-2026-09-30.md)),
  made from a render and recording which one.
- Formats people are sent: 16- or 24-bit WAV, and AAC or MP3 through the system's
  encoder or an installed one.
- A level policy for edits of mastered songs. The source is already at full scale,
  so at 0 dB with a one-shot on top the mix clips and the render refuses. The
  choices are a small trim, a limiter, or matching the source's loudness, and the
  export should say which was applied and by how much.

### 7. Personal skills and assets

**Done, October 1, 2026.** A personal skill lives in `.claude/skills/NAME/`, which
Git ignores apart from the generic skills `.gitignore` names; the person's own
skills folder works too. `.claude/skills/song-edit/` is the tracked, generic skill
for this kind of edit, and a personal skill refers to it and holds only what is
personal. `daw describe` has the topics `edit`, `beats`, `joins` and `export`,
with importing under `samples` in `project`. AGENTS.md names personal skills in
its Git scope and points to the skill and the topics. Reusable sounds live under
`content/` and are imported by path. See [skills.md](skills.md) and D52 in
[decisions.md](decisions.md). The generic skill was run once by hand on a
generated song; no personal skill was written or run, and the skill still builds
parts from pads and has no speed change, until features 4 and 3. What follows is
the analysis as it was written.

Today nothing in the repository mentions skills except the design notes. There is
no `.claude/` directory, `.gitignore` covers `/projects/` and `/content/` but not
skill files, and the Git scope in AGENTS.md does not name them. A skill written to
`.claude/skills/NAME/SKILL.md` would load in Claude Code and would also show as an
untracked file that `git add -A` commits.

Needed:

- A decision on where a personal skill lives. In the checkout,
  `.claude/skills/NAME/SKILL.md`, ignored by Git. Outside it, the person's own
  skills folder, which works from any directory once `daw` is on the PATH (the Mac
  app's menu links it). Later, the workspace's `skills/` folder of
  design-notes-2026-09-30.md §3.
- Ignore rules that keep personal skills out while letting the repository ship
  generic ones: ignore `.claude/skills/` except named, tracked examples.
- AGENTS.md: personal skills are personal content like songs; a skill names `daw`
  commands and never an absolute library path.
- A tracked, generic example skill for this kind of edit: import, beat map,
  regions, joins, checks, export. A personal skill then holds only what is
  personal, and refers to the generic one for the mechanics.
- A place for the sounds a skill reuses, such as the one-shots at the start and
  end: a folder under ignored `content/`, imported by path into each project, or
  scanned as a small library of its own.
- A `daw describe` topic for each feature above, so a skill can say what to do and
  leave how to the authoring contract, as AGENTS.md already does for the sampler,
  effects and automation.

### 8. Smaller gaps

**Three done, October 1, 2026**, as one command, `daw timeline`
([timeline.md](timeline.md)): positions in seconds (`--seconds` and `--beats`
convert for a song), end-aligned placement (`--end-at BEAT --pad TRACK.PAD` gives
the start) and session length (`--fit`, with `--tail`). Commands still take beats;
the command converts rather than every command accepting `m:ss`. A fitted session
does not end before its last clip, so a long pattern holds it. Automation curves
with a shape, memory and other time signatures are not done. What follows is the
analysis as it was written.

- **Positions in seconds.** Commands take beats only. Accepting `m:ss.mmm` where a
  position is expected, or a command that converts between the two for a song,
  saves arithmetic in every request.
- **End-aligned placement.** A swell has to end on a beat. Today the agent
  subtracts the sample's length from the target and writes the start.
- **Session length.** `length_beats` is set by hand to cover the last sound; a
  command that fits it to the content, with room for a tail, would do.
- **Automation curves.** A lane segment is `linear` or `hold`. Frequency lanes
  already move exponentially in hertz. A sweep that starts slowly and finishes
  fast needs several points today; a curve with a shape parameter would need two.
- **Memory.** A decoded file is held whole as 64-bit floats, about 170 MB for a
  four-minute stereo song, with a copy of each region. Acceptable now, worth
  knowing when songs are long or many.
- **Time signature.** The model is 4/4 only, and a beat map would assume it.

## Open questions

1. Is the first version for songs made to a click, with a steady tempo, or must it
   handle drifting tempo from the start?
2. Which stretcher, judged by ear at a few percent on real songs, and is an
   installed program acceptable or must it be built in?
3. Audio clips in the schema now, or the stopgap command over pads first?
4. Should a numpy and scipy beat tracker be tried first, with a learned one as an
   optional extra if downbeats prove unreliable?
5. Where do personal skills live for now: ignored in the checkout, or in the
   person's own skills folder?
6. Which export formats are actually asked for?
