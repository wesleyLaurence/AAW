# The agent panel's look — implemented October 13, 2026

How the conversation in [the agent panel](agent-panel.md) reads: the
person's words in a bubble, the agent's in blocks, its tools folded into a
row, a field that grows, the project's conversations in a popover where
they are renamed and deleted, and a panel whose width the person drags.
Asked for by the person after using the panel (D113); the choices made in
building it are D115. Built on the panel's engine as it is, the developer's
own Claude Code, which a shipped app will not use (D114); nothing here
assumes it, and nothing in the window is named or drawn as Claude's.

## What

Nothing the panel does changed; how it reads did. The three items after
this one, a skill chosen with `/` ([skill commands](skill-commands.md)),
files dropped in ([files and clips in the
conversation](agent-attachments.md)) and the microphone ([speaking to the
agent](voice-input.md)), all built since, each add something to the
field at the foot, so the field was rebuilt here first as an AppKit text
view they build into, with their two controls already in it, gray.

## The rows

| Row | Drawn as |
|---|---|
| The person's | A bubble: the text on a rounded fill in the person's color at 0.18 alpha, set in 36 points from the left, so the two are told apart by shape as in Claude's and ChatGPT's apps |
| The agent's | Blocks: paragraphs, headings a size up (17, 15, then 13 points), bullet and numbered lists with hanging indents and nested items set in, fenced code on a darker rounded fill in monospace with Copy at its corner, which shows a tick for a moment, quotes with a bar at their left, rules, and tables as their lines in monospace. Inline markdown, bold, italic, `code` and links, is read inside each block. A thin blinking bar in the agent's color stands at the end while the text streams |
| Tools | Two or more in a row, with no words between, are one line: "Running 3 commands" with the last one's command while they run, "Ran 3 commands" with a tick when all returned, a cross when one failed; "tools" in place of "commands" when any is not Bash. A click opens the line into its tools, each as a single tool is drawn. One tool stays one line: gray while it runs, a tick or a cross after, a click shows what it returned |
| Approval | The same card, the command on the code block's fill, Allow in the accent color |
| Notes | A centered gray line, as "Stopped." reads in both apps |

The conversation is at 13 points, the tools and the notes at 11, with 12
points between rows; the window's other panels stay as they are.

**The block reader.** `MarkdownBlocks` in `MarkdownBlocks.swift` cuts a
text into blocks by its lines: a fence of three or more backticks or tildes
to its close, a heading by its hashes, a list item by `-`, `*`, `+` or a
number with `.` or `)`, its depth from two spaces a level, a line indented
under an item going on with it, a quote by `>`, a rule, a table by `|`, a
blank line ending a paragraph, and the rest a paragraph with its lines
kept. It is pure and tested. A text that stops anywhere, as a streaming one
does, reads as what it is so far: a fence with no close is a code block
still open. The whole text is read again on each piece that streams in,
which is microseconds for a message of a few kilobytes; the design's
appending to the last block was not needed. Foundation's inline reader,
which drops block structure, reads the inline markdown of each block.

**Scrolling.** The panel follows the stream while the person is at the
bottom, or within 60 points of it, and stops following once they scroll
up, with a round "↓" pill at the foot to go back. A jump the panel made
itself, in the half second after it scrolled, is not read as the person
scrolling.

## The field

An AppKit text view, `ComposerTextView` in `AgentComposer.swift`, in a
rounded well: one line to start, growing with the text to eight lines and
scrolling past that, a placeholder while it is empty, no smart quotes or
dashes, since the person types commands. Return and Enter send; Shift-Return
starts a new line; Escape gives the keys back to the arrangement. A
composition under way with an input method takes Return itself. At the
well's right, `+` for attachments ([files and clips in the
conversation](agent-attachments.md), built after this), the microphone
([speaking to the agent](voice-input.md), built after this too), and
Send, gray until there is something to send and while the agent works. Under the well, in the faint color, "Return sends ·
Shift-Return starts a new line", until the conversation has a row.
Option-Return no longer breaks the line.

## The header and the conversations

The conversation's title at the left, Stop beside the spinner while the
agent works, then `+` for a new conversation and a clock. The clock opens a
popover of the project's conversations, most recent first, each with its
title and when it was last used, the one shown in bold on a fill, and a
`…` menu with Rename and Delete; a click on a row shows that conversation.
Rename opens a field in the row: Return keeps the name, cut to a title's
length, and an empty one is not taken. Delete takes the thread out of
`.daw/threads.json` and leaves Claude Code's own record alone; the ID goes
into a `removed` list in the same file, so that the record, which the
panel merges into the list when the project opens, does not bring the
thread back. A deleted thread that was the one shown leaves a new
conversation; deleting the one shown while the agent works is refused with
the reason, as changing conversations is.

## The width

A line between the arrangement and the column at the right, 5 points wide,
drags the column from 280 to 640 points, no further than leaves the
arrangement 400, and the width is kept in the app's defaults between
projects, as the detail panel's height is. The column is the agent panel
and the activity panel both, whichever shows. 320 to start.

## Scripted runs and tests

`--ask` and `AAW_AGENT` are as they were. The stand-in the tests speak
through gains a message, "blocks", answered with a heading, a list, a fence
and three `daw` commands in a row, each allowed by the panel, and a last
word. Tests hold the block reader to its blocks, the folding of tools to
runs, a thread renamed and deleted with the `removed` list keeping it out
of a merge, and the field's keys and its height over the text view with no
window. What was seen in pictures is under What was tried.

## What was tried

Scripted runs only, on a blank project in a scratch folder with the
stand-in in Claude Code's place; nothing by hand, and no real session:

- `--ask blocks`: the person's bubble, the heading, the paragraph, the
  list with its bold, the code block with Copy, the three commands folded
  into "Ran 3 commands" with a tick and the last command beside it, and
  "Done." under them, in a picture; the field's hint gone once the
  conversation had rows.
- A click in the field, `--type`, `--key shift+return` and `--type` again:
  two lines in the field, grown by a line, Send in the person's color, in
  a picture.
- `--drag` from (957, 300) to (857, 300) on a window 1280 wide, then a
  message typed and `--key return`: the column 420 points wide in a
  picture, read back from `side.width` in the app's defaults, the message
  in the stand-in's log with the context ahead of it, and in the picture
  the note centered, "Running 2 commands" with the dotted mark while the
  card for `touch x` waited, the command on the code fill and Allow in
  the accent color.
- Nothing scrolled, so the pill was never seen; no popover was opened,
  since it is a window of its own; nothing was copied; no real session.

## Limits

- Tables are drawn as their lines in monospace, and images in the agent's
  text as links; neither has been sent by the agent yet.
- The person's bubble takes plain text: markdown they type is shown as
  typed.
- A folded row opens and closes as a whole; which tools' results are open
  is not kept when the conversation is drawn again.
- Delete leaves Claude Code's record of the thread where it was; the
  panel does not move it to the Trash.
- The popover is a window of its own, which a scripted picture of the
  window does not show.

## Open questions

1. **Bubbles for the person only, or both.** The person's only, as both
   apps have it; easy to change.
2. **13 points.** A size up from the window's controls. The person reads
   it and says.
3. **Tool results open by default.** Folded; a setting if wanted.
4. **Delete,** which Claude Code's own record keeps. Whether Delete should
   say so, or also move the record's file to the Trash.
5. **0.18 for the bubble.** The design said a tenth, which was too faint on
   the panel's gray to read as a bubble at all; the person says whether
   this is too much.
