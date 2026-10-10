# Generated audio — proposed October 2, 2026, revised October 10, 2026

Status: proposed, not built. Backlog: Next, Generated audio. What is said here
about the ElevenLabs API was read on October 10, 2026, from its API reference,
its pricing page, its music terms and the source of its Python package
(`elevenlabs` 2.71.0). No request was made with a key, so nothing here about
headers, costs or what a plan is given has been seen in a response.

## What

A generator in the DAW. It makes a sound, a loop or a whole song from a
description with the ElevenLabs API, and saves the result as a sample in the
project. It also saves a copy to a folder of everything the person has
generated, so a later song can find it by search. The person uses it from a
Generate panel in the app; the agent uses it as a `daw generate` command. What
comes back is sliced, placed, layered and mixed like any other sample.

The person's own ElevenLabs key turns it on. They enter it once, in a Settings
window, and it is kept in the macOS Keychain. Until a key is there, the panel
shows where to enter one and the command says the same. The agent can use the
key through `daw` but cannot read it.

## Why

When the library does not have a sound, there is nowhere to go today. The
agent picks the nearest sample, or writes a script that synthesizes one. A
generator fills that gap from a sentence: a riser in the song's key, a
two-bar drum loop at the song's tempo, a texture under a breakdown, or a whole
song made only to be cut up and replayed on pads.

It is a tool in the kit and never the product. A prompt that returns a
finished song is the thing this app is not ([concept.md](../concept.md)). What
comes back is material that the person and the agent still have to make
something of.

## Stages

1. **The key and `daw generate sound`.** Sounds and loops from the command
   line, the key in the Keychain, the generated folder, and the agent's daily
   budget.
2. **`daw generate music` and `daw generate plan`.** A whole song to sample,
   from a prompt or from a section-by-section plan the agent writes.
3. **In the app.** A Settings window with the key's field, and a Generate tab
   in the browser.
4. **Later, each its own backlog line:** regenerating one section of a
   generated song, stems from ElevenLabs, and spoken or sung phrases from its
   voices. See [Not in these stages](#not-in-these-stages).

Each stage is a backlog line and a pull request of its own when it is picked up.

## What ElevenLabs gives

Base address `https://api.elevenlabs.io`. The key goes in an `xi-api-key`
header. Errors come back as `{type, code, message, request_id, param}`.

### Sound effects

[`POST /v1/sound-generation`](https://elevenlabs.io/docs/api-reference/text-to-sound-effects/convert)
takes a JSON body:

| Field | What it does |
|---|---|
| `text` | The description (required). |
| `duration_seconds` | 0.5 to 30. When it is left out, the model picks a length. |
| `prompt_influence` | 0 to 1, 0.3 by default. Higher follows the text more literally. |
| `loop` | A sound with no audible start or end, made to repeat. |
| `model_id` | `eleven_text_to_sound_v2`, the only model. |

`output_format` is a query parameter. The response is the audio's bytes. The
documentation says the model takes musical prompts ("drum loop, 90 BPM", "brass
stabs in F minor"). It costs 40 credits a second when a length is given.

### Music

All under `/v1/music`
([compose](https://elevenlabs.io/docs/api-reference/music/compose),
[overview](https://elevenlabs.io/docs/overview/capabilities/music)).

| Endpoint | What it returns |
|---|---|
| `POST /v1/music` | The audio. A `song-id` header names the song. |
| `POST /v1/music/stream` | The same audio, in chunks. |
| `POST /v1/music/detailed` | A `multipart/mixed` response with two parts: JSON (the composition plan used, and a title, genres and languages) and the audio. It can add word timestamps. |
| `POST /v1/music/plan` | A composition plan made from a prompt. It costs no credits and is rate-limited. |
| `POST /v1/music/upload` | Stores a file for editing, and can extract its plan. It costs the same as a generation and is screened for copyright. |
| `POST /v1/music/stem-separation` | A ZIP of 2 or 6 stems of an uploaded file. |

The body of a generation takes:

- **One of:** `prompt` or `composition_plan`, never both.
- **With a prompt:** `music_length_ms`, 3,000 to 600,000, and
  `force_instrumental`.
- **With a plan:** `seed`. A seed helps a result stay the same and does not
  promise it.
- **Always:** `model_id` (`music_v1`, the default; `music_v2`; `music_v2_5`,
  the newest) and `store_for_inpainting`, which keeps the song on the service
  so a later plan can refer to it.

**A plan for v2 and v2.5** is a list of `chunks`, each 3 to 120 seconds long.
There are two kinds of chunk:

- **A chunk to generate** has `text` (a section label such as `[Chorus]`, any
  lyrics, and `{directions}`), `duration_ms`, positive and negative styles,
  and how closely it follows what came before.
- **A chunk of a stored song** is a `song_id` and a range of it, kept as it
  is.

That second kind is how the service regenerates one section and keeps the
rest. The v1 plan uses `sections` with global and local styles instead.

A prompt that names an artist, a song or a label is refused as `bad_prompt`,
with a suggested prompt in the error.

### Formats and plans

- **What comes back.** `output_format` defaults to `auto`: MP3 at 44.1 kHz
  and 128 kbps for v1, MP3 at 48 kHz and 192 kbps for v2. PCM is raw 16-bit
  samples with no WAV header, at 8 to 48 kHz. Opus is offered too.
- **Formats by plan.** MP3 at 192 kbps needs Creator or above. PCM at 44.1 kHz
  needs Pro or above. The pages for sound effects and stems say this; that it
  holds for music is assumed.
- **Who can generate music.** The Music API is for paid plans only.
- **How many requests at once.** Music runs 2 at a time on Starter, Creator
  and Pro, and 5 on Scale and Business. The response headers
  `current-concurrent-requests` and `maximum-concurrent-requests` give the
  numbers.
- **Prices.** Music is about $0.15 a minute through the API. Plans run from
  Starter at $6 a month to Business at $990.

### The key and the account

- **What a key can be limited to.** The endpoints it reaches, a credit quota,
  the addresses it is used from, and a date it expires. A key found in a
  public GitHub repository is disabled by the service.
- **Checking a key.** `GET /v1/user/subscription` gives the tier, the credits
  used and allowed, and when they reset. A key limited to generation may be
  refused there and still work; this is not checked.
- **What each status means.**
  - 401: the key is wrong or expired.
  - 402 (`insufficient_credits`): no credits are left.
  - 403: the plan does not include it (`subscription_required`), or the key
    is not allowed this endpoint.
  - 429: too many requests (`rate_limit_exceeded`), or too many at once
    (`concurrent_limit_exceeded`).
  - 422: a field is invalid.
- **Response headers kept in the recipe.** `request-id`, and the cost in
  `character-cost` (one page names it `x-character-count`, so both are read,
  in any case).

### The right to use it

From the [music terms](https://elevenlabs.io/music-terms):

- Starter and above may use what they generate commercially, online and off.
  Releasing it to streaming services needs Creator. Film, TV, radio and large
  games need an Enterprise music licence.
- Free requires attribution.
- Output keeps the terms of the plan in effect when it was made.
- Self-serve plans may not use it in a "music library", meaning a catalogue of
  sounds offered to others.

So the generated folder here is the person's own working material, and
nothing that packages a library for other people (Skills, devices and presets
packaged for other people, under Shipping) may include it. Prompts may not
name artists, songs or labels.

### The package and plain HTTP

The official Python package is generated code that pulls in httpx, pydantic,
requests and websockets. This feature uses five endpoints, so it makes plain
HTTPS requests from Rust instead. The only awkward parts are reading the
`multipart/mixed` reply of `/v1/music/detailed` and the headerless PCM.

## Design

### Commands

```
daw generate sound PROJECT "vinyl crackle, warm" [--seconds 4] [--loop]
                   [--bars 2] [--influence 0.3] [--id crackle] [--estimate]
daw generate music PROJECT "dusty soul ballad, 82 bpm, Rhodes and brushed drums"
                   [--seconds 60] [--instrumental] [--plan FILE] [--seed 7]
                   [--model music_v2_5] [--id soul] [--estimate]
daw generate plan  "dusty soul ballad, 82 bpm" [--seconds 90]
daw generate list  PROJECT
daw keys set elevenlabs | daw keys status | daw keys remove elevenlabs
daw keys budget --agent-seconds 120
```

**`daw generate sound` and `daw generate music`.** Each makes the request,
saves what comes back, imports it into the project and replies with JSON:

- the sample's ID, path, length and sample rate;
- its recipe;
- the credits the service says it cost, and the credits left when known.

It places nothing. An audio clip, a pad or a slice is made with the commands
that exist. This is the rule [stem separation](stem-separation.md) also
follows.

**What the options do.**

- `--bars N` sets the length from the song's tempo and meter, and writes the
  tempo into the request ("at 92 BPM"). The sample records it as its
  `source_bpm`, so a loop stretches with the song. Sounds can be at most 30
  seconds; a `--bars` past that is refused before anything is sent.
- `--estimate` sends nothing. It replies with the seconds, the credits by the
  documented rate, and what is left of the agent's budget today.
- `--plan FILE` sends a composition plan in place of a prompt. The plan is
  JSON, checked here against the chunk shape before it is sent.

**`daw generate plan`** returns the service's composition plan for a prompt.
It costs no credits. The agent can read the plan, change sections, lengths
and styles, and pass it to `--plan`. This is how the agent writes a song to
sample section by section rather than in one sentence.

**`daw generate list`** lists what this project has generated, with each
recipe, newest first.

**`daw describe generate`** says all of this for the agent: what each kind is
good for, the limits, the costs, the budget, the terms, and that a prompt
names no artist.

The command is named for what it does, not for the service. `--service` is
added if a second service ever is.

### Where the audio goes

1. **The bytes as they came back** go to the workspace's generated folder,
   `~/Music/AAW/library/generated/` (or under `AAW_WORKSPACE`), with the
   recipe beside them as `NAME.generated.json`. The folder is added to the
   sample library's folders, so `daw samples search` and the browser find
   every generated sound in later songs, analyzed like any other sample.
2. **The project gets a copy.** The file is then imported into the project as
   a file is imported today, through `samples import`:
   - A WAV is copied.
   - An MP3 is decoded once to WAV, and the copy is named by its hash.
   - The recipe is copied beside the project's copy.

The project then plays and renders with the network off, and nothing is
generated again at a render.

**Which format to ask for.** `daw keys status` reads the plan's tier from the
service and keeps it in `generate.json` in the data folder. That file holds
the tier and the budget, and never the key. `daw generate` then asks for:

- PCM at 48 kHz (music v2) or 44.1 kHz (sounds) on Pro and above. Headerless
  PCM is written as a WAV file before anything else reads it.
- MP3 at 192 kbps on Creator.
- MP3 at 128 kbps otherwise.

A 403 for the format is retried once with the next lower one, and the tier is
corrected.

### The recipe

`NAME.generated.json` holds:

- the service, endpoint and model;
- the prompt or plan, and every parameter;
- the `song-id`, the `request-id`, the cost and the tier;
- the date, and the origin that asked (the person or the agent).

It records how the sound was made; it cannot remake it, since the same request
returns different audio. The tier is there because the terms that apply are
the plan's when the sound was made. A project lists the outside services it
used by reading these files.

The recipe stays a file beside the audio, not a field on the sample. The
song's schema rejects unknown fields, and a file needs no schema change.
`daw generate list` is how the agent sees the recipes. `daw inspect` does not
show them.

### It is an edit like any other

With a host running, the import is one undoable step, marked with the origin
of whoever asked. The activity panel shows the prompt and the seconds
generated. Undo takes the sample out of the song; the files stay, in the
project and in the generated folder.

### The key

**Where it is kept.** One Keychain item: a generic password for the service
`AAW` and the account `elevenlabs`.

**How it is entered.**

- In the app's Settings window, which this adds (stage 3).
- With `daw keys set elevenlabs`, which asks on the terminal without showing
  what is typed (stage 1).

**What `daw keys status` says.** Whether a key is there and, from
`/v1/user/subscription`, the tier, the credits left and when they reset. It
never prints the key; no command does.

**How `daw` uses it.** It reads the key from the Keychain for the one request,
with the `security-framework` crate. The key is never:

- a flag or an environment variable;
- a file in the project or the workspace;
- a line in a log, or part of an error.

**Who else can read it.** The item's access list names the app and its bundled
`daw`. Any other program that asks for it, the `security` tool included, gets
a macOS prompt the person answers. So an agent that runs commands as the
person can use the key through `daw`, but cannot read it without the person
seeing.

**While developing in this checkout,** the `daw` in `engine/target/release` is
signed ad hoc. macOS is expected to ask again after each rebuild; "Always
Allow" covers that build only.

**Tests never touch the Keychain.** `AAW_GENERATE_URL` points the commands at
a stand-in server. When it does, and only when it points at a loopback
address, the key is read from `AAW_TEST_GENERATE_KEY`. A real key set there
could only ever reach this machine.

### What the agent may spend

Each second of audio costs credits, and an agent can loop. So:

- **A daily budget.** Settings and `daw keys budget` set the seconds of audio
  the agent may generate in a day without asking: 120 by default, and 0 to
  require asking every time.
- **What counts as the agent.** Requests from the agent's origin count against
  it. A request from the app's panel is the person's and does not.
- **When it is spent,** the command refuses with what was spent today and how
  the person raises the limit. The agent tells the person and does not work
  around it.
- **`--estimate`** lets the agent check the cost before it asks for the audio.
- **The service's own limits still apply.** A key can be given a credit quota
  on the ElevenLabs side, which is the hard limit. The docs say so.

### What leaves the machine

The prompt, its parameters and the composition plan, and only after the
person has entered a key. No audio leaves and no project text is sent. Stage
4's uploads would change that, and would ask first.

### Text that comes back is data

A suggested prompt, an error message, a title and a file's name are shown and
never followed. A `bad_prompt` suggestion is returned to the agent as a field
it may offer the person. It is never sent on its own.

### Errors

Each says what happened and what to do:

| Situation | What the command says |
|---|---|
| No key | Enter one in Settings, or with `daw keys set elevenlabs`. |
| 401 | The key is wrong or expired. Enter it again. |
| 402 | No credits are left, and when they reset. |
| 403 | The plan does not include this, or the key is limited and not allowed this endpoint. |
| 429 | Too many requests at once, and how many the plan allows. One retry after the time `retry-after` gives. |
| `bad_prompt` | The prompt named something it may not, and the suggestion. |
| Budget spent | What was spent today, and where the person raises the limit. |
| No network | Nothing was spent. |

### In the app (stage 3)

**Settings.** The app menu gets **Settings…** (⌘,), a small native window with
an **ElevenLabs** section:

- a secure field and **Save**, which writes the Keychain item with its access
  list;
- **Remove**;
- the tier, the credits left and the reset date, read through `daw keys
  status`;
- the agent's daily budget.

Saving runs `daw keys status` once, so a wrong key is said at once.

**The Generate tab** sits in the browser, beside Samples. It goes there rather
than on a track because a result can go to any track, a pad or a slice. It has:

- a description;
- the kind: Sound, Loop or Song. Loop sets `--loop` and a length in bars from
  the song.
- a length, Instrumental for a song, and **Generate**.

With no key, the tab shows a line and a button that opens Settings.

**While a request runs.** The app runs `daw generate` as a process, off the
main thread, with the time so far shown. **Cancel** stops the process, and
nothing is imported. The song keeps playing.

**What has been generated.** Below the form, newest first: this project's
results, then the generated folder's. Each shows its prompt and length, is
auditioned on click, and is dragged to the timeline or a header like any
sample. Its menu has **Generate Again**, which fills the form from the recipe,
and **Show Recipe**.

## Building it

### Stage 1: the key and `daw generate sound`

1. **The client.** A module `generate` in `aaw-host`, or a crate
   `aaw-generate` if it grows. It holds:
   - the client, on `ureq` with rustls: blocking and small, with no async
     runtime;
   - the base address from `AAW_GENERATE_URL`;
   - the errors mapped to the table above;
   - PCM written as WAV with `hound`.
2. **The key store.** `keys.rs`: Keychain read, write and delete through
   `security-framework` on macOS, with an access list naming the calling
   binary and, once there is one, the app. On other systems it says it
   needs a Mac. The loopback test key is read here.
3. **The commands.** `Generate` and `Keys` subcommands in
   `engine/crates/aaw-cli/src/main.rs`:
   - Import goes through `python::run("samples", ["import", ...])`, until
     sample import is in Rust.
   - The generated folder is registered once with the library's folder
     command.
   - `generate.json` (the tier, the budget and today's seconds by origin)
     lives in the data folder.
4. **The budget,** counted by origin. Requests through a running host take
   the host's origin. Requests without one count as the agent's unless the
   app passes `--origin person`.
5. **`daw describe generate`.** Add the topic in `aaw-model/src/describe.rs`.
6. **Tests.** Rust tests in `aaw-host/tests/generate.rs`, against a stand-in
   server (`tiny_http`, dev dependency only) that returns a generated sine as
   PCM or MP3 and replays each error status. They cover:
   - the import and the recipe;
   - `--bars` against the song's tempo, and `--estimate`;
   - the budget;
   - each error;
   - that the key never appears in output, errors or files. The run is
     grepped for it.

   A pytest checks that the CLI's JSON is as documented. On Linux, the
   Keychain tests are skipped.
7. **Docs,** in the same pull request:
   - `architecture.md` and `engine/README.md`;
   - this file, rewritten for what stage 1 built;
   - `concept.md` and a decision: a new D-entry that revises D11 and the
     line "No generative audio model at the core…", to allow a generator, at
     the person's own key, as a source of material and never as the product;
   - the backlog line moved, and Verify lines for what nobody has heard.

### Stage 2: `daw generate music` and `daw generate plan`

1. **The detailed reply.** Music is asked for through `/v1/music/detailed`,
   so the plan the service used comes back with the audio. The
   `multipart/mixed` reply is split on its boundary. Its JSON becomes the
   recipe's `plan`, and its title becomes the sample's suggested name.
2. **The song is stored.** `store_for_inpainting` is set, and the `song-id` is
   kept, so stage 4 can regenerate a section without generating the whole
   song again.
3. **`daw generate plan`.** It calls `/v1/music/plan`, and `--plan FILE`
   checks a plan's chunks and lengths before sending.
4. **The reply maps the song.** It runs `daw samples beats` on the result and
   gives the tempo and downbeats, so the agent can cut the song into audio
   clips on its bars straight away.
5. **Tests.** The stand-in returns a click track at a known tempo as a
   multipart reply. Tests check the plan in the recipe, the stored song ID,
   and that the beat map finds the tempo.

### Stage 3: Settings and the Generate tab

1. **`SettingsWindow.swift`** with the ElevenLabs section, and **Settings…**
   added to the app menu in `App.swift`. The key is written with
   `SecKeychainItem` and an access list (`SecAccessCreate`) naming the app
   and its bundled `daw`. The modern data-protection keychain needs an
   entitlement and a team signature, so it waits for Developer ID signing.
2. **The tab.** `GenerateView.swift`, and a tab in `BrowserView.swift`.
   - It runs `daw generate ... --origin person` through the bundled tool
     (`CommandLineTool.swift`).
   - It lists results with `daw generate list`.
   - Drag uses the samples' existing drag type.
3. **Tests.** Layout and state tests in `apps/mac/Tests`: no key, a request
   running, an error, and results. A snapshot of each with `./build.sh
   --snapshot`.

## Not in these stages

Each becomes a backlog line when wanted:

- **Regenerating one section** of a generated song: a plan that keeps the
  stored song's other sections as chunks of it, and replaces one.
- **A song continued from a person's own audio,** through `/v1/music/upload`.
  This sends audio off the machine, so it asks every time.
- **Stems from ElevenLabs.** [Stem separation](stem-separation.md) runs on
  this Mac and sends nothing, so it comes first. The service's split would be
  an option for those without the local runtime.
- **Phrases from its voices,** text to speech and voice changing, as vocal
  chops.
- **Video to music.** Not a use this app has.

## Depends on

- **The concept.** Picking this up changes the concept and revises D11, as in
  stage 1's docs.
- **Signing with a Developer ID** (Other Macs, under Shipping), for an access
  list that holds across rebuilds. Until then, macOS asks again after each
  build.
- **Sample import in Rust** (An app that stands alone, under Shipping), for
  the app's panel on a Mac without Python. Until then the panel needs this
  checkout, as the browser does.

## Done when

- `daw generate sound` with a description puts a sample in the project, with
  its recipe beside it, that a pad or an audio clip plays. The same sound is
  in the generated folder and `daw samples search` finds it.
- `--loop --bars 2` gives a sound at the song's tempo that repeats as an audio
  clip with no click at the join.
- `daw generate music` gives a song whose beat map is in the reply and that is
  cut into audio clips on its downbeats. A plan from `daw generate plan`,
  edited and passed to `--plan`, gives the sections it asked for.
- With no key, the command and the panel say where to enter one. A wrong key,
  spent credits, a plan without music, a refused prompt and a spent budget
  each say which.
- The key is in no file, no command's output and no log. Reading it with
  another program asks the person.
- The project plays and renders with the network off.
- The Settings window saves, checks and removes the key. The panel generates,
  auditions and drags a result onto the timeline while the song plays.
- An agent asked for a song with a sound the library lacks checks the cost,
  generates the sound, and uses it within its budget.
- A person has done each of the above by hand, with a real key, and heard the
  results. The response headers, costs and formats seen are written here.

## Open questions

1. **Who reads the Keychain.** The plan above has `daw` read it, which works
   with the app closed. The alternative is that only the app reads it and `daw`
   asks the app, which keeps the key out of any process the agent starts but
   needs the app running. A spike on a signed and an unsigned build should show
   what each one prompts for.
2. **Formats by plan, seen.** Whether `pcm_44100` really needs Pro for music,
   and what the 403 says. One request on each tier the person has.
3. **The longest song.** The reference allows 600,000 ms; the overview says
   five minutes. Stage 2 refuses past five minutes until a request shows
   otherwise.
4. **What a sound costs with no length,** and whether the cost header is
   present on sound effects. `--estimate` assumes 30 seconds when no length is
   given.
5. **Which music model is the default.** `music_v2_5` is the newest and the
   API still defaults to `music_v1`. The person hears the same prompt on
   each.
6. **A key limited to generation.** Whether such a key is refused by
   `/v1/user/subscription`. If it is, `daw keys status` says the key is there
   and its tier is unknown, and formats fall back to MP3.
7. **The budget's unit.** It is in seconds of audio because seconds are what
   the agent controls. Credits would be more exact, and the service's own key
   quota already counts them.
8. **The generated folder and the terms.** It is the person's own material.
   Whether anything that copies a workspace (a backup, a move to another Mac)
   needs to say so.
