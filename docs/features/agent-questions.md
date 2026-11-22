# The agent's questions in the panel — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 2. The person's note of November
22, 2026 (D194): "whenever claude does this ask user question type of event
I dont get to see or select any of the options." What is said here about the
panel was read from the checkout on November 22, 2026.

## What

When the agent asks a question with choices, the panel draws it in the
conversation: each question with its short header, its options as rows with
their descriptions, one or several selectable as the question allows, and
Other with a field for the person's own words. Submit sends the answers and
the agent goes on; until then the turn waits, as it does for an approval. A
question answered stays in the conversation with the choice marked, and a
resumed thread draws it so.

## Why

Claude Code asks through its `AskUserQuestion` tool. In the panel's stream-json
process every tool the engine wants to run comes to the app as a
`can_use_tool` control request (`AgentPanel`'s `control_request`), and the
app decides with `Approval.decide`, which treats `AskUserQuestion` like any
other tool: allowed with its input unchanged, or shown as an approval to
allow or deny. Neither shows the questions or carries an answer, so the
person never sees the options and the agent gets no choice back.

## Design

- **The request.** A `can_use_tool` request whose `tool_name` is
  `AskUserQuestion` bypasses `Approval` and becomes a new conversation entry,
  `.question(request:, questions:, answers:)`, with the input's `questions`:
  each `question`, `header`, `multiSelect` and `options` with `label` and
  `description`, and a `preview` drawn in a monospaced box where an option
  has one.
- **The answer.** Submit answers the request `allow` with `updatedInput` the
  input as it came plus `answers`, keyed by each question's text, the chosen
  labels joined by commas or the Other text, as the Agent SDK documents for
  answering this tool. Esc or Stop answers `deny` with "The person did not
  answer", as a stopped approval is denied.
- **Plan approval.** `ExitPlanMode` comes the same way and is drawn as the
  plan with Approve and Keep planning, so a plan the agent proposes is not
  lost the same way.
- **Other requests.** Any control request the panel does not answer is shown
  as a line saying so, never dropped silently.

## Done when

- A real turn where the agent asks a question with two to four options shows
  them, the person picks one, and the agent's next words follow from it.
- Multi-select, Other and a stopped question each answer as said.

## Open questions

1. Whether the request carries `AskUserQuestion` this way under `--permission-mode
   auto` with this Claude Code version, or answers itself before the app
   sees it: to be read from a real turn's stream first.
