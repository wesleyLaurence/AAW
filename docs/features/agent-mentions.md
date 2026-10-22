# `@` mentions in the panel — implemented October 20, 2026

`@` typed in the field of [the agent panel](agent-panel.md) lists the song's
things over the list `/` opens for skills: the tracks, groups and returns,
the clips, the samples, the sections, the markers, the references kept in
the library and the project's files. The one chosen is `@name` in the
field, drawn as a chip wherever it stands, and each thing the words name
goes with the message as a line of the attachments list with the reference
`daw` takes, as a clip attached with `+` does ([files and clips in the
conversation](agent-attachments.md)). Asked for by the person (D123, the
first of the notes still open); the choices made in building it are D134.
Built on the panel's engine as it is, the developer's own Claude Code (D114);
the list is the app's and the lines are the message's, so an engine run
with an API key reads the same ([the agent's engine](agent-engine.md)).

## What

Typing `@` at the start of the field or after a space opens a list over
the conversation's foot of what the song and the project hold, in groups
under small headings, filtered as the person types: a name that starts
with the letters first, then a name that has them, then a line that has
them. ↑ and ↓ move, Return and Tab choose, a click chooses, Escape closes
the list and leaves the `@` typed, and the next key opens it again. The
one chosen replaces the `@` and the letters with `@name` and a space, the
caret after it, and `@name` is a chip in white on the person's blue, as a
skill's `/name` is; so is a name typed in full. Backspace at a chip's end
takes it whole; a caret put inside lands after it. The message sends with
the words as they read, `make @hook.2 quieter in @verse`, and a line for
each thing named, so the agent reads `tracks.drums`, `@12` or
`samples.kick` and works on that. The person's row draws the names on the
color, and so does a resumed conversation. The person asked for "`@` in
the field opens the song's names over the list `/` opens: tracks, clips,
samples, sections, markers, references and the project's files; the chosen
one goes with the message by the reference `daw` takes, as Attach Selected
Clips sends a clip."

## How it works

**What is listed** comes from the arrangement the window draws, the
library and the project's folder, read once a revision and again each time
the list opens, in `AgentMentions.candidates`:

| Group | Each entry | Goes to the agent as |
|---|---|---|
| Tracks | A track by its ID, with its kind, its instrument, how many clips it has, its group, muted or soloed; a group with what is in it; a return with its effects | `tracks.T`, `groups.G`, `returns.R` |
| Clips | Every clip on the timeline in time order: a note clip by its ID, a pattern clip by its pattern, an audio clip by its sample; where several share a name they are `hook.1`, `hook.2`…; with its track, bar, repeats, length and notes | `@N`, the host's reference |
| Samples | The song's `samples` by ID, placed or not, with the file, the root note and the tracks whose pads or audio clips play it | `samples.ID` |
| Sections | By ID, from its bar to its end | `sections.ID` |
| Markers | By ID, with its bar and its text, which the filter searches | `markers.ID` |
| References | The folders of `library/references` under the workspace `daw` uses, `AAW_WORKSPACE` or `~/Music/AAW` | The name, for `daw reference show` and `daw compare --reference` |
| Files | The project's files and folders two levels deep by their path within the folder, `samples/kick.wav`, sorted as the Finder sorts, `.daw`, `.daw.lock`, `.DS_Store`, the project's `.aaw` and the folder's icon left out, three hundred at the most | A file as a dropped one goes: an image as a picture the agent sees, a sound with its length, channels and rate, a folder with its count |

The song's samples reach the app through a new `samples` list on the
arrangement view (`SampleView`: ID, path, root note), since pads and
audio clips alone would miss a sample imported and not yet placed, which
is the one most worth naming.

**The list** is the view the skills use, now `PickList` in
`AgentPanelView.swift`, with rows of a monospace lead, a faint hint and a
gray line: `@hook.2`, the reference, and "pattern clip of hook on drums
at 2.1.1, 1 repeat". The skills' list keeps the first word: with `/`
first and no space the skills show and `@` does nothing until the skill is
chosen or the slash goes. The caret decides which `@`: the field tells the
panel where it is after each change and move, and `AgentMentions.query`
walks back from the caret to an `@` at the start or after a space with no
space between, so an `@` inside a word, as in an address, lists nothing.
Escape closes the list for this draft, as for `/`; a caret moved alone
does not open it again, the next key does.

**The chips.** `ComposerTextView` keeps the names that can be mentioned
and finds each `@name` in its text with `AgentMentions.found`: an `@` at
the start or after a space followed by the longest name that ends at a
word's end, so `@hook.2,` and `@kick.wav.` are found whole and a file with
spaces in its name too. The fill and the white are drawn as the skill's
chip is, as many as there are, and Backspace and the caret treat every
chip alike. A name edited so that it is no name is words again.

**What is sent.** `AgentPanel.send` reads the names in the words with
`AgentMentions.mentioned` and makes an attachment for each thing named,
once, after what `+` attached, a thing attached both ways once: a file
through `AgentAttachments.make(file:)` by its path, the rest as a line of
the kind and the reference, with the app's line of detail, in the
`<attachments>` list after the context. Two things with one name, a track
`kick` and a sample `kick`, both go, since the words say no more; the
prompt tells the agent to ask or say which it took. The attachments'
kinds gain `track`, `group`, `return`, `sample`, `section`, `marker` and
`reference`, each with a symbol for its chip. A line read back from the
record gives the name as it was mentioned, `drums` from `tracks.drums`,
and a clip's from its detail, so a resumed thread's chips read `lead1`
rather than `@12`, which the clip chips of `+` gain too.

**What the agent is told.** The appended system prompt says that a name
after `@` in the words is something in the song or the project with a line
of its own in the list, what each kind's reference is, to read it with
`daw get` and address it by that reference, and to ask or say which it
took where two lines share a name.

**The person's row** draws the words as an attributed string with each
`@name` of an attachment in white on the person's color, found by the same
rule; a path mention read from the record, `@samples/kick.wav` with the
line's name `kick.wav`, is found by its last part.

**Scripted runs.** `--type @` after a click in the field lists the song's
things in the picture, since the list is a view in the window; `--ask`
with `@name` in its text sends the thing named with the message.

## Scripted runs and tests

Tests hold, over pure functions, where the `@` the caret is in begins and
where it does not, where `@name` stands in a text with names of every
shape and the longest first, which things a text names and that two with
one name both go, the groups' order and the filter's ranks, the project's
files two deep with the app's own left out and the library's references by
`AAW_WORKSPACE` and by the home folder, and a mention's line written and
read back for every kind; over the text view with no window, the mention
chips' ranges, colors and rects, the caret put out of a chip and told, a
chip taken whole and a chip broken by an edit, both kinds of chip
together and the caret put where asked; and through the panel over the
stand-in, on a song of two pattern clips of one pattern, a note clip, two
samples, a section, a marker, a reference folder and files in the project:
every candidate with its detail and reference, `@` opening the list and
the letters filtering it, the keys' selection and its wrap, the chosen name
in the draft with the caret request after it, Escape and the caret moved
alone, the skills' list first, a message of seven mentions sent with the
words as typed and a line each in order with the right kinds, references,
paths and details, the chips on the person's row by their names, and a
clip attached with `+` and named too sent once. What was seen in pictures
is under What was tried.

## What was tried

Scripted runs only, from the package's debug build with no other AAW
open, on a scratch copy of a song made with `daw`: a `drums` track with
one pad on an imported kick, a pattern `hook` placed at bar 1 and bar 2, a
section `verse` over the first two bars, a marker "too busy" at bar 2, a
`Night Drive` folder under a scratch workspace's `library/references`,
and the project's own `SONG.md`; nothing by hand:

- Over the stand-in: a click in the field and `--type "make @ho"` showed
  the list over the conversation's foot with one heading, Clips, and two
  rows, `@hook.1 @2` and `@hook.2 @3` in monospace with the reference in
  the faint color and "pattern clip of hook on drums at 1.1.1, 1 repeat"
  under each, the first on the fill, in a picture. `--type "make @hoo"
  --key down --key return --type "quieter in @ver" --key return --type
  "and @kick"` left `make @hook.2 quieter in @verse and @kick` in the
  field with the three names as chips in the person's blue, `@kick` typed
  in full and never chosen among them, and the list still open under
  Samples with `@kick samples.kick` and under Files with
  `@samples/8577dfe0ad38_kick.wav sound`, in a picture.
- `--ask "make @hook.2 quieter in @verse, like @Night Drive, and read
  @SONG.md short"` sent the words as typed after the context and a list of
  four lines, `clip: @3`, `sections.verse`, `reference: Night Drive` and
  `document:` with the file's path and size, each with its detail, read
  from the stand-in's log; the person's row drew the four names on the
  color within the words and four chips under them with their symbols,
  and the thread's title read the words, in a picture.
- Against the person's own Claude Code, on a fresh copy of the song with
  no conversation yet: `--ask` with "say which clip @hook.2 is by reading
  it with daw get, what @verse covers, and the first line of @SONG.md,
  without changing anything". It ran `daw get . @3; daw get .
  sections.verse` in one command, answered that the clip named is `@3`,
  the second placement of the hook on drums at bar 2, that the verse
  covers bars 1 to 3, the first 8 beats, read SONG.md's first line and
  said it read rather than heard; in a picture. A first try on the copy
  the stand-in had used resumed that thread, which Claude Code had no
  record of, and stopped with `error_during_execution` before any turn:
  a run's artifact, not the mentions'.
- The field's text sits about 20 points under the well's top and the
  panel keeps the width it had, so the click that focuses the field was
  at (1000, 733) on a 1280 by 760 window; a click at (1120, 450) landed
  in the blank conversation and typed nothing. Nobody pressed a key by
  hand, chose with a click or moved the caret through a chip.

## Limits

- Two things with one name, a track and a sample both `kick`, both go
  with the message; the agent is told to ask or say which it took.
- A clip's name, `hook.2`, is its place among the clips of its pattern in
  time order, so it changes when a clip is added before it; the reference
  sent is the host's, which does not.
- A file with spaces in its name is listed and found whole once chosen,
  and can be typed by hand only with its exact name.
- The project's files are listed two levels deep and three hundred at the
  most; a deeper file is attached with `+` or dropped.
- A reference is listed by its folder's name under `library/references`;
  `daw reference list` is the agent's, and the panel does not run it.
- The skills' list comes first: `@` lists nothing while `/` with no space
  is the whole draft.
- The person's row draws a name on the color without the chip's rounded
  corners, as an attributed string's background.

## Open questions

1. **A name that is nothing in the song.** `@nothing` is a word, drawn
   plain and sent as typed; whether the field should say so.
2. **A thing chosen for a name two share.** The list shows both and the
   words keep one `@kick`; the kind chosen could be kept with the draft
   and sent alone.
3. **Mentions from the window.** A track's header or a clip dragged into
   the field as `@name`, as stage 2 of the attachments would drag a clip.
