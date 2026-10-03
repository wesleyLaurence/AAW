# The agent panel — proposed October 2, 2026

Status: proposed, not built. Backlog item 7. What it should feel like is in
[concept.md](../concept.md#the-agent-in-the-window); this file is how, and what
was checked about Claude Code on October 2, 2026 (version 2.1.287 on this Mac).

## What

A conversation with an agent inside the project's window, started in the
project's folder, with the workspace's instructions, taste and skills attached,
and a list of the project's past conversations.

## Why

Today the agent is a terminal beside the app. It has to be told which project is
open, it loads instructions and skills from wherever it was started, and its
conversations are not tied to the project.

## Stages

1. **A terminal beside the app.** Works today: the agent's edits reach the app's
   host and are seen and heard.
2. **A native panel over the person's own Claude Code.** This file.
3. **Other agents and sign-in in the app.** Backlog item 9.

## What Claude Code gives

From its documentation ([running it programmatically](https://code.claude.com/docs/en/headless),
[sessions](https://code.claude.com/docs/en/agent-sdk/sessions),
[the Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview)):

- **The Agent SDK is Python and TypeScript only.** From another language the
  documented route is the `claude` program as a subprocess with `-p` and
  `--output-format stream-json`, which prints each message, tool call and result
  as a line of JSON.
- **It loads what a terminal session would:** the instructions, skills and
  settings in the working folder's `.claude/` and in `~/.claude`.
- **Skills in a folder attached with `--add-dir` load too,** from its
  `.claude/skills/`, by name and description.
- **`--append-system-prompt` and `--append-system-prompt-file`** add instructions
  without replacing its own.
- **Conversations are saved per working folder,** under
  `~/.claude/projects/<the folder's path with punctuation replaced>/`, one file
  each. `--resume ID` continues one, and since version 2.1.223 it is found from
  any folder. `--continue` takes the latest in the current folder.
- **`--allowedTools`** takes rules such as `Bash(daw *)`, and
  `--permission-prompt-tool` names an MCP tool that answers the requests that
  are left, which is how a host program shows them.

## Design

**One process per project window,** started in the project's folder with the
workspace attached and its `AGENTS.md` and `profile/` appended to the prompt. The
project's own `SONG.md` and skills are then the session's by location.

**Threads.** The project keeps its conversations' IDs and titles in
`.daw/threads.json`. The panel lists them and resumes one by ID, so the list
survives a move of the folder, and a Save As… in the middle of a conversation
restarts the process in the new folder on the same ID.

**What each message carries.** The app adds the selection, the playhead and loop,
and `daw changes --since` the agent's last turn, so "make this darker" needs no
question and the person's edits are read as direction.

**Approvals.** `daw` commands on the open project are allowed. Anything else the
agent asks for is shown in the panel to allow or refuse.

**What the panel draws.** Messages as they stream, each tool call with its
result, and the host's changes the turn made, which the activity list already
has.

## Depends on

- [New and Untitled projects](new-and-untitled-projects.md), which is built, for
  a project there from the first moment and a path that follows Save As….
- Backlog item 6, for the workspace, `SONG.md` and where skills live.
- Turns in the host (Later) would let the panel keep or revert a request as one.
  The panel is useful without them.

## Done when

- Opening a project shows a panel in which a typed request is answered, and an
  edit the agent makes is seen and heard in the window.
- The agent, asked which project it is in and what the person's rules are,
  answers from the project and the workspace without being told.
- Closing and reopening the project lists the earlier conversation, and resuming
  it continues with its context.
- A conversation survives Save As… and a move of the folder.
- A request for something other than `daw` on this project asks first.
- With Claude Code missing or signed out, the panel says which and what to do.
- A person has done each of the above by hand.

## Open questions

1. **Sign-in for other people.** Anthropic's documentation says: "Unless
   previously approved, Anthropic does not allow third party developers to offer
   claude.ai login or rate limits for their products, including agents built on
   the Claude Agent SDK", and points to API keys. On this Mac the panel uses the
   person's own installation and login. Whether a distributed app may drive a
   person's own signed-in Claude Code has to be confirmed with Anthropic.
2. **Naming.** Anthropic's branding rules do not allow a product to call its
   agent "Claude Code" or to look like it. "Claude Agent" or "Powered by Claude"
   are the allowed forms.
3. **Where a project's skills sit.** Claude Code finds them in `.claude/skills/`;
   the concept draws `skills/`. A link, a plugin folder, or a list in the prompt.
4. **How messages are sent in** to a running process, and how a turn is
   interrupted, from the CLI reference.
5. **The transcript** stays in Claude Code's own folder on this Mac. Whether a
   copy should travel with the project.
6. **Another agent** needs the same four things from its own program: start in a
   folder, stream, resume, ask permission.
