# A skill named anywhere in a message — proposed November 22, 2026

Status: proposed, not built. Backlog: Next 6. The person's note of November
22, 2026 (D194): "whenever I use the chat and a slash command skill, it only
works if the / is the first character if i try to use a slash command
midway through a message it does not show any options or allow for it."
What is said here about the panel was read from the checkout on November
22, 2026.

## What

A `/` typed anywhere in the agent panel's field, after a space or at the
start, opens the same list of skills ([skill commands](skill-commands.md)).
The one chosen becomes a chip where it was typed, as an `@` mention does
([mentions](agent-mentions.md)), and the message runs that skill on the
whole message: "take the second chorus and /clean-edit it, keep the ad-libs"
runs clean-edit with those words.

## Why

The list opens only when `/` is the field's first character, and
`AgentCommand.parse` reads a command only when the message starts with `/`
(`AgentCommands.swift`), because Claude Code runs a skill by name only from
the start of a message, as a terminal session does. A `/name` in the middle
reaches the agent as plain text.

## Design

- **The field.** The `/` trigger is the `@` trigger's rule: after a space or
  at the start, the word after it filters the list. One skill chip per
  message; a second `/` lists nothing and says one skill a message.
- **What is sent.** A skill chip in the middle is sent as `/name` at the
  start followed by the message as typed, with the chip's place read as the
  skill's name, so Claude Code runs the skill with the person's words
  whole: `/clean-edit take the second chorus and clean-edit it, keep the
  ad-libs`. The person's row draws the message as they typed it, with the
  chip in place.
- **A `/` that is not a skill**, as in "1/4" or "and/or", opens nothing: a
  `/` after a letter or a digit is text.

## Done when

- A skill chosen mid-message runs as the skill, in a real turn, and the row
  shows the chip where it was typed.
- "1/4 notes" and "and/or" type as text.

## Open questions

1. Whether two skills in one message should be allowed, run one after the
   other, rather than refused.
