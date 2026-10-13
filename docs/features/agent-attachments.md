# Files and clips in the conversation — proposed October 13, 2026

Status: proposed, not built. Backlog: Next, Files and clips in the
conversation. Builds into the field of [the agent panel's
look](agent-panel-look.md). What Claude Code takes in a message was read
from its documentation on October 13, 2026 and not yet tried from the
panel; see What was checked. Built on the panel's engine as it is, the
developer's own Claude Code, which a shipped app will not use (D114); the
image block and the attachments list are the message's, so they carry to
an engine run with an API key or to a harness of the app's own
([the agent's engine](agent-engine.md)).

## What

A file dragged into the panel, from the Finder, the browser or the
timeline, or pasted into the field, goes with the next message: an image
goes to the agent as a picture it sees; a sound, a MIDI file, a document or
anything else goes as its path, with what the app knows about it, which the
agent reads with the tools it has. A clip dragged from the timeline goes as
its reference. Each shows as a chip above the field before the message is
sent, and on the message afterwards. The person asked: "being able to drag
audio or midi clips into the chat as context for the agent to be able to
work directly with... maybe it can just link to that particular file path...
I could drop in an image or video too and it would link to the file Claude
code could then read as context."

## Why

Today the agent is pointed at a sound by its path typed in, or by selecting
a clip and hoping the context block says enough. A reference track, a
screenshot of a chain in another DAW, a `.mid` from a keyboard, a vocal
take: each is a file the person has in hand, and dropping it where the
conversation is, as in both chat apps, is the gesture they know.

## What was checked

| Read, October 13, 2026 | Says |
|---|---|
| The Agent SDK's streaming input page | A user message's `content` may be an array of blocks: `text` blocks and `image` blocks with `source: {type: base64, media_type, data}`; "attach images directly to messages for visual analysis". No other kind of block for a file; a missing or wrong image source is sent to the model as a text note and the session goes on |
| The Agent SDK's permissions | A read outside the working folder and the added directories asks; the panel's `Approval` sees the request |
| `AgentThreads.swift` | The panel sends `content` as one string today, and the transcript reader takes the text blocks of a user message and nothing else |

The model does not hear audio or watch video, so those go as paths, as the
person supposed.

## Design

**What can be dropped, and what it becomes.**

| Dropped | From | Goes to the agent as |
|---|---|---|
| An image (PNG, JPEG, GIF, WebP, HEIC, TIFF) | The Finder, or pasted (a screenshot) | An `image` block, as PNG or JPEG, scaled to 1568 points on its long side and under 5 MB, which is the API's limit; and its path in the attachments list, so the agent can read it again later |
| A sound (WAV, AIFF, MP3, M4A, FLAC, and what `afconvert` reads) | The Finder, the browser's samples | Its path; its duration, channels and rate from the app's decoder; and for a sample the browser knows, its ID in the library |
| A MIDI file | The Finder | Its path and its tracks' names and note counts, read as `--drop` reads a `.mid` today |
| A document (PDF, text, Markdown, YAML, JSON) | The Finder | Its path and size; the agent's Read tool reads PDFs and text |
| A video, or anything else | The Finder | Its path, size and kind, named and not read |
| A clip on the timeline | A drag from the arrangement (stage 2), or `+` › Attach Selected Clips | Its reference as `daw` takes it, `tracks.keys.clips.clip1` or `@3`, its track and bar, and for an audio clip its sample's path |
| A patch or a rack | The browser | Its name and the file's path |

**The chips.** Each attachment is a chip above the field: an icon by kind,
or a thumbnail for an image, the name, and `×`. The `+` in the field opens
a menu: Attach File…, which opens the file panel, and Attach Selected
Clips, gray with nothing selected. ⌘V with an image on the pasteboard
attaches it. A drag over the panel outlines the whole panel, "Add to the
message", and lands in the chips wherever it is let go, so the field need
not be hit.

**What the message carries.** The message's `content` becomes an array: the
`image` blocks first, then one `text` block, the app's context block with a
list under it between `<attachments>` tags, a line each with the kind, the
path and what the app knows, then the person's words. A clip's line has its
reference and, for an audio clip, the sample's path. The person's words
alone are shown in the panel, as today; the chips are drawn from the
attachments the panel kept for that message, and on a resumed thread from
the record (open question 2).

**What the agent is told.** The appended system prompt gains a paragraph:
an attached sound is read with `daw samples inspect`, `analyze` and
`beats`, measured with `daw listen` when it is a mix, and imported with
`daw samples import` before it is placed; a MIDI file is read and placed
with `daw midi`; a document with Read; an image is seen; a video is a path
and nothing in the toolbox reads it, so say so; a clip is read with `daw
get` and `daw clip`. The agent says what it read and did not hear.

**Reading it is allowed.** An attached path is outside the project more
often than not. `Approval` keeps the paths attached in the conversation
and allows a Read of one, and a `daw samples …`, `daw midi …`, `daw
listen`, `afinfo` or `file` whose arguments name one, even outside the
project; a write to it, or a copy of it anywhere but into the project by
`daw samples import`, still asks. Auto mode may allow the read itself
before the app is asked; either way no card for a file the person just
handed over.

**The file stays where it is.** Nothing is copied unasked; a sound the
agent places goes into the project through `daw samples import`, as any
sound does. A file moved or deleted afterwards leaves a chip whose path
the agent finds missing; the chip says so when it is clicked.

**Stage 2: a drag from the timeline.** The arrangement view moves clips by
tracking the mouse and starts no system drag today. A drag of selected
clips whose pointer leaves the arrangement's bounds toward the panel
becomes an `NSDraggingSession` carrying the clips' references under a
pasteboard type of the app's, and the move in the arrangement is cancelled;
the panel takes the type. This is a change to the arrangement's mouse
handling and is its own pull request after stage 1, which has Attach
Selected Clips in its place.

**Scripted runs and tests.** `--attach FILE` on the command line adds a
file to the next `--ask`, as `--drop` stands in for a drag; the stand-in
records the message it was given, and tests read the image block's size
and type, the attachments list, the chips in a picture, and `Approval`
allowing a read of an attached path and refusing a write. One `--ask` with
a small generated PNG and a generated WAV against the real Claude Code, on
a scratch project, read for whether it saw the picture and ran `daw samples
inspect` on the sound.

## Done when

- A screenshot pasted or dropped is seen: the agent describes what is in
  it.
- A WAV from the Finder and a sample from the browser each attach, and the
  agent reads it with `daw samples inspect` without a card and imports it
  when asked to place it.
- A `.mid` from the Finder attaches and the agent places it with `daw
  midi`.
- A selected clip attaches with `+`, and the agent reads it by reference.
- A video attaches and the agent says it cannot read it.
- The chips show on the sent message, and on the thread when the project
  is opened again.
- Stage 2: a clip dragged from the timeline into the panel attaches.
- The person has dropped a sound and a picture of their own and watched
  what the agent did with each.

## Open questions

1. **Copy into the project.** A dropped file that is later moved leaves the
   thread pointing at nothing. Whether a sound should be imported into the
   project at once rather than when it is placed, which puts files in the
   project the person may not keep.
2. **What the record keeps.** Whether Claude Code's transcript keeps an
   image block's data or a note in its place, and whether the attachments
   list survives the reader's stripping of the context block; the chips of
   a resumed thread depend on it. Read from a real record in building this.
3. **Large images.** Scaled to the API's limit here; whether to say so on
   the chip.
4. **A folder dropped.** Its path, and the agent lists it; or each file in
   it. Proposed as its path.
5. **Several images at once.** The API takes many a message; the panel
   takes up to ten and says so past that.
6. **A video's frames.** `afconvert` reads a video's sound and `ffmpeg`, if
   the person has it, a frame; whether the app should offer either.
