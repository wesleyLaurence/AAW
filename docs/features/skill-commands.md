# Skills as slash commands in the panel — proposed October 13, 2026

Status: proposed, not built. Backlog: Next, Skills as slash commands. Builds
into the field of [the agent panel's look](agent-panel-look.md). What
Claude Code does with a `/name` was read from its documentation and seen
from a script on October 13, 2026; see What was checked. Built on the
panel's engine as it is, the developer's own Claude Code, which a shipped
app will not use (D114); the handshake and the dispatch are the protocol's,
so an engine run with an API key answers the same
([the agent's engine](agent-engine.md)).

## What

A `/` typed at the start of the panel's field lists the skills the agent
has, the project's own, the workspace's and the person's, filtered as the
person types; one chosen becomes a blue chip in the field, `/roar-edit`,
with the caret after it for the words that follow. The message sends as
`/roar-edit edit from :41 to 1:43`, which Claude Code runs as the skill, as
a terminal session does. The person asked: "when a user types / the
different skills they have pop up and you can select from them and then
when you select one it shows as a blue highlighted slash command."

## Why

A skill is how the person keeps a process ([skills](skills.md)), and today
the panel has no way to run one by name: the agent chooses skills by
description, or the person types the name and hopes. Claude Code's own
field lists them under `/`; the panel should too. The concept had `@skill`
for this; `/` is what the person knows from Claude Code (D113).

## What was checked

With Claude Code 2.1.296 on this Mac, October 13, 2026:

| Tried | Result |
|---|---|
| `control_request` with subtype `initialize`, from a script, in the checkout | Answered with `commands`: 69 entries, each `{name, description, argumentHint}`, some with `aliases`; the project's `roar-edit` among them with its description ending "(project)", the person's synced skills, plugin skills named `plugin:skill`, and Claude Code's own commands. No model turn, under two seconds |
| The documentation's word on dispatch | "Send a command by including it in your prompt string": a user message whose text starts with `/name` runs the skill, and its arguments follow. A name that matches nothing goes to the model as an ordinary message with a note. The `system` init message of the first turn lists `slash_commands` as names and `skills` as the user-invocable ones |

The panel already sends the handshake when a process starts
(`AgentPanel.start`), so the list is on hand once a conversation runs. It
does not keep the answer today.

## Design

**The list comes from the handshake.** The answer's `commands` are kept
on the panel as `[Command]`, name, description, hint, and shown under `/`.
So that the list is there before the first message, a conversation's
process starts when the thread is opened or the panel shown, not at the
first message; it waits on its input, costs no turn, and ends as it does
today when the conversation changes or the project closes. The last list a
project saw is kept in memory for the moment before the handshake answers,
and `/` before any answer says "Loading skills…".

**What is listed.** Three groups, each under a small heading, in the order
the person is likely to want them:

| Group | Which entries | Told by |
|---|---|---|
| This project | Skills in the project's `.claude/skills/` | The description's "(project)" suffix, and the folder's `SKILL.md` names |
| For all my music | Skills in the workspace's `.claude/skills/` | The folder's `SKILL.md` names; these also carry "(project)" since the workspace is an `--add-dir` |
| Mine and Claude Code's | The rest: `~/.claude/skills/`, plugins, bundled skills | Everything else |

Claude Code's built-in commands that have no meaning in the panel, `clear`,
`compact`, `model`, `effort`, `fast`, `config`, `rename`, `mcp` and the
rest of a short list kept in the app, are left out; `clear` would empty a
conversation the panel lists as a thread. Open question 2.

**The popover.** `/` as the first character of the field opens a list over
the field, 12 rows at most, each a name in monospace and its description in
one gray line, the hint after the name in the faint color. Typing filters
by the name, then the description, as Claude Code's own list does; ↑ and ↓
move, Return or Tab chooses, Escape closes and leaves the `/` typed. A `/`
anywhere else in the text is a slash.

**The chip.** The chosen command is the text `/name` under an attribute of
its own in the field's text view, drawn by the layout manager as a rounded
fill in the person's blue (`Theme.color(of: .user)`) with white text, the
hint drawn after it in the faint color until the person types. Backspace at
its end takes the whole chip; a click inside it puts the caret after it.
One chip a message, at the start, as Claude Code dispatches one command a
prompt.

**What is sent** is the field's text as it reads, `/name` and the words
after it, with the app's context block ahead of it as every message has
([the agent panel](agent-panel.md)). Open question 1 is whether the context
should go ahead of a command.

**What is shown.** The person's row draws the chip as the field did. A
resumed thread draws it from Claude Code's record, where a dispatched
command is a user line whose text starts with `/name` or a
`<command-name>` line, which the reader skips today; the reader learns the
shape in building this (open question 3).

**A skill's own needs.** A skill that reads the person's files or runs
`daw` runs under the same rules as any turn: auto mode, `daw` on the
project allowed by the app, the rest a card.

**Scripted runs and tests.** The stand-in program answers the handshake
with a `commands` list, as Claude Code does; a test opens the list with
`--key /`, filters it, chooses with Return, reads the field's text and the
message the stand-in received. The popover is seen in a picture. One
scripted `--ask "/roar-edit"` against the real Claude Code, on a scratch
project outside the checkout, is run and its reply read, to see a skill
dispatched from the panel.

## Done when

- `/` lists the project's, the workspace's and the person's skills in
  three groups, with descriptions, filtered as the person types, and
  `roar-edit` is among them in a project under the checkout.
- A chosen skill is a blue chip; the words after it send as its arguments;
  the agent's reply shows the skill ran.
- The list is there before the first message of a new conversation.
- A resumed thread shows the chip on the message that ran a skill.
- The person has run one of their own skills from the panel.

## Open questions

1. **The context block ahead of a command.** `<app-context>` goes ahead of
   every message; a command is read from the start of the prompt, so the
   context would hide it. The command goes first, then the context, or the
   context is left off a command's message. Tried in building this.
2. **Which of Claude Code's own commands to keep.** `compact` has a use in
   a long conversation; `clear` does not. A short list in the app that
   lists the kept ones, or hide them all.
3. **How Claude Code records a dispatched command,** as a plain user line
   or as `<command-name>`, which the transcript reader skips. Read from a
   real session's record in building this.
4. **Arguments the skill names.** Claude Code's `argumentHint` is a string;
   the chip shows it and does nothing else.
5. **`@` mentions** of a track, a clip, a section or a reference, which the
   concept lists beside `/`: the same popover over `@`, with the song's
   names. Later, under The app.
