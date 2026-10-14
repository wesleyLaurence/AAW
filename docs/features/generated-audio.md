# Generated audio — stage 1 implemented October 14, 2026

`daw generate sound` makes a sound or a loop from a description with the
ElevenLabs API, at the person's own key, and saves the result as a sample in
the project. It also saves a copy to a folder of everything the person has
generated, which the sample library searches, so a later song can find it.
Stage 1, the key and sounds from the command line, is built (D120); whole songs
from a prompt or a plan (stage 2) and the Settings window and Generate tab in
the app (stage 3) are the remaining Next line in [the backlog](../backlog.md),
designed below under [Not built yet](#not-built-yet).

It is a tool in the kit and never the product. A prompt that returns a
finished song is the thing this app is not ([concept.md](../concept.md)). What
comes back is material that the person and the agent still have to make
something of: it is sliced, placed, layered and mixed like any other sample,
and the command places nothing.

What is said here about the ElevenLabs API was read on October 10, 2026, from
its API reference, its pricing page, its music terms and the source of its
Python package (`elevenlabs` 2.71.0). No request has been made with a real
key: the costs, the headers and what each plan is given are as documented, and
every run so far was against a stand-in on the machine.

## Commands

```sh
uv run daw generate sound PROJECT "vinyl crackle, warm" [--seconds 4 | --bars 2] [--loop]
                          [--influence 0.3] [--id crackle] [--estimate]
uv run daw generate list PROJECT
uv run daw keys set elevenlabs | daw keys status | daw keys remove elevenlabs
uv run daw keys budget [--agent-seconds 120]
```

**`daw generate sound`** sends the description to `POST /v1/sound-generation`
with `model_id: eleven_text_to_sound_v2`, saves what comes back, imports it
into the project as the sample `--id` (the first words of the description as a
slug unless given, numbered when the song has the name) and replies with the
sample's `sample_id`, `path`, `seconds`, `sample_rate`, `channels` and
`format`; `bpm` and `bars` when `--bars` set them; `recipe` and
`generated_file`; the service's `request_id` and `cost_credits`; the `tier`;
the `origin`; the `budget`; `revision` under a host; and `notes` when a
format fell back or the folder was not registered. It places nothing: a pad
(`daw pad add`), an audio clip (`daw audio add`) or a slice is made with the
commands that exist, the rule [stem separation](stem-separation.md) also
follows. The reply says so in `next`.

- `--seconds S` is 0.5 to 30. With neither it nor `--bars`, the model picks a
  length, and the budget is held to 30 s until the audio says how long it is.
- `--bars N` sets the length from the song's tempo and meter (`N × beats in a
  bar × 60 / tempo`), writes "at T BPM" into the text sent unless the
  description already says a BPM, and records the tempo in the recipe. A
  length past 30 s is refused before anything is sent. The sample's entry
  does not carry the tempo: a sample has no such field in the schema.
- `--loop` asks for a sound with no audible start or end, made to repeat.
- `--influence`, 0 to 1 and 0.3 by the service's default, is how literally
  the text is followed.
- `--estimate` sends nothing and replies with the seconds (`seconds_assumed`
  when 30 was taken), the credits by the documented rate of 40 a second, the
  format the tier would get, whether the budget allows it, and whether a key
  is there.
- `--origin user` (the global flag) marks the request as the person's, which
  the budget does not count; the default is the agent's.

**`daw generate list`** reads the project's recipes, newest first, each with
its prompt, kind, length, tempo, date, origin, cost, tier, parameters, the file
in the generated folder and the `sample_id` that plays it when the song still
has one.

**`daw keys`**: `set elevenlabs` asks for the key on the terminal without
showing it (or reads the first line of a pipe) and keeps it in the Keychain,
then reads the plan once so a wrong key is said at once; `status` says
whether a key is there and, from `GET /v1/user/subscription`, the tier, the
credits used and allowed and when they reset, with the budget and the
generated folder; `remove elevenlabs` forgets it; `budget --agent-seconds N`
sets the agent's day, and without a value shows it. No command prints a key.

**`daw describe generate`** says all of this for the agent: what each kind is
good for, the result, the files, the recipe, the key, the budget, the
formats, the errors, the terms and the limits.

## Where the audio goes

1. **The bytes as they came back** go to the workspace's generated folder,
   `~/Music/AAW/library/generated/` (or `library/generated` under
   `AAW_WORKSPACE`), as `DATE-TIME-ID.wav` or `.mp3` (`2026-10-14-153012-crackle.wav`),
   with the recipe beside them as `DATE-TIME-ID.generated.json`. Headerless PCM
   is given a WAV header before anything else reads it. After each sound the
   folder is scanned into the sample library with `daw samples scan`, which
   is incremental and registers the folder among the library's folders the
   first time, so `daw samples search` and the browser find every generated
   sound in later songs, analyzed like any other sample; a scan that fails,
   as without Python, is a note in the reply, not a refusal.
2. **The project gets a copy.** A WAV is copied to `samples/HASH12_DATE-TIME-ID.wav`,
   named by the first twelve characters of its hash as `samples import` names
   a copy, with the recipe beside it as `samples/HASH12_DATE-TIME-ID.generated.json`,
   and added to the song's `samples` by one `apply`, labelled `Generate ID`,
   routed to a running host or made headless like any edit. An MP3 goes
   through `samples import` itself, which decodes it once to WAV and adds it
   as an edit of its own; the recipe is then written beside the decoded copy.

The project then plays and renders with the network off, and nothing is
generated again at a render. Undo takes the sample out of the song; the files
stay, in the project and in the generated folder.

**Which format is asked for.** `daw keys status` and the first `generate`
read the plan's tier and keep it in `generate.json` in the data folder
(`~/Library/Application Support/AAW/`, or `AAW_DATA_DIR`), which holds the
tier, the budget, the day's spend by origin and whether the folder was
registered, and never the key. The sound is asked for as `pcm_44100` on Pro,
Scale, Business and Enterprise, `mp3_44100_192` on Creator and
`mp3_44100_128` otherwise, which is also what an unknown tier gets. A 403, or
a refusal that names `output_format`, whose code is not the plan's
(`subscription_required`, `missing_permissions`, `invalid_api_key`,
`quota_exceeded`, `insufficient_credits`) is asked for again one format lower,
once a step, and the tier is corrected downward (`creator` after PCM failed,
`starter` after 192 kb/s). A 429 is retried once after `retry-after`, capped
at 30 s. Nothing else is retried, so a request is paid for at most once.

## The recipe

`DATE-TIME-ID.generated.json` holds the service, endpoint and `model_id`; the
`prompt` as given and the `text_sent`; every parameter (`duration_seconds`,
`prompt_influence`, `loop`, `output_format`); `bars` and `bpm`; `song_id`
(null for a sound); the `request_id`, read from `request-id` or
`x-request-id`; `cost_credits`, read from `character-cost` or
`x-character-count`; the `tier`; the `seconds`, `sample_rate` and `channels`
of the file; the `file` name; the `created` date; and the `origin` that asked.
The project's copy adds `generated_file`, the path in the generated folder.

It records how the sound was made; it cannot remake it, since the same request
returns different audio. The tier is there because the terms that apply are
the plan's when the sound was made. The recipe stays a file beside the audio,
not a field on the sample: the song's schema rejects unknown fields, and a
file needs no schema change. `daw inspect` does not show them.

## The key

**Where it is kept.** One Keychain item: a generic password for the service
`AAW` and the account `elevenlabs`, written and read with the
`security-framework` crate (`keys.rs`). The item is created by `daw`, so its
access list names that build of `daw`: it reads the key for a request without
a prompt once the person has allowed it, and any other program that asks, the
`security` tool included, gets a macOS prompt the person answers. So an agent
that runs commands as the person can use the key through `daw` but cannot
read it without the person seeing. In this checkout `daw` is signed ad hoc, so
macOS asks again after a rebuild; "Always Allow" covers that build only.

**What never happens.** The key is never a flag or an environment variable,
never a file in the project, the workspace or the data folder, never a line
in a reply, a note or an error. `Key` has no `Display` and prints as `Key(…)`,
and the client prints the same. The tests grep every reply and every file for
the key.

**Tests never touch the Keychain.** `AAW_GENERATE_URL` points the commands at a
stand-in server; when it does, and only when it names a loopback address
(`127.0.0.1`, `localhost`, `::1`), the key is read from `AAW_TEST_GENERATE_KEY`
instead, and `daw keys set` refuses to write the Keychain. A real key set there
could only ever reach this machine. On Linux the Keychain functions say they
need a Mac.

## What the agent may spend

Each second of audio costs credits, and an agent can loop. So:

- **A daily budget**, in seconds of audio the agent may generate without
  asking: 120 unless `daw keys budget --agent-seconds N` set it, 0 to require
  asking every time. The day turns at midnight UTC.
- **What counts as the agent.** Requests with the agent's origin, the default.
  A request with `--origin user`, as the app's panel will send it, is the
  person's and does not count; it is still recorded in the day's spend.
- **Before a request** the budget is held to the length asked for, or 30 s
  when the model picks it. **After,** the length of the audio that came back
  is what is counted, whether or not the import then succeeds.
- **When it is spent,** the command refuses with what was spent today, what
  was asked, and how the person raises the limit; nothing is sent. The topic
  tells the agent to tell the person and not work around it.
- **The service's own limits still apply.** A key can be given a credit quota
  on the ElevenLabs side, which is the hard limit.

## What leaves the machine

The description, its parameters and the model's name, and only after the
person has entered a key. No audio leaves and no project text is sent. A
suggested prompt, an error message or a file's name that comes back is shown
and never followed: a `bad_prompt` suggestion is returned in the error as text
for the agent to offer the person, and is never sent on its own.

## Errors

Each says what happened and what to do:

| Situation | What the command says |
|---|---|
| No key | Enter one with `daw keys set elevenlabs`. |
| 401 | The key is wrong or expired; enter it again. |
| 402 | No credits are left; `daw keys status` says when they reset. |
| 403 | The plan does not include this, or the key is limited and not allowed this endpoint (after the format fallback above). |
| 429 | Too many requests, how many the plan allows at once from `maximum-concurrent-requests`, and that one retry was made. |
| 422 | The service refused a field, with its message. |
| `bad_prompt` | The prompt named something it may not, and the suggestion as text. |
| Budget spent | What was spent today, what was asked, and where the person raises the limit. |
| Unreachable | Nothing was sent and nothing was spent. |
| Generated but not imported | Where the file is in the generated folder, and why the import failed. |

The service's error bodies are read in the shapes its documentation and its
package show: `{detail: {status, message}}`, `{detail: "text"}`, `{detail:
[{msg}]}` and `{code, message}`.

## How it is built

- `aaw-host/src/generate.rs`: the client on `ureq` 3 with rustls, blocking,
  with no async runtime, with HTTP statuses read rather than raised and the
  proxy left out for a loopback address; `Settings` and `generate.json`;
  `Format`; `SoundRequest` with the bars arithmetic, the checks, the body and
  the estimate; `sound` with the retries; `save`, the WAV header,
  `project_copy`, `project_recipe`, `list` and `default_id`.
- `aaw-host/src/keys.rs`: the Keychain through `security-framework` on macOS,
  a refusal elsewhere.
- `aaw-cli/src/generate.rs`: `daw generate` and `daw keys`, the import routed
  through `apply` or `samples import`, the folder registered once, the key
  read with `rpassword` on a terminal.
- `aaw-model/src/contract.rs`: the `generate` topic.
- Tests: `aaw-host/tests/generate.rs` against a stand-in server written in
  the test over `TcpListener` (no `tiny_http`), covering the request as
  documented, the files and the recipe, the edit through a session, the
  format fallback, the 429 retry, each error, the subscription, an
  unreachable service, the checks before sending, the settings and the
  estimate; unit tests of the loopback check, the key's `Debug`, the budget
  and the bars; `tests/test_generate.py` running the binary against a
  Python stand-in, with and without a host: the sound found by the library,
  played by a pad and rendered, the estimate, the budget, the undo, the
  refusals, an MP3 plan decoded by import where a decoder is present, the
  help and the topic, and the key in no output and no file.

## Not built yet

Each is a backlog line when it is picked up, and this file is rewritten then.

### Stage 2: `daw generate music` and `daw generate plan`

```
daw generate music PROJECT "dusty soul ballad, 82 bpm, Rhodes and brushed drums"
                   [--seconds 60] [--instrumental] [--plan FILE] [--seed 7]
                   [--model music_v2_5] [--id soul] [--estimate]
daw generate plan  "dusty soul ballad, 82 bpm" [--seconds 90]
```

All under `/v1/music` ([compose](https://elevenlabs.io/docs/api-reference/music/compose),
[overview](https://elevenlabs.io/docs/overview/capabilities/music)):

| Endpoint | What it returns |
|---|---|
| `POST /v1/music` | The audio. A `song-id` header names the song. |
| `POST /v1/music/detailed` | A `multipart/mixed` response: JSON (the composition plan used, a title, genres and languages) and the audio. |
| `POST /v1/music/plan` | A composition plan made from a prompt. It costs no credits and is rate-limited. |
| `POST /v1/music/upload` | Stores a file for editing; costs the same as a generation and is screened for copyright. |
| `POST /v1/music/stem-separation` | A ZIP of 2 or 6 stems of an uploaded file. |

The body takes one of `prompt` or `composition_plan`; with a prompt,
`music_length_ms` (3,000 to 600,000) and `force_instrumental`; with a plan,
`seed`; always `model_id` (`music_v1` the default, `music_v2`, `music_v2_5`
the newest) and `store_for_inpainting`. A plan for v2 and v2.5 is a list of
`chunks`, each 3 to 120 seconds, either a chunk to generate (`text` with a
section label, lyrics and `{directions}`, `duration_ms`, styles, how closely
it follows what came before) or a chunk of a stored song (`song_id` and a
range), which is how a section is regenerated and the rest kept. A prompt
that names an artist, a song or a label is refused as `bad_prompt`. The Music
API is for paid plans; music runs 2 at a time on Starter to Pro and 5 on Scale
and Business (`current-concurrent-requests`, `maximum-concurrent-requests`);
about $0.15 a minute; `output_format` defaults to MP3 at 48 kHz and 192 kb/s
for v2.

Plan: music asked for through `/v1/music/detailed`, the multipart reply split
on its boundary, its JSON the recipe's `plan` and its title the sample's
suggested name; `store_for_inpainting` set and the `song-id` kept;
`daw generate plan` calling `/v1/music/plan`, `--plan FILE` checking a plan's
chunks and lengths before sending; the reply running `daw samples beats` on the
result so the agent cuts the song into audio clips on its bars; PCM at 48 kHz
for music on Pro and above; the stand-in returning a click track at a known
tempo as a multipart reply. Stage 2 refuses past five minutes until a request
shows the reference's 600,000 ms holds.

### Stage 3: Settings and the Generate tab

**Settings.** The app menu gets **Settings…** (⌘,), a small native window with
an **ElevenLabs** section: a secure field and **Save**, which writes the
Keychain item with an access list (`SecAccessCreate`) naming the app and its
bundled `daw`, so the two share it without a prompt; **Remove**; the tier,
the credits left and the reset date through `daw keys status`; the agent's
daily budget. Saving runs `daw keys status` once. The modern data-protection
keychain needs an entitlement and a team signature, so it waits for Developer
ID signing.

**The Generate tab** sits in the browser beside Samples, since a result can go
to any track, a pad or a slice: a description; the kind, Sound, Loop or Song
(Loop sets `--loop` and a length in bars); a length, Instrumental for a song,
and **Generate**. With no key, a line and a button that opens Settings. The app
runs `daw generate … --origin user` as a process off the main thread
(`CommandLineTool.swift`) with the time so far shown; **Cancel** stops the
process and nothing is imported; the song keeps playing. Below the form,
newest first, this project's results then the generated folder's, each
auditioned on click and dragged to the timeline or a header with the samples'
drag type, with **Generate Again** and **Show Recipe** in its menu. Tests:
layout and state tests in `apps/mac/Tests` and snapshots of no key, a request
running, an error and results.

### Later, each its own backlog line

- **Regenerating one section** of a generated song: a plan that keeps the
  stored song's other sections as chunks of it, and replaces one.
- **A song continued from a person's own audio,** through `/v1/music/upload`.
  This sends audio off the machine, so it asks every time.
- **Stems from ElevenLabs.** [Stem separation](stem-separation.md) runs on
  this Mac and sends nothing, so it comes first.
- **Phrases from its voices,** text to speech and voice changing, as vocal
  chops.
- **Video to music.** Not a use this app has.

## Depends on

- **Signing with a Developer ID** (Other Macs, under Shipping), for an access
  list that holds across rebuilds. Until then, macOS asks again after each
  build.
- **Sample import in Rust** (An app that stands alone, under Shipping), for an
  MP3 result and the folder's registration on a Mac without Python; a PCM
  result needs no Python but that.

## Done when

Stage 1, by scripted runs; nothing yet by a person:

- [x] `daw generate sound` with a description puts a sample in the project,
  with its recipe beside it, that a pad or an audio clip plays. The same sound
  is in the generated folder and `daw samples search` finds it.
- [ ] `--loop --bars 2` gives a sound at the song's tempo that repeats as an
  audio clip with no click at the join. The length and the text are held by
  tests; whether the join is clean nobody has heard.
- [x] With no key, the command says where to enter one. A wrong key, spent
  credits, a plan without the format, a refused prompt and a spent budget each
  say which.
- [x] The key is in no file, no command's output and no log, by tests against
  the stand-in. That reading it with another program asks the person is the
  Keychain's behavior and has not been tried.
- [x] The project plays and renders with the network off.
- [ ] An agent asked for a song with a sound the library lacks checks the
  cost, generates the sound, and uses it within its budget.
- [ ] A person has done each of the above by hand, with a real key, and heard
  the results. The response headers, costs and formats seen are written here.

Stages 2 and 3: `daw generate music` gives a song whose beat map is in the
reply and that is cut into audio clips on its downbeats, and a plan from
`daw generate plan`, edited and passed to `--plan`, gives the sections it
asked for; the Settings window saves, checks and removes the key; the panel
generates, auditions and drags a result onto the timeline while the song plays.

## Open questions

1. **Who reads the Keychain.** `daw` reads it, which works with the app
   closed. The alternative is that only the app reads it and `daw` asks the
   app. A spike on a signed and an unsigned build should show what each one
   prompts for; stage 3's access list naming both is the plan.
2. **Formats by plan, seen.** Whether `pcm_44100` really needs Pro for sounds
   and for music, and what the refusal's status and code are; the fallback
   here reads a 403 and any message naming `output_format`. One request on
   each tier the person has.
3. **What a sound costs with no length,** and whether `character-cost` is
   present on sound effects and under that name. `--estimate` assumes 30
   seconds when no length is given; the first real reply says what it was.
4. **A sound's channels.** The file is written as mono from PCM. If the service
   sends stereo PCM for sounds, the header is wrong and the length halves;
   the first real reply says.
5. **A key limited to generation.** Whether such a key is refused by
   `/v1/user/subscription`. If it is, `daw keys status` says the key is there
   with a `problem`, and formats fall back to MP3 at 128 kb/s.
6. **The budget's unit.** Seconds of audio, since seconds are what the agent
   controls. Credits would be more exact, and the service's own key quota
   already counts them.
7. **The origin of an MP3's import.** `samples import` applies the edit
   without the caller's `--origin`, so a person's MP3 result is logged as the
   agent's; a PCM result keeps the origin. Import in Rust would end this.
8. **The generated folder and the terms.** It is the person's own material.
   Whether anything that copies a workspace (a backup, a move to another Mac)
   needs to say so.
9. **Which music model is the default** for stage 2. `music_v2_5` is the
   newest and the API still defaults to `music_v1`.
