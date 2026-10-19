# The agent's engine for other people — proposed October 13, 2026

Status: proposed, not built. Backlog: Later › Shipping, The agent's engine
for other people. What Anthropic allows was read from its terms, support
pages and the Agent SDK's documentation on October 13, 2026, and is
summarized under What was checked; nothing here is legal advice, and the
terms are to be read again before anything ships.

## What

How the agent panel gets its agent on a Mac that is not the developer's.
Today the panel drives the Claude Code the person installed and signed in to
themselves ([the agent panel](agent-panel.md)). That is the right engine
while one person builds and uses the app, and the four panel items at the
top of Next are built on it (D114). It is not an engine a distributed app
may use. This file holds the routes past it, so that each of those items is
built in a way that carries over.

## Why

The goal is to distribute the app. Anthropic does not let a third-party app
use people's claude.ai login or subscription limits, and since early 2026
blocks it server-side; tools that sent Claude Code's own headers with a
subscription token were cut off. Wrapping the real `claude` for one's own
use is fine; shipping that wrapper as the engine behind a product is
routing other people's traffic against their subscriptions, which the terms
name. So the shipped panel needs an engine the person pays for in a way the
provider allows, or no engine in the window at all.

## What was checked

| Read, October 13, 2026 | Says |
|---|---|
| Anthropic's support page on third-party apps | "Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK", and points to API keys |
| The Agent SDK's and `claude -p`'s documentation | The engine behind Claude Code is sold as the Agent SDK for exactly this use: an app's own agent, billed to an API key. `--bare` with `ANTHROPIC_API_KEY` is the recommended mode for programmatic use and "never reads OAuth credentials or the system keychain" |
| OpenAI | "Sign in with ChatGPT" with plan usage sharing exists, through a curated partner program only |
| Anthropic's branding terms | A product may not call its agent "Claude Code" or look like it; "Powered by Claude" is the allowed form. The panel says "the agent" |

## The routes

Three engines for the window and one route around it. The panel's protocol
is one process a conversation, stream-json in and out, which the first two
keep, so the look, the `/` skills, the attachments and the microphone are
built once.

**1. Claude Code's engine with the person's own API key.** The same
process the panel runs today, with the key in its environment and no login,
the binary carried in the bundle or found installed. Keeps everything:
skills, `AGENTS.md` and `SONG.md` by location, auto mode, sessions,
compaction. Costs the person API rates, which for an hour of editing can be
dollars where a subscription costs nothing more, and needs a console
account. Needs in the app: a Settings window with the key in the Keychain,
the cost of each turn shown in the panel from the `result` message's
`total_cost_usd`, and a budget the person sets, which `--max-budget-usd`
takes. Whether the engine's binary may be carried in the bundle, and under
what license, is open question 1; `--bare` skips `AGENTS.md`, memory and
the project's `.claude/commands/`, so the panel would pass the instructions
itself, as it already does for a project outside the workspace.

**2. The app's own harness.** An agent loop the app owns: a provider's
messages API with tool use, the tools being `daw`'s verbs, reading files in
the project and the workspace, and the window's context; permissions
decided by the app; conversations kept by the app in the project. The
provider is the person's choice: Anthropic or OpenAI by key, or a local
model on the Mac through an OpenAI-compatible server such as Ollama, LM
Studio or an MLX server, which costs nothing and sends nothing off the
machine. What it gives up is everything Claude Code brings for free, which
the harness has to do itself: find and load skills, read `AGENTS.md` and
`SONG.md`, compact a long conversation, resume a session, judge a command
safe. That is the cost of being free of any one provider. Whether a local
model can drive `daw` well enough, a song of several sections through
`describe`, `start` and the checks, is open question 3 and is answered by
the sessions under Verify run against one. On October 19, 2026 the person named the shape they want explored: a fork of an open agent framework such as LangChain's deep agents, customized for this system, with a sandbox for the code and file system work it does and more of the audio processing native to it, once the panel's items are built.

**3. Claude Code's engine with a sign-in.** Not available: the button the
concept once named needs a program Anthropic has not opened. Kept here so
that the day it opens, it is route 1 with a login in place of a key.

**4. No engine in the window: AAW as an MCP server.** `daw`'s verbs, the
window's context and the approvals offered as tools to the agent the person
already pays for, in Claude Code, Claude's desktop app, ChatGPT or Codex,
all of which speak MCP. The person keeps their subscription and the app
pays nothing. Loses the conversation in the window, the context ahead of
each message as the panel sends it, and two projects each with their agent;
some of that comes back as tools and resources the agent asks for. One
adapter serves every agent, which is the strongest reason for it. The
backlog has it under Later as a reliability question; distribution makes it
a route.

**Proposed:** route 1 and route 4 ship first, since both keep the agent on
a provider's own terms with the least built; route 2 follows if a local
model proves able, or a second provider is wanted in the window. The
panel's engine is a choice in Settings, with the developer's own installed
Claude Code offered only when the app runs from this checkout.

## What this means for the panel items now

The four items at the top of Next are built on the installed Claude Code
and nothing in them should assume it:

- `ClaudeCode.locate` becomes one way of finding an engine; the engine's
  executable, arguments and environment are one value the panel is given.
- The handshake's `commands` list, the stream's shapes and the approval
  request are the protocol's, and the stand-in program the tests use
  speaks the same lines whichever engine is behind it.
- Nothing names Claude in the window beyond the missing-and-signed-out
  messages, and the look is a conversation's, not Claude's app's.

## Depends on

- An app that stands alone (Shipping), so the engine has a bundle to live
  in and a Settings window to be chosen in.
- A decision when it is picked up: which routes ship, in what order, is a
  new entry in [decisions.md](../decisions.md).

## Done when

- A person with an API key and no Claude Code installed opens a project,
  enters the key once, and has the panel as this checkout has it, with
  each turn's cost shown and a budget that stops a turn.
- The app refuses to drive a subscription login on a Mac where it is not
  run from this checkout, and says why.
- Claude's desktop app connected to AAW's MCP server edits a song in the
  window.
- The licenses of the engine and of everything it carries are in the
  dependency record.

## Open questions

1. **The engine's binary.** Whether Claude Code's engine may be carried in
   a Mac bundle, or must be installed by the person, and its license.
2. **Cost in the open.** Whether the panel should show a running total for
   the day, as the generated-audio design does for ElevenLabs, or each
   turn's cost alone.
3. **A local model.** Which model and server, on how much memory, drives
   `daw` through a session of the kind under Verify; and whether the
   harness's own skill loading can match Claude Code's.
4. **Who writes the harness's safety rule.** Auto mode is Claude Code's
   classifier; the harness would have the app's `Approval` and nothing
   else, so everything outside `daw` on the project would be a card again,
   as it was under D111.
5. **MCP's share of the concept.** How much of "the agent in the window"
   survives when the window is Claude's: the context as a resource, the
   approvals as the agent's own, and threads by project in the agent's app
   rather than the project.
