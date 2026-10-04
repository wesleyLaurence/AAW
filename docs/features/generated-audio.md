# Generated audio — proposed October 2, 2026

Status: proposed, not built. Backlog: Next, Generated audio. What was checked about
the ElevenLabs API is from its documentation on October 2, 2026; no request was
made with a key.

## What

A generator in the DAW: a sound, a loop or a whole song made from a description
by the ElevenLabs API, saved in the project as a sample. The person opens it as
a panel in the app; the agent runs it as a `daw` command. What comes back is
sliced, placed, layered and mixed like any other sample. The person's own
ElevenLabs key unlocks it, kept in the macOS Keychain where the agent uses it
and does not read it.

## Why

A sound the library does not have is a dead end today: the agent picks the
nearest sample, or writes a script that synthesizes one. A generator fills that
gap from a sentence.

It is a tool in the kit and never the product. A prompt that returns a finished
song is the thing this app is not ([concept.md](../concept.md)); a generated
riser under a drop, or a generated song cut into four bars and replayed on
pads, is material the person and the agent still have to make something of.

## Stages

1. **`daw generate sound` and the key.** A one-shot or a loop, from the command
   line, with the key in the Keychain.
2. **`daw generate music`.** A whole song to sample.
3. **The panel in the app,** and the key's field in a Settings window.

Each is a backlog line and a pull request of its own when it is picked up.

## What ElevenLabs gives

From its API reference ([sound effects](https://elevenlabs.io/docs/api-reference/text-to-sound-effects/convert),
[music](https://elevenlabs.io/docs/api-reference/music/compose)) and its
overview pages:

- **Sound effects** are `POST /v1/sound-generation` with the key in an
  `xi-api-key` header. It takes `text`, `duration_seconds` from 0.5 to 30 (it
  chooses a length when none is given), `prompt_influence` from 0 to 1 (0.3 by
  default; higher is more literal), `loop` for a sound with no audible start or
  end, and `output_format`. The model is `eleven_text_to_sound_v2`. It costs 40
  credits a second when a length is given.
- **Music** is `POST /v1/music`. It takes a `prompt` or a `composition_plan`
  of sections, `music_length_ms`, `force_instrumental`, a `seed` that helps
  consistency and does not promise the same audio, `model_id` (`music_v1`,
  `music_v2`, `music_v2_5`) and `output_format`. It is for paid plans only. A
  prompt that names copyrighted material is refused as `bad_prompt`, with a
  prompt suggested in its place.
- **The file** is MP3 at 44.1 kHz and 128 kbps by default. PCM at 44.1 kHz
  needs the Pro tier.
- **Also there, and not used here:** a composition plan made from a prompt,
  regenerating one section of a song, and stem separation.

## Design

**One command, two kinds.**

```
daw generate sound PROJECT "vinyl crackle, warm" [--seconds 4] [--loop]
                   [--influence 0.3] [--id crackle]
daw generate music PROJECT "dusty soul ballad, 82 bpm" [--seconds 60]
                   [--instrumental] [--seed 7] [--id soul]
```

It makes the request, saves what comes back, imports it and replies with the
sample's ID, path and length. It places nothing: an audio clip, a pad or a
slice is made with the commands that exist. It is written in Rust, in `daw`,
so the app's panel needs no Python, and it hands the file to `samples import`
until that is in Rust too (An app that stands alone, under Shipping).

**The file is in the project.** The bytes that came back are kept under
`generated/` in the project and imported from there as a file is today: a WAV
is copied, an MP3 is decoded once to WAV, and the copy is named by its hash.
PCM is asked for where the plan allows it, and MP3 otherwise. The project then
plays and renders with the network off, and nothing is ever generated again at
a render.

**The recipe is beside it.** `NAME.generated.json` holds the service, the
endpoint, the model, the prompt, every parameter and the date. It records the
sound; it cannot remake it, since the same request returns different audio. A
project lists the outside services it used by reading these.

**It is an edit like any other.** With a host running, the import is one
undoable step with the origin of whoever asked, and the activity panel shows
the prompt and the seconds generated. Undo takes the sample out of the song
and leaves the file.

**The key.** One Keychain item, a generic password for the service `AAW` and
the account `elevenlabs`.

- It is entered in the app's Settings window, which this adds, or with `daw
  keys set elevenlabs`, which asks on the terminal without showing what is
  typed. `daw keys status` says whether one is there. No command prints it.
- `daw` reads it from the Keychain for the one request. It is never a flag, an
  environment variable, a file in the project or the workspace, a line in a
  log, or part of an error.
- The item's access list names the app and its bundled `daw`. Any other
  program that asks for it, the `security` tool included, brings up a macOS
  prompt the person answers. So an agent that runs commands as the person can
  use the key through `daw` and cannot read it without the person seeing.

**What leaves the machine** is the prompt and its parameters, and only when the
person has entered a key. No audio and no project text is sent. With no key,
the command fails with where to enter one, and the panel shows the field.

**Text that comes back is data.** A suggested prompt, an error and a file's
name are shown and never followed.

**The panel in the app.** A Generate tab beside the samples in the browser: a
description, the kind (Sound, Loop, Song), a length, and Generate. Below it,
what this project has generated, newest first, each with its prompt, auditioned
and dragged to the timeline or a header as a sample is. A request runs off the
main thread with its progress shown and can be cancelled; the song keeps
playing.

**Tests never call the service.** The address is a setting, and tests point it
at a stand-in on this machine that returns generated audio.

## Depends on

- Signing with a Developer ID (Other Macs, under Shipping). The Keychain's access list is
  tied to the code signature, so on a build signed ad hoc it is expected to ask
  again after each rebuild.
- The concept. It allows a generative model "as an instrument for texture,
  never as the product". A whole song to sample is wider than texture, so
  picking this up changes that sentence and the list of inputs, with an entry
  in [decisions.md](../decisions.md) that revises D11.
- Sample import in Rust (An app that stands alone, under Shipping), for the app's panel on a Mac without
  Python.

## Done when

- `daw generate sound` with a description puts a sample in the project that a
  pad or an audio clip plays, with its recipe beside it.
- `--loop` gives a sound that repeats as an audio clip without a click at the
  join.
- `daw generate music` gives a song that `daw samples beats` maps and that is
  cut into audio clips on its downbeats.
- With no key, the command and the panel say where to enter one. With a wrong
  key, a spent quota or a refused prompt, each says which.
- The key is in no file, no command's output and no log, and reading it with
  another program asks the person.
- The project plays and renders with the network off.
- The panel generates, auditions and drags a result onto the timeline.
- An agent asked for a song with a sound the library lacks generates it and
  uses it.
- A person has done each of the above by hand and heard the results.

## Open questions

1. **The right to use what is generated.** ElevenLabs says its music is
   "cleared for nearly all commercial uses" and that what is allowed differs by
   plan. Its terms were not read. What the recipe records of the plan, and
   whether sampling a generated song is covered.
2. **Who reads the Keychain.** `daw` itself, which works with the app closed,
   or only the app, with `daw` asking it, which keeps the key out of any
   process the agent starts. A spike on a signed and an unsigned build should
   show what each prompts for.
3. **A key for this checkout.** Whether `ELEVENLABS_API_KEY` is read at all
   while developing. An agent can read an environment variable.
4. **Where the panel sits.** A tab in the browser, as drawn here, since a result
   can go to any track; or a device on a track, as an instrument is.
5. **What the agent may spend.** Each second costs credits. A limit on the
   seconds an agent generates in a day without asking, set beside the key.
6. **The recipe as a file or in the song.** The schema rejects unknown fields,
   so a `recipe` on a sample is a schema change. A file beside the audio is
   not, and is not seen by `daw inspect`.
7. **The plan a file format needs.** The reference gives PCM at 44.1 kHz to
   the Pro tier; the overview speaks of 48 kHz WAV for sounds that do not
   loop. What a lower plan gets when it asks.
8. **The longest song.** The reference allows 600,000 ms and the overview says
   five minutes.
9. **A composition plan the agent writes,** section by section, in place of one
   prompt, and regenerating a single section.
10. **Another service.** The command is named for what it does. Whether a
    second service is a `--service` or a new command.
