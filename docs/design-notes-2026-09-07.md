# Design review and first-principles notes

Date: 2026-09-07

Reviewed revision: `e572dcb`

Status: analysis and recommendations, not yet a replacement specification

This is intentionally a deep review. For a fast decision pass, read the executive conclusion, sections 3–7, section 14, and section 17. Sections 8–13 and 15–16 are the detailed specification, architecture, verification, and risk audit.

## Executive conclusion

This idea is worth a bounded validation phase. The strongest version of it is narrower and more durable than the current framing.

The valuable product is not fundamentally “a new DAW that an agent can operate.” It is **delegation without surrender**: a person can delegate the mechanical and exploratory work of production while retaining editable notes, clips, samples, routing, revisions, provenance, and final judgment. A new audio engine is one possible way to deliver that value, not a first-principles requirement.

The existing documents are thoughtful and unusually coherent for a pre-implementation design. They establish a strong vocabulary, a transparent state model, a render/evaluate loop, useful boundaries, and disciplined decision records. The central design instincts are good:

- Local, structured, inspectable project state.
- Offline-first, reproducible rendering.
- Real units and self-describing controls.
- User-supplied samples as first-class material.
- Reversible work with named, listenable artifacts.
- Human taste paired with agent execution.
- One semantic core shared by tools and any future workspace.

The current plan nevertheless makes four strategic mistakes:

1. It treats a greenfield renderer and in-house DSP suite as necessary before proving that the agent workflow produces music people want to keep.
2. It postpones perception and the agent harness until after several phases of conventional DAW engineering, even though the closed loop is the differentiating hypothesis.
3. It describes stateful rendering, caching, concurrent editing, and exact workspace monitoring as simpler than they are.
4. Its competitive premise is already out of date. Agentic multitrack products and incumbent DAW APIs now exist.

The recommended direction is:

> Build a local-first, model-portable production workbench that turns a producer’s authorized sounds and direction into editable, reversible arrangements and mix revisions, with every action inspectable and exportable.

Start with one narrow promise:

> Given a folder of user-supplied samples the producer is authorized to use and a brief, produce several editable 8–16 bar directions, make precise revisions, render honest A/B comparisons, and export stems plus MIDI into the producer’s existing workflow.

Prove that loop before building a full device catalog, semantic embeddings, stem separation, global preference learning, a visual DAW, or autonomous full-song production.

### Recommendation at a glance

| Area | Recommendation |
|---|---|
| Core thesis | Keep “high leverage plus high control”; reframe it as delegation without surrender. |
| Initial user | A technically comfortable, sample-based producer who already uses a DAW and coding agents. |
| First product | A sample-to-editable-draft workbench, not an Ableton replacement. |
| Architecture | Always own the typed operation vocabulary, artifact/analysis contracts, and evaluation loop. Own a canonical project model and reference renderer only if the IR-authoritative branch earns those roles. |
| Greenfield engine | Treat as a hypothesis to earn, not as an axiom. Benchmark an incumbent-DAW adapter. |
| Project file | If the IR is authoritative, restricted versioned YAML is reasonable and the semantic schema—not YAML bytes—is the contract. A host-authoritative path must not create a second canonical song. |
| Mutations | Direct text remains useful on the IR path, but normal automated writes should use revisioned typed operations; host-native writes need readback and undo/rollback semantics. |
| Perception | Move a thin, measured perception layer into the first vertical slice. Treat metrics as diagnostics, not taste. |
| Devices | Own the AI-native device contract; do not require every DSP algorithm to be invented in-house. |
| DSP language | Run a measured Python/Rust/Faust spike before the sampler becomes production code. |
| Workspace | Defer until the core loop has user evidence. Label instant stem monitoring as a proxy, not exact audio. |
| Autonomy | Make it bounded, resumable, and best-so-far preserving. “Technically complete” is not “artistically finished.” |
| Interoperability | Bring stems, MIDI, and DAWproject export forward. |
| Licensing | Decide open/proprietary distribution before adopting Rubber Band, Essentia, models, or a plugin host. |

### Decisions to retain, revise, test, and defer

**Retain**

- Offline-first rendering.
- A human-readable project bundle on an IR-authoritative path, or human-readable operations/provenance without a duplicate song on a host-authoritative path.
- Sample-first workflow.
- Arrangement-view vocabulary.
- Real-world units at the agent boundary.
- Seeded randomness.
- Outputs as reusable inputs.
- Explicit provenance and named artifacts.
- Human-in-the-loop default.
- Theory as advice, not enforcement.
- UI secondary to the agent loop.

**Revise**

- “The format is the API” to “the versioned semantic model is the contract; text and typed operations are two interfaces.”
- “All devices are built in-house” to “the project owns device semantics, identity, descriptions, reproducibility requirements, and tests.”
- “The agent cannot hear” to a pluggable sensor model combining measurements, optional auditory-semantic critics, and human judgment.
- “Same document, same audio” to a precise environmental reproducibility contract.
- “Stems make workspace controls exact” to a clearly labeled monitor-proxy model followed by a canonical render.
- Git as the entire undo/history mechanism to Git plus immutable snapshots, asset hashes, and revisioned operations.

**Test before settling**

- Whether a greenfield backend is better than controlling Ableton or REAPER for the chosen workflow.
- Whether agents edit the proposed representation more reliably through raw YAML, semantic patches, or a hybrid.
- Whether numerical reports cause better human-rated revisions.
- Whether the target user accepts offline audition latency and a limited built-in device set.
- Whether semantic sample search beats filename, tags, and measured descriptors.
- Whether a compiled DSP kernel is needed for measured workloads.

**Defer**

- A custom whole-project language.
- Most effects and synthesis.
- General plugin hosting.
- Stem separation as a core dependency.
- Global learned preferences and decay.
- Unbounded autonomous song completion.
- Writable or responsive workspace stages beyond a read-only viewer.

## 1. What was reviewed

This review covers all current project documents:

- `docs/README.md`
- `docs/idea.md`
- `docs/spec.md`
- `docs/plan.md`
- `docs/decisions.md`
- the root `idea.md`

There is no implementation yet, so this is a review of the product thesis, specification, architecture, technical choices, and delivery logic rather than a code review.

The documents succeed at progressive disclosure: vision, specification, plan, and decisions are separated cleanly. The decision log is particularly useful. The main documentation problem is not lack of detail; it is that several untested product and engineering hypotheses have been promoted to settled principles.

## 2. First-principles analysis

### 2.1 Start with the desired outcome

A producer does not intrinsically need a new renderer, a YAML file, or an autonomous agent. The desired outcome is:

> Reach a musically useful, editable result with less mechanical effort while retaining control, ownership, and the ability to understand and reverse every material decision.

A simple value model is:

`user value = result quality × controllability × iteration speed × trust − setup and supervision cost`

If any multiplicative term approaches zero, the product fails:

- A beautiful result with no editability competes with prompt-to-waveform tools.
- Perfect editability with mediocre music is a programming demo.
- Good results with slow feedback do not feel collaborative.
- Fast results with hidden or unrecoverable state lose trust.
- A powerful system requiring constant repair does not save labor.

This means render speed is important, but **time to a user-retained editable result** is more important. “The agent finished a song” is not a useful north star by itself.

### 2.2 The irreducible system

The minimum agentic production loop needs six capabilities:

1. **Representation:** a machine-readable model of musical intent and production state.
2. **Operations:** safe ways to inspect and change that state.
3. **Execution:** a backend that turns state into audible artifacts.
4. **Sensing:** measurements and other observations of what was produced.
5. **Evaluation:** a method for deciding what to try next and when to stop.
6. **Human feedback:** preference signals that objective measurements cannot supply.

The current design’s four layers cover most of this, but evaluation and safe operations deserve to be explicit layers. The audio engine is only part of execution. It is not the product’s center of gravity.

The real product loop is:

`intent → plan → reversible operations → validate → render → sense → compare → accept/reject → revise/export`

Every architecture choice should be judged by how much it improves this loop.

### 2.3 Necessary principles versus contingent choices

The following are close to first principles:

- The agent must have structured, discoverable control rather than screen coordinates.
- State must be inspectable, recoverable, and attributable.
- The user must be able to audition work frequently.
- Render and analysis results must identify exactly which state produced them.
- Measurements must distinguish observations from aesthetic judgments.
- A person must be able to interrupt, compare, reject, and preserve.

The following are contingent implementation choices:

- YAML.
- A single `song.yaml`.
- Python.
- A greenfield renderer.
- No plugin hosting.
- Every effect written from scratch.
- Git branches as the user-facing version model.
- A browser workspace.
- A specific pattern notation.
- Numerical perception without an audio-capable critic.

Those choices may prove correct, but none follows inevitably from “the operator is an agent.”

### 2.4 The strongest revised thesis

The phrase “give an agent its own Ableton” is evocative and useful internally. Externally, it points toward feature parity and obscures the more defensible idea.

A stronger thesis is:

> An agent-native music production substrate should preserve the leverage of natural-language delegation and the control of structured production. Its state is transparent, its actions are reversible, its artifacts are reproducible, its sensors are auditable, and its projects can leave the system.

This wording survives changes in model vendors, audio modalities, UI shells, and render backends.

## 3. The 2026 landscape changes the premise

The documents’ “Why not Suno, why not Ableton” section should be rewritten immediately.

### 3.1 Suno is no longer only a waveform vending machine

As of August 2026, [Suno Studio 2.0](https://help.suno.com/en/articles/13670529) documents a chat-driven production environment with audio and MIDI clips, arranging, effects, automation, a playable wavetable synth, stem separation, multitrack export, and conversationally designed effects. It still differs materially from this project: it is a hosted browser product centered on Suno’s generative workflow, lacks conventional plugin support, and its public product documentation does not describe the proposed local text project or deterministic artifact graph. But the categorical comparison in `docs/idea.md` is no longer accurate.

The opportunity is not “Suno has no editable production primitives.” It is:

- Local storage, execution, and analysis rather than mandatory cloud execution.
- Model-portable through a tested contract rather than tied to one generator.
- Inspectable and diffable rather than opaque.
- Reproducible and auditable rather than probabilistic by default.
- Interoperable with a producer’s existing tools.
- Built around material the person supplies and is authorized to use.

[Soundverse’s Studio Agent](https://help.soundverse.ai/create/studio-faq) is another direct comparison. Its official documentation describes a conversational producer that can plan and invoke generation, stem separation, section analysis and edits, effects, layering, and follow-up changes inside a browser workflow. That strengthens the same conclusion: conversational multistep production is no longer unique. The differentiation has to be trust, editability, portability, local execution, workflow fit, or demonstrably better outcomes—not the presence of an agent alone.

### 3.2 Ableton is no longer click-only or completely closed to agents

[Ableton Extensions](https://www.ableton.com/en/live/extensions/) now provides a documented JavaScript SDK, available to Live Suite beta users at the time of this review, that can read an entire Set and rewrite tracks, clips, structure, MIDI, devices, tempo, and related state. The current beta and extension lifecycle may not provide the same headless, deterministic, model-independent environment envisioned here, but “every action is a click” and “an agent will always be a tourist” are now hypotheses to benchmark rather than premises.

Likewise, [REAPER’s official ReaScript API](https://www.reaper.fm/sdk/reascript/reascript.php) exposes a large programmable surface, and an early third-party [REAPER MCP project](https://github.com/danishaft/reaper-mcp) already demonstrates typed, validated, reversible operations for composition, editing, mixing, analysis, and rendering. This is not evidence that the REAPER route is the right product; it is evidence that it is an obligatory comparison.

### 3.3 Open standards reduce the need to own everything

[DAWproject](https://github.com/bitwig/dawproject) is a stable, MIT-licensed exchange format covering substantial track, channel, send, audio, note, automation, device, tempo, and time-signature information, with support listed across several established DAWs. Its own project says it is an exchange format rather than a native DAW format, and real fidelity is limited by what source and destination applications represent, so its XML need not become this project’s authoring representation. It should nevertheless inform the schema and become an early export target with a published compatibility matrix.

[CLAP’s parameter API](https://github.com/free-audio/clap/blob/main/include/clap/ext/params.h) includes stable parameter IDs, plain ranges/defaults, automation and modulation flags, and value-to-text/text-to-value conversion. Third-party devices still have inconsistent semantics, hidden binary state, unsafe code, licensing issues, and portability problems. The blanket claim that modern plugin parameters are always meaningless normalized floats is nonetheless too strong.

Existing Python-facing audio engines also make useful disposable prototypes. [DawDreamer](https://github.com/DBraun/DawDreamer) provides a block-based render graph, beat- and sample-domain automation, MIDI, warp markers, intermediate outputs, plugins, and Faust devices; [Spotify Pedalboard](https://spotify.github.io/pedalboard/) supplies native effects plus VST3/AU loading. Both are GPL-licensed and therefore may be unsuitable for a proprietary shipping core, but they can cheaply test workflows and expose edge cases the spec has not yet modeled.

These sources establish that the capabilities and integration paths exist; vendor feature pages and young open-source projects are not independent evidence of musical quality, reliability, or adoption. Those need direct evaluation.

### 3.4 Why now

Several enabling pieces have crossed a practical threshold at the same time: coding agents can plan and repair structured multistep work; audio-capable models can serve as optional critics; established DAWs expose broader programmable surfaces; DAWproject offers a shared interchange vocabulary; and current Apple Silicon can run substantial local rendering and analysis workloads. None proves demand or musical quality. Together they make the end-to-end experiment materially cheaper than it would have been when the original “give an agent its own Ableton” premise was formed.

### 3.5 Strategic implication

The category now contains:

- Generative studios moving toward editability.
- Incumbent DAWs becoming programmable.
- Agent-to-DAW bridges.
- Open interchange formats.
- Programmatic render engines.

Therefore, “agentic DAW” is a category description, not a moat. Potential sources of advantage are:

- A reliable semantic operation and device ontology.
- An excellent local sample workflow.
- An artifact/provenance graph users trust.
- A calibrated evaluation corpus and feedback loop.
- Tested portability across selected models and DAWs.
- Personal taste data learned with explicit consent.
- Workflow reliability measured across many real production tasks.

The renderer, YAML syntax, generic UI, and access to a foundation model are unlikely to be durable advantages on their own. Even the assets above become defensible only through demonstrated reliability, consented evaluation data, ecosystem adoption, useful integrations, or distribution—not merely by existing in the design.

## 4. Target user and job to be done

### 4.1 Recommended initial user

The best first user is:

> A technically comfortable, sample-based electronic or hip-hop producer on macOS who already uses a DAW and a coding agent, has a meaningful library of material they are authorized to use, values privacy and control, and is willing to finish or polish exported material in an existing DAW.

This user has a concrete pain that the proposed architecture can address without first replacing Ableton:

- Finding sounds in a large personal library.
- Turning breaks, loops, vocals, and one-shots into several directions.
- Creating controlled MIDI and arrangement variations.
- Applying precise revisions consistently.
- Comparing versions without losing the earlier idea.
- Preserving the recipe and moving stems/MIDI into an established workflow.

Likely secondary users are sound designers doing repeatable transformations and creative coders who want a programmable musical representation.

Poor version-one targets are:

- Beginners who primarily want a finished song from a prompt.
- Recording musicians and live performers.
- Professional mixers dependent on particular plugins and hardware.
- Producers expecting a complete replacement for an established DAW.
- Users unwilling to install or supervise a coding-agent workflow.

### 4.2 The initial job

The initial job should not be “make a finished song autonomously.” It should be:

> Convert my material and direction into a small set of useful, editable candidates; revise the candidate I choose; show exactly what changed; and hand it back in formats I can keep working with.

This makes the **candidate** the natural unit of work. It encourages branching and A/B comparison instead of a long sequence of overwrites optimized against imperfect metrics.

### 4.3 Product success metric

Recommended north star:

> **Validated-draft rate:** the percentage of eligible target-user sessions that produce a retained, editable 16-bar draft within a fixed elapsed-time and corrective-intervention budget.

Calibrate the budget in M-1; an initial test might use 20 minutes and no more than three repair/correction interventions. Define session eligibility before recruiting and count every eligible failure in the denominator. “Retained” means the directing producer intentionally exports/reopens the candidate, resumes it in a later session, or uses it downstream after at least 72 hours, and it contains a declared minimum agent contribution such as one accepted musical element or requested material edit. Merely clicking “keep” during the demo is not retention.

Supporting measures:

- Percentage of agent-created clips and revisions retained.
- Median time-to-retained-result, with interventions reported as a guardrail rather than mixed into the same score.
- Concrete revision success rate without manual repair.
- Invalid or silently incorrect edit rate.
- Time to first listenable artifact.
- Warm edit-to-audition latency.
- Human A/B preference after a revision.
- Successful export and reopen rate in the user’s normal DAW.
- Recovery success after interruption, conflict, or process failure.
- User comprehension of what changed and why.

Render time, cache-hit rate, and autonomous task completion matter only insofar as they improve these outcomes.

### 4.4 Substitutes, creative boundaries, distribution, and economics

The main competitor is often the producer’s current practice, not another AI product: a DAW template, macros, a sample browser, manual duplication and editing, or a trusted collaborator. Every pilot should compare complete time-to-retained-result—including setup, prompting, supervision, repair, export, and re-import—against that baseline. A locally rendered workflow can still lose if it adds more coordination than it removes.

The product must also learn which labor users actually want to delegate. For some producers, auditioning sounds, manipulating notes, or struggling toward an arrangement is the art rather than overhead. Research should force-rank search, cleanup, controlled variations, arrangement, sound design, mixing, and mastering into **delegate**, **co-create**, and **keep manual**. Do not infer enthusiasm for creative delegation from enthusiasm for automation in general.

Plausible acquisition paths are:

- A GitHub/CLI package plus agent skill for technical early adopters.
- An Ableton or REAPER extension that meets producers in an existing workflow.
- Stems and MIDI as broadly compatible exit paths, plus DAWproject for named source/destination versions whose supported fields have passed a published fidelity test.
- Later, a packaged desktop application if clean-machine tests show the terminal is the limiting factor.

Plausible project postures are open research, open-source/open-core infrastructure, a paid local desktop or extension, or a commercial SDK. A cloud-compute subscription is less aligned with the clearest local-first advantage, though optional paid remote models could coexist with a local core. Do not settle the business model from armchair reasoning, but test commitment: ask design partners to install it, bring a live project, return for another session, and eventually pay or sign a paid-pilot letter of intent. Local compute keeps marginal render cost low; packaging, host compatibility, expert support, and listening QA are more likely to dominate operating cost.

Generative audio need not be ideologically excluded. A future model can be a typed, provenance-bearing instrument or candidate source behind the same operation and artifact contracts. The product’s identity should remain structured control and ownership of the editable result, not loyalty to either synthesis or generation.

## 5. Falsifiable hypothesis stack

The project currently presents several hypotheses as design facts. They should be ranked and tested in dependency order.

| Hypothesis | Risk | Fast evidence | Suggested initial gate | Response if it fails |
|---|---|---|---|---|
| The target user has recurring production work they actively want to delegate. | Existential | Observe 12–15 exact-persona producers, force-rank delegate/co-create/manual tasks, and run repeated pilots. | At least half identify a weekly delegation job; several return for repeated use. | Narrow the job or focus on assistance rather than delegation. |
| An LLM can produce musically useful structured candidates from samples. | Existential | Blind evaluation against templates, manual baselines, and incumbent tools. | Agent material is retained in a majority of real sessions. | Focus on retrieval, transformations, and constrained assistance. |
| Structured editability is valuable enough to offset setup and slower generation. | Existential | Compare end-to-end time and retained output against the user’s current workflow and fast generative alternatives. | Users save net time, continue editing/exporting, and return. | Reduce workflow overhead, add a host/generative backend, or change the job. |
| The proposed representation improves agent reliability. | High | Run the same task corpus through raw YAML, semantic patches, and a DAW adapter. | High valid-edit and semantic-success rates with low repair cost. | Change the interface; do not defend the syntax. |
| Measured perception improves human-rated results. | High | Plant technical problems and run blinded before/after listening tests. | High diagnostic precision and consistent directional improvement. | Keep reports descriptive and require another critic/human. |
| A greenfield backend is materially better than controlling a mature DAW. | High | Implement five identical workflows through a tiny renderer and Ableton/REAPER. | Clear gains in intervention count, reliability, reproducibility, or portability. | Ship an adapter/control layer first. |
| Built-in devices can satisfy the initial user. | High | Complete real pilot pieces without external plugins. | Most users can reach material they want to continue. | Support curated devices/plugins or export earlier. |
| The offline loop feels collaborative. | High | Measure realistic warm edits and observe user patience. | Latency meets a user-tested threshold, not a toy benchmark. | Reduce scope, compile the hot path, or add a realtime backend. |
| Semantic sample search beats simpler retrieval. | Medium | Human-labeled query set versus filenames/tags/descriptors. | Material improvement in Recall@5 or nDCG and selection time. | Keep the lightweight baseline. |
| Global preference learning is trusted. | Medium | Show every inferred preference for confirmation and scope selection. | High confirmation and low deletion/correction rate. | Keep memory explicit and project-scoped. |
| Bounded autonomous iteration improves a draft. | High | Preserve every candidate and run blind best-of comparisons. | Later best-so-far candidates beat the first draft consistently. | Sell guided iteration, not autonomy. |
| Users will install a coding-agent workflow. | High | Clean-machine onboarding tests. | Most target pilots reach first render without developer help. | Package an app or DAW extension sooner. |
| A viable commercial posture exists, if commercialization is intended. | High | Paid pilot, preorder, or signed letter-of-intent test after value is demonstrated. | Several target users make a real commitment rather than offering praise. | Treat it as research/open infrastructure or revisit the buyer and channel. |

The highest-risk work is not writing a mixer. It is proving user demand, agent musical usefulness, and the value of the feedback loop.

## 6. SWOT analysis

| Strengths | Weaknesses |
|---|---|
| Clear high-leverage/high-control product tension. | No implementation or empirical validation yet. |
| Coherent agent-first vocabulary and project model. | Current “v1” combines several large products. |
| Local, transparent, model-portable direction. | Initial customer and purchasing context are not explicit. |
| Real units and self-describing parameters are strong agent affordances. | Competitive claims in the vision are already outdated. |
| Sample-first workflow is concrete and creatively credible. | Product thesis is entangled with YAML, Python, greenfield DSP, and UI choices. |
| Offline-first execution avoids early callback and recording complexity. | Numerical descriptors are asked to carry too much aesthetic judgment. |
| Named artifacts, provenance, and reversibility build trust. | No-plugin/no-import boundaries reduce appeal to experienced producers. |
| Outputs-as-inputs can compound into a useful personal library. | Raw file editing and one-file concurrency are fragile. |
| Decision log and explicit exclusions reduce accidental drift. | Region rendering and exact stem monitoring are understated. |
| One semantic core can support CLI, adapters, and UI. | Licensing, rights, privacy, migration, and resource controls are missing. |

| Opportunities | Threats |
|---|---|
| Own the trusted local and inspectable end of the agentic-music category. | Suno and similar studios are rapidly adding structured editing. |
| Let producers use authorized personal material and keep production data local by default. | Ableton and other incumbents can add agents atop mature engines and ecosystems. |
| Export through MIDI, stems, and DAWproject instead of demanding replacement. | Existing DAW bridges may make a new renderer unnecessary. |
| Turn an agent operation ontology and evaluation suite into reusable infrastructure. | Foundation-model improvements can commoditize composition and tool selection. |
| Use Ableton/REAPER adapters as distribution channels or benchmarks. | Mediocre early DSP can discredit the whole workflow. |
| Add audio-understanding critics without binding the core to one model. | Metric optimization can confidently make music worse. |
| Serve sound design, batch variation, and procedural-media workflows later. | Producers have strong plugin dependence and switching costs. |
| Build consented taste and recipe data that improve over repeated use. | Copyright, stem-separation, and sample-clearance mistakes create trust risk. |
| Open formats and modern plugin APIs reduce integration cost. | Native/model dependencies can block packaging or commercialization. |
| Reproducibility can matter for education, games, and team workflows. | The overlap of serious producers and coding-agent users may be small. |

## 7. Strategic options

### Option A: Continue with the full greenfield DAW

**Advantages**

- Maximum semantic and rendering control.
- Strong local determinism and portability for the supported subset.
- No host application or plugin behavior to work around.
- A coherent system can eventually serve agents and a workspace elegantly.

**Costs**

- Slowest route to validating musical usefulness.
- Requires DAW engine, DSP, MIR/ML, asset management, agent framework, and UI expertise.
- Early sound quality and interoperability will trail established tools.
- The project competes directly with products that already have users and realtime engines.

**When it is rational**

- The main goal is research, craft, learning, or building a long-term open instrument.
- Owning the engine is intrinsically part of the mission.
- A benchmark demonstrates that incumbent integrations cannot meet the core reliability or transparency requirement.

### Option B: Build an agent layer over Ableton or REAPER

**Advantages**

- Fastest route to real users, high-quality devices, plugins, realtime audition, and mature edge-case handling.
- Lets the team test agent planning, operation design, perception, and feedback immediately.
- Fits producers’ existing projects and habits.

**Costs**

- Host dependency, version churn, hidden state, and imperfect headless behavior.
- Weaker portability and deterministic reproduction.
- A host-authoritative product needs native snapshot/undo/readback guarantees; a mirrored text song creates dangerous dual authority unless round-trip synchronization is explicitly the product.
- The product may become an integration feature rather than an independent platform.

**Recommendation**

Build a disposable benchmark spike, even if this is not the intended final architecture. The current thesis has not earned the right to skip this comparison.

### Option C: Own a neutral agent-native IR with backend adapters

**Advantages**

- Keeps the valuable semantic model, history, provenance, and tested model portability.
- Can target a minimal internal renderer, existing DAWs, and interchange formats.
- Gives projects an escape route and makes the operation layer the center.

**Costs**

- Adapter complexity and semantic mismatch.
- Risk of designing a lowest-common-denominator model.
- Exact reproduction differs by backend.
- Source-of-truth and round-trip policy become architectural decisions: either the IR is authoritative and host-only edits can diverge, or host state is authoritative and inspectability/reproducibility guarantees weaken.

**Recommendation**

This is the strongest long-term strategic architecture if the IR is intentionally capability-aware rather than pretending all backends are identical. Opaque or backend-specific devices must be represented honestly and backend/version/state must participate in render identity.

### Option D: Build a narrow sample workbench on the cheapest credible executor

**Advantages**

- Fastest distinctive end-to-end product.
- Aligns with the best-defined persona and workflow.
- Retains text-native experimentation; it can retain strong local determinism if the internal executor wins the bake-off.
- Avoids most plugin, recording, and realtime scope.

**Costs**

- Narrower market and initially modest sound palette.
- Users still need another DAW for finishing.
- Could remain a useful tool rather than grow into a full platform.

**Recommendation**

This is the best first product shape. Its executor should be earned in M0A rather than embedded in the product definition.

### Recommended sequence

Use Option D to validate the workflow, compare the execution paths in Options A and B at disposable scale, and evolve toward Option C if users value the semantic project outside a single backend. Pursue the breadth of Option A only where measured limitations justify owning more of the stack.

Do not maintain two production backends prematurely. The point of the early adapter and renderer spikes is to choose intelligently, not to create permanent parallel implementations.

## 8. Documentation coherence and contradictions

The documents largely agree with one another, but several phrases promise mutually incompatible behavior or blur the line between the first release and the end state.

| Topic | Current tension | Recommended correction |
|---|---|---|
| “Day one” | The listed day-one workflows require nearly every phase, including separation, search, effects, synthesis, perception, and autonomy. | Rename the list “target workflows” and define one true day-one workflow. |
| Version one | `spec.md` says the workspace and daemon are out of version one, while `plan.md` starts U0 after Phase 0 and the CLI includes `serve`. | Define `prototype`, `MVP`, `private alpha`, `v1`, and `end state` explicitly. Put each capability in exactly one. |
| UI timing | The workspace is described as built second and also as a parallel track beginning after Phase 0. | Do not begin it until the agentic value gate passes. A disposable artifact player is not the workspace. |
| Platform | The product is called “macOS-native,” but the proposed shell is a browser and most of the stack is portable Python. | Say “macOS-targeted local application” until a native shell or OS-specific integration exists. |
| State interface | The agent directly edits state, but the workspace and CLI are simultaneous writers. | Keep readable files, but coordinate automated writes through revisioned transactions. |
| Source of truth | The workspace supposedly holds no state outside the file, yet it needs playhead, zoom, selection, queued jobs, and render progress. | Say “no authoritative musical state outside the project”; ephemeral client and job state is allowed. |
| Communication | There is allegedly no channel besides the directory, while `daw serve` uses WebSockets and accepts jobs. | Say “no agent-specific hidden state channel”; files hold durable music, RPC coordinates work. |
| Render history | Renders are not versioned, but every step must remain listenable and mixes use revision names. | Store content-addressed artifacts plus a small versioned catalog and retention policy. |
| Python decision | D18 says Python first, while the device language remains open until Phase 3. | Treat Python as orchestration/prototype policy; decide production DSP before Phase 1 devices. |
| Exact monitoring | D22 forbids approximations, but the workspace’s instant path skips sends, groups, and the master chain. | Call it an audition proxy and distinguish it from canonical/published audio. |
| Time | Time is said to be musical “always,” while source downbeats use seconds and device parameters use milliseconds. | Say timeline time is musical; source time is integer frames; DSP duration may be physical or tempo-synced. |
| Hearing | “The agent cannot hear” is stated as timeless fact. | Define a sensor interface; the active harness may use measurements, images, audio-capable models, and/or a human. |
| Key | No session-level key avoids a false constraint, but inferred or intended tonal context still matters. | Keep key non-binding; allow optional tonal annotations and versioned analysis results outside the hard constraint model. |
| Git | Git is described as both checkpoint history and complete restoration of sound while media/renders are unversioned. | Use Git for readable state history and an immutable asset/artifact manifest for audible restoration. |

The decision log should remain, but a decision marked “settled” should mean “supported by current evidence,” not “immune to new facts.” The changed landscape, product gates, authority branch, and audio/security correctness issues are valid reasons to revisit D1–D3, D8, D10, D13, D15, D18–D24.

## 9. Specification review: issues to resolve before implementation

### 9.1 P0: concurrent writes can silently lose work

Atomic rename prevents readers from seeing a half-written file. Re-reading before a write reduces some stale-edit failures. Neither is compare-and-swap:

1. Writer A reads revision R.
2. Writer B reads revision R.
3. Writer B writes R+B.
4. Writer A writes R+A over it.

The file is valid, but B’s edit is gone. Git does not automatically detect an overwritten, uncommitted edit.

If the text bundle remains authoritative, use this contract:

- Every authoritative authoring snapshot has an `authoring_revision` hash covering all project inputs relevant to concurrency, including non-audio fields that must not be lost.
- Automated mutations carry `base_authoring_revision` and explicit field/object preconditions.
- A project-scoped lock serializes the final read/validate/apply/write step.
- Reject a stale base by default. Auto-merge only operation classes proven commutative, with stable identities, field preconditions, and validation of the combined result; “different objects” alone does not make changes independent.
- All other stale changes fail with a structured conflict; they never guess.
- Publish a whole-bundle generation manifest or immutable snapshot, not merely a sequence of individually atomic files. Otherwise the renderer can observe a new `song.yaml` with old patterns or patches.
- Automated writers must transact through the project store. Raw text editing is an explicit exclusive/single-writer mode; on detecting uncoordinated changes, capture an observed-version backup and retry or report conflict.
- Writes use unique temporary paths, flush as appropriate, and atomically publish the new generation pointer.
- The result returns the new revision and a semantic diff.
- Render jobs capture an immutable authoring snapshot and publish both its `authoring_revision` and the separately resolved `render_fingerprint`.
- A stale job may finish and remain addressable, but must never replace `latest`.

Direct file editing can remain an escape hatch and a pleasant single-writer authoring mode. The harness should normally use `daw apply` or equivalent once a workspace or second agent is active. A host-authoritative architecture needs a parallel `host_revision` plus native snapshot/undo/readback contract; it should not mirror the host into a second writable bundle.

### 9.2 P0: exact region rendering is state restoration, not slicing

A compressor, filter, reverb, delay, LFO, sampler voice, random modulator, or lookahead limiter depends on samples outside the requested region. A change at bar 9 may alter device state through the end of the song. Rendering only bars 25–29 from a reset device will generally not equal those bars from a full render.

Each render node therefore needs to declare:

- Latency.
- Future lookahead.
- Required history or warm-up.
- Tail behavior and maximum/unknown tail.
- Reset semantics.
- Serializable state, if exact checkpoints are supported.
- Whether parameter or topology changes invalidate all later chunks.

Every render profile also needs a deterministic finite-end rule. For unknown or theoretically infinite feedback tails, define an explicit transport end plus maximum tail duration and truncation/fade policy. Include that policy in the render fingerprint and region-invalidation rules.

Define two distinct operations:

- **Isolated render:** deliberately resets at the requested start, useful for auditioning a self-contained clip.
- **In-context render:** guarantees equivalence to the corresponding window of the full project, using a valid state checkpoint or rendering from an earlier safe point.

An exact region engine requires state checkpoints, pre-roll, post-roll, latency compensation, downstream invalidation, and equivalence tests. For the MVP, use whole-track caching and full downstream rerenders. Add chunk/state caching only after profiling shows it is needed.

### 9.3 P0: caching is a dependency-graph problem

“Only the changed track rerenders” is true only for the Phase 0 linear mixer. Once routing exists:

- A kick edit changes a bass compressor’s sidechain.
- It also changes any drum group, shared return, and master.
- A return depends on every sender.
- A group or nonlinear master cannot be reconstructed from an arbitrary post-processed stem.

Compile the project to an explicit render DAG. Cache graph nodes or taps using Merkle-style dependency hashes. A source change invalidates all descendants and no unrelated branches.

A cache key must include at least:

- Semantic node specification.
- All upstream keys.
- Referenced asset byte hashes.
- Resolved patches and defaults.
- Device and engine versions.
- Render profile, sample rate, channel layout, and canonical block size.
- Seed namespace.
- Any algorithm/model dependency that affects output.

Start with coarse nodes and whole-track PCM. Fine-grained chunks are an optimization, not a prerequisite for hearing the first song.

### 9.4 P0: stem playback is not exact through a real mix graph

The current workspace model says mute, solo, volume, pan, and sends can be applied instantly over cached stems and remain exact. That is false once any of the following exists:

- Post-fader sends.
- Sidechains.
- Group or return processing.
- Nonlinear bus compression, saturation, or limiting.
- A master chain.
- A different pan law or channel conversion in Web Audio.

Changing a fader can change the signal entering a bus compressor; muting a kick can change bass sidechain gain reduction; changing a send cannot be represented by scaling only the direct stem.

Recommended product language:

- **Canonical audio** is always rendered by the authoritative graph and device implementation.
- **Monitor audio** may use cached taps for instant, provisional gain/pan/mute/solo audition.
- The workspace visibly labels the monitor as provisional when it diverges from the canonical graph.
- The daemon coalesces rapid changes, cancels stale work, rerenders affected dependencies, and crossfades to the canonical revision when ready.
- Sends and known dependent controls always request a graph render.

If even a labeled proxy is unacceptable, remove instant controls and accept canonical render latency. The spec cannot honestly promise both zero approximation and instant arbitrary mixing over stems.

### 9.5 P0: the time model needs exact domains

The friendly `bar.beat.fraction` notation is not a sufficient formal time model:

- A decimal-looking fraction is ambiguous.
- Common tuplets are not represented exactly by arbitrary decimals.
- “Beat” is ambiguous in compound meter.
- Source offsets, trim points, and detected downbeats are naturally sample/frame time.
- Converting musical time to samples needs a deterministic rounding rule.

Use three explicit domains:

1. **MusicalTime:** exact rational quarter-note positions internally. Quantize only at an I/O boundary such as MIDI export, where the target PPQ and rounding/error policy are explicit.
2. **AssetTime:** integer frames in the source asset’s native sample rate.
3. **RenderTime:** integer frames in the render sample rate.

Seconds are an input/display convenience, not canonical asset identity. A constant-tempo version can still compile through a one-segment `TempoMap` abstraction so later tempo changes do not require replacing every interface.

Represent BPM as an exact decimal or rational value in authoritative state. Define one deterministic rational-to-frame rounding rule. This avoids silently losing tuplets that do not divide a chosen PPQ; the initial grammar may still restrict which denominators users can author.

Specify:

- Whether BPM means quarter notes per minute and how compound meter is displayed.
- One- versus zero-based indexing.
- Half-open ranges `[start, end)`.
- Musical-time-to-frame rounding.
- Tuplet syntax.
- Note-on/note-off ordering at equal positions.
- Notes crossing clip and repeat boundaries.
- Repeat truncation.
- Pickup/negative positions, even if rejected in v1.
- Project end versus rendered tail.
- Section overlap, gaps, and nesting.

The principle should be: **timeline positions are musical; media offsets are frames; DSP parameters use the units natural to the parameter.**

### 9.6 P0: reproducibility needs environmental closure

A seed is necessary but insufficient. “Same document, same audio” changes when any of these changes:

- Sample bytes.
- A library patch or device default.
- Engine, compiler, native library, or model version.
- Render quality profile.
- Sample rate, channel layout, or block size.
- Floating-point reduction order.
- CPU architecture, SIMD path, or math library.
- Decoder/resampler behavior.

Keep identity roles separate:

- `authority_revision`: the optimistic-concurrency identity—an `authoring_revision` over all authoritative bundle inputs on the IR path, or a `host_revision` over the selected saved host state/journal on the host-authoritative path.
- `render_fingerprint`: resolved sound-affecting state plus source/decoded asset hashes, patches/defaults, engine and dependency builds, render profile, block/reduction policy, dither policy, and seed scheme.
- `artifact_id`: the produced bytes and manifest, normally content-addressed.
- `analysis_id`: the artifact identity plus analyzer/model/configuration version.

Recommended guarantee:

- Default to numerically or perceptually equivalent output within a declared tolerance for supported environments.
- Offer a **certified bit-exact tier** only for operations, builds, architectures, codecs, schedules, and profiles that repeatedly pass byte-identity tests. A same-build label alone is not proof when native math, threading, codecs, or ML components can vary.
- Never use the same `render_fingerprint` for executions that are permitted to emit different bytes unless artifact identity is deliberately separated and the guarantee says so.

Use stable event ordering and fixed reduction order. Randomness should use independent keyed streams derived from a global seed plus stable device/event/voice identity; a single sequential random generator will diverge under parallel, cached, or regional execution.

An engine or default upgrade can change `render_fingerprint` without creating an edit conflict, while a concurrent brief or metadata edit can change the applicable authority revision even when it does not alter sound. The artifact manifest should distinguish both from a Git commit. Dirty but valid project state must still have unambiguous authority and render identities.

### 9.7 P0: the device decision occurs before Phase 1

The plan delays the Python/Rust decision until before Phase 3 because effects are numerous. But the sampler and drum rack in Phase 1 are stateful devices too. They need polyphony, envelopes, interpolation, voice stealing, MIDI event timing, and potentially realtime-safe execution.

There are two honest paths:

- **Offline product:** choose Python/NumPy for simplicity, optimize for whole-array or large-block processing, and explicitly accept that future live playback may require a rewrite.
- **Shared offline/realtime product:** choose a compiled, realtime-disciplined kernel before productionizing the sampler. Rust is plausible; Faust deserves a serious spike.

A Python API shaped like block processing is not itself realtime-safe. It can also be slower than whole-array processing if it repeatedly crosses Python for small blocks. Do not pay a permanent performance cost merely to preserve a hypothetical future.

### 9.8 P0: perception cannot wait until Phase 4

The project’s novel claim is a closed agent loop, yet the plan builds the document, instruments, sample operations, effects, and routing before giving the agent even a minimal way to assess its output.

Move this thin slice into the first end-to-end milestone:

- Peak/over-range, detected clipping, NaN/Inf, and silence checks.
- Integrated loudness.
- Coarse spectral-band energy.
- Waveform and log-frequency spectrogram.
- Structured comparison between two renders.
- MIDI/project checks for density, ranges, collisions, and section occupancy.

Then run human A/B evaluation. If the agent cannot use these observations to make preferred revisions, improve the loop before expanding the DAW.

### 9.9 P0: asset paths and mutable library objects break restoration

A project that points to `samples/kick.wav` or a global patch by path is reproducible only while those bytes remain unchanged. A Git checkout of text cannot restore a deleted linked sample or a modified global preset.

Recommended asset model:

- Compute a source-blob SHA-256 for imported bytes; optionally also compute a canonical decoded-PCM hash when equivalence across containers or decoders matters, and never confuse the two.
- Use immutable content identity and a separate friendly alias.
- Store or pin the decoder and source format metadata that affects interpretation.
- Vendor project assets by default for portability, or reference a content-addressed global store and provide a bundle/export command.
- Pin patches, device schemas, model weights, and analysis models by digest.
- Verify every reference in `daw check`.
- Record lineage without leaking private absolute paths.
- Make promotion freeze both the artifact bytes and the recipe; avoid live provenance cycles.

Git remains a valuable human-facing view of state history. An immutable snapshot and content-addressed asset catalog make the sound restorable.

### 9.10 P1: object identity is not fully stable

The spec promises stable IDs but example clips, automation envelopes, points, and modulators do not have them. Array selectors such as `osc[1]` change meaning when items are reordered. Human-readable dotted paths break if a parent or display name is renamed and need escaping if IDs contain dots.

Use:

- Immutable machine identity for every referential object.
- Editable display name and optional stable readable slug.
- Typed internal references.
- Dotted selectors only as a CLI presentation syntax.
- An explicit restricted ID grammar.
- Semantic diffs keyed by identity rather than list position.

Materialize chord-symbol expansion into explicit notes when it is accepted. Otherwise a future theory-library update can silently change an old project’s audio.

### 9.11 P1: audio graph semantics are incomplete

Before effects and routing, define:

- Normalized representation of ordinary, group, return, and master tracks.
- Whether “group parent” implies output routing or is only organization.
- Clip/instrument/insert/fader/pan/send order.
- Send tap points and whether returns/groups can send.
- Sidechain control edges and cycle rules.
- Mute/solo behavior for pre-fader sends and sidechains.
- Solo-safe returns.
- Pan law and mono/stereo conversion.
- Overlapping-clip behavior.
- Internal sample type, summing precision, headroom, and clipping policy.
- Device latency compensation.
- Bypass and wet/dry topology.
- Tail-after-project behavior.
- Exact definitions for dry, post-insert, pre-fader, post-fader, group, return, and delivery stems.

Rejecting graph feedback in v1 is sensible. Feedback inside a delay device can remain encapsulated and deterministic.

### 9.12 P1: automation and modulation need executable semantics

“Any parameter is automatable” is too broad. File paths, topology, oversampling mode, and some enumerations cannot safely change per sample.

Each parameter descriptor should specify:

- Type: quantity, integer, Boolean, enum, string, asset reference, or object reference.
- Unit and internal base unit.
- Legal range and default.
- Mutability class: audio-rate, control-rate, event-boundary, or graph-rebuild.
- Interpolation domain: linear amplitude, dB, log frequency, pitch, stepped, or custom.
- Smoothing policy.
- Clamp, wrap, or reject behavior.
- Modulation support and combination rule.

Also define:

- Value before the first and after the last automation point.
- Which point owns each segment’s curve.
- Exponential curves near or across zero.
- Ordering of static value, automation, modulation, velocity/note expression, and range limiting.
- Global versus per-voice modulation.
- LFO phase/retrigger behavior.
- Seed derivation for random modulation.

### 9.13 P1: pattern semantics need testing and completion

Keeping separate drum-grid and melodic-event notations is reasonable because they optimize different reading tasks. Both should compile to one note/event IR.

The grammar still needs:

- Schema/version marker.
- Exact grid and PPQ relationship.
- Velocity digit mapping.
- Nudge units and bounds.
- Row-length validation derived from pattern length, grid, and meter.
- Rests, ties, overlaps, retriggers, legato, and note-off behavior.
- Pitch spelling and octave convention.
- Tuplets.
- Sustain/controller data policy.
- Order of quantize, swing, nudge, and seeded humanization.
- Repeat truncation and notes extending past pattern end.

Benchmark the notation with several fresh agents. Measure edit correctness, token use, repair attempts, and semantic accuracy; do not choose based only on visual compactness.

### 9.14 P1: CLI interaction has unresolved behavior

The import policy says to ask the user when tempo confidence is low, while the CLI is explicitly noninteractive. Return a structured `needs_decision` result with candidates and confidence; the harness asks the user or reruns with an explicit choice.

Add or plan these verbs:

- `daw init`
- `daw inspect` or `daw query` for context-sized project slices
- `daw apply --base <revision> <patch>`
- `daw diff`
- `daw migrate`
- `daw doctor`
- `daw revert`
- `daw artifacts list/prune`
- `daw cache stats/prune`

Every structured response needs a versioned error/event schema, stable exit categories, the applicable `authority_revision`, relevant `render_fingerprint`/`artifact_id`, and explicit partial/uncertain outcomes. Long jobs need cancellation, timeout, resume policy, validated publication/selection, and cleanup of partial artifacts.

### 9.15 P1: “finished” needs a bounded definition

Technical checks can establish that an export exists, does not clip, includes required sections, and meets an explicit delivery target. They cannot certify that a song is compelling, original, emotionally appropriate, or finished in the artistic sense.

Autonomous work should be bounded by:

- Wall-clock, iteration, render, token/model, disk, and branch budgets.
- Best-so-far preservation.
- Candidate branching rather than serial destruction.
- A non-improvement/plateau rule.
- Explicit escalation and human-review gates.
- Resumable state after process termination.
- A distinction between “deliverable complete” and “artistically accepted.”

“Do not stop until done” is good user language. Internally it must compile to a finite job contract.

## 10. Recommended target architecture

The project should separate its common operation/evaluation contracts from the authority mechanism selected in M0A. A host-authoritative product must not secretly maintain a second canonical musical project:

```text
             agent CLI + workspace
                      |
      typed commands, queries, and events
                      |
         authority chosen by M0A ADR
              /                 \
      IR-authoritative       host-authoritative
              |                 |
      project transactions    host operations + native
      + canonical bundle      project/revision/undo
              |                 |
      immutable generations   saved snapshots + read
      + render resolution     projections/journal
              |                 |
      internal renderer or     host render/export
      capability-aware adapter |
              \                 /
       immutable artifacts + provenance
                    |
      versioned measurements and critics
                    |
       reports, A/Bs, feedback, next action
```

### 10.1 Component boundaries

| Component | Responsibility |
|---|---|
| Common operation/domain schema | Typed commands, queries, events, units, result identities, errors, artifacts, and evaluation vocabulary shared where semantics truly match. |
| IR project store | IR-authoritative branch only: parse/normalize/serialize, `authoring_revision`, locks, patch transactions, generation publication, snapshot capture. |
| Host authority adapter | Host-authoritative branch only: native revision/undo or compensating rollback, saved snapshots, validated read projections, operation journal, capability/version contract. |
| IR resolver | IR-authoritative branch: resolve defaults, patches, assets, library objects, environment lock, and provenance closure. |
| Compiler/backend mapper | IR-authoritative branch: validate capabilities and graph, schedule events, calculate latency, create `render_fingerprint`, and emit an internal render plan or host operations. |
| Render graph | Deterministic nodes, taps, routing/control edges, state/latency/tail contracts. |
| Scheduler | Dependency execution, cancellation, priorities, stale-job suppression, progress. |
| Internal DSP kernel | Authoritative instruments/effects and deterministic processing when the internal-renderer path is selected. |
| Host executor adapter | Capability map, reversible host operations, state capture, version compatibility, and explicit opaque/backend-specific state; may execute an authoritative IR or own authority itself, but those modes must not be conflated. |
| Cache/artifacts | Merkle keys, PCM/state objects, atomic publication, catalogs, retention, corruption recovery. |
| Analysis | Versioned numeric features and optional model adapters; observations before prose. |
| Library | Immutable assets and mutable metadata overlays; search indexes are rebuildable derivatives. |
| Application service | One command/query API used in-process by CLI and over local RPC by the daemon. |
| Agent harness | Planning, skills, bounded jobs, feedback interpretation; depends on public application contracts. |
| Workspace | A client of operations, snapshots, artifacts, and progress; no separate musical implementation. |

The CLI and daemon should remain thin shells, as the existing documents propose. “Thin” means they share application services and contracts, not that the daemon is absent from coordination.

### 10.2 Authority-specific snapshots and publication

If the IR/bundle is authoritative, a render must never read a changing project tree piecemeal. The project store should:

1. Under the project transaction protocol, capture every authoritative source file into one immutable `AuthoringSnapshot` and atomically publish a generation manifest.
2. Parse and validate that snapshot, then compute its `authoring_revision` before resolving environment-dependent state.
3. Resolve defaults, immutable asset/library references, patches, devices, backend capabilities, and the generated environment lock into a separate immutable `RenderSnapshot`.
4. Compute the `render_fingerprint` from sound-affecting closure.
5. Compile and execute only from the `RenderSnapshot`.
6. Hash produced bytes/manifests as `artifact_id`; derive each `analysis_id` from that artifact and analyzer closure.
7. Tag progress and logs with all relevant identities, and update friendly aliases only if their expected authority revision is still current.

Non-audio fields such as color, journal text, selection, or display order should change the authoring revision when they are authoritative but should not invalidate sound. The renderer derives sound-only fingerprints for cache keys. Raw editors that bypass the transaction protocol cannot receive strong cross-file concurrency guarantees; require exclusive mode or detect a moving tree with read-hash-read/retry and preserve the observed version.

If the host project is authoritative, do not create a second canonical bundle. Capture a `host_revision` from a saved host snapshot plus pinned host/plugin state and the validated operation journal; expose a derived read projection for inspection, not for round-trip authorship. When the host cannot commit a compound operation atomically, apply it in an undo group or disposable duplicate, validate readback, and select/publish the new snapshot only after success. Failed partial mutations must be rolled back or quarantined and never become the selected revision or source of a published artifact. If the API cannot provide adequate snapshot, rollback, and readback behavior for the target tasks, the host-authoritative path fails the M0A safety gate rather than inheriting guarantees it cannot implement.

### 10.3 Suggested bundle shape

On the IR-authoritative branch, keep the current directory-oriented idea, but regard the **bundle** rather than one YAML file as the document:

```text
project.yaml             schema version, globals, structure, references
tracks/                  optional canonical track shards when scale demands
patterns/                compact versioned pattern files
patches/                 pinned text device definitions
brief.md                 current creative intent
events/                   transaction-owned or one-event-per-file journal
assets/                  vendored or content-addressed media
artifacts/catalog.json    small versioned artifact index
daw.lock                 generated resolved closure, never an independent source of intent
generations/             immutable source manifests/snapshots
.cache/                  rebuildable node/state/analysis objects
```

For the feasibility slice, one `song.yaml` plus a brief and a small set of separate patterns/patches is simpler. Add file-per-track only when real context size or conflicts justify it. Cross-file transactions are themselves complexity. Do not allow several writers to append casually to one `events.ndjson`; let the transaction owner serialize a journal or publish content-addressed event records.

### 10.4 Device processing contract

Any production device interface should expose:

- `prepare(sample_rate, max_block_size, channel_layout)`
- Sample-offset note and parameter events.
- `process(inputs, outputs, events, frames)`
- Deterministic `reset`.
- Latency and lookahead.
- Tail behavior plus a profile-level finite maximum/truncation/fade policy.
- State snapshot/restore if exact region checkpoints are supported.
- Seed namespace.
- Bypass and wet/dry semantics.
- Versioned descriptor and state schema.

If realtime support is a committed goal, the process path must also forbid allocation, locks, file/network I/O, unbounded work, and interpreter callbacks.

### 10.5 Artifact model

Separate:

- **Cache objects:** disposable, content-addressed acceleration data.
- **Candidate artifacts:** named renders a user may compare and keep.
- **Published exports:** immutable deliverables with a full manifest.
- **Friendly aliases:** `latest`, section names, or revision labels pointing to immutable objects.

Add per-key single-flight/advisory locking, unique temporary paths, checksum-before-publish, loser reuse, stale-lock recovery, disk quotas, last-access metadata, pinning, garbage collection, and recovery from corrupt or partial cache entries. Candidate or published artifacts referenced by a committed catalog/checkpoint are pinned and never treated as disposable cache. A long autonomous run can otherwise create gigabytes of PCM without limit.

## 11. Technical choices

### 11.1 Project syntax: use restricted YAML, not a custom language yet

Recommended choice for the first implementation:

This applies when the text IR is authoritative. A host-authoritative product may still use the same types for commands, read projections, briefs, and artifacts, but it should not serialize a competing canonical song.

- YAML 1.2 authoring syntax parsed by a safe loader into a strict typed model.
- Reject duplicate keys, anchors, aliases, merge keys, custom tags, implicit timestamps, unknown fields, NaN/Inf, and paths outside allowed roots. Define Unicode normalization per field: normalize identifiers and semantic tokens consistently, preserve user prose where possible, and account explicitly for macOS decomposed filenames.
- Quote all domain-specific scalars such as positions, durations, percentages, pitches when ambiguous, and unit-bearing values.
- Emit one documented canonical block style with fixed key order, indentation, quoting, number spelling, newline, Unicode, and default-omission rules.
- Do not promise comment preservation. Put durable explanation in explicit `notes`, brief, or journal fields.
- Hash a canonical semantic representation rather than raw source bytes.
- Add `schema_version`, project identity, device-state versions, and migrations on the first day.

A purpose-built language is not canonical “by construction.” It still needs a grammar, escaping, error recovery, semantic validation, formatting, versions, migrations, editor support, and tests. Build one only if an agent-edit benchmark shows that restricted YAML or canonical JSON is the bottleneck.

Canonical JSON is a reasonable alternative if experiments show equal agent readability. It provides simpler cross-language tooling at the cost of verbosity. The current examples and human readability make restricted YAML the pragmatic starting point.

### 11.2 Semantic operations: add a hybrid mutation interface

Preserve direct text authoring for:

- Patterns.
- Patches.
- Briefs and notes.
- Large, coherent creative changes in single-writer mode.

Prefer typed operations for:

- Creating, deleting, moving, and routing objects.
- Renaming anything with references.
- Concurrent workspace/agent edits.
- Repetitive transformations.
- Changes that require complex invariant checks.

Useful command shape:

```text
daw inspect section drop --json
daw plan-apply change.json --base sha256:...
daw apply change.json --base sha256:...
daw diff sha256:old sha256:new --semantic
```

The plan/dry-run response should show affected objects, downstream render invalidation, expected artifacts, and any decision still needed.

### 11.3 DSP language: decide by product commitment and benchmark

| Choice | Prototype speed | Offline performance | Realtime path | Main concerns |
|---|---:|---:|---:|---|
| Python/NumPy/SciPy | High | Good for vectorized, large-block work | Poor | Interpreter crossings, GIL around custom code, packaging native dependencies, likely rewrite for callback use. |
| Rust with Python bindings | Medium-low | High | Strong | More setup, DSP expertise, binding/build complexity, realtime discipline still required. |
| Faust plus an owned host | Medium | High | Strong/multi-target | Additional language/toolchain, generated-code and library licensing audit, integration/state semantics. |
| Existing DAW or plugin backend | High for product tests | High and mature | Already available | Hidden state, host/version dependence, crash isolation, inconsistent semantics, weaker reproducibility. |

[Faust’s architecture system](https://faustdoc.grame.fr/manual/architectures/) explicitly separates signal processing from drivers/controllers, supports multiple deployment targets, exposes parameter metadata, and can generate a machine-readable description. That makes it a serious third option missing from the current Python-versus-Rust question.

Recommended sequence:

1. Use Python for orchestration, schema work, analysis, and a disposable linear-mixer feasibility slice.
2. Before making the sampler production infrastructure, implement one representative stateful device in the serious candidates.
3. Benchmark sound correctness, event timing, automation, determinism, development time, packaging, state serialization, offline throughput, and callback viability.
4. If live playing is not a committed requirement, choose the simplest backend meeting measured offline targets and explicitly allow a future rewrite.
5. If one implementation must serve offline and realtime, choose a compiled kernel before expanding the catalog.

Do not constrain every offline algorithm to tiny blocks solely for a hypothetical U4. Use a stable device interface while allowing an implementation to process efficiently.

### 11.4 Device sourcing: own semantics, curate implementation

Use three tiers:

1. **Portable reference devices:** a small built-in set with strong reproducibility guarantees.
2. **Curated DSP kernels:** mature implementations wrapped behind owned IDs, units, descriptors, automation behavior, state versions, and tests.
3. **Optional external devices:** allowlisted CLAP/VST/AU or DAW-hosted devices with explicit capability and reproducibility limitations.

The first tier may include:

- Utility/gain/pan.
- One-shot sampler and drum rack.
- A basic filter or EQ.
- A simple delay.
- An optional, visibly active monitor-only safety limiter. Canonical audio and export must never acquire implicit processing; export protection is an explicit device or named recipe.

Add a compressor or reverb when the target workflow requires it and listening tests support the implementation. A full modulation synth, chorus, phaser, saturation family, true-peak mastering limiter, and broad plugin host are not MVP requirements.

This preserves the agent-native advantage without making filter design, reverb design, dynamics, oversampling, antialiasing, and years of listening QA prerequisites for product validation.

### 11.5 Rendering profiles

Keep the project sample rate fixed between preview and final where possible. Downsampling the entire graph changes filter response, dynamics timing, reverb behavior, nonlinear aliasing, and analysis. Prefer named immutable profiles:

- `preview`: same sample rate, reduced oversampling, shorter/lower-quality convolution, fewer expensive analysis passes.
- `final`: full oversampling, final tail policy, export precision/dither.

Cache profiles separately and never present a preview-versus-final comparison as if the edit were the only variable. Every report must name its render profile.

### 11.6 Internal audio and export policy

The spec should choose and test:

- Supported platform and architecture: begin with Apple Silicon and a named minimum macOS release.
- Mono and stereo only for the first product, unless evidence demands more.
- Internal non-interleaved float32 audio with a documented summing strategy; consider float64 accumulation at mix points only if benchmarks justify it.
- One explicit pan law.
- Sample-rate conversion and channel-conversion rules.
- Headroom and behavior on internal over-range samples.
- NaN, Inf, denormal, DC, and silence handling.
- Export types such as float32 cache, 24-bit PCM delivery, and optional 16-bit with a deterministic keyed dither seed; canonical WAV chunk/metadata ordering must be fixed for any bit-exact tier.
- Latency compensation and tail inclusion.

“Master to a streaming target” should be a versioned named recipe containing explicit LUFS and dBTP values, not a timeless platform convention.

### 11.7 Library and search

Recommended storage:

- Content-addressed immutable media objects.
- SQLite for metadata, provenance, tags, and full-text search.
- Rebuildable waveform, spectrogram, descriptor, and vector indexes.
- User annotations as mutable overlays rather than changes to the media object.
- Model and feature schema versions on every derived record.

Start search with:

- Filename and path tokens.
- User tags.
- Category.
- Duration.
- Loudness/peak/crest.
- Coarse spectral descriptors.
- Root pitch and tempo where confidence is adequate.

Only add an audio-text embedding model if a labeled retrieval benchmark shows a material improvement over this baseline. For an initial library, brute-force cosine/dot-product search over normalized vectors may be simpler than operating a dedicated vector database.

### 11.8 Perception architecture: sensors, not a taste oracle

Use a hierarchy:

1. **Invariant sensors:** NaN/Inf, missing audio, silence, alignment, and file validity; report float over-range, true peak, and detected flat-top/conversion clipping separately rather than calling every sample above 0 dBFS “clipped.”
2. **Engineering measurements:** loudness, peak, crest, spectral bands, correlation, transient/onset measures.
3. **Relational diagnostics:** diffs, reference deltas, candidate comparisons, possible masking with confidence.
4. **Auditory-semantic critic:** optional and provider-pluggable.
5. **Human preference:** authoritative for taste and acceptance.

Audio-capable models are already a real option; for example, [Google’s official audio-understanding API](https://ai.google.dev/gemini-api/docs/audio) accepts audio and documents music/emotion analysis. This does not make such judgments reliable, private, cheap, or reproducible. It does mean “never send audio to a model” should be a deployment policy, not an architectural assumption. Keep the core sensor path local and require separate explicit consent for any external critic.

Reports should separate:

- Measurement.
- Method/version.
- Reference basis.
- Confidence or calibration.
- Observation.
- Suggested experiment.

“Kick and bass overlap heavily from 60–120 Hz” is an observation. “The bass is wrong” is a judgment. The agent should propose a reversible A/B change rather than silently optimize every metric toward a generic target.

For loudness and true peak, name and test against the actual standards. The current reference is [ITU-R BS.1770-5](https://www.itu.int/rec/R-REC-BS.1770-5-202311-I/en), and the [EBU Loudness Test Set](https://tech.ebu.ch/publications/ebu_loudness_test_set) provides compliance material.

### 11.9 Plugin and worker isolation

If third-party plugins or ML workers are introduced:

- Run crash-prone or memory-heavy work out of process.
- Identify binaries and model weights by digest.
- Enforce time, memory, file, and output limits.
- Capture stdout/stderr and structured failure states.
- Publish outputs only after checksum/validity checks.
- Make cancellation and orphan cleanup explicit.
- Record whether results are deterministic, seeded, tolerance-reproducible, or inherently variable.

Plugin support should begin with an allowlist and semantic sidecar manifests, not arbitrary scanning of every installed plugin.

## 12. Licensing, rights, privacy, and security

This is an architecture gate, not release paperwork.

### 12.1 Dependency licensing

| Dependency/option | Current concern | Action |
|---|---|---|
| Rubber Band | [GPL v2-or-later or commercial licensing](https://breakfastquay.com/rubberband/license.html); its publisher specifically flags Apple App Store constraints for GPL distribution. | Decide project/distribution license; buy a commercial license or choose another implementation when required. |
| Essentia | [AGPLv3 for the open library](https://essentia.upf.edu/licensing_information.html); its pretrained models are noncommercial under CC BY-NC-ND unless separately licensed, and optional dependencies add obligations. | Avoid as a default proprietary dependency or budget commercial licensing; implement the thin baseline with permissive tools. |
| Demucs | MIT source, but the [original Meta repository is archived and no longer actively maintained](https://github.com/facebookresearch/demucs). Source and model-weight terms/maintenance are separate concerns. | Treat separation as an optional versioned worker; audit exact weights, fork/owner, redistribution, and macOS support. |
| LAION-CLAP family | Code and checkpoint/training-data provenance are not one licensing question. The [repository](https://github.com/LAION-AI/CLAP) notes copyright restrictions around training data. | Select an exact model only after code, weights, data provenance, redistribution, size, and quality review. |
| DawDreamer/Pedalboard | Useful feature-complete prototypes, both published under GPLv3. | Use for research/prototyping only unless the shipping license is compatible. |
| Faust and libraries | Compiler, runtime/architecture pieces, imported libraries, and generated code can carry different terms. | Audit the exact compilation and distribution path before adoption. |

Maintain:

- A machine-readable dependency and model registry.
- Exact download URL, version, digest, license, and redistribution status.
- SPDX/SBOM output and third-party notices.
- Automated policy checks for Python, Rust/native, frontend, and model assets.
- A clean path to disable optional licensed workers.

### 12.2 User asset rights

Provenance says where bytes came from; it does not prove permission.

Add optional rights metadata:

- Declared owner/source.
- License or purchase/source note.
- Attribution requirement.
- Allowed use such as project-only, commercial-ok, noncommercial, or unknown.
- Whether redistribution is allowed.
- Derivative lineage.

Separated stems and chops inherit uncertainty or restrictions from the source. Do not promote stems from a copyrighted commercial song into a global reusable library by default. A strict export mode should identify assets with unknown or incompatible declarations without pretending the software can provide a legal determination.

### 12.3 Privacy

“Local-first” should have a testable meaning:

- Core editing, rendering, indexing, and baseline analysis work with the network disabled.
- Optional models and downloads are explicit.
- A remote coding agent may receive project text, prompts, reports, or images; selecting one is itself an explicit per-project egress choice, not “fully local” operation.
- Remote audio critique is a separate, more sensitive consent boundary. Do not infer permission to upload audio from permission to send text.
- A fully local-agent mode should be claimed only when a supported local planner actually passes the workflow tests; local rendering alone is not a local agent.
- No audio, embeddings, spectrograms, prompts, project text, or preference data leave the machine without the relevant opt-in.
- Logs and provenance omit private absolute paths when exported.
- A project can enumerate every external service used and every uploaded asset.

### 12.4 Security and resource safety

At minimum:

- Safe YAML loading; no executable constructors.
- Allowed project/library roots.
- Symlink and path-traversal defense.
- File-type validation and defensive decoding for malformed media.
- Loopback-only daemon with a random session token and origin/CORS checks.
- No shell interpolation from project values.
- Treat filenames, tags, media metadata, project notes, imported bundles, and model output as untrusted data, structurally separate them from agent instructions, and never execute content-derived commands without validation.
- Verified model downloads.
- CPU, memory, disk, duration, render-count, and iteration limits.
- Plugin/ML worker isolation.
- Recovery from crashes, power loss, stale locks, and cache corruption.

Coding agents are powerful local principals. Document two threat models: the core tool can enforce transaction, path, and policy checks on calls it receives, while an unrestricted shell agent running as the same OS user can bypass those checks and edit files directly. Stronger “immutable rules” require OS ownership/permissions, sandboxing, or signatures; otherwise they are harness policy plus tamper detection. The project should make safe paths easy and dangerous path expansion explicit.

## 13. Memory and collaboration design

The rules/brief/preferences separation is good. Simplify it for early releases.

### Early model

- **Rules:** policy-protected, tamper-evident, and always loaded; truly immutable only under an enforcement boundary outside the agent’s principal.
- **Brief:** project-scoped intent and explicit acceptance/rejection notes.
- **Evidence log:** append-only structured events such as keep, reject, manual edit, reason, and scope.
- **Current summary:** bounded, regenerable context derived from the evidence log.

A hand edit is not automatically a global preference. It may be an experiment, a correction unique to one song, or an accidental movement. Treat it as candidate evidence and ask for scope when it would affect future projects.

Do not make an ever-growing Markdown journal the only machine state. A human-readable summary plus structured events gives both auditability and bounded context. Serialize events through the project transaction owner or store one immutable event per object; a shared append-only text file is not a multi-writer transaction mechanism. Load relevant preferences by retrieval under a token budget, not the full history.

The precedence order—rules, brief, confirmed preferences, defaults—is sound.

## 14. Revised development plan

### 14.1 Planning principle

Preserve the end-state vision, but separate it from the experiment that earns the right to build it.

The current roadmap is feature-layered:

`document → instruments → sample tools → effects → perception → synth → autonomy`

The product risk is end-to-end, so the roadmap should be vertical:

`one user job → safe edit → render → observe → compare → human decision → export`

Every milestone should end in measured user value and failure evidence, not only a happy-path demo.

The milestone names and market gates below assume the recommended path is an external product for target producers. If M-1 instead chooses a personal instrument or research platform, replace demand, retention, willingness-to-pay, and “private alpha” gates with explicit research questions, benchmark contribution, reproducibility, and learning criteria. Do not retain product language while silently changing the purpose.

### 14.2 Order-of-magnitude effort

Assuming one experienced full-time engineer, existing open-source building blocks, no novel foundation-model training, and honest audio QA:

| Outcome | Cumulative effort estimate |
|---|---:|
| Validated closed-loop feasibility slice plus backend comparison | 7–12 engineer-weeks |
| Useful sample-based MVP | 18–30 engineer-weeks |
| Sample-workflow private alpha after M2 | 30–55 engineer-weeks |
| Mixing-capable private alpha after M3 | 50–85 engineer-weeks |
| Full documented vision before extensive commercial hardening | Roughly 120–220+ senior engineer-weeks |

The complete vision is plausibly a 3–5+ year solo project once normal non-coding overhead and integration risk are included, and longer if “production-quality custom DSP,” a polished DAW workspace, plugin hosting, cross-machine exactness, and advanced MIR are literal requirements. Coding agents may reduce implementation time, but they do not eliminate listening evaluation, compatibility work, DSP validation, user research, packaging, or maintenance.

These are uncertainty ranges, not commitments. Cumulative ranges include roughly 30–50% allowance for integration, user studies, rework, and release hardening; milestone estimates below are focused engineering increments. Calendar time also includes recruiting, listening studies, licensing/provisioning, beta-access waits, and feedback latency. The increments do not sum to the full-vision number: that also includes the deferred synth/effect catalog, advanced MIR, separation, interoperability breadth, migrations, packaging, UX polish, compatibility, release hardening, and rework. Measure velocity during the feasibility slice and re-estimate after each gate.

### M-1 — Product, licensing, and evaluation contract

Estimated effort: 1–2 engineer-weeks.

Decide:

- Is the primary goal a personal/research instrument, an open-source platform, or a commercial product?
- Exact initial persona and explicit non-users.
- Three benchmark jobs and their inputs/expected properties.
- North-star and supporting metrics.
- Distribution license and whether proprietary/App Store distribution matters.
- Local-only privacy promise and optional cloud policy.
- The first named incumbent DAW/version, export compatibility target, and acquisition channel—not merely “a DAW.”

Create:

- A rights-cleared sample corpus.
- Several reference projects and expected semantic diffs.
- A human A/B protocol.
- A small set of deliberately bad technical fixtures.
- Baseline implementations in a conventional tool where practical.

Exit gate:

- Run discovery with 12–15 exact-persona participants who can show recent examples of the chosen job; at least half identify it as recurring and at least four commit a real project and a second formative session rather than only expressing interest.
- If commercialization is intended, define when a paid pilot or equivalent commitment test will occur.
- The dependency/license posture is compatible with the intended distribution.
- The experiment can distinguish success from an impressive demo.

Discovery participants shape the job and thresholds; committed formative pilots may help debug M0B, but they are not the frozen M1 product holdout.

If the user job is not validated, change it before writing an engine.

### M0A — Workflow proof and backend bake-off

Estimated effort: 1–2 engineer-weeks and intentionally disposable. This assumes the selected host API is already accessible; extend it rather than faking the comparison if beta access or integration setup takes longer.

Run the same three tiny, rights-cleared jobs through:

- The producer’s current manual/template workflow.
- A minimally coded internal audio-placement and sum path.
- One of Ableton Extensions or a typed REAPER integration.

Manual assistance is acceptable if it is logged; the purpose is to expose the cheapest credible route to the user outcome before hardening an engine. Keep two preregistered scorecards so prototype maturity does not masquerade as backend merit:

- **Common-subset architecture:** semantic targeting reliability, observability, readback, undo/recovery, revision safety, and latency for operations every path can represent.
- **End-to-end product outcome:** time, supervision, repair, installation burden, artifact quality, access to user sounds/devices, and export back to the real workflow while each path uses its legitimate strengths.

Publish a provisional, versioned ADR using the same task corpus and revisit it after M1 evidence. If no path wins materially, choose the lower-cost path rather than defaulting to greenfield by inertia. The ADR must choose one initial architecture:

- **Internal-renderer path:** the text bundle is authoritative; M1 can later implement the sampler, event engine, and cache.
- **Host-authoritative adapter path:** the host Set/project is the only canonical song; specify state capture, undo, host-only edits, version pinning, and read-projection policy before M0B. It must demonstrate saved snapshots, validated readback, and atomic undo or compensating/quarantine behavior; otherwise it is ineligible for tasks requiring old-or-new publication integrity.
- **Neutral-IR path:** the text IR is authoritative and one host adapter is the primary executor; specify round-trip/capability-loss behavior, keep only a minimal reference implementation if useful, represent opaque/backend-specific state honestly, and avoid claiming identical sound across backends.

Only one executor receives production hardening at first. Later milestones describe product outcomes, not an unchanged implementation recipe for all three branches.

### M0B — Closed-loop feasibility slice on the selected path

Estimated effort: 3–5 engineer-weeks for prototype-quality work; 5–8 is more credible if transaction safety, clean-machine smoke testing, and a real user study are all included rather than stubbed.

One promise: create and revise an audio-only 8–16 bar arrangement, then compare the result.

Constrain the fixture deliberately: pre-trimmed, rights-cleared, 48 kHz mono/stereo one-shots and tempo-labelled, already aligned loops. Define mono/stereo conversion and pan behavior. Do not imply arbitrary WAV/AIFF import, resampling, tempo detection, slicing, or stretching in this milestone.

Build:

- `daw init`, `check`, `inspect`, `apply`, `render`, `listen`, and `compare`, mapped to the selected executor.
- A versioned restricted canonical project schema/serializer on an IR-authoritative path, or a versioned typed operation/brief/artifact schema plus host read projection on a host-authoritative path.
- The applicable `authority_revision`: transaction-safe immutable source generations on the IR path, or selected saved host snapshots plus undo/operation journal on the host path; both produce render and artifact identities.
- A minimal project brief/brief schema containing the requested direction and acceptance constraints.
- Content-hashed, vendored fixture assets.
- Constant 4/4, constant tempo, and 48 kHz output for the experiment.
- Audio clips with source trim, placement, gain, one explicit pan law, and fades.
- An internal deterministic master sum only if the internal path won; otherwise equivalent, explicitly versioned host operations.
- Peak/over-range, detected clipping, NaN/Inf, silence, integrated loudness, coarse spectrum, waveform, and spectrogram.
- One beat-arrangement workflow and one precise-revision workflow.
- Candidate naming, A/B manifests, a structured operation/error/event log, and either Git checkpoints for readable IR state or pinned host-native snapshots with text provenance.
- An early clean-machine installation smoke test, even if packaging remains crude.

Do not build:

- Production sampler, drum rack, synth, routing graph, broad effects, embeddings, separation, region cache, daemon, or workspace.

Before running a frozen holdout, preregister the task/participant count, denominators, thresholds, evidence artifacts, and pivot action. Suggested gates to calibrate during M-1 are:

- **Publication integrity:** 100% of completed or interrupted tool transactions leave the previously selected valid authority revision or a validated new one. On the host path, partial native mutations may transiently occur only inside the declared undo/duplicate/quarantine boundary and are never selected or used for a published artifact.
- **First-attempt reliability:** at least 95% of benchmark agent changes are accepted by the typed operation and authority layer without repair; separately report rejected attempts, repair count, wrong-object changes, and silent semantic errors.
- **Semantic success:** at least 85% of concrete revision requests are implemented as intended, with zero silent destructive/critical errors in the holdout.
- **Latency:** median time to first listenable result is under five minutes and a simple revision under two minutes on the declared setup.
- **Instruction adherence:** request-aware raters see randomized/blinded system identity and A/B order; the revision must beat the prior candidate above chance with a reported confidence interval, not merely a small-sample point estimate.
- **Taste and retention:** run a separate loudness-matched blind preference test unless loudness is the requested variable, and record whether the directing producer keeps or continues the result. Do not merge adherence and taste into one number.
- **Reproducibility:** repeated independent cold renders and interrupted-publication tests satisfy the declared guarantee tier.
- **Net leverage:** complete time-to-retained-result, including setup/supervision/export, improves on the user’s current-workflow baseline often enough to justify another milestone.

If the agent cannot improve a simple arrangement under this loop, fix the job, representation, operations, prompts, or sensing before adding features.

### M1 — Sample-composition MVP

Estimated effort: 8–14 engineer-weeks after M0B.

The outcome is common, but implementation follows the M0A branch.

Add on every path:

- Filename/tag/descriptor library search over the supplied material.
- Semantic operations for one-shots, drum patterns, sections, and repeat placement.
- Track, mix, stems, and MIDI export.
- Two or three hardened agent workflows.
- Robust diagnostics, rollback/recovery, and capability-aware errors.

On an IR-authoritative path, add minimal drum-grid and melodic-event grammars plus exact musical/event-time semantics. If the internal renderer is the executor, also make the production sampler/DSP decision, implement the one-shot sampler/drum rack, and add whole-track caching with full downstream invalidation. If a host is authoritative or executes the IR, map the outcomes to its sampler, timeline, rendering, and undo/state facilities and harden the selected host/version capability contract.

Keep effects to utility and perhaps one filter or simple delay if users need them.

Exit:

> From 20–50 supplied one-shots, target users can produce and revise a 60–90 second arrangement, retain meaningful agent material, and continue from exported stems/MIDI.

Evidence should include retention, revision accuracy, elapsed time, and user return—not only that a demo renders.

Provisional required gates: at least eight held-out target producers who were not used to tune the workflows; a majority meet the operational retention definition in section 4.3; concrete-revision accuracy remains at or above the calibrated M0B threshold; every declared export fixture reopens in the selected DAW/version with timing within the published tolerance; and no session corrupts or loses authoritative state. Before M2, at least half of eligible pilot producers should complete a second real session within two weeks. This is enough to justify another private-alpha iteration, not a market-size conclusion.

### M2 — Sample-workflow private alpha

Estimated effort: 10–18 engineer-weeks after M1, potentially split into sample features and distribution/interoperability hardening.

Add selectively:

- Transient slicing with replay validation.
- Constant-ratio stretch/repitch after licensing is settled.
- Loop tempo/downbeat candidates with explicit uncertainty.
- MIDI import/export hardening.
- DAWproject export.
- A benchmarked search embedding only if it beats the baseline.
- A small curated EQ/delay or other device set driven by observed workflows.
- Schema/device migrations.
- macOS packaging, optional model management, clean-machine install, and offline tests.
- Artifact retention and garbage collection.

Exit:

> A target producer can build a short track from a loop plus one-shots, revise it repeatedly, reopen it on a clean supported machine, reproduce the approved render from its locked snapshot, and continue in a conventional DAW.

Freeze required gates before the holdout: at least 90% of supported clean-machine installs reach first render without developer intervention; 100% of positive fixtures with complete supported lockfiles restore their approved artifact within the declared guarantee; only deliberately incomplete negative fixtures may pass by producing a precise missing-dependency diagnosis; declared slice/replay fixtures meet their timing tolerance; exports reopen successfully in every host/version pair declared eligible before the holdout; and a multi-session crash-free workload target is met. Publish actual denominators and confidence intervals rather than treating these example thresholds as universal truths.

### M3 — Routing, mixing, and validated perception

Estimated effort: 15–25 engineer-weeks after M2.

Before implementation, specify graph order, identity, latency, state, automation, stem definitions, and preview policy.

The list below is an internal/IR implementation plan. On a host-authoritative path, expose and test the host’s actual graph, automation, latency, tail, stem, and preview capabilities instead of rebuilding a shadow graph; unsupported semantics remain explicit capability limits. On an IR-to-host path, define the mapping and loss policy field by field.

Then add:

- Groups, sends, returns, and sidechain/control edges.
- Device latency, lookahead, history, and tail reporting.
- Automation semantics, rate classes, and smoothing.
- A deliberately small high-quality effects set.
- Dependency-graph caching.
- Known-bad-mix corpus and calibrated relational diagnostics.
- Optional auditory-semantic critic interface.
- State-aware exact region rendering only if benchmarks justify it.

Exit:

- Objective faults and subjective/relational mix warnings have separate preregistered precision, false-positive, and abstention targets on a frozen holdout.
- Metric-informed revisions improve request adherence and human A/B preference in separate tests rather than only moving the metrics.
- If exact regional rendering is included, in-context region output matches the corresponding full render within the declared tolerance; otherwise the feature remains explicitly deferred.
- The complete declared routing matrix passes cold-versus-cached equivalence, correct descendant invalidation, latency/tail alignment, and deterministic/tolerance-tier graph-output tests.

### M3.5 — Guided long-form gate

Estimated effort: 4–8 engineer-weeks after short-form workflows are reliable; much of this is study and hardening rather than new surface area.

Before autonomous long-horizon work, preregister and complete at least six held-out guided 3–5 minute projects across at least three target producers and fresh sample libraries. Calibrate exact budgets before the holdout; provisional required gates are: concrete-revision accuracy no worse than 10 percentage points below the short-form gate, operational retention in a majority of projects, 100% successful interruption recovery and declared export-fidelity fixtures, context/render/storage within fixed per-project ceilings, and median net time-to-retained-result better than the producers’ baselines. Publish a binary proceed/rework/stop ADR. If short-loop success collapses at song length, improve hierarchy, context retrieval, project partitioning, or render economics before adding autonomous search.

### M4 — Bounded long-horizon work

Estimated effort: 8–14 engineer-weeks after the guided long-form gate.

Add:

- Global rules, confirmed preference evidence, and scope-aware retrieval; the minimal project brief already exists in M0B.
- Persistent job plans and resumable state.
- Branch-and-compare candidate search.
- Best-so-far preservation.
- Time, iteration, render, model, and disk budgets.
- Cancellation, rollback, plateau detection, and escalation.
- Confirmed preference evidence only after project-level signals are understood.

Exit:

- Every kill/resume fixture preserves canonical integrity and either resumes idempotently or reports a recoverable terminal state.
- Cancellation latency and budget overshoot remain within preregistered bounds; no holdout run mutates a policy-protected rule through the public operation interface.
- In a powered, randomized best-of-N study, the selected later candidate beats the first draft often enough—and at low enough cost per retained result—to justify autonomous iteration.
- Every plateau fixture stops within its bound and distinguishes deliverable completeness from human acceptance.

### M5 — Shared workspace

Treat this as separate products rather than one 12–24 week item:

- **M5A, viewer and simple operations:** roughly 10–20 engineer-weeks for a read-only arrangement/artifact viewer, canonical transport, revision-aware simple operations, conflict UX, and background canonical catch-up.
- **M5B, production editor:** roughly 20–40+ additional engineer-weeks for serious pattern, device, automation, and responsive-audio editors.
- **Live instrument playback:** a separate realtime-engine program with its own product decision, estimates, callback architecture, device matrix, and latency gates.

Sequence:

1. Read-only arrangement/artifact viewer and canonical mix transport.
2. Revision-aware simple operations.
3. Pattern/device/automation editors.
4. Labeled monitor proxies and background canonical rendering.
5. State-checkpoint/region responsiveness only after the engine passes equivalence and latency gates.
6. Consider live instrument playback only as the separate program above.

Do not run this as a parallel track if it delays proving or hardening the agent loop. A minimal browser artifact player used for research is disposable support tooling, not U0. Give each UI stage separate usability, conflict-safety, operation-success, monitor-latency, and canonical-catch-up gates before expanding the editor.

### 14.3 Critical dependency order

```text
user job + evaluation + backend/authority decision
          ↓
schema + time + identity
          ↓
revisions + immutable snapshots + assets
          ↓
selected executor correctness + minimum sensing
          ├──→ passed M1/M2 value gate → M5A viewer/simple operations
          └──→ sampler/event semantics or host capability mapping
                         ↓
              graph/latency/tail/automation semantics
                         ↓
              effects + dependency cache
                         ├──→ canonical responsive mixing UI
                         └──→ optional exact region rendering
                                      ↓
                          exact regional responsiveness
```

```text
evaluation corpus
       ↓
minimum perception
       ↓
agent revision study + human A/B
       ↓
calibrated critics
       ↓
bounded autonomy
```

### 14.4 Go/no-go register

Milestone owners should instantiate these IDs in a small machine-readable gate manifest. Section 15 is the test catalog; this table says which families are mandatory for each decision.

| Gate | Required evidence | Mandatory section-15 families | Decision unlocked |
|---|---|---|---|
| `G-1 demand` | Recurring delegation job, committed pilots, license/privacy/channel posture. | Evaluation governance and distribution feasibility. | Spend on a technical comparison. |
| `G0A authority` | Separate architecture and outcome scorecards; provisional ADR covering source of truth, rollback, readback, and loss policy. | Relevant editing, distribution, and recovery fixtures. | Select one executor/authority path for M0B. |
| `G0B loop` | Publication integrity, first-attempt/semantic success, latency, adherence, preference, retention, reproducibility, and net leverage. | Applicable 15.1–15.4 and 15.6–15.8 tests for the selected authority and constrained fixture. | Build the sample MVP or rework/pivot. |
| `G1 MVP` | Operational retention, repeat session, revision accuracy, export fidelity, no lost state. | Applicable 15.1–15.4 and 15.6–15.8 tests for supported M1 capabilities. | Begin private-alpha features. |
| `G2 alpha` | Clean install, positive restoration, negative diagnostics, slice/replay and declared host-matrix fidelity. | All applicable 15.1–15.8 tests; region tests excluded unless shipped. | Add routing/mixing or distribute the narrow product. |
| `G3 mix` | Calibrated diagnostics, human improvement, routing/cache/latency/tail correctness. | 15.2–15.7 across the full declared routing matrix. | Attempt long-form work and responsive mix UI. |
| `G3L long` | Six guided songs meet accuracy, retention, recovery, resource, export, and net-time gates. | Applicable 15.1–15.8 at 3–5 minute scale. | Invest in bounded autonomy. |
| `G4 autonomy` | Kill/resume, cancellation/budget, policy, plateau, cost, and powered best-of-N evidence. | 15.1, 15.3, 15.6–15.8 plus adversarial job fixtures. | Ship a bounded autonomous mode. |
| `G5 workspace` | Stage-specific usability, conflict safety, operation success, latency, and canonical catch-up. | 15.1–15.4 and 15.7 for each UI stage. | Expand to the next workspace stage. |

## 15. Verification and evaluation plan

Separate development/calibration fixtures from a frozen holdout. Before evaluating a milestone, record each required gate’s corpus, denominator, metric, threshold, evidence artifact, uncertainty method, and failure/pivot action. Informative metrics may remain exploratory, but required gates must not be tuned after seeing holdout results.

### 15.1 Schema and editing

The parse/format/migration fixtures below apply to authoritative IR files. A host-authoritative path substitutes typed operation-schema, saved-snapshot, undo/compensation, revision, and validated-readback fixtures; the no-loss/no-invalid-selected-state invariants apply to both.

- Parse → format → parse is semantically invariant.
- Formatting twice is byte-identical.
- Semantic hashes are independent of inconsequential source formatting.
- Duplicate keys/IDs, unknown fields, invalid units, missing references, and path traversal are rejected.
- Every released schema version has migration and round-trip fixtures.
- Fuzzed documents never crash the process or produce a partial write.
- A stale base revision is rejected.
- Stale edits are rejected unless their exact operation classes are declared commutative and pass combined-state validation; approved merges or serialization never lose a change.
- Conflicting edits produce a precise conflict.
- Renaming a display name cannot break routes or automation.

### 15.2 Time and event scheduling

- Musical-time/frame conversions cover supported tempos, meters, grids, tuplets, and boundary cases.
- Conversion rounding is deterministic.
- Note-on, note-off, automation, choke, and retrigger ordering is fixed.
- Repeat truncation and notes crossing clip boundaries have fixtures.
- Swing, quantize, nudge, and humanization order is tested.

### 15.3 Renderer and cache

Apply node/cache tests to the internal executor. A host path instead tests saved-project/render repeatability, host-version closure, operation readback, render/export identity, and every capability it claims; it should not claim control over an opaque host cache.

- Gain, pan, fades, overlaps, phase cancellation, channel conversion, and project tails have golden fixtures.
- Certified bit-exact fixtures repeat byte-for-byte under the exact declared closure; tolerance-tier fixtures stay within their numerical/perceptual contract.
- Serial and parallel execution are byte-equal in the bit-exact tier; use tolerance only when their fingerprints or declared tier explicitly permit it.
- Cold and warm cache outputs agree.
- Concurrent requests for the same cache key single-flight or safely converge on one verified object; stale locks and losing writers recover without corrupt publication.
- Changing each input category invalidates exactly the expected graph descendants.
- Cache corruption is detected and safely regenerated.
- Garbage collection never removes an artifact pinned by a committed catalog or checkpoint.
- If exact regional rendering ships, full render and in-context region render agree for stateful devices after defined restoration/pre-roll; otherwise this test is not an earlier milestone gate.
- Unknown/infinite-tail fixtures terminate at the profile’s deterministic bound and apply the specified truncation/fade.
- No stale revision is published as current.
- Audio replacement/crossfade creates no click in supported monitor paths.

Useful metamorphic tests include:

- Bypass equals the declared identity behavior.
- A +6.0206 dB gain doubles linear amplitude within tolerance.
- Splitting a stateless render into canonical chunks equals one full call.
- Reordering independent graph evaluation does not change summation order/output.
- Repeating a fixed seed gives the same humanization and round robin.
- Changing an unrelated color or prose `notes` metadata does not invalidate audio.

### 15.4 DSP and loudness

- Every device handles silence, impulse, DC, extreme legal settings, long runs, denormals, NaN/Inf prevention, and reset.
- Frequency and gain responses match documented tolerances.
- Compressor steady-state reduction and attack/release behavior match its specified model.
- Limiter tests include inter-sample peaks if it makes a true-peak claim.
- Bypass preserves declared latency and gain.
- Device output is invariant to caller chunking if the interface promises that behavior.
- Loudness and true-peak implementations pass official/reference compliance material.

### 15.5 Analysis and search

- Tempo evaluation credits a correctly ranked half/double-time alternative and measures confidence calibration.
- Downbeat tolerance is specified in frames, milliseconds, or beat fraction.
- Pitch/root and category classifiers publish per-class precision/recall.
- Slicing uses onset precision/recall plus replay tests.
- Search uses human relevance labels and compares Recall@5/nDCG and selection time against the filename/tag/descriptor baseline.
- “Masking” evaluation uses intended and problematic overlaps and reports precision, false-positive rate, and abstention.
- Reference comparisons are loudness-matched and identify alignment uncertainty.

### 15.6 Agent behavior

Maintain a versioned development corpus of at least 20 task types across repeated runs and model versions, plus a separately frozen holdout large enough for the chosen confidence/power target. Twenty observations alone cannot substantiate 95%, 85%, or near-chance listening claims. Record:

- Syntax-valid and schema-valid edit rate.
- Semantic task success.
- Tool calls and repair attempts.
- Tokens/context size.
- Render count and elapsed time.
- Human preference/retention.
- Recovery after injected failures.

The initial 95% first-attempt acceptance and 85% semantic-revision thresholds above are proposed gates to calibrate, not claims about current performance. Publication integrity remains an invariant. Instruction adherence, aesthetic preference, and directing-producer retention are three different outcomes and should never be collapsed into one “better” rating.

Test:

- Raw YAML versus typed patch operations.
- Concise versus verbose device descriptions.
- Context slices versus full-project reads.
- Numeric-only sensing versus numeric plus image versus optional audio critic.
- First candidate versus best-of-N branching.
- Kill/resume at every workflow step.
- Cancellation and hard budgets.
- Policy-protected rules that the public operation interface must not mutate, plus tamper detection when a shell-capable agent bypasses that interface.

Pin results to the model/harness version. A model upgrade is an evaluation event, not automatically an improvement.

### 15.7 Performance

Replace “four bars under one second” with a benchmark manifest specifying:

- Mac model/CPU, RAM, OS, power mode.
- Engine and dependency build.
- Song duration and tempo.
- Sample rate and channel layout.
- Track/clip density.
- Device/routing/automation workload.
- Cold versus warm cache.
- Preview versus final profile.

Report p50/p95 latency, real-time factor, peak memory, disk written, and cache hit rate.

Suggested gates to test:

- Warm simple-track revision audible within one second p95 for the MVP.
- If U3 is promised, a representative playhead region ready within roughly 250–400 ms p95 and swapped without glitches.
- Final production fixture with `render wall time / audio duration ≤ 0.5` on the declared reference Mac (at least 2× realtime throughput).

ML separation and indexing workloads should be reported separately; they must not make renderer latency numbers meaningless.

### 15.8 Distribution

- Clean supported Apple Silicon Mac installation.
- Signed/notarized package if distributed.
- Network-disabled core workflow after explicit optional model installation.
- Full dependency/model/license/digest inventory.
- Upgrade and rollback tests.
- Old projects migrate or render through a pinned compatibility path.
- Uninstall and cache cleanup behavior.

## 16. Risk register

| Risk | Likelihood / impact | Early signal | Mitigation or pivot |
|---|---|---|---|
| Agent revisions do not improve music. | High / Critical | Blind A/Bs fail or require frequent repair. | Test in M0B; improve constraints/sensing or focus on retrieval and transformations. |
| No sufficiently motivated initial user. | Medium-high / Critical | Positive interviews but no repeated real projects. | Narrow the job/persona; test paid or high-commitment pilots. |
| Scope consumes years before value. | High / Critical | Two milestones pass without retained user output. | Enforce gates; defer UI, synth, broad effects, ML, and autonomy. |
| Numerical perception creates false confidence. | High / High | Metric “fixes” reduce human ratings. | Calibrated fixtures, confidence/abstention, human/audio critic A/B. |
| Greenfield engine adds no user advantage. | Medium-high / High | Adapter reaches outcomes faster and more reliably. | Make the product an IR/control/evaluation layer or extension. |
| Custom DSP sounds amateur. | High / High | Technical tests pass but users reject sound. | Fewer devices, curated kernels, listening tests, optional export/plugins. |
| Region/cache output differs from full mix. | High / High | Null/equivalence tests fail after stateful devices. | Whole-track first; explicit state/latency/tail; defer responsive UI. |
| Concurrent/non-atomic edits lose work, select partial host state, or render a mixed bundle generation. | Medium / Critical | Agent, workspace, raw editor, or incremental host API mutates authority during a compound change. | IR generations or host snapshot/undo/quarantine contract, authority revisions, transaction-owned writes, conflict results, backups, validated selection. |
| Reproducibility promise is too broad. | Medium / High | Output hash changes across upgrade/machine. | Lock full environment and define bit/tolerance guarantee tiers. |
| Dependency license blocks distribution. | High / High | Proprietary/App Store goal conflicts with GPL/AGPL. | License gate before adoption; permissive alternative or commercial license. |
| Optional model becomes unavailable or unmaintained. | Medium / High | PyTorch/macOS breakage or archived upstream. | Versioned worker interface, frozen integration, alternate provider. |
| Plugin/device support becomes an endless matrix. | High / High | User bugs are plugin-specific and unreproducible. | Portable core plus curated allowlist and process isolation. |
| Copyright/privacy concerns erode trust. | Medium / High | Commercial sources are promoted/reuploaded unknowingly. | Local default, rights metadata, strict export, explicit consent. |
| Autonomous runs consume resources or degrade work. | Medium / High | Repeated renders with no preference gain. | Budgets, cancellation, plateau detection, best-so-far branches. |
| Git checkout cannot restore audible state. | High / Medium | Missing source or mutated global patch. | Content-addressed assets and immutable snapshot manifests. |
| Format evolves incompatibly. | Medium / High | Breaking fields/device defaults appear before migrations. | Version from day one; fixtures and explicit compatibility policy. |
| Coding-agent onboarding is too technical. | High / Medium | Clean-machine pilots need developer intervention. | Better diagnostics/package; extension or app shell earlier if evidence supports it. |
| Benchmarks overfit fixed packs and prompts. | High / High | Development scores rise while fresh libraries/users fail. | Separate calibration and frozen holdout corpora; preregister gates; add new user libraries and multiple raters. |
| Cross-disciplinary capacity or bus factor stalls quality. | High / High | One person becomes the sole reviewer for DSP, agents, packaging, frontend, product research, and listening. | Narrow scope; schedule explicit DSP, security/license, UX, and expert-listening review points. |
| End-to-end overhead exceeds labor saved. | High / Critical | Setup, supervision, repair, export, and re-import take longer than the incumbent workflow. | Measure complete time-to-retained-result against the current workflow; simplify or move into the host. |
| DAW API access or churn invalidates the adapter path. | Medium / High | Beta access blocks tests or a host update breaks mappings. | Capability tests, pinned host versions, adapter maintenance budget, and an alternate comparison path. |
| Interchange succeeds syntactically but loses intent. | High / High | Reopened stems/MIDI/DAWproject drift in timing, routing, automation, or devices. | Test named host/version pairs and publish a field-level compatibility/loss matrix. |
| Remote-model cost, availability, or behavior drifts. | Medium / High | Cost per retained result rises or a model upgrade regresses tasks. | Record model/harness versions and cost; maintain evaluation gates and a fallback/manual path. |
| Imported metadata prompt-injects the agent. | Medium / Critical | A filename, tag, note, or shared bundle changes instructions or triggers a command. | Keep untrusted data separate from instructions; typed operations, least privilege, validation, and adversarial fixtures. |
| Short-loop success fails at song length. | High / High | Context, coherence, invalidation, or artifact storage degrades on 3–5 minute projects. | Enforce M3.5; hierarchical summaries, project partitioning, render budgets, and guided long-form tests. |
| Recruitment delay or friendly-user bias distorts evidence. | Medium / High | Only collaborators complete sessions or scheduling dominates elapsed time. | Track recruiting lead time; require exact-persona, recent-work evidence and participants beyond the immediate network. |
| Subjective QA becomes the throughput bottleneck. | High / Medium | Code iterations outpace trustworthy listening review. | Budget expert listening, randomized review queues, and regression triage as first-class delivery work. |

## 17. Recommended resolutions to current open questions

### OQ1 — Project syntax

If M0A selects an IR-authoritative path, choose restricted YAML 1.2 plus a strict typed semantic IR and canonical serializer. Hash semantic canonical form and add migrations immediately. If the host is authoritative, keep YAML only for briefs, operation fixtures, and other sidecars where useful; do not serialize a second song. Do not build a custom whole-project language without comparative edit evidence.

### OQ2 — Melodic notation

On an IR-authoritative path, keep drum-grid and melodic event-list formats as two separate canonical grammars compiling to one note IR. On a host-authoritative path they may be command input forms that materialize into host notes rather than canonical project storage. Avoid a third inline form until benchmarks demand it. Materialize accepted chord expansions into notes.

### OQ3 — Embedding model

Do not select one yet. Build SQLite metadata/full-text search and a labeled real-library benchmark first. If embeddings win materially, pin one exact code/weight version and store its digest and feature schema; the vector index is always rebuildable.

### OQ4 — Context loading

Always load rules, the current brief, and a bounded current project summary. Retrieve relevant confirmed preferences and the last N structured events under a stated token budget. The full journal is audit history, not default context.

### OQ5 — Draft rendering

Keep the session sample rate fixed. Reduce oversampling, convolution/reverb cost, and nonessential analysis under named preview profiles. Cache and label preview/final separately.

### OQ6 — CLI name

Defer branding. Before release, check binary/package/domain conflicts and whether the name communicates a workbench rather than a full incumbent replacement.

### OQ7 — Device language

On the internal-renderer path, move the gate before Phase 1/M1 production devices and run a representative Python/Rust/Faust benchmark. If realtime/live playing remains only hypothetical, optimize for the offline product and acknowledge future rewrite risk. If one implementation for live and offline is strategic, use a compiled kernel from the first production sampler. A host-authoritative path replaces this with a host/device compatibility decision.

### OQ8 — Workspace shell

A browser tab is sensible for a read-only or simple local client. Keep the frontend transport-neutral. Use Tauri later for packaging/native integration only if needed; do not let the shell decide the engine architecture.

### New decisions needed before M0A/M0B

1. Personal/research/open-source/commercial goal.
2. Initial persona, delegation boundary, job, and north-star metric.
3. Commercial/research posture, first distribution channel, and commitment test.
4. Product and dependency license.
5. Local core, remote planner, and remote-audio privacy boundaries.
6. Greenfield-versus-incumbent benchmark protocol and weighted decision rule.
7. Selected source of truth, host round-trip, and capability-loss policy after M0A.
8. `authority_revision` (`authoring_revision` or `host_revision`), render fingerprint, artifact, and analysis identity contracts.
9. Exact rational time domains, supported meter scope, and frame-rounding rule.
10. Reproducibility guarantee tiers.
11. Asset copy/link/content-address policy and generated-lock semantics.
12. Canonical versus monitor audio policy.
13. Finite project-end/tail policy.
14. Supported Mac/OS/audio format matrix.
15. Rights metadata and export behavior.

## 18. Suggested first eight-week validation cycle

### Weeks 1–2 — Make the bet explicit

- Rewrite the problem statement around delegation without surrender.
- Choose the initial persona and non-users.
- Name the first incumbent DAW/version, export matrix, and likely acquisition channel.
- Select three benchmark jobs.
- Assemble a rights-cleared sample/test corpus.
- Define A/B and operation-success measures.
- Decide intended distribution license and the eventual commitment/WTP test.
- Complete dependency/model/license inventory.
- Write ADRs for time, identity, revisions, reproducibility, privacy, and monitor audio.

### Weeks 3–4 — Select the execution path

- Run the same small jobs through the current workflow, a disposable linear renderer, and one Ableton/REAPER path.
- Log assistance, setup, tool calls, errors, latency, observability, and artifact quality.
- Publish the M0A ADR with weighted evidence.
- Decide executor, authoritative state, round-trip, and capability-loss policy.

### Weeks 5–7 — Close the loop on the selected path

- Implement the common typed operation/brief/artifact schema and applicable authority safety: canonical formatting, authoring revisions, and generation snapshots on the IR path; host revisions, saved snapshots, undo/quarantine, and readback on the host-authoritative path.
- Add `init`, `check`, `inspect`, and revisioned `apply`.
- Implement or map audio placement, gain, pan, fades, and render/export on the selected executor.
- Produce immutable candidate artifacts.
- Add peak/over-range, loudness, coarse bands, waveform/spectrogram, and compare.
- Write one agent workflow.
- Run the benchmark tasks repeatedly with fresh agent contexts.

### Week 8 and, if needed, weeks 9–10 — Test the thesis

- Freeze the holdout and run separate instruction-adherence, blind preference, and directing-producer retention sessions.
- Measure publication integrity, first-attempt acceptance, semantic correctness, repairs, net end-to-end time, latency, and onboarding.
- Review failures before adding a sampler.
- Decide whether the loop earns M1, needs another iteration, or should narrow/pivot.

The eight-week deliverable is not “the DAW skeleton.” It is a selected architecture, a working closed-loop slice, and initial evidence about whether the control-and-evaluation loop deserves a platform. The focused implementation increments total 5–9 engineer-weeks, while the validated cumulative outcome is estimated at 7–12 once study, integration, and rework are included. Finish the preregistered study after week eight when necessary rather than compressing the bake-off or listening evaluation.

## 19. Recommended follow-up edits to the existing documents

This review intentionally does not rewrite the source design docs. A subsequent revision should:

### `docs/idea.md`

- Replace the outdated Suno and Ableton comparison.
- Lead with delegation without surrender and the chosen user.
- Separate durable principles from implementation hypotheses.
- Rename “workflows on day one” to “target workflows,” then add the actual initial workflow.
- Replace categorical “agent cannot hear” language with the sensor hierarchy.
- Clarify whether the goal is a commercial product, research platform, or personal instrument.

### `docs/spec.md`

- Add the authority branch/source-of-truth policy; on the IR path add schema versioning, migrations, exact time, revision transactions, immutable generations, separate render/artifact identities, and manifests.
- Define audio graph order, pan law, sample format, latency/tails, automation semantics, and stems.
- Separate canonical audio from monitor proxies.
- Add error/progress/job contracts.
- Add rights, privacy, security, packaging, compatibility, and resource requirements.
- Make analysis methods versioned and confidence-bearing.

### `docs/plan.md`

- Replace layer phases with the vertical milestones in this review, starting with the M0A authority/backend decision.
- Move minimum perception and an agent workflow into the first slice.
- Add product, user, reliability, and listening gates.
- Move the DSP-language decision before the sampler.
- Defer the workspace until after the agentic value gate.
- Put stems/MIDI/DAWproject interoperability earlier.
- Add effort ranges and explicit stop/pivot criteria.

### `docs/decisions.md`

- Keep superseded decisions rather than deleting them.
- Add status, date, evidence, consequences, and superseding-decision links.
- Reopen D1–D3, D8, D10, D13, D15, and D18–D24 for the reasons in this review.
- Distinguish product principles, architectural decisions, policies, and experiments.

### `docs/README.md`

- Add document status and last-reviewed date.
- Put this review after the vision and before the old detailed plan until decisions are reconciled.
- Mark any sections known to be outdated.

## 20. Final assessment

The project contains a genuinely good kernel worthy of a bounded validation phase. Its strongest insight is that people should be able to delegate production labor without giving up the editable musical object. Transparent state, reversible operations, frequent artifacts, real units, authorized user-supplied samples, and human taste form a coherent system.

The weakest part of the current reasoning is the jump from that insight to “therefore build a new DAW engine, all devices, an analysis suite, memory, autonomy, and a workspace.” That conclusion does not follow from first principles and is now under greater competitive pressure than the documents acknowledge.

The project should continue if early evidence shows all three:

1. Target producers repeatedly want the selected delegation job.
2. The agent produces or revises structured material they actually retain.
3. The local, inspectable workflow offers a meaningful advantage over an incumbent DAW adapter or a cloud generative studio.

It should narrow or pivot if:

- Users praise the architecture but do not keep the music.
- The agent is useful only for deterministic edits that an incumbent integration handles better.
- Numerical perception does not improve human preference.
- Lack of plugins/interoperability blocks most serious sessions.
- Setup and render latency exceed the labor saved.

The immediate goal should not be to prove that a DAW can be represented as text or rendered from Python; both are already credible. It should be to prove that **this structured operation layer plus this feedback loop creates better creative leverage without sacrificing control**.

Build the experiment that earns the platform.
