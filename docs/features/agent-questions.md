# The agent's questions in the panel — implemented November 22, 2026

Status: built (D196). The person's note of November 22, 2026 (D194): "whenever
claude does this ask user question type of event I dont get to see or select
any of the options."

## What

When the agent asks a question with choices, the panel draws it in the
conversation as a card: each question with its short header, its words, its
options as rows with their descriptions, a circle for one choice and a box
for several, and Other with a field for the person's own words. An option
with a preview shows it under the options, in monospace, while chosen.
Submit sends the answers once each question has one, and the agent goes on
from them; Skip tells the agent the question was not answered. Until then
the turn waits, as it does for an Allow card. An answered card stays in the
conversation with the choice marked, and a resumed thread draws it so.

A plan the agent proposes before it acts is a card of its own: the plan in
blocks as the agent's words are, with Approve and Keep Planning.

Any other request from the agent's process that the panel does not answer is
refused and said in a line of the conversation, never dropped silently.

## Why

Claude Code asks through its `AskUserQuestion` tool, and proposes a plan
through `ExitPlanMode`. In the panel's stream-json process every tool the
engine wants to run comes to the app as a `can_use_tool` control request.
The app had treated both like any other tool: allowed with their input
unchanged, so the person saw nothing and the agent got no choice back.

## Design

- **The request.** A `can_use_tool` request whose `tool_name` is
  `AskUserQuestion`, and whose questions can all be read, bypasses
  `Approval` and becomes `.question(request:, questions:, outcome:)` in the
  conversation; `AgentQuestion` reads each one's `question`, `header`,
  `multiSelect` and `options` with `label`, `description` and `preview`. One
  that cannot be read is shown as an Allow card, as before.
- **The answer.** Submit answers the request `allow` with `updatedInput` the
  input as it came plus `answers`, keyed by each question's words: the
  chosen labels in the options' order, then Other's words, joined by `, `.
  Skip, a stop, a new conversation or the turn's end answer `deny` with
  "The person did not answer the question in the app's panel."
- **Choosing.** For one choice, an option replaces the last and Other
  replaces the option; for several, options toggle beside Other. Typing in
  Other chooses it. Other with no words is not an answer.
- **Plans.** `ExitPlanMode` is `.plan(request:, plan:, approved:)`. Approve
  answers `allow` with the input unchanged, and Claude Code goes back to
  auto mode; Keep Planning answers `deny` with "The person did not approve
  the plan in the app's panel and wants to keep planning; ask what to
  change."
- **Their tool lines.** The assistant's `tool_use` line for either tool is not
  drawn: the card stands for it.
- **No keys.** Neither card answers to Escape or Return: Escape clears the
  timeline's selection, and would skip a question while a card waits.
- **A resumed thread.** `ClaudeTranscripts.parse` reads an
  `AskUserQuestion` tool use as `.question` with the answers from its result
  line's `toolUseResult.answers`, or none when the result failed or never
  came; an `ExitPlanMode` as `.plan`, approved when its result did not fail.
- **Other requests.** A control request that is not `can_use_tool` is
  answered with an error, as before, and a note says "The agent asked the
  panel for SUBTYPE, which the panel does not answer; it was told so."

## What was checked

With Claude Code 2.1.330 on this Mac, November 22, 2026:

| Tried | Result |
|---|---|
| A script driving `claude -p` as the panel does, `--permission-mode auto` and `--permission-prompt-tool stdio`, asking for one question | The `tool_use` came first, then `can_use_tool` for `AskUserQuestion` with `requires_user_interaction: true`; `allow` with `answers` came back as the tool's result, and the agent named the choice |
| The same for `EnterPlanMode`, then `ExitPlanMode` | `EnterPlanMode` ran with no request; `ExitPlanMode` came as `can_use_tool`; `allow` approved the plan and the mode went back to `auto` |
| The record of both | The question's result line holds `toolUseResult` with `questions` and `answers`; the plan's holds the plan |
| The debug app, `--ask` for one question, on a scratch project | The card with its header, two options with descriptions, Other, Submit and Skip, in a picture |
| The debug app, the same, then a scripted click on the second option and on Submit | The card marked "Answered:" with the option chosen, and the agent's next words named it |
| The debug app, `--ask` for a plan | The plan card with the plan's heading and list, Approve and Keep Planning |

Tests drive the panel over the stand-in program: two questions, one of
several choices, answered and sent back whole; skipped; stopped; a plan sent
back and approved; a request the panel does not answer said and refused.
The choosing and the answers' words, and the record read back, have tests of
their own. Nothing was clicked by hand, Other was not typed into in the
app, and no question of several choices came from a real session.
