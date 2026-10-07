# The agent working in steps the person can watch (implemented 2026-10-07)

Status: built in the pull request that closed the person's first note of
October 6, 2026, decision D87. The concept's Edit mode has the agent editing
live, as turns the person can revert
([concept.md](../concept.md#working-together)); this is the pace of those
edits while a person is watching.

## What

- **Guidance first.** [music.md](../music.md#edit-together) says that with a
  host running the agent makes the song a part at a time: a sound found and
  imported, a track added, a clip and its notes, each a command as soon as it
  is decided, so it appears on the timeline and in the activity as it is made
  and the person can say what they think before the next; the next part is
  planned while the last one plays, and a batch is one musical edit, not the
  song held back until it is finished. `daw describe start` says the same
  under `pace`, beside its recipe, which is already one command a step.
- **A hint from the host.** A host whose agent sent nothing for five minutes
  says so in the reply of the agent's next edit, as `hint`:

  ```
  A host is running and nothing reached the song for 12 minutes: work in steps
  the person can watch, one sound, part or clip a command as soon as it is
  decided, so each appears on the timeline and in the activity as it is made,
  and plan the next while the last plays. A batch is one musical edit, not the
  whole song.
  ```

  The silence is measured from the agent's last command of any kind, a read
  such as `inspect` or `status` included, so an agent that reads the song and
  then thinks for ten minutes is hinted and one that reads and places is not.
  The person's edits in the app do not shorten it. The hint rides on edits
  alone, not on reads, undo or the person's own edits, and is in no change:
  the activity and the change log are as they were.

## How

- `host::pace_hint(gap, pace)` makes the text, and `Options.pace` is how long
  the silence may be: `host::PACE`, five minutes, in `daw` and the app, and
  milliseconds in the host's tests. The host keeps the `Instant` of the
  agent's last request and compares at the next edit.
- Nothing in the app changes: a batch is still one entry of the activity,
  named for its label, and a step at a time is an entry each.

## Limits

- The hint reaches an agent only through its next edit; an agent that sends
  nothing hears nothing. The host cannot interrupt a session.
- Five minutes is a guess at "a long time" and is not a setting. An agent
  waiting for the person's answer is hinted too, once, on its next edit.
- Whether an agent reads the guidance and keeps the pace is under Verify.
