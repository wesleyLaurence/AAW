# A skill named anywhere in a message — implemented November 22, 2026

A `/` typed in the agent panel's field at its start or after a space lists
the skills ([skill commands](skill-commands.md)); the one chosen becomes a
chip where it was typed, as an `@` mention does
([mentions](agent-mentions.md)), and the message runs that skill on the
whole message: "take the second chorus and /clean-edit it, keep the
ad-libs" runs clean-edit with those words. The person's note of November
22, 2026 (D194): "whenever I use the chat and a slash command skill, it only
works if the / is the first character if i try to use a slash command
midway through a message it does not show any options or allow for it."
The choices made in building it are D200.

## What

Claude Code runs a skill by name only from the start of a message, as a
terminal session does, so before this the list opened only for a `/` as the
field's first character and a `/name` in the middle reached the agent as
plain text. Now the list opens wherever the `/` is, the chip stands where
it was put, and the message is sent with the skill first.

## How it works

**The field.** The `/` trigger is the `@` trigger's rule
(`AgentCommands.query`, over `AgentMentions.query` with `/` as its
character): a `/` at the start or after a space, with no space between it
and the caret, opens the list filtered by the word after it; the caret
says which `/`, so moving it into one opens its list. A `/` after a letter
or a digit, as in "1/4" or "and/or", is text, and so is a path past its
first part. Choosing a skill puts `/name` and a space in place of `/` and
what was typed after it, or no space when one is already there, and the
caret after it.

**The chip** is the first `/name` at the start or after a space with a
space, a line or the end after it (`AgentCommands.range`); the field draws
it on the person's color wherever it stands, Backspace at its end takes it
whole and a caret put inside it lands after it, as before. The skill's
argument hint shows only after a chip that is all the field holds. A skill
typed out whole, `/name` followed by a space, whose name the list has, is
the chip too, so a person who types the name rather than choosing it gets
the skill all the same. Editing the name away drops the chip.

**One skill a message.** While a chip stands, another `/` after a space
opens the list with no rows and the line "One skill a message: /name is
chosen.", and keeps no key: Return sends and the arrows move the caret.
A list with nothing matching keeps no key either, so "see /zz" sends with
Return; only the list waiting for the handshake keeps Return.

**What is sent.** A message whose chip is at its start is sent as typed.
One whose chip is in the middle is sent as `/name` first, a space, then
the words exactly as typed, the chip's `/name` kept in its place:
`/clean-edit take the second chorus and /clean-edit it, keep the
ad-libs`. Claude Code runs the skill and gives it everything after the
first name as its arguments, the person's words whole with the `/name` in
them; the app's context block goes after the words, as for any skill.

**What is shown.** The person's row draws the words as typed with the chip
in place, in monospace on the person's color; a chip at the start is drawn
first as before. A resumed thread reads the same from Claude Code's
record: the command's arguments hold the words, and a `/name` among them
is where the chip was (`AgentCommands.shown`). A thread's title is the
words as typed.

## Limits

- One skill a message, as Claude Code dispatches one command a prompt.
  Two skills in one message, run one after the other, were not built (the
  design's open question).
- A message whose chip is at its start and that names the same skill again
  later in its words is drawn back with the chip at the later place.
- A skill name followed straight by punctuation, `/clean-edit,`, is no
  chip; the list puts a space after a chosen one.

## Scripted runs and tests

Pure tests hold the query (after a space only, "1/4", "and/or", a path),
where a chip stands, a name typed out whole, the message sent and the row
and title read back, the record's line included. The field's tests hold a
chip in the middle drawn white, with no hint, the caret and Backspace. The
panel's tests over the stand-in hold the list mid-message, choosing in
place with and without a space after, a second `/` listing none and
keeping no key, a non-matching list leaving Return to send, a name typed
out whole becoming the chip, and the message sent with `/name` first, the
row and the title.

## What was tried

Scripted runs only, of the debug build on a scratch project outside the
checkout with a `hello` skill that answers PINEAPPLE and its arguments,
against Claude Code 2.1.330, with the person's AAW open meanwhile; nothing
by hand:

- A click in the field and `--type "say it back /he"` listed `/hello`
  under This project, first and on the fill, from the real handshake, in a
  picture.
- Return chose it and `with one two` typed after it showed the field as
  "say it back", the `/hello` chip, "with one two".
- Return sent it: the skill ran and answered `PINEAPPLE: "say it back
  /hello with one two"`, and the person's row drew the chip where it was
  typed, in a picture.
- The project opened again drew the thread from the record the same, the
  chip in place, titled "say it back /hello with one two".
