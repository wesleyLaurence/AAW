# Generated audio — stage 1 implemented October 14, 2026; stage 2 October 24, 2026; stage 3 October 25, 2026

`daw generate sound` makes a sound or a loop from a description with the
ElevenLabs API, at the person's own key, and saves the result as a sample in
the project; `daw generate music` makes a song to sample from, from a prompt or
a composition plan that `daw generate plan` writes, and gives its beat map. Each
also saves a copy to a folder of everything the person has generated, which
the sample library searches, so a later song can find it. Stage 1, the key and
sounds from the command line, is built (D120); stage 2, songs and plans, is
built (D160); the app's ElevenLabs pane in Settings and its Generate tab in
the browser (stage 3) are built (D161), described under [In the app](#in-the-app).

It is a tool in the kit and never the product. A prompt that returns a
finished song is the thing this app is not ([concept.md](../concept.md)). What
comes back is material that the person and the agent still have to make
something of: it is sliced, placed, layered and mixed like any other sample,
and the command places nothing.

What is said here about the ElevenLabs API was read on October 10, 2026, from
its API reference, its pricing page, its music terms and the source of its
Python package (`elevenlabs` 2.71.0), and for music read again on October 24,
2026, from the reference of `/v1/music/detailed` and `/v1/music/plan`, the
music guide, the inpainting guide, the API pricing page and the package's
`music_custom.py`. No request has been made with a real key: the costs, the
headers and what each plan is given are as documented, and every run so far
was against a stand-in on the machine.

## Commands

```sh
uv run daw generate sound PROJECT "vinyl crackle, warm" [--seconds 4 | --bars 2] [--loop]
                          [--influence 0.3] [--id crackle] [--estimate]
uv run daw generate music PROJECT "dusty soul ballad, 82 bpm, Rhodes" [--seconds 60] [--instrumental]
                          [--model music_v2_5] [--id soul] [--estimate]
uv run daw generate music PROJECT --plan plan.json [--seed 7] [--model music_v2_5] [--id soul] [--estimate]
uv run daw generate plan "dusty soul ballad, 82 bpm" [--seconds 90] [--model music_v2_5] [--out plan.json]
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

**`daw generate music`** asks `POST /v1/music/detailed` for a song and imports
it as `sound` imports a sound: the files, the recipe, one `apply` labelled
`Generate ID`, the folder scanned. It takes a prompt or `--plan FILE`, not both.

- With a prompt: `--seconds S`, 3 to 300, sent as `music_length_ms`, and
  `--instrumental`, sent as `force_instrumental`. Without `--seconds` the model
  picks the length and the budget holds five minutes until the audio says,
  which the agent's default day of 120 s refuses, with a line saying to give
  `--seconds`.
- With `--plan FILE`: the file is a composition plan, or the reply of `daw
  generate plan`, whose `plan` it takes. `--seed N` goes with it. The plan
  gives the length; `--seconds` and `--instrumental` are refused with one, and
  `--seed` with a prompt, since the reference takes them only so.
- `--model` is `music_v2_5` unless given, the newest, where the service's own
  default is still `music_v1`; `music_v2` and `music_v1` are taken. A v2 plan
  is `{chunks}`, a v1 plan `{positive_global_styles, negative_global_styles,
  sections}`, and a plan of the other model's shape is refused naming the
  model it is for.
- `store_for_inpainting` is always sent, so the song is kept on the service
  and its `song-id` header, read into the recipe and the reply, can name its
  parts in a later plan.
- `--id` is the sample's ID; unless given, the reply's title as a slug
  (`dusty-soul-ballad`), else the prompt's first words, numbered when taken.
- `--estimate` sends nothing and replies with the seconds (`seconds_assumed`
  when five minutes were taken), the dollars at the API's $0.15 a minute,
  `credits: null` since no rate in credits is documented for music, the
  format, the budget and whether it allows it.

The reply has what `sound`'s has, `model_id`, `title`, `genres`,
`is_explicit`, `song_id`, and two that let the agent cut it up:

- **`beat_map`**, from `daw samples beats` run on the project's copy with the
  song's meter and `--all`: `tempo` (its `bpm`, whether it is steady, what it
  may be confused with), `meter`, `beats_per_bar`, `first_downbeat_seconds`,
  `downbeat_confidence`, `beat_count`, `phrases`, and `bars`, every bar's
  downbeat in seconds, bar N at `bars[N-1]`. When Python cannot run, it is
  null with a note.
- **`sections`**: the plan the service used, or else the one sent, a row a
  part, with its `name` (the `[Name]` its text starts with, or a v1
  `section_name`), `start_seconds`, `seconds`, `styles`, `kept_from` for a
  chunk of a stored song, and, with a beat map, the `bar` whose downbeat is
  nearest its start, that downbeat's `bar_seconds` and `off_seconds`.

`next` says how: `daw audio add PROJECT TRACK ID --at BEAT
--source-start-seconds S --source-end-seconds E` with S and E from the bars,
and `--source-bpm` at the measured tempo with `--stretch preserve_pitch` to fit
the song's tempo. Nothing is placed.

**`daw generate plan`** asks `POST /v1/music/plan` for a plan from a prompt,
with `--seconds` as `music_length_ms` and `--model`. It costs no credits, is
not counted in the budget and needs the key and no project. The reply gives
the `plan`, its `seconds`, its `sections` as above without bars, and a
`problem` when `generate music` would refuse it as it is; `--out FILE` writes
the plan alone, ready to edit and pass to `--plan`.

**A plan is checked before anything is sent**, by the reference's limits and
the inpainting guide's: at most 30 chunks; a chunk 3,000 to 120,000 ms; a
section name of 1 to 100 characters; at most 30 lines after the name, each at
most 200 characters; `positive_styles` required and `negative_styles`
optional, each at most 50; `context_adherence` one of low, medium, high, and
`condition_strength` one of low, medium, high, xhigh; a chunk of a stored song
with a `song_id` and a range of at least 50 ms; a `conditioning_ref` at most
30,000 ms; a v1 plan's global styles, and each section's local styles, length
and lines; and the whole 3 s to five minutes. A refusal names the chunk and
the limit. What the service refuses beyond these is its 422, passed on.

**`daw generate list`** reads the project's recipes, newest first, each with
its prompt, kind, title, model, `song_id`, length, tempo, date, origin, cost,
tier, parameters, the file in the generated folder and the `sample_id` that
plays it when the song still has one.

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
   routed to a running host or made headless like any edit. An MP3 is
   decoded once to WAV by `samples import --copy-only`, which changes nothing
   in the song; the recipe is written beside the decoded copy, and the entry
   it prints is added by the same `apply`, with the request's origin.

The project then plays and renders with the network off, and nothing is
generated again at a render. Undo takes the sample out of the song; the files
stay, in the project and in the generated folder.

**Which format is asked for.** `daw keys status` and the first `generate`
read the plan's tier and keep it in `generate.json` in the data folder
(`~/Library/Application Support/AAW/`, or `AAW_DATA_DIR`), which holds the
tier, the budget, the day's spend by origin and whether the folder was
registered, and never the key. The sound is asked for as `pcm_44100` on Pro,
Scale, Business and Enterprise, `mp3_44100_192` on Creator and
`mp3_44100_128` otherwise, which is also what an unknown tier gets. A song is
asked for as `pcm_48000` on Pro and above and `mp3_48000_192` otherwise, the
format the service picks for its v2 models when asked for none; below those
is `mp3_44100_128`. A refusal
whose code is not the plan's (`subscription_required`, `missing_permissions`,
`invalid_api_key`, `quota_exceeded`, `insufficient_credits`) and that names the
format, in its message (`output_format` or the format's name) or its code, is
asked for again one format lower, once a step, and the tier is corrected
downward (`creator` after PCM failed, `starter` after 192 kb/s); a song steps
from `pcm_48000` to `mp3_48000_192` to `mp3_44100_128`. A 403 that
says nothing of why may be the format and may be anything else, so it is
asked for again one format lower once in a request, and the tier is left as
it was. A 429 is retried once after `retry-after`, capped at 30 s. Nothing
else is retried, so a request is paid for at most once.

**A song's reply** is `multipart/mixed`: a JSON part with `composition_plan`
(the plan used) and `song_metadata` (`title`, `description`, `genres`,
`languages`, `is_explicit`), then the audio. It is split on the boundary its
content type names, or else on its first line, as the package does; each
delimiter counts only at the top or after a line's end, the line end before a
delimiter is the delimiter's, and a part's headers end at a blank line of
`\r\n` or `\n`. A reply that is not multipart is taken as the audio alone,
with a note that it carried no plan. PCM has no header, and whether a song's
is stereo is not documented, so its channels are whichever of one or two
makes it as long as the plan the service used, or the length asked for; a
note says when neither is within a tenth, and with no length known it is
taken as stereo with a note.

## The recipe

`DATE-TIME-ID.generated.json` holds the service, endpoint, `kind` and
`model_id`; the `prompt` as given and the `text_sent`; every parameter
(`duration_seconds`, `prompt_influence`, `loop`, `output_format` for a sound;
`music_length_ms`, `force_instrumental`, `seed`, `store_for_inpainting` and
`output_format` for a song); `bars` and `bpm`; a song's `plan_sent`, `plan`
(the one the service used), `title`, `description`, `genres`, `languages` and
`is_explicit`; `song_id` (null for a sound); the `request_id`, read from `request-id` or
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
macOS asks again after a rebuild; "Always Allow" covers that build only. The
app's Settings › ElevenLabs writes the key the same way, through the bundle's
`daw keys set`, so the item names the `daw` that reads it for every request,
and the app never reads or holds the key itself.

**What never happens.** The key is never a flag or an environment variable,
never a file in the project, the workspace or the data folder, never a line
in a reply, a note or an error. `Key` has no `Display` and prints as `Key(…)`,
and the client prints the same. The tests grep every reply and every file for
the key.

**Tests never touch the Keychain.** `AAW_GENERATE_URL` points the commands at a
stand-in server; when it does, and only when it names a loopback address
(`127.0.0.1`, `localhost`, `::1`), the key is read from `AAW_TEST_GENERATE_KEY`
instead, and `daw keys set` refuses to write the Keychain. Any other address
but the service's own, `https://api.elevenlabs.io`, is refused before the
Keychain is read, so an agent that can set the variable cannot have the
person's key sent where it can read it; that includes this machine named in a
way the check does not take as loopback, such as `127.1` or `0.0.0.0`. The
client follows no redirect, since the key's header would go with it to any
host. On Linux the Keychain functions say they need a Mac.

## What the agent may spend

Each second of audio costs credits, and an agent can loop. So:

- **A daily budget**, in seconds of audio the agent may generate without
  asking: 120 unless `daw keys budget --agent-seconds N` set it, 0 to require
  asking every time. The day turns at midnight UTC.
- **What counts as the agent.** Requests with the agent's origin, the default.
  A request with `--origin user`, as the browser's Generate sends it, is the
  person's and does not count; it is still recorded in the day's spend.
- **Sounds and songs share it.** A song's seconds count as a sound's do; a
  plan from `daw generate plan` costs nothing and is not counted.
- **Before a request** the length asked for, or 30 s when the model picks a
  sound's and five minutes when it picks a song's, is held in the day's spend, under a lock on `generate.json`, so
  requests made at once each count what the others hold. **After,** the
  length of the audio that came back replaces the hold, whether or not it is
  then saved or imported: from the file, or for PCM not saved from its bytes,
  or the length asked for when neither says. A request refused or not
  reached gives the hold back.
- **When it is spent,** the command refuses with what was spent today, what
  was asked, and how the person raises the limit; nothing is sent. The topic
  tells the agent to tell the person and not work around it.
- **The service's own limits still apply.** A key can be given a credit quota
  on the ElevenLabs side, which is the hard limit.

## What leaves the machine

The description or the plan, its parameters and the model's name, and only
after the person has entered a key. No audio leaves and no project text is sent. A
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
| `bad_prompt`, `bad_composition_plan` | The prompt or the plan named something it may not, and the suggested prompt or plan as text. |
| A plan past a limit | Which chunk and which limit; nothing sent. |
| Budget spent | What was spent today, what was asked, and where the person raises the limit. |
| Unreachable | Nothing was sent and nothing was spent. |
| Generated but not imported | Where the file is in the generated folder, and why the import failed. |

The service's error bodies are read in the shapes its documentation and its
package show: `{detail: {status, message}}`, `{detail: "text"}`, `{detail:
[{msg}]}` and `{code, message}`. The suggestion is read from
`detail.data.prompt_suggestion`, where the music guide puts it, from
`detail.data.composition_plan_suggestion` for a plan, and from the
`suggested_prompt` the first reading named.

## How it is built

- `aaw-host/src/generate.rs`: the client on `ureq` 3 with rustls, blocking,
  with no async runtime, with HTTP statuses read rather than raised, the
  proxy left out for a loopback address and fifteen minutes for a reply;
  `Settings` and `generate.json`; `Format`; `SoundRequest` with the bars
  arithmetic, the checks, the body and the estimate; `post_audio`, the
  request with the retries and the formats stepped down, under `sound` and
  `music`; `MusicRequest`, `check_plan`, `plan_seconds`, `plan_sections` and
  `plan_from`; `multipart` and `pcm_channels`; `compose_plan`; `save`,
  `save_music`, the WAV header, `project_copy`, `project_recipe`, `list` and
  `default_id`.
- `aaw-host/src/keys.rs`: the Keychain through `security-framework` on macOS,
  a refusal elsewhere.
- `aaw-cli/src/generate.rs`: `daw generate` and `daw keys`, the budget held
  and settled, the import routed through `apply` after `samples import
  --copy-only` decodes an MP3 (`import`, shared by sounds and songs), the
  folder registered once (`scan`), a song's beat map from `samples beats`
  and its sections given bars (`beat_summary`), the key read with
  `rpassword` on a terminal.
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
  help and the topic, and the key in no output and no file. For songs, Rust
  tests of the request as documented, the multipart reply split with the plan,
  the title and the `song-id`, stereo told by its length, the 48 kHz WAV and
  its recipe, music's formats stepped down, a plan asked for, every limit of a
  plan and of the request, a refused prompt's and plan's suggestions, and the
  parser on CRLF, bare LF, no declared boundary and no multipart; Python tests
  of `daw generate plan --out`, the plan edited and passed to `generate music
  --plan` against a stand-in that returns a drum track at 100 BPM as
  multipart, the beat map's tempo and bars, each section on its bar, a
  section cut on its bars and rendered with no request; the estimate, the
  five-minute hold, the checks, the person's song as one undoable edit, a
  refused prompt, and an MP3 song decoded by import.
- The app: `aaw-ffi/src/library.rs`, `library_generated` over
  `generate::list` and `generate::folder_list`; `Generate.swift`, with
  `DawProcess` (the bundle's `daw` off the main thread, its standard input
  written and closed, stopped by `cancel`), `KeyStatus`,
  `ElevenLabsAccount`, `GenerateRequest` and `Generator`;
  `GenerateView.swift`; the pane in `SettingsWindow.swift`, now an
  `NSTabViewController` of toolbar panes. Swift tests in
  `GenerateTests.swift` run a shell script standing in for `daw`.

## In the app

**Settings › ElevenLabs.** AAW › Settings… (⌘,) has panes chosen in its
toolbar, Audio and ElevenLabs, and opens on the one shown last. The
ElevenLabs pane says whether a key is kept, from `daw keys status` asked each
time it shows; a secure field and **Save**, which writes what was pasted to
the standard input of the bundle's `daw keys set elevenlabs` and clears the
field, so the key is never an argument and the Keychain item is made by the
`daw` that uses it; **Remove…**, after a question, as `daw keys remove`; the
plan, its credits left of how many and when they renew; and the agent's
daily budget as a menu (none, 30 s, one, two, five, ten, thirty or sixty
minutes, and one set from the command line), set as `daw keys budget
--agent-seconds`, with what the agent and the person made today. A key the
service refuses is kept and the refusal said. A link opens ElevenLabs' page
for making a key.

**The browser's Generate.** A section of the browser after Samples: Sound,
Loop or Song; a description; a length, a sound's seconds or Auto, a loop's
bars at the song's tempo and meter, a song's seconds (30 unless changed,
3 to 300) with Instrumental; and **Generate** (⌘↩). What the command would
refuse is said before anything is sent. The app runs `daw --origin user
generate sound|music SONG TEXT …` as a process off the main thread, so the
request is the person's and does not count against the agent's budget, and
the song keeps playing; the time so far is shown beside **Cancel**, which
stops the process: nothing is imported, and what the service made is paid
for and lost. A song's window closing stops its request the same way. After,
the line under the form says the sample it added to the song, or the
command's refusal, a refused prompt's suggestion included, and the reply's
notes. With no key, the form is a line and **Open Settings…**, which opens
Settings at ElevenLabs.

Below the form, what this song has generated, then what else the generated
folder holds, newest first, read from the recipes by `library_generated` in
the Rust core: a song by its title, anything else by its description, with
its kind, length and when. A result plays on a click, as a sample does; + or
a double-click adds it where the samples' + does; and it is dragged onto the
timeline or a header as a sample is. The file played and dragged is the
generated folder's while it is there, which an import names as the song's
copy was named, so adding it again copies nothing; else the song's copy. Its
menu has **Generate Again**, which fills the form with the recipe's request
and sends it, and is off for a song made from a plan, and **Show Recipe in
Finder**. The search field narrows the list by description, title and ID.

## Not built yet

Each is a backlog line when it is picked up, and this file is rewritten then.

### Later, each its own backlog line

- **Regenerating one section** of a generated song: a plan that keeps the
  stored song's other sections as chunks of it, and replaces one. The plan's
  check and the `song-id` in the recipe are there; a command that builds the
  plan from the recipe is not.
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

Stage 2, by scripted runs against the stand-in; nothing yet by a person:

- [x] `daw generate music` gives a song whose beat map is in the reply, and a
  section of it cut into an audio clip on its bars plays and renders.
- [x] A plan from `daw generate plan`, edited and passed to `--plan`, is sent
  as edited, and the reply's sections are its chunks on their bars.
- [x] A plan past a limit, a prompt with a plan, and the budget without a
  length are each refused with nothing sent.
- [ ] A song made with a real key and heard: whether the beat map finds its
  downbeat, whether the sections land on the bars the plan's lengths imply,
  and whether a chunk's length in bars at the prompt's tempo comes back on
  the bar. The headers, the channels and the cost seen are written here.

Stage 3, by Swift tests and scripted pictures against a stand-in on this
Mac; nothing yet by a person:

- [x] The ElevenLabs pane shows the key kept, the plan and its credits, and
  the budget, from `daw keys status`; saving pipes the key to `daw keys set`
  and never makes it an argument (a test reads what a stand-in `daw` was
  given).
- [x] Generate makes a sound in the song as one edit through the host while
  the window stays live, the time shown; Cancel stops the process and
  nothing is added; a refused prompt says why with the suggestion.
- [x] The song's results then the generated folder's are listed, and a click
  plays one.
- [ ] A person saves a real key in Settings, generates a sound and a song
  while the song plays, hears them, drags one onto the timeline and a MIDI
  track's header, and cancels one. Whether macOS asks for the Keychain at
  Save, at the first request, or after each build is written here.

## Open questions

1. **Who reads the Keychain.** `daw` reads it, which works with the app
   closed, and the app's Settings writes it through `daw keys set`, so the
   item's access list names the bundle's `daw` and the app never needs it
   (D161), in place of the access list naming both that the design planned.
   What macOS asks on a signed and an unsigned build is still to be seen.
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
7. **The generated folder and the terms.** It is the person's own material.
   Whether anything that copies a workspace (a backup, a move to another Mac)
   needs to say so.
8. **Which music model is the default.** `music_v2_5`, the newest and the
   one the music guide's examples use (D160); `--model music_v1` remains.
9. **A song's PCM channels.** Undocumented; told by length. The first real
   song says, and the rule can then be fixed.
10. **Five minutes or ten.** The reference allows 600,000 ms, the overview
    and the pricing page say five minutes. Five until a request shows more.
11. **What a song costs in credits,** and whether `character-cost` is sent
    on music. Only dollars are documented; `cost_credits` records what the
    reply says.
12. **Whether storing for inpainting costs anything** or keeps the song
    longer than the person wants. Not documented; it is always asked for so
    a section can be made again.
13. **Music's MP3 on Starter.** Whether `mp3_48000_192` is allowed below
    Creator, as the service's own default for v2 suggests; a refusal steps
    down to 128 kb/s.
