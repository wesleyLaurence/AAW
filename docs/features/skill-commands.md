# Skills as slash commands in the panel — implemented October 13, 2026

A `/` typed at the start of the field of [the agent panel](agent-panel.md)
lists the skills the agent has; the one chosen is a chip, and the message
sends as `/name words`, which the engine runs as the skill. Asked for by
the person (D113); the choices made in building it are D116. Built on the
panel's engine as it is, the developer's own Claude Code, which a shipped
app will not use (D114); the list and the dispatch are the stream-json
protocol's, so an engine run with an API key answers the same
([the agent's engine](agent-engine.md)).

## What

Typing `/` as the field's first character opens a list over the field of
the skills the agent can run by name, the project's own, the workspace's
and the person's, each with its description, filtered as the person types.
One chosen becomes a chip at the field's start, `/roar-edit` in white on a
rounded fill in the person's blue, with the caret after it and the skill's
argument hint in the faint color until the words are typed. The message
sends as `/roar-edit edit from :41 to 1:43`, which Claude Code runs as the
skill, as a terminal session does; the person's row shows the chip, and so
does a resumed conversation. The person asked: "when a user types / the
different skills they have pop up and you can select from them and then
when you select one it shows as a blue highlighted slash command."

## How it works

**The list comes from the handshake.** The `initialize` request the panel
sends when a conversation's process starts is answered with `commands`,
each a name, a description and an argument hint; `AgentPanel` keeps them
and groups them. So that the list is there before the first message, a
conversation's process now starts when the panel shows, a thread is opened
or a new one begun, not at the first message: it waits on its input, costs
no turn, and ends as before when the conversation changes or the project
closes. A new conversation's first message goes to the process started
for it, under the ID it was started with. The last list a project saw is
kept in memory for the moment before a new process answers, and `/`
before any answer says "Loading skills…", or that Claude Code was not
found or is signed out.

**What is listed,** in three groups under small headings, told by the skill
folders rather than by the engine, which marks the workspace's skills
"(project)" too since the workspace is an `--add-dir`:

| Group | Which entries |
|---|---|
| This project | Names with a `SKILL.md` under the project's `.claude/skills/` or `.agents/skills/` |
| For all my music | The same under the workspace's |
| Mine and Claude Code's | The rest: `~/.claude/skills/`, plugins as `plugin:skill`, bundled skills and Claude Code's own commands |

The " (project)" the engine adds to a folder's description comes off.
Claude Code's own commands that mean nothing in the panel or would undo
what it keeps are left out, a list in `AgentCommands.hidden`: `clear`,
which would empty a conversation the panel lists as a thread, `model`,
`effort`, `fast`, `config`, `rename`, `mcp`, `usage` and the rest of the
terminal's; `compact` and `reload-skills` stay, and a name that starts
with `__` goes. Open question 2 of the design is answered that way.

**The list.** Over the well, its foot at the well's top, twelve rows tall
at the most and scrolling, each a `/name` in monospace with the hint after
it in the faint color and the description in one gray line, the keys' row
on a fill. Typing filters within each group: a name that starts with the
text first, then a name that has it, then a description that has it, in
any case. ↑ and ↓ move and wrap, Return and Tab choose, a click chooses,
Escape closes the list and leaves the `/` typed, and the next key opens it
again. A `/` anywhere else in the text is a slash. The list is a view in
the window, not a popover, so a scripted picture shows it.

**The chip.** `ComposerTextView` in `AgentComposer.swift` draws it: while
the text starts with the chosen skill's `/name` followed by a space, a
line or nothing, the view fills a rounded rect under those characters in
the person's color before it draws its text, and gives them white as a
temporary attribute the storage does not keep. Backspace at the chip's end
takes the whole chip; a caret put inside it lands after it. Editing the
text so that it no longer starts with the name drops the chip and, with
`/` still first and no space yet, shows the list again. One chip a
message, at the start, as Claude Code dispatches one command a prompt.

**What is sent** is the field's text as it reads, `/name` and the words
after it. The app's context block, which goes ahead of every other
message, goes after a skill's words instead, since Claude Code reads the
command from the prompt's start and would see none behind the block; the
panel's prompt tells the agent so, and that the block is not part of the
arguments. Claude Code gives the skill everything after the name as its
`$ARGUMENTS`, the block included, so a skill that quotes its arguments
quotes the block too (open question 1, answered as the design's first
choice).

**What is shown.** The person's row draws `/name` as the chip and the words
after it. A resumed thread draws it from Claude Code's record, where a
dispatched command is a user line of `<command-message>`, `<command-name>`
and `<command-args>` tags, read back as `/name words` with the context
taken off, and the skill's expansion that follows is a meta line the
reader passes over, as before (open question 3).

**A skill's own needs.** A skill that reads the person's files or runs
`daw` runs under the same rules as any turn: auto mode, `daw` on the
project allowed by the app, the rest a card.

## Scripted runs and tests

The stand-in the tests speak through answers the handshake with five
commands, two marked "(project)", and takes the context block off a
message's end as it does off its start. Tests hold the process started
before any message, the groups told by skill folders made in the test,
the hidden command left out, the "(project)" taken off, the filter, the
keys' selection and its wrap, the chip set and dropped as the draft
changes, Escape, a slash elsewhere, the message sent as `/name words`
with the block after it through the warmed process, the thread titled by
it, the person's row, and a new conversation warmed too; the grouping,
the filter and the record's tags over pure functions; the field's keys to
the list, the chip's range, hint, caret and Backspace over the text view
with no window; and the record read as the person's line. What was seen
in pictures is under What was tried.

## What was tried

Scripted runs only, on blank projects in a scratch folder, with another
AAW in front the whole time; nothing by hand:

- Over the stand-in, which lists a project skill, a workspace-style one,
  one of the person's, `clear` and `compact`: a click in the field and
  `--type /` showed the list over the conversation's foot, This project
  with `/song-skill`, its `[bars]` hint and its description, then Mine and
  Claude Code's with the other three and `clear` left out, the first row
  on the fill, in a picture; `--type /o --key down` filtered to the four
  that have an `o` and moved the fill to the second; `--type /so --key
  return` left the chip `/song-skill` in the field with `[bars]` after it
  and Send in the person's color; `--type /ro --key return`, `8 bars
  short` typed and `--key return` sent `/roar 8 bars short` with the
  context block after it, read from the stand-in's log, and showed the
  person's bubble with the chip and the words and the reply under it.
- Against Claude Code 2.1.296, on a scratch project outside the checkout
  with a `hello` skill of its own that answers with a word and its
  arguments: `+` for a new conversation and `--type /` listed `/hello`
  under This project, `/roar-edit` under For all my music and the plugins
  and bundled skills under Mine and Claude Code's, from the real
  handshake with no model turn, twelve rows tall and scrolling, in a
  picture; `--ask "/hello one two"` ran the skill, whose reply carried the
  word, the arguments and the context block, in a picture; the project
  opened again showed the chip on that message and on one a script had
  sent to the same session earlier, both read from the record.
- Three things were fixed to get there: the list drawn over the field's
  well and clipped to it, now over the conversation's foot; the list's
  fixed height pushing the window taller than asked, now a most; and the
  scripted driver's keys, sent to the app's key window, which there is
  none of while another AAW is in front, now sent to the window itself,
  with the window made key before each scripted action.
- A scripted click in the well's top padding, a few points over the
  text, does not focus the field; the text sits about 20 points under the
  well's top. Nobody pressed a key by hand, chose with a click or ran a
  skill of their own.

## Two fixes made on the way

- The whole Mac test suite hung after the agent panel's tests, from PR
  #104 on: the panel's process read its pipes through readability
  handlers, and a handler left at a pipe's end kept the file-handle
  monitoring spinning on every core, so the export test's own read never
  got a thread. Each pipe is now read to its end on a thread of its own,
  as the exporter and the locator read theirs.
- The login shell the panel asks for `claude`'s place, `zsh -lic`, ignores
  the SIGTERM it was sent when it took longer than fifteen seconds and
  stayed behind; a few dozen sat on this Mac from earlier runs. It is
  killed now. The ones already there are the person's to end.

## Limits

- The groups are told by skill folders, so a skill Claude Code finds
  elsewhere, as in a parent folder, is listed under Mine and Claude Code's.
- The argument hint is shown and does nothing else (open question 4).
- Claude Code's own commands that stay, `compact` among them, run as the
  engine runs them; the panel shows what comes back and nothing else of
  them.
- One chip a message, at the start.

## Open questions

1. **Which of Claude Code's own commands to keep.** `compact` and
   `reload-skills` stay and a list hides the rest; the person says which
   they miss or want gone.
2. **`@` mentions** of a track, a clip, a section or a reference, which the
   concept lists beside `/`: the same list over `@`, with the song's
   names. Later, under The app.
