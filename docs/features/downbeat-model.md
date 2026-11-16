# The downbeat model — implemented November 16, 2026

The second half of Next item 7 of the backlog, why the agent missed the
downbeat (D181). `daw samples beats` scored one downbeat for the whole song from
what tends to happen on one (a kick or bass note, a change of chord or of sound),
so a song with a bar of three, an edit off the downbeat or a riff on one chord
came out a beat or two off, and stayed off. Its beats also slipped on songs
played without a click. Beat This! (Foscarin, Schlüter and Widmer, "Beat this!
Accurate beat tracking without DBN postprocessing", ISMIR 2024; code and weights
MIT) is a model trained on thousands of annotated songs to hear beats and
downbeats; it does not assume a bar's length. Where it is installed it now leads
the beat map, and the onset measurement backs it. It runs on the Mac; no audio
leaves it.

## Setup

`daw separate setup` installs it with the stem models: the packages `beat-this`
1.1.0 and `torchaudio` 2.11.1 in the separation environment, and its weights,
`beat_this-final0.ckpt`, 81 MB, under `models/`. An environment made before
this is given the two packages; nothing else changes in it.
`daw separate setup --models beat-this-final0` gets the weights alone. The
manifest lists the authors' address and a Hugging Face mirror, since the
authors' server refused downloads under its brute-force guard the day this was
built; whichever answers, only a file of the official size and sha256 is kept.
`daw separate models` lists it under `analysis`. Without it, `daw samples beats`
measures as before and says why in `model.note`.

## How a map is made

1. **The model hears the song.** `downbeat_runner.py` runs in the separation
   environment, on the GPU where there is one (about 3 s for a four-minute song,
   start-up and all), and prints, 50 times a second, the model's odds of a beat
   and of a downbeat. Everything after is `downbeats.py`, so it is tested with
   odds made up (`tests/stand_in_downbeats.py`).
2. **Its beats and downbeats** are the frames likelier than not and likeliest
   within 60 ms, its own reading. Of two beats closer than half the usual beat,
   the less likely is dropped; a gap of two or more beats, a break it passed
   over, is filled evenly.
3. **The tempo counted.** The model's, doubled when it is under 70 BPM, or under
   90 and what starts between its beats starts nearly as strongly as what starts
   on them (the onset envelope's median between them at least 0.8 of its median
   on them): trap counted at 70 is 140, a slow groove counted at 82 stays 82.
   `--bpm` takes half, the same or double the model's, whichever is within 4%;
   a tempo none of those is near leaves the beats to the onsets.
4. **The beats.** The onset grid is measured at that tempo, as before. When
   80% of its beats and the model's are within 40 ms of one another, the grid's
   are kept, since they sit to a fraction of a millisecond; when not, the
   model's are taken, each moved to the sharpest attack within 30 ms of it.
5. **The bars.** A path through the beats takes a downbeat where the model's
   odds say so and keeps the bars one length, the length the model most often
   hears (or `--meter`); a bar of another length costs about two bars' worth of
   the model's word. So a stray downbeat the model hears mid-bar, as in a
   sparse intro, is passed over, and a change that holds for a couple of bars is
   followed, with the odd bar listed. A doubled count's bars are cut in two: the
   model's bar of four at 70 is two bars of four at 140.
6. **Corrections hold.** `--downbeat TIME`, given once or more, makes the beat
   nearest each time a downbeat, and bars of the map's length run from it to the
   next one given, whatever the model hears there; before the first, the
   model's bars. Kept with the map until `--refresh`.

## Report

The map gains:

- `model`: `used`, or a `note` on why not; `heard_bpm`; `counted` (as heard,
  doubled, halved, at --bpm); `beats_from` (onsets or model) with `agreement`,
  the share of beats the two had in common; `device` and `seconds`.
- `odd_bars`: each bar of another length than `beats_per_bar`, with its bar,
  seconds and beats. The bars after one are counted from it.
- `downbeat.uncertain_bars`: the bars whose downbeat the model gives under even
  odds, and `downbeat.confidence`, the median of the model's odds at the bars'
  downbeats. `set_by` is model, measurement or request; `candidates` is in a map
  made without the model only.
- `meter_set_by`: model, request or assumed (4/4 without the model).
- `index` on each beat, its count from the first downbeat. `hits`, the
  reference's sections and the picture rows use it, so a bar of three moves
  what comes after it rather than leaving it a beat off.
- `tempo.ambiguous_with` adds the onsets' own first choice where it is another.

Every saved map is measured again once, since the measuring code changed, and a
map made without the model is measured again once the model is installed.

## What was checked

- **Generated songs** with a stand-in model: the bars where it hears them and
  the beats to 2 ms; a bar of three listed and the bars after it counted from it;
  one stray downbeat passed over, and two downbeats a bar over four bars kept to
  the beats heard most; a 140 song counted at 70 doubled with bars of four, and
  `--bpm 70` taking the model's count; an 82 song with quiet eighths not doubled;
  beats placed ±20 ms moved to their attacks within 2 ms; corrections holding to
  the next; a failing model leaving the map as before; `hits` and the reference
  counting bars past a bar of three.
- **Eight real songs** with the real model, in a throwaway environment, against
  the earlier maps:
  - A slow hip-hop song and a 154 trap song kept their maps, the trap song
    counted at 77 and doubled.
  - A 140 trap song was doubled, with its first downbeat a 140 beat earlier than
    the old map put it. Not yet heard: one of the two is a beat off.
  - A hip-hop song the old map had at 111 was counted at 83 and doubled to 166;
    its in-between beats start as strongly as its beats. Not heard; `--bpm 83`
    if 166 is wrong.
  - A live rock song without a click had beats from the model (only 23% shared
    with the onset grid, whose tempo drifted off it) and no odd bars: the
    model's two-beat bars in the intro were passed over.
  - The live rock song with the riff intro: on its drum stem, a bar of three at
    bar 17 (0:29), where its kicks move from beats 1 and 3 to 2 and 4 by
    `daw samples hits`. On the full mix the model put the bars a beat apart from
    the stem's in the intro and found a bar of three at bar 49 instead. The two
    disagree, and neither has been heard.
  - A rap song with a beat switch had beats from the model and four odd bars
    and five uncertain bars in its first minute.

Nobody has heard any of it, and no agent session has used it on a song.
