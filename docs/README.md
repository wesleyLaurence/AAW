# Agentic DAW — design docs

These documents capture the design conversation for an AI-native digital audio workstation: a DAW built to be operated by a coding agent (Claude Code, Codex) rather than by a human clicking a UI. The sampler MVP is now implemented; see [../README.md](../README.md) and [mvp.md](mvp.md) for the current build. The documents below remain the broader design draft. Read in this order:

1. **[idea.md](idea.md)** — the vision, the first-principles reframes, and the working rhythm between a person and the agent. Start here.
2. **[spec.md](spec.md)** — the technical specification: primitives, document format, devices, rendering, perception, library, intent and memory, CLI surface, stack, and the end-state workspace UI.
3. **[plan.md](plan.md)** — phased build plan with exit criteria per phase, risks, and open questions.
4. **[decisions.md](decisions.md)** — decision log with rationale, so a future session does not relitigate settled choices.

Conventions used throughout: Ableton vocabulary (session, track, clip, device, rack, send, return, automation) because the models already know it. `daw` is a placeholder CLI name. Time is always in bars and beats. Units are always real-world units.
