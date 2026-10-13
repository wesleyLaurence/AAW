# The agent panel's look — proposed October 13, 2026

Status: proposed, not built. Backlog: Next, The agent panel's look. The
panel it changes is [the agent panel](agent-panel.md), built October 12,
2026 as PR #104. The person asked for this after using it: "more polished
stylistically, more on par with how Claude desktop or ChatGPT works. Don't
need to copy exactly but using that as a goal." Built on the panel's engine
as it is, the developer's own Claude Code, which a shipped app will not use
(D114); the protocol is the same for the engines that will, so nothing here
assumes it ([the agent's engine](agent-engine.md)).

## What

The conversation in the panel reads like a conversation in Claude's or
ChatGPT's own apps: the person's words in a bubble, the agent's words as
proper text with headings, lists and code blocks, the tools it ran folded
into one quiet line that opens, a field at the foot that grows as the person
types and holds the controls the next three items add, and a panel whose
width the person sets. Nothing the panel does changes; how it reads does.

## Why

The panel is where the person and the agent meet, and the person said it
does not yet feel like a place to talk. The three items after this one, a
skill chosen with `/`, files dropped in and the microphone, each add
something to the field at the foot, so the field is rebuilt here first and
they build into it.

## Design

**The rows.** Two chips, `you` and `agent`, label every row today. They go.

| Row | Today | Proposed |
|---|---|---|
| The person's | A chip and plain text | A bubble: the text on a rounded fill in the person's color at a tenth, set in from the left so the agent's text and the person's are told apart by shape, as in both apps; attachments as chips over the text ([attachments](agent-attachments.md)) |
| The agent's | Inline markdown only; lists and code as typed | The text in blocks: paragraphs, headings a size up, bullet and numbered lists with hanging indents, fenced code on a darker rounded block in monospace with a Copy button, inline code in monospace, links that open. A thin cursor at the end while it streams |
| Tools | One row a tool, gray while it runs, a tick or a cross after, a click shows the result | Consecutive tools folded into one row, "Ran 3 commands" with the last one's text while they run, a tick or a cross for the run when they return; a click opens the row into its tools, each a line with its result under it as today. One tool stays one row |
| Approval | A card with the command and two buttons | The same card, the command on the code block's fill, Allow in the accent color |
| Notes | Italic gray text | A centered gray line, as "Stopped." reads in both apps |

**The text's blocks.** SwiftUI's `AttributedString(markdown:)` reads inline
markdown and drops block structure, which is why lists and code show as
typed. The agent's text is cut into blocks first, by a small reader of
paragraphs, headings, list items and fences, and each block is a view, its
inline markdown read as today. Tables are drawn as code until they are
wanted. The reader is pure and tested; the stream appends to the last block
without re-reading the ones before it.

**Type.** The conversation at 13 points, the tools and notes at 11, as both
apps run a size up from an app's controls; the window's other panels stay as
they are. Rows get 12 points between them, and a message of the agent's that
follows its own tools has the tools folded above it, not between its
paragraphs.

**Scrolling.** The panel follows the stream while the person is at the
bottom and stops following once they scroll up, with a round "↓" pill at the
foot to go back; today every new row scrolls, under the person's reading.

**The field.** An AppKit text view in place of the SwiftUI field, in a
rounded well: it grows to eight lines, holds a chip for a skill and the
words spoken into it, and takes a pasted image. In the well, at its right,
the controls: `+` for attachments, the microphone, and Send, which is gray
until there is something to send. Return sends and Shift-Return starts a
new line, as both apps have it, in place of Option-Return; Escape gives the
keys back. A hint under the well, in the faint color, says the two keys
once, until the first message.

**The header.** The conversation's title at the left as now, and at the
right `+` for a new conversation and a clock mark that lists the project's
conversations in a popover with each one's title and when it was last used,
Rename and Delete in each row's menu; Delete takes the thread out of
`.daw/threads.json` and leaves Claude Code's record alone. Stop stays
beside the spinner while the agent works. Deleting a thread is a limit of
the panel today.

**The width.** The divider between the panel and the arrangement drags
from 280 to 640 points, and the width is kept as the panel's showing is.
Whether 320 is right is the first question under Verify; this lets the
person answer it.

**Dark only.** The window is always dark (`Theme.swift`), so the panel's
fills are shades of the window's grays with the person's and the agent's
colors at low alpha; no light theme.

**Scripted runs and tests.** `--ask` and `AAW_AGENT` stay; the stand-in's
lines gain a message with a list, a heading and a fence, and three tools in
a row, seen in a picture. The block reader and the folding of tools are
tested as values; the field's keys through `--key`.

## Done when

- The person's words are in a bubble and the agent's are text in blocks:
  a list, a heading and a fenced block from a real session read as such,
  and Copy on a block copies its text.
- Three `daw` commands in a row are one folded row that opens.
- The panel stops following the stream once the person scrolls up, and
  the pill brings them back.
- The field grows to eight lines, Return sends and Shift-Return breaks the
  line, and the three controls sit in it, the two not yet built gray.
- A conversation is renamed and deleted from the popover.
- The divider drags, and the width comes back when the project is opened
  again.
- The person has used it for a session and said whether it reads like a
  conversation.

## Open questions

1. **Bubbles for the person only, or both.** Claude's app gives the person
   a bubble and the agent none; ChatGPT the same. Proposed as theirs, and
   easy to change.
2. **13 points.** A size up from the window's controls, in a panel that is
   a quarter of it. The person reads it and says.
3. **Tool results open by default.** The last of the first questions under
   Verify. Folded here; a setting if wanted.
4. **Delete,** which Claude Code's own record keeps. Whether Delete should
   say so, or also move the record's file to the Trash.
5. **Markdown tables and images in the agent's text.** Drawn as code and as
   links until the agent sends them.
