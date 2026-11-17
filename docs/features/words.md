# Words by bar — implemented November 16, 2026

Item 4 of the reverse-engineer track (D182, D187): the words a voice sings,
each with its time, bar and beat on the song's beat map, as lines, with the
lines that come back and the hook. `daw transcribe --words` does it. The form
of `daw samples form` uses what it heard to name the chorus. OpenAI's Whisper
hears the words (Radford, Kim, Xu, Brockman, McLeavey and Sutskever, "Robust
Speech Recognition via Large-Scale Weak Supervision", ICML 2023; MIT, code and
weights), through Apple's MLX on the Mac's GPU. No audio leaves the Mac.

## Command

```sh
uv run daw transcribe projects/NAME/samples/ID_song.wav --words
uv run daw transcribe SONG.wav --words --lyrics "projects/NAME/exports/SONG (lyrics).txt"
uv run daw transcribe SONG.wav --words --near 1:12 --window 8
```

`SOURCE` is what `daw transcribe` takes: a file or library sample, or with
`--project` one of the song's samples or an audio clip, `tracks.T.audio.N`.
Given a song that `daw separate` has taken apart, it reads the vocals part
beside it, found by its recipe as `daw samples form` finds the parts. Given a
part, it reads the part and places it on its source song's beat map. A file
with neither is read whole, and the reply warns that a voice part is heard
better.

- `--language` is the language sung, `en` unless given; `''` lets Whisper
  tell from the first thirty seconds, which a silent intro can mislead.
- `--lyrics OUT.txt` writes the lines as text, a line each with its time and
  bar, marked where a line was read again.
- `--once` reads once and does not read the doubtful stretches again.
- `--again` hears the voice again instead of using the words kept beside it.
- `--near TIME --window S` lists only the lines around a time.
- `--beats FILE` places the words on another file's beat map.

Notes' flags (`--line`, `--range`, `--min-confidence`, `--text`, `--midi`,
`--grid`, `--quantize`, `--track`, `--at`, `--drums`) are refused with it.
With an audio clip, only the words the clip plays are listed, each line with
`song_beat`, where the clip plays it.

## Setup

`daw separate setup` installs mlx-whisper 0.4.3 into the separation
environment with its dependencies, which add 14 packages and replace none.
It downloads two models into the models folder, pinned to a revision and
checked against the manifest's hashes:

| Model | Files | Reads |
|---|---|---|
| `whisper-large-v3-turbo` | config, 1.6 GB of weights (`mlx-community/whisper-large-v3-turbo`) | the whole voice |
| `whisper-large-v3` | config, 3.1 GB of weights (`mlx-community/whisper-large-v3-mlx`) | the doubtful stretches again |

A file the Hugging Face cache already holds with the manifest's hash
(`HF_HUB_CACHE`, or `HF_HOME/hub`, or `~/.cache/huggingface/hub`) is
hard-linked into the models folder, or copied where a link cannot be made,
instead of downloaded again. On a Mac where mlx-whisper ran before, setup costs
no disk. `daw separate models` lists both under `analysis`. A command that
lacks the package or the first model says to run `daw separate setup`.
Without the second model the doubtful stretches are not read again, and the
reply says which model to get.

## How the words are made

1. **The voice at 16 kHz.** The command decodes the voice to 16 kHz mono and
   hands it to `words_runner.py` as a NumPy file, so nothing needs ffmpeg.
2. **One reading of the whole voice** by large-v3-turbo with the options the
   clean-edit sessions settled on in November 2026. Each 30-second window is
   read on its own (`condition_on_previous_text` off), which stopped the model
   looping a hook it had heard. A silence of two seconds or more around a word
   it is unsure of is skipped (`hallucination_silence_threshold` 2), where it
   would invent one. Where a window loops, Whisper reads it again, sampling at
   a higher temperature. The runner seeds MLX's random numbers before each
   reading, so the same voice gives the same words: unseeded, two readings of
   a rap record's hook differed.
3. **What is in doubt.**
   - A stretch of two seconds or more with no words in which the voice part
     is loud for a second or more: within 20 dB of the part's loud frames, its
     99th percentile over 50 ms frames, so a voice that sings a twentieth of
     the song still sets it. Turbo skipped 18 s of loud vocal in one record's
     intro and 9 s of another's second verse. On a whole mix the band is loud
     where no one sings, so only the next kind is looked for.
   - A line whose words' mean probability is under 0.65.
   - Whisper can stretch a word's end over the stretch after it that it did
     not read: one word ran from 80.3 s to 92.3 s, over 11 s of voice. A word
     is taken to end at most 2 s after its start, so that stretch is still
     found.
   - Stretches that touch are one.
4. **Read again on short clips.** Each stretch in doubt, of a skip its loud
   part, is cut into pieces short enough that each, padded 1.5 s either side,
   is at most 10 s, and widened about its middle to at least 6 s. Large-v3
   reads each clip on its own; it is never run over a whole song, where it
   writes lines that are not there. Each clip gives only the words in its own
   piece, so where clips overlap a word is heard once. Whisper's own
   `clip_timestamps` were tried first and read on past a clip's end where
   clips overlapped, so the runner cuts each clip itself.
5. **The surer reading is kept.** In each stretch the second reading replaces
   the first where the first heard no words or the second is surer on average.
   `read_again` says, for each stretch, why it was read again, its clips,
   which reading was kept, both readings' words and their mean probabilities.
6. **Lines.** A new line starts where Whisper starts a segment, after a pause
   of 0.5 s, after a word that ends a sentence, and after a comma once the
   line is 2.5 s long. Whisper sometimes leaves a whole verse unpunctuated in
   one segment, so a line still over 6 s is split where both parts are at
   least 2 s long: at its longest pause there, or, the pauses alike, nearest
   its middle.
7. **On the beat map.** Each line and word is placed by its nearest sixteenth,
   so a word sung a few milliseconds before a bar line is in that bar.
8. **What comes back.** Lines of three words or more whose words are 0.8
   alike, compared lower case without apostrophes ("it's" and "its" are
   one), are one line. `repeats` lists those sung twice or more,
   most often first, each with its commonest wording, count, bars and times.
   `hook` is the first.

## The reply

`path` is the voice read and `song` the song it is placed on, with
`voice_part`. `models` gives each reading's model, version and seconds.
`language`, `words_file` and `kept` say whether the words came from the file
kept beside the voice. Then the beat map's `tempo_bpm`, `meter` and
`first_downbeat_seconds`, followed by `word_count`, `sure` (probability 0.5 or
more), `line_count`, `read_again`, `hook` and `repeats`.

Each line in `lines` has `text`, `seconds`, `until_seconds`, `bar`, `beat`,
`sixteenth`, `off_grid_ms`, `bars` (first and last), `probability` (the mean
of its words'), `read_again` when a word in it came from the second reading,
and `words`, each with the same places. `warnings` always says that Whisper
writes down the lead voice. It also says when a mix was read, when stretches
were in doubt and not read again, and where the voice is loud and no words
were heard even on a second reading, with the times to listen at.

## What is kept

What the models heard is kept beside the voice as `NAME.words.json`: the
voice's hash, the language, each model's version and time, the first
reading's segments, the stretches in doubt with their clips, and the second
reading's segments. The lines, bars and repeats are made from it again on
each call, so a corrected beat map moves them. The file is read again when
the voice changes, when the language asked for differs, when `VERSION` in
`words.py` moves (what is read again changed), with `--again`, and when the
second model was missing and is now here. Nothing is written beside a file in
the library index, as for beat maps.

## Naming the form

`daw samples form` reads the words kept beside the voice part, when
`daw transcribe --words` has heard them; it never runs Whisper itself. See
[form.md](form.md#with-the-words).

## Tried

On November 16, 2026, on this Mac, with the models linked from the Hugging
Face cache:

- **A rap record** (2:56; the Mel-RoFormer vocals part). It took 51 s,
  of which 30 s was the first reading and 19 s the second. 543 words in 110
  lines, 519 of them sure. The lines, in their bars, follow the transcript
  made by hand with mlx-whisper for the clean edit on November 8, 2026, which
  nobody checked by ear either. Five unsure lines were read again, and in one a
  misheard word was put right. The hook, a three-word line, came 16 times. Before the seed, one run skipped 9 s of the second verse, which was
  found and read again. Another hid 11 s behind a word stretched over it,
  which the 2-second limit on a word now finds.
- **A pop record** (3:03; the fast vocals part). It took 52 s. 263
  words, 54 lines, the chorus's title line the hook, 10 times. Seven
  stretches were read again. The second reading was kept in three, two of
  them unsure lines. In four loud stretches neither reading found words:
  backing vocals, which the warnings list with their times.

Nobody has listened along to either.

## Limits

- Whisper writes down the lead voice. Backing vocals, doubles and ad-libs under
  it are mostly missing, as the clean-edit sessions found. No installed model
  splits lead from backing vocals.
- Word times say where to look within about 150 ms. Placing a cut on a word
  needs the stem's level, as the clean-edit skill does.
- A hook Whisper hears differently each time comes round as different lines.
  The repeats join wordings 0.8 alike, and a garbled one stays apart.
