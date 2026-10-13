# The agent panel — implemented October 12, 2026

A conversation with the agent in the project's window, over the person's own
Claude Code. What it should feel like is in
[concept.md](../concept.md#the-agent-in-the-window); what a session is given
by location is [agent-context.md](agent-context.md). The choices made in
building it are D111. What was checked about Claude Code is at the end, with
what nobody has tried.

## What

Each project's window has a panel at its right, shown with View › Agent
(⌥⌘C) or the speech mark in the transport bar, with the project's
conversations in a menu at its top, the conversation shown as it streams,
each tool the agent runs with what it returned, each request the agent makes
that the person answers, and a field to type in at the foot. The agent is the
person's own Claude Code, started in the project's folder, so it has what a
terminal session there has: the project's `SONG.md` and skills, and the
workspace's instructions, taste and skills. Its `daw` commands on the project
land in the window, in the agent's color, as a terminal's do. Two projects
open in two windows each have their own agent, and both work at once.

## How it works

**One process per running conversation.** Sending a message starts Claude
Code in the project's folder with

```
claude -p --input-format stream-json --output-format stream-json --verbose
    --include-partial-messages --permission-prompt-tool stdio
    --session-id ID | --resume ID
    --add-dir WORKSPACE --append-system-prompt TEXT
```

and keeps it running between turns, each message a line of JSON on its input
and each event a line on its output. The process goes when the conversation
changes, the project closes or, after Save As…, at the next message, which
resumes the same conversation in the new folder. `AgentProcess` in
`AgentSession.swift` runs it; `AgentPanel` reads its events.

**Finding Claude Code.** The app asks the person's login shell where `claude`
is and what its PATH is, once, since an app opened from the Finder has a bare
PATH and Claude Code is usually installed by a version manager the shell sets
up. Failing that it looks in the usual places. The agent's own commands get
that PATH with the bundle's `Helpers` folder ahead of it, so `daw` is the
app's own. `AAW_AGENT` names a program to run in Claude Code's place, for a
scripted run or a test.

**What the agent is told.** `--append-system-prompt` adds a paragraph to
Claude Code's own prompt: the project's path, that PROJECT is `.`, that `daw`
is on the PATH, what runs without asking and what is shown to the person,
what each message carries, and that the person reads a panel, so replies
should be short and say what changed. The workspace is attached with
`--add-dir`, which loads its skills and lets its files be read without a
question. The workspace is the checkout, named by `AAWCheckout` in the
bundle's `Info.plist`, which `build.sh` writes; a build that is no bundle
finds it as the folder above the project with an `AGENTS.md` and a `.claude`
folder. A project under the workspace has the workspace's instructions by
location, as [agent-context.md](agent-context.md) describes; a project kept
elsewhere is given the root `AGENTS.md` and the person's two rules files in
the appended prompt instead.

**What each message carries.** Ahead of the person's words, between
`<app-context>` tags, the app puts the start position, the loop, whether the
song plays, what is selected (clips with their track and bar, a row, a marker,
notes or events in the editors) and what the person changed in the window or
the file since the agent's last turn ended, from the activity list, the
newest twelve. The panel shows the words alone, and takes the context off
again when a conversation is drawn from its record.

**Approvals.** Claude Code applies its own rules first: reads in the
project's folder and its safe commands need nobody. What is left comes to
the app as a request on the output, which `Approval` decides: a Bash command
that is `daw`, or `uv run daw`, on this project, each command of the line
either `daw` with no path leading outside the project's folder or a reader
such as `head`, `grep` or `jq` its output goes through, is allowed at once.
Anything else, a command with another program, a path outside the project,
a home path, a redirection, a subshell, or any other tool, an edit of
`SONG.md` included, is a card in the panel with Allow and Refuse, and a beep.
A request still waiting when the conversation changes or the turn is stopped
is refused.

**Threads.** A project's conversations are in `.daw/threads.json` beside
the song: each one's session ID, its title, the first line the person typed,
and when it started and was last used, most recent first. The list goes with
the folder, so a move or a Save As… keeps it, and a copy made with Save As…
has the same threads, which resume in the copy's folder. The menu at the
panel's top lists them with New Conversation; choosing one shows it where it
stopped, drawn from Claude Code's own record of it, under
`~/.claude/projects/` (or `CLAUDE_CONFIG_DIR`) in a folder named for the
project's folder, every character that is not a letter or a digit made `-`;
the next message resumes it. A session the person started in a terminal in
the project's folder is in that record too, and is listed under its first
message. Opening a project shows its most recent thread. One thread of a
project runs at a time: while the agent works, changing conversations is
refused with the reason, and Stop interrupts the turn.

**Being missing or signed out.** With no Claude Code found, the panel says
so and what to do. A session that ends saying it is not logged in, or whose
first result does, is shown as signed out, with the way back: run `claude` in
a terminal and sign in.

## In the window

| Part | Does |
|---|---|
| View › Agent, ⌥⌘C, the speech mark in the transport bar | Shows or hides the panel, 320 points at the right, over the activity panel when both show; kept between projects |
| The menu at the panel's top | The project's conversations, most recent first, and New Conversation |
| The field at the foot | Return sends; Option-Return starts a new line; Escape gives the keys back. The arrow is gray while the agent works |
| Stop, beside the spinner | Interrupts the turn; the agent says where it stopped |
| A tool's row | Gray while it runs, a tick when it returned, a cross when it failed; a click shows or hides what it returned |
| Allow, Refuse | Answer a request; the card then says which, and the agent goes on |
| `--ask TEXT` on the command line | Shows the panel and sends the text as a message; the picture waits for the turn to end, up to ten minutes, or until a request waits for a person |

## Limits

- The workspace attached with `--add-dir` is the checkout, so reading any
  file under it, another project's included, needs no approval; a `daw`
  command that names another project does. The concept's `~/Music/AAW/`
  workspace is in the backlog.
- Claude Code's own rules and the person's settings decide before the app
  does: a command the person has allowed in their settings runs without a
  card. The app adds nothing to those settings.
- The agent's text is shown with its inline markdown; code blocks and lists
  are shown as typed.
- A conversation's earlier messages are drawn from Claude Code's record of
  it; where that cannot be read, the panel says so and the conversation
  still resumes. A copy of a project made with Save As… lists the original's
  threads and draws them from the original folder's record.
- A thread cannot be renamed or removed in the panel; `.daw/threads.json` can
  be edited.
- The approval request's shape was taken from Claude Code's SDK; no card from
  a real session has been answered: see below.

## What was checked

With Claude Code 2.1.296 on this Mac, October 12, 2026, driven from a script
as the panel drives it:

| Tried | Result |
|---|---|
| A message as a line of stream-json input, with `--include-partial-messages` | `system` init with the session ID, `stream_event` lines with the text in `text_delta` pieces, the whole `assistant` message, `user` lines with each tool result, and a `result` line with `is_error`, `session_id` and the text |
| `control_request` with subtype `interrupt` on the input | Answered with a `control_response`; the pending tool use was rejected and the turn ended with `error_during_execution`; the next message in the same process was answered |
| `control_request` with subtype `initialize` first | Answered with Claude Code's commands and settings |
| `ls`, `echo`, `sleep` with `--permission-prompt-tool stdio` | Each ran with no request, allowed by Claude Code's own rules |
| A command that would need a request, from a script | Not tried: the request's fields (`can_use_tool`, `tool_name`, `input`) and the answer (`behavior` allow with `updatedInput`, or deny with a `message`) are as the Agent SDK documents them |
| The built app, `--ask` with a request to say which project it is in, what `SONG.md` says under Instructions and to run `daw status .`, on a scratch project outside the checkout | Answered from the folder and the file, ran the command and read `host: true` back, in a picture of the panel; the thread was kept in `.daw/threads.json`. Whether the app answered a request for the command or Claude Code's own rules allowed it is not in the record, so the first card from a real session is still for the person to see |
| The built app, `--ask` with the stand-in program | A picture of the panel with the streamed greeting, the `daw` command ticked, and the card for `touch x` with Allow and Refuse |

Tests drive the panel over a program standing in for Claude Code that speaks
the same lines: the arguments, the folder and the message with its context,
a streamed answer shown once, a `daw` command allowed by itself, a `touch`
asked about and refused, threads started under an ID and resumed with
`--resume`, the list read back when the project opens again, a stop, a
session with no login and a program that is not there. The approval rule,
the shell splitter, the thread list, the context's wrapping, the record's
folder name and the reading of a transcript have tests of their own. Nothing
was typed by hand, and no card was answered in a real session.

## Open questions

1. **Sign-in for other people.** Anthropic's documentation says: "Unless
   previously approved, Anthropic does not allow third party developers to
   offer claude.ai login or rate limits for their products, including agents
   built on the Claude Agent SDK", and points to API keys. On this Mac the
   panel uses the person's own installation and login. Whether a distributed
   app may drive a person's own signed-in Claude Code has to be confirmed
   with Anthropic.
2. **Naming.** Anthropic's branding rules do not allow a product to call its
   agent "Claude Code" or to look like it. "Claude Agent" or "Powered by
   Claude" are the allowed forms. The panel says "the agent" and names Claude
   Code only where it is missing or signed out.
3. **Two threads of one project at once.** Both would edit one song through
   one host, in one undo history. The second waits.
4. **Where a project's skills sit.** Claude Code finds them in
   `.claude/skills/`; the concept draws a visible `skills/`.
5. **The transcript** stays in Claude Code's own folder on this Mac. Whether
   a copy should travel with the project.
6. **Another agent** needs the same four things from its own program: start
   in a folder, stream, resume, ask permission.
7. **Edits of `SONG.md`.** The agent keeps Direction and Decisions current,
   and each edit is a card today. Whether edits inside the project's folder
   should pass, with Vision and Instructions still the person's by the rule.
