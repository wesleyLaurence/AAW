# Concept: what AAW should be

This is the end state: the mission, the principles and everything the app should
do. It says nothing about what is built. [architecture.md](architecture.md) and
[completed.md](completed.md) say what exists, and [backlog.md](backlog.md) says
what is next. A change of direction is made here and its reason recorded in
[decisions.md](decisions.md).

## Mission

**Delegation without surrender.** A person hands an agent the mechanical and
exploratory work of making music and keeps everything that makes the music
theirs: the editable notes, clips, samples, routing and versions, and the final
judgment.

AAW is a digital audio workstation that a person and an agent operate together.
It feels like a normal DAW, with an arrangement of tracks and clips, a mixer,
device chains and a transport. The agent works in the same project with the same
tools: it adds tracks, moves clips, sets values and plays the song, and the person
sees and hears each change as it lands. Small things by hand, large things by
delegation, one project underneath.

It is not a generative music model. The composing intelligence is the language
model's; the DAW gives it hands, a workspace and a way to perceive what it made.

## Who it is for

A producer who works from samples and songs on a Mac, has a library of material
they are entitled to use, and wants to direct an agent the way they would direct
a capable assistant: find sounds, turn a break or a vocal into several directions,
make precise revisions, compare versions without losing the earlier one, and
finish.

It is not for someone who wants a finished song from a prompt, for recording
musicians and live performers, or for a mixer who depends on particular
third-party plugins.

## Principles

1. **The song is a plain text document.** The format is the interface: readable,
   diffable, in real units. While a project is open, one session host is the
   authority for it and the file follows every edit (D2, D36).
2. **Every change is a command.** From the window, the agent or a script, a
   change goes through the host, is validated against the whole song, is heard,
   lands in one undo history and carries its origin (D38).
3. **One engine plays and exports.** The same document always makes the same
   audio, and what the person hears while editing is what a render contains
   (D22, D33, D37).
4. **Real units and Ableton's words.** Hz, dB, ms, semitones and beats; session,
   track, clip, device, send, return. Models and producers already know them
   (D5, D9).
5. **Perception is for the reader, and it is not listening.** The agent cannot
   hear, so sound becomes numbers and images made for it to reason about. They
   are measurements; taste and acceptance are the person's (D10).
6. **Samples first, and the person's library is never changed.** Sounds are
   copied into a project; originals are only read (D11).
7. **Every step leaves something to hear.** The agent never works in silence
   (D13).
8. **Outputs are inputs.** Anything rendered can come back as a sample, with the
   recipe that made it (D14).
9. **Intent is layered memory.** The person's rules, the song's brief and learned
   taste are separate files with separate owners, and the agent can say which one
   drove a decision (D15, D16).
10. **Theory is a library, not a constraint.** A song declares no key. Scales,
    chords and voicings are tools the agent may consult (D6).
11. **Devices are built here, in the agent's terms.** Text patches, self-describing
    parameters, one implementation of each. Existing open source is used where it
    is clearly best: time stretch, stem separation, decoding (D3, D4).
12. **Any agent.** The agent's tools are `daw` and plain files, so nothing ties
    the app to one company's agent.
13. **Local first.** Editing, playing, rendering, indexing and analysis work with
    the network off. Nothing leaves the machine without the person choosing it.
14. **A feature earns its place by serving the shared work** of a person and an
    agent, not by matching another DAW.

## What a person can do with it

- Point it at a folder of samples and ask for "a dark punchy kick" without
  auditioning a thousand files.
- Give it a song or a sample and say "make a beat with this": it finds the tempo
  and downbeat, slices, loads a kit, programs patterns and builds an arrangement.
- Give it a finished song and timecodes and get an edit whose joins cannot be
  heard, exported under a name.
- Ask for a sound. It designs one, renders it and hands over the file, and a sound
  worth keeping goes to the library with its recipe.
- Program drums, bass, chords and melodies over samples and synths, with groove.
- Build effect chains, route sends to returns, sidechain, and automate any
  parameter across the song.
- Reach in at any moment: drag a clip, turn a knob, mute a track. The agent sees
  what changed and treats it as direction.
- Say "keep that" or "I don't like this part" and have it remembered.
- Say "make a song and don't stop until it is done" and come back to a finished,
  exported song with stems and a record of what was done.

## Opening the app

- **Launch opens a new Untitled project.** There is no welcome window. The
  arrangement is empty at 120 BPM in 4/4, and the person can add a track, drop a
  sample or ask the agent for something at once.
- **File menu:** New (⌘N), Open… (⌘O), Open Recent, Save As…, Close. Several
  projects can be open, each in its own window.
- **The word is "project".** The window, menus and alerts say project, as the
  command line does. "Song" stays the word for the music in it.

### Untitled is a real project

An Untitled project is a folder on disk from its first moment, kept out of sight
in the app's own data folder. The host saves after every edit, and the agent's
commands, imported samples and renders all go by the project's folder. "Unsaved"
therefore means "not yet named", never "only in memory", and nothing is lost if
the app quits.

| When | What happens |
|---|---|
| Save As… on an Untitled project | Asks for a name and a place, moves the folder there and sets the song's title. The undo history, the open conversation and anything playing carry on |
| Save As… on a project that has a name | Makes a copy under the new name and carries on in it. The original stays as it was |
| Closing an Untitled project nobody changed | It is deleted without a question |
| Closing an Untitled project with changes | Save…, Delete or Cancel |
| The app quit or crashed with an Untitled project open | The project is still in the app's data folder and is offered again |

An agent that was given the old path keeps working through a Save As…: the host
answers at the old path until it closes, and each reply names the new one.

## The window

```
┌──────────────────────────────────────────────────┬─────────────────┐
│ transport · position · tempo · agent activity    │  Agent panel    │
├─────────┬────────────────────────────────────────┤                 │
│ samples │ ruler and sections                     │  chat           │
│         ├──────────────┬─────────────────────────┤  turns          │
│         │ track headers│ clip lanes              │  tool activity  │
│         │              │                         │  keep · revert  │
│         │              │                         │  A/B            │
│         ├──────────────┴─────────────────────────┤                 │
│         │ detail: device chain, pads, pattern    │  (collapsible)  │
└─────────┴────────────────────────────────────────┴─────────────────┘
```

The arrangement is Ableton's Arrangement View: tracks stacked, clips left to
right, returns and the master at the bottom, automation lanes folding out under a
row. Clicking a row shows its devices and clicking a clip shows its pattern.
Device panels are drawn from what each device says of itself, so a new device
needs no panel built by hand. What the agent changes eases to its new place and
lights up for a moment, and every change is listed with who made it.

## A project on disk

A project is a plain folder with a project file in it, as an Ableton project is a
folder with an `.als` file:

```
My Beat/
  My Beat.aaw      the song, a readable text document
  SONG.md          what this song is, what was decided, where it stands
  skills/          skills for this project only
  samples/         every sound the song uses, copied in
  references/      tracks to match or learn from; never played in the song
  scripts/         one-off tools written for this song
  renders/         working renders; safe to delete
  exports/         deliverables the person keeps
  versions/        named snapshots
  .daw/            the project's threads, analysis output and scratch
```

- **`.aaw` is the app's own file type.** Double-clicking it opens the project,
  Finder shows the app's icon on it, and Open… offers only projects. The file is
  still readable text that the agent and Git can work with.
- **The folder stays a plain folder,** not a package Finder shows as one file, so
  the person and the agent can reach exports, renders and notes directly.
- **A project is self-contained.** Sample paths are relative to the folder and
  every sample is copied in, so the folder can be moved, synced or handed over
  whole.
- **A project can be kept anywhere.** Save As… opens at the workspace's
  `projects/` folder, which is only where it starts.
- **Every `daw` command takes the folder** as well as the file.
- **A project carries an ID** in its file, which is how it is recognized after it
  has moved.
- **Fixed places, free contents.** Every project has the same top-level folders
  and nobody invents new ones; anything can go inside `scripts/`. The tool creates
  the layout, and `daw check` flags files outside it. `SONG.md`, the song, the
  samples, references and exports are the person's; `renders/` and `.daw/` are
  caches.
- **Versions are a command.** A named snapshot is taken with one command, and
  snapshots are what comparing and branching work from.

## Where everything lives

| Place | Holds | Whose |
|---|---|---|
| `/Applications/AAW.app` | The app, the engine, `daw` and every library they need | The installed tool |
| `~/Library/Application Support/AAW/` | Running projects' sockets, the project index, the sample library's index, Untitled projects, settings | The app's own data, hidden |
| `~/Music/AAW/` (the workspace; movable) | The person's instructions, taste and skills for all their music, their own devices and saved sounds, and `projects/` | The person's |
| A project folder, anywhere | One song and everything about it | The person's |

The sample index belongs to the app, not to a folder above the project, so the
sample browser works wherever a project is kept.

## The project index

The app keeps a list of every project it has created, opened or saved, so that
neither the person nor the agent has to remember where things are.

- **`daw projects`** lists the open projects: path, title, whether it is still
  Untitled and which window is in front. **`daw projects --all`** lists every
  known project with when it was last opened. An agent in a terminal resolves a
  path once and keeps to it; no command follows the front window, because the
  person switching windows must not redirect an agent's edits.
- **Copy Project Path** in the app gives the person the path to hand to an agent.
- **A project that is moved** is found again, by a bookmark the index keeps for it
  and by the ID in its file.
- **A project that cannot be found** is listed as missing, and the app offers
  Locate….
- **A folder copied in Finder** gets a new ID the first time the copy is opened.

## The agent: one home, and each project

The agent works at two levels, and both are always in play.

| Level | Lives in | Holds |
|---|---|---|
| Everything the person makes | The workspace, `~/Music/AAW/` | How to use `daw` (kept current by the app), the person's rules, what the agent has learned of their taste, skills and devices usable in any project |
| This project | The project folder | `SONG.md`, skills for this project, this project's conversations |

```
~/Music/AAW/
  AGENTS.md             how to use daw and the conventions; managed by the app
  profile/
    instructions.md     the person's rules; the agent never edits them
    taste.md            what the agent has learned about the person, with evidence
  skills/               the person's skills, usable in any project
  devices/              the person's own effects and instruments
  library/              sounds the person keeps: bounces, saved chains, presets
  projects/             where Save As… starts
```

- **Precedence:** the person's rules, then the project's `SONG.md`, then taste,
  then defaults.
- **Skills load by description.** For every skill, in the workspace or in the
  project, the agent is given only the name and the description from the top of
  the file. It reads the body when the skill applies.
- **Memory belongs to the app's files,** not to one agent's private memory, so it
  survives a change of agent and the app can show it.
- **A project's files travel with it.** `SONG.md`, the project's skills and its
  thread list are in its folder.

Because projects can be anywhere, a project is not found by being under the
workspace. Either the agent is started in the project, as the panel does, or it
starts in the workspace and reaches projects by path through the index.

### SONG.md

The song's README: the first thing the agent reads, and the file it keeps current.
Every section has an owner, so the agent can update it without rewriting the
person's words.

| Section | Owner | Contents |
|---|---|---|
| Vision | The person; the agent suggests edits | The feel, style, references, what the song is for |
| Feedback | The agent, quoting the person, dated | What the person liked and disliked |
| Decisions | Both | Settled musical choices and why |
| Now | The agent | Where the song stands and what is next |
| Log | The agent | One line per session or version |

### Taste

Each entry in `taste.md` carries evidence and dates, and fades unless reconfirmed:

```markdown
- Keeps lead vocals dry and upfront. Evidence: asked to remove vocal reverb in two
  songs (2026-10-02, 2026-10-15).
```

- **The agent says what it learned** at the end of a session; it never adapts
  silently.
- **A hand edit is candidate evidence, not a preference.** It may be an experiment
  or a fix for one song. The agent asks whether it applies beyond this song before
  writing it down.
- **Evidence is structured, the summary is readable.** Keeps, rejections and
  feedback are stored as records, and `taste.md` is the bounded summary made from
  them.

## The agent in the window

- **One session per project, started in the project's folder.** The project's
  `SONG.md` and skills are the session's own, and the workspace is attached to it.
  The agent never has to be told which project it is in, including an Untitled
  one.
- **Threads belong to the project.** The panel lists the project's past
  conversations and resumes any of them. The list survives a move, and a Save As…
  in the middle of a conversation continues it in the new folder.
- **Each message carries what the agent would otherwise ask for:** the selection,
  the playhead and loop, and what the person changed since the agent's last turn.
- **Mentions:** `@track`, `@clip`, `@section`, `@skill`, `@reference`. A request
  typed with clips or a region selected applies to them.
- **Approvals are in the panel.** `daw` commands on the open project need none.
- **A terminal still works.** An agent started in the workspace can work across
  projects: reuse a sound from another song, compare two mixes, apply a saved
  chain.

### Working together

- **Turns.** A request such as "make the hats busier" is often a dozen commands.
  They are one turn, named for the request, that can be kept, reverted or compared
  as one. The song can be restored to before any turn.
- **A/B while looping.** One key flips between before and after a turn without
  stopping the loop.
- **Variations.** "Give me three hat patterns" makes three versions the person
  flips through on the loop and keeps one of.
- **Locks.** "Don't touch the drums" is a rule the host enforces.
- **Listening markers.** During playback a key drops a note at the playhead: "too
  busy", "love this". The agent gets feedback tied to beats.
- **How much the agent may do:**

  | Mode | The agent |
  |---|---|
  | Ask | Reads and answers; makes no edits |
  | Edit | Edits live, as turns the person can revert |
  | Propose | Edits a version the person auditions and merges |
  | Background | Works through a task list against the brief while the person is away |

## What the agent works with

The command line is the floor and it stays: it works with any agent, composes in
a shell and describes itself. It is not the ceiling.

| Interface | Good for |
|---|---|
| The document | Reading the whole state, diffs, review |
| Verbs (`daw set`, `daw clip move`) | Small targeted edits, working live, undo |
| Programs (a client library over the host) | Generating, varying and transforming across a song; each call is a host command and a block of them is one turn |
| Queries | Answering a question without loading the song: which bars the bass plays in, every snare off the backbeat |
| Views | An arrangement map of tracks by bars in a few hundred tokens; a text piano roll of one clip |
| Checks | A linter for music: a note cut off by its clip's end, two tracks in one register, a root note that disagrees with the audio |
| Perception | Measuring what an edit did: loudness, spectrum, stereo, energy by section, what changed between two renders, images on a bar grid |

The document should keep musical structure where it helps: a notation for each
kind of material (steps for drums, notes for melody, chord symbols), named motifs,
and perhaps clips defined by their relation to another. Every object has an
address that stays valid as other edits land, and every error says how to fix it.

## Sound

- **MIDI tracks and clips:** a MIDI track can hold editable note clips with no
  instrument attached. A sampler, synthesizer or another software instrument
  can be attached or replaced without rewriting the notes. A person can draw
  melodies, chords and rhythms, edit each note's pitch, start, duration and
  velocity, and move notes ahead of or behind the beat without quantizing them.
  Copying and pasting a clip makes an independently editable copy. The agent
  reads and edits the same notes through the shared command interface (D60).
  The first version stores numeric MIDI pitches alone; note names in the piano
  roll are derived, and explicit enharmonic spelling waits for evidence that
  it is needed (D61).
- **Sampler tracks and audio clips:** pads that play, hold and repitch samples,
  and parts of a file placed on beats with fades.
- **Instruments:** a subtractive synth with text patches and modulation (LFOs,
  envelopes, velocity, seeded randomness), enough to design kicks, basses, pads
  and leads from nothing. Glide and sustain loops on the sampler.
- **Effects:** filter, equalizer, compressor with sidechain, limiter, delay,
  reverb, saturation and a clipper, utility, chorus, phaser, gate, transient
  shaper, multiband compressor, pitch shift.
- **Routing:** sends and returns, groups, sidechains from any track or pad.
- **Automation** of any continuous parameter.
- **Time:** tempo and time signature changes, stretch at pitch, warp markers for
  material whose tempo drifts.
- **Audio operations:** tempo, downbeat and pitch detection, slicing a break into
  a kit with the pattern that replays it, stem separation, a beat map of a whole
  song.
- **Notes:** transformations (quantize, humanize with a seed, arpeggiate, vary),
  a theory library, MIDI file import and export, and a MIDI keyboard the person
  can play a riff on for the agent to build around.
- **Export** of a mix, stems and MIDI under a name, at a level policy, in the
  formats another DAW opens.

### Tools and devices the agent writes

| Tier | What it is | Heard live |
|---|---|---|
| One-off scripts | A script in the project's `scripts/`, whose result is imported with the recipe that made it | As a sample |
| Offline devices | A script used as an effect, re-run in the background when its input changes | From cache |
| Real-time devices | DSP written in a sandboxed language that the engine compiles and runs in playback and export alike | Yes |

A script that changes the arrangement issues commands through the host, never by
writing the file. A tool worth reusing moves to the workspace's `skills/` or
`devices/`. Effect chains, pad setups and whole tracks save as named text presets.

## Library and perception

- **The library index** covers the person's samples with measured pitch, tempo,
  kind, loudness and spectrum, the person's own tags, and search by words and
  filters. Search by description ("dark punchy kick") is added if it measurably
  beats names and measurements.
- **Promotion.** A render, slice, patch or pattern worth keeping goes to the
  library with where it came from and what made it.
- **A reference library.** Tracks the person points to as "this is what good
  sounds like", analyzed once and compared against every mix.
- **Perception reports** keep measurement apart from judgment: "kick and bass
  overlap heavily from 60 to 120 Hz" is an observation, and the agent proposes a
  reversible change to compare instead of optimizing a number.
- **An audio-capable model as a critic** is optional, beside the numbers, and
  needs its own consent.

## Skills

A skill is a plain-English file: a name, when to use it and the steps. A person
writes down their process once and runs it by name.

- **Captured from a session:** walk the agent through a process, then say "save
  that as a skill".
- **Chained:** "run my mix pass, then my master chain".
- **Built-in and personal.** The app ships craft skills, such as mixing to a
  reference, finishing a song and preparing stems, and updates them. The person's
  own take precedence.
- **Visible in the app:** a panel lists them and `@skill` runs one.
- **Shared:** skills, devices and presets can be packaged for other people.

## Long work on its own

"Make a song and don't stop until it is done" runs a producer's loop: read the
rules, taste and brief; plan sections; build one at a time; render; measure;
revise; master; export. It is bounded and resumable, and it keeps the best so far.
A definition of done, kept as a skill, is the checklist it grades against:
arrangement complete, no clipping, loudness in range, sections distinct, nothing
unresolved in the brief, stems and mix exported. "Technically complete" is not
"artistically finished"; that judgment stays the person's.

## Signing in

The end state is a button in the panel: **Sign in with Claude**, **Sign in with
ChatGPT**. The person picks the agent they already pay for, signs in on that
company's page and comes back to a working panel. They install nothing else and
never see a terminal.

That depends on the companies' terms as well as on this app. Until a sign-in is
approved, the app uses the agent the person has installed and signed in to
themselves, or an API key, and the panel says which step is missing.

## Installing

- **Like any Mac app:** a signed and notarized download dragged to Applications,
  with an icon in the Dock, that opens `.aaw` files and links `daw` onto the PATH
  so any terminal and any agent can run it.
- **Nothing outside the bundle.** The app and `daw` need no checkout, no Homebrew
  and no Python.
- **First launch** creates the workspace and offers to index a sample library.
- **Not the Mac App Store,** whose sandbox would keep `daw` from the app's socket
  and the app from projects in folders of the person's choosing.

## Privacy, rights and safety

- **What leaves the machine is the person's choice.** Choosing a remote agent
  sends it project text, reports and images; sending audio to a model is a
  separate consent. A project can list every outside service it used.
- **Where a sound came from is recorded,** and a sound can carry what the person
  declares about their right to use it. The app does not judge that.
- **File names, tags and imported text are data, never instructions.** Scripts
  and devices the agent writes run with limits on time, memory and output.
- **The licenses of everything shipped** are in the bundle.

## What it is not

- No recording of new audio. Input is samples, songs and files, what the agent
  synthesizes, and notes played on a keyboard.
- No clip launching or session view. The model is the arrangement.
- No hosting of third-party plugins, unless a need appears that cannot be met
  here.
- No generative audio model at the core. One may appear as an instrument for
  texture, never as the product.
- Not a replacement for every DAW. Stems and MIDI go out to the tools a person
  already uses.

## Open questions

1. Is the project file named for the project (`My Beat.aaw`, renamed with it) or
   fixed (`song.aaw`)?
2. On a project that already has a name, is Save As… a rename or a copy with a
   new ID? Probably both exist: Rename… and Save a Copy….
3. How are the workspace's and the project's skills handed to an agent that looks
   for skills in its own folders?
4. A conversation's transcript is kept by the agent, outside the project. Is the
   thread list enough to travel with the project, or should the transcript come
   too?
5. Do leftover Untitled projects appear in Open Recent, or only in an offer at
   launch?
6. Are versions snapshots in `versions/` or commits in a repository?
7. Should the undo history survive closing the project?
8. How much musical structure should the document carry, such as clips derived
   from other clips, against flat events plus the agent's programs?
9. Which language is the client library in?
10. Which format do real-time devices the agent writes use?
11. With no app to mark them, how does an agent in a terminal open and close a
    turn?
12. What do the agent companies' terms allow for sign-in from this app, and for
    driving a person's own installation?
