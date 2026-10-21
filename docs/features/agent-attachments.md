# Files and clips in the conversation — implemented October 13, 2026

A file dropped, pasted or chosen in [the agent panel](agent-panel.md) goes
with the next message: an image as a picture the agent sees, anything
else by its path with what the app knows of it; a selected clip goes by
its reference. Asked for by the person (D113); the choices made in
building it are D117. Built on the panel's engine as it is, the
developer's own Claude Code, which a shipped app will not use (D114); the
image block and the list are the message's, so they carry to an engine
run with an API key or to a harness of the app's own
([the agent's engine](agent-engine.md)). Stage 2, a clip dragged from the
timeline, is not built: see Limits.

## What

A file dragged into the panel from the Finder or the browser, pasted
into the field, or chosen with `+` › Attach File…, is a chip above the
field until the message is sent, then a chip on the message. An image
goes to the agent as a picture it sees; a sound, a MIDI file, a document
or anything else goes as its path, with what the app knows about it,
which the agent reads with the tools it has. A clip selected on the
timeline attaches with `+` › Attach Selected Clips, by the reference `daw`
takes. The person asked: "being able to drag audio or midi clips into the
chat as context for the agent to be able to work directly with... maybe
it can just link to that particular file path... I could drop in an
image or video too and it would link to the file Claude code could then
read as context."

## How it works

**What can be attached, and what it becomes.** `AgentAttachments.swift`
reads each one; the kind is told by the extension.

| Attached | From | Goes to the agent as |
|---|---|---|
| An image (PNG, JPEG, GIF, WebP, HEIC, TIFF, BMP) | The Finder, or pasted | An `image` block, scaled to 1568 pixels on its long side and encoded as PNG, or as JPEG when the PNG is over 5 MB, the API's limits; and its path and pixels in the list, "320×200; seen" |
| A sound (WAV, AIFF, FLAC, MP3, M4A and the rest `AVAudioFile` reads) | The Finder, or a sample dragged from the browser | Its path; its length, channels and rate, "0.50 s, mono, 44.1 kHz"; for a sample from the browser its library ID, category, BPM and note too |
| A MIDI file | The Finder | Its path, its tracks with their names and its notes, "2 tracks: Piano, Bass; 3 notes", from a reader of the app's own, `MIDISummary` |
| A document (PDF, text, Markdown, YAML, JSON, CSV, RTF, HTML, XML, TOML) | The Finder | Its path and size |
| A folder | The Finder, or chosen | Its path and how many items it holds |
| A video, or anything else | The Finder | Its path, size and "not read" |
| A clip on the timeline | `+` › Attach Selected Clips, with clips selected | Its reference as `daw` takes it, `@12`; its kind, track and bar, a pattern clip's repeats, a note clip's length and notes, an audio clip's sample name |
| A patch or a rack | The browser | Its name and its file's path, or "factory" |
| A track, a group, a return, a clip, a sample, a section, a marker, a reference or a project file | `@name` in the words ([`@` mentions](agent-mentions.md)) | The reference `daw` takes, `tracks.drums`, `@12`, `samples.kick`, `sections.verse`, `markers.m1`, a reference's name; a file as a dropped one goes |

A pasted image has no file, so it is written as PNG under the project's
`.daw/attachments/`, named `pasted-DATE-TIME.png`, and attached by that
path, so that the agent can read it again and the record points at
something. Nothing else is copied: a sound the agent places goes into
the project through `daw samples import`, as any sound does.

**The chips.** Each attachment is a chip: a thumbnail for an image, else
a symbol by kind, the name, and `×` above the field. Chips wrap in rows.
A click shows the path, the reference and the detail in a popover, and
"This file is no longer there." when the file was moved or deleted. On
the sent message the chips are under the words in the bubble, without
`×`. The same file attaches once; ten images go with one message at the
most, and the eleventh is refused in the transport bar. Attachments with
nothing typed send as "Here is what I attached."

**Getting them in.** A drag over the panel outlines the whole panel in the
person's color with "Add to the message", and lands wherever it is let
go. The panel takes file URLs from the Finder; a sample's path from the
browser, which drags its path as text, with what the browser knows of the
sample it dragged; and the browser's patch and rack types. ⌘V in the field
attaches file URLs or an image on the pasteboard and pastes text as
text. `+`, at the left of the field and centered on its height (D132),
opens a menu: Attach File…, the open panel as a sheet, several files and
folders allowed; Attach Selected Clips, gray with none selected.

**What the message carries.** With nothing attached the message is the
string it was. With attachments, `content` is an array: the images as
`image` blocks with base64 data and their media type, then one `text`
block holding the app's context block, then the list between
`<attachments>` tags, a line each, `- kind: path or reference (detail)`,
then the person's words. A skill's message, `/name words`, has the
context and the list after the words. The panel shows the words alone
and draws the chips from the attachments it kept for that message; a
resumed thread draws them from the record, where the text block keeps
the list (open question 2, answered: see What was tried).

**What the agent is told.** The appended system prompt gains a
paragraph: what is listed between the tags, that an image among them is
in the message to see, that reading an attached file needs no approval,
how to read each kind, `daw samples inspect`, `analyze` and `beats` and
`daw listen` for a sound and `daw samples import` before it is placed,
`daw midi import` for a MIDI file, Read for a document, that a video is a
path nothing it has reads, `daw get` and `daw clip` for a clip by its
reference, and to say what it read and did not hear.

**Reading it is allowed.** The panel keeps the paths attached in the
conversation, from the chips of its messages and from the record of a
resumed thread. `Approval` allows a Read of one; a `daw` command, or `uv
run daw`, that names one, so `daw samples inspect` and `daw samples
import` work on a file in the Downloads folder; and a command of
`afinfo`, `file`, `ffprobe`, `mdls`, `xxd` or `strings` that names one
and nothing else outside the project. A folder attached covers the files
in it. Writing to the file, or copying it anywhere but into the project
by `daw samples import`, still asks, since `cp`, `sox` and the rest are
not `daw`. Claude Code's auto mode decides first, as for any request,
and may allow the read itself.

**Scripted runs.** `--attach FILE` attaches a file to the panel's next
message and `--attach-clips` the selected clips; `--ask TEXT` now sends
as the field does, so what was attached goes with it.

## Scripted runs and tests

The stand-in the tests speak through reads a message's `content` as a
string or as blocks. Tests hold a file read by its kind, an image scaled
to the limit and under its bytes, a sound's line, a MIDI file's tracks
and notes from a file written in the test with running status, a
document's size, a folder, a missing file, an image that does not read, a
pasted image written to the project with a second paste in the same
second named apart; the list written and read back, with and without
context, after a skill's words, and the content as a string or blocks;
through the panel, four files attached with one twice, a video taken
off, the selected clip attached once by reference, the message sent as
an image block whose data decodes to the file's pixels and a text block
with the context, the list and the words, the thread titled by the
words, the person's row with its chips, the attached paths allowed for
Read, `daw samples inspect` and `afinfo` and refused for `cp` and Write,
attachments alone sent with a word, the eleventh image refused, and the
chips and the paths read back from a record written as Claude Code
writes one; and the approval rule over attached files and folders.

## What was tried

Scripted runs only, from the package's debug build with another AAW in
front, on a scratch project outside the checkout, with a generated
320×200 PNG of a red and a blue half, half a second of a 60 Hz kick as a
mono WAV, and a MIDI file of two named tracks; nothing by hand:

- Over the stand-in: `--attach` of the three files, ⌘A and
  `--attach-clips` showed four chips above the field, the image as a
  thumbnail, the others with their symbols, wrapped onto a second row,
  and Send in the person's color with nothing typed, in a picture;
  `--ask "use these short"` after `--attach` and `--attach-clips` sent
  an image block of 2.4 KB and a text block with the context, the list
  and the words, read from the stand-in's log, and showed the bubble
  with the words and three chips under them.
- Against Claude Code 2.1.296: the three files attached and a message
  asking what is in the picture, to inspect the sound and to read the
  MIDI file without placing anything. It described the picture as two
  flat blocks, red left and blue right, ran `daw samples inspect` on the
  WAV and read its length, channels and rate back with the numbers it
  did not hear, read `daw midi import --help`, saw that it would place a
  clip, and parsed the MIDI file with a script of its own instead,
  listing the three notes by track; in a picture. Claude Code's record
  of the session keeps the user message as the image block with its
  data and the text block with the context, the list and the words, so
  a resumed thread's chips come from it; the thread opened again showed
  the conversation at its end. Whether the reads were allowed by Claude
  Code's auto mode or by the app's rule is not in the record. Nothing
  was dropped, pasted or chosen from the menu by hand, and no patch, rack
  or browser sample was dragged.

## Limits

- **A clip dragged from the timeline** (stage 2) is not built. The
  arrangement moves clips by tracking the mouse and starts no system
  drag; a drag whose pointer leaves the arrangement toward the panel
  would become an `NSDraggingSession` carrying the clips' references,
  with the move cancelled. It is a change to the arrangement's mouse
  handling, and its own pull request; Attach Selected Clips stands in.
- An audio clip's line names its sample, not the sample's file; the
  agent reads the file through `daw inspect` or `daw get`.
- A clip's chip on a resumed thread is named from its line's detail, "note
  clip lead1…", "pattern clip of hook…", "audio clip of sample kick…"; a
  line with none of these shows the reference.
- A `daw` command that writes into an attached folder, such as an
  export with `--to` under it, is allowed as any `daw` command naming an
  attached path is.
- A video's frames and sound are not read (open question 6).
- The browser's patch and rack drops are read through the pasteboard
  types the browser registers, and were not tried.
- A large image is scaled and the chip does not say so (open question
  3); the list's line gives the pixels sent.

## Open questions

1. **Copy into the project.** A dropped file that is later moved leaves
   the thread pointing at nothing; the chip says so when clicked. Whether
   a sound should be imported at once rather than when it is placed
   stays open; a pasted image is written to the project since it has no
   file of its own.
2. **A folder dropped** goes as its path with its item count, and the
   agent lists it; or each file in it.
3. **The pasted images** under `.daw/attachments/` are never removed; a
   thread deleted leaves them.
