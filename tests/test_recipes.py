"""Every recipe `daw describe recipes` lists runs as written, on a song made
here from generated audio, with its capitals filled from the replies as its
steps say."""

import json
import os
import re
import shlex
import subprocess

import numpy as np
import pytest
import soundfile as sf

from helpers import SR, daw

# A word of two or more capitals in a command: a placeholder. A note's name,
# A#1 or C4, is not one.
CAPITALS = re.compile(r"\b[A-Z]{2,}(?:_[A-Z]+)*\b")


def run(*args, cwd):
    return subprocess.run([os.environ["AAW_DAW"], *map(str, args)], capture_output=True, text=True, cwd=cwd)


def kick(seconds=0.25):
    t = np.arange(int(SR * seconds)) / SR
    return 0.8 * np.sin(2 * np.pi * (50 + 120 * np.exp(-30 * t)) * t) * np.exp(-8 * t)


def noise(seconds, decay, seed):
    n = int(SR * seconds)
    return 0.4 * np.random.default_rng(seed).standard_normal(n) * np.exp(-decay * np.arange(n) / SR)


def library(tmp_path, monkeypatch):
    """A sample library of a kick, a snare and a hat."""
    folder = tmp_path / "library"
    folder.mkdir()
    for name, sound in [("kick", kick()), ("snare", noise(0.25, 20, 1)), ("hat", noise(0.1, 60, 2))]:
        sf.write(folder / f"{name}.wav", sound, SR)
    monkeypatch.setenv("AAW_LIBRARY", str(tmp_path / "library.sqlite"))
    assert daw("samples", "scan", folder)["indexed"] == 3
    return folder


def steps(name, work, found, learn=lambda args, reply, found: None):
    """Run a recipe's steps in work, filling its capitals from found and
    from what learn takes from each reply. The last reply."""
    recipe = daw("describe", f"recipes.{name}")
    if "batch" in recipe:
        (work / recipe["batch"]["file"]).write_text(json.dumps(recipe["batch"]["content"]))
    reply = None
    for step in recipe["steps"]:
        args = shlex.split(step["run"])
        assert args[0] == "daw", step
        left = [w for a in args for w in CAPITALS.findall(a) if w not in found]
        assert not left, (step["run"], left)
        args = [CAPITALS.sub(lambda m: str(found[m[0]]), a) for a in args[1:]]
        out = run(*args, cwd=work)
        assert out.returncode == 0, (step["run"], out.stderr)
        reply = json.loads(out.stdout)
        learn(args, reply, found)
    return reply


def start(work, tmp_path, monkeypatch):
    """The song `daw describe start` makes, run as written."""
    folder = library(tmp_path, monkeypatch)

    def learn(args, reply, found):
        if args[:2] == ["samples", "search"]:
            found[f"{args[2].upper()}_PATH"] = reply[0]["path"]

    reply = steps("start", work, {}, learn)
    assert reply["kind"] == "listen"
    assert [t["id"] for t in daw("inspect", work / "song")["tracks"]] == ["drums", "bass", "keys"]
    assert (folder / "kick.wav").exists()


def given_start(work, tmp_path, monkeypatch):
    start(work, tmp_path, monkeypatch)
    return {}


def given_drums(work, tmp_path, monkeypatch):
    folder = library(tmp_path, monkeypatch)
    song = work / "song"
    daw("init", song, "--tempo", 96)
    for name, row in [("kick", "x... .... x.x. ...."), ("snare", ".... x... .... x..."), ("hat", "x.x. x.x. x.x. x.x.")]:
        daw("samples", "import", folder / f"{name}.wav", "--project", song, "--id", name)
        daw("track", "add", song, name)
        daw("pad", "add", song, name, name, "--sample", name)
        daw("pattern", "add", song, name, "--length-beats", 4)
        daw("pattern", "steps", song, name, name, row)
        daw("clip", "add", song, name, name, "--repeats", 8)
    return {}


def given_empty(work, tmp_path, monkeypatch):
    daw("init", work / "song", "--tempo", 120)
    return {}


def given_markers(work, tmp_path, monkeypatch):
    start(work, tmp_path, monkeypatch)
    song = work / "song"
    daw("--origin", "user", "marker", "add", song, 16, "--text", "bass too loud here")
    daw("--origin", "user", "marker", "add", song, 24, "--text", "love this")
    return {}


def given_finished(work, tmp_path, monkeypatch):
    """A finished song of sixteen bars at 120 BPM: a kick on every beat, and
    a tone that changes every four bars."""
    beat = 0.5
    total = np.zeros(int((0.5 + 64 * beat + 1) * SR))
    hit = kick()
    for n in range(64):
        at = int((0.5 + n * beat) * SR)
        total[at : at + hit.size] += hit
    t = np.arange(total.size) / SR
    for part, hz in enumerate([220, 262, 196, 247]):
        a, b = (int((0.5 + (part + i) * 16 * beat) * SR) for i in (0, 1))
        total[a:b] += 0.15 * np.sin(2 * np.pi * hz * t[a:b])
    path = tmp_path / "finished.wav"
    sf.write(path, total, SR)
    daw("init", work / "song")
    return {"SONG_PATH": path}


def learn_mix(args, reply, found):
    if args[:2] == ["render", "song"] and "BEFORE" not in found:
        found["BEFORE"] = reply["directory"]
    if args[0] == "listen" and len(args) == 2:
        pair = reply["overlap"]["pairs"][0]
        found["STEM_A"], found["STEM_B"] = pair["a"], pair["b"]
        track, peaks = next((t, v["audio"]["resonances"]) for t, v in reply["tracks"].items() if v["audio"]["resonances"])
        found["TRACK"], found["PEAK_HZ"] = track, peaks[0]["freq_hz"]
        found["PEAK_GAIN"] = max(-24, -peaks[0]["prominence_db"])
        found["PEAK_Q"] = min(18, peaks[0]["q"])


def learn_edit(args, reply, found):
    if args[:2] == ["samples", "import"]:
        found["SAMPLE_PATH"] = reply["path"]
    if args[:2] == ["samples", "beats"]:
        found["TEMPO"] = reply["tempo"]["bpm"]
        found["DOWNBEAT"] = reply["first_downbeat_seconds"]
        found["RATE"] = reply["sample_rate"]


# Each recipe's song, and what it learns from the replies.
GIVEN = {
    "drum-bus": (given_drums, None),
    "phrase": (given_empty, None),
    "mix-pass": (given_start, learn_mix),
    "markers": (given_markers, None),
    "finished-edit": (given_finished, learn_edit),
}


def test_every_recipe_is_run_here():
    listed = daw("describe", "recipes")["recipes"]
    assert list(listed) == ["start", *GIVEN]
    assert daw("describe", "recipes.start") == daw("describe", "start")
    # Each says what it starts from.
    assert all(daw("describe", f"recipes.{name}")["given"] for name in listed)


def test_the_first_song_recipe_runs_as_written(tmp_path, monkeypatch):
    # The recipe says to run each step as it is decided, with the app open.
    assert "run each step as soon as it is decided" in daw("describe", "start")["semantics"]["pace"]
    work = tmp_path / "work"
    work.mkdir()
    start(work, tmp_path, monkeypatch)


@pytest.mark.parametrize("name", GIVEN)
def test_a_recipe_runs_as_written(name, tmp_path, monkeypatch):
    work = tmp_path / "work"
    work.mkdir()
    make, learn = GIVEN[name]
    found = make(work, tmp_path, monkeypatch)
    reply = steps(name, work, found, learn or (lambda *_: None))
    song = work / "song"
    if name == "drum-bus":
        assert reply["effects"]["tracks"]["drums"][0]["max_gain_reduction_db"] > 0
        assert daw("get", song, "groups.drums.sends")
    if name == "phrase":
        assert reply["map"][1:3] == ["lead  AABBAABB", "bass  CCCCCCCC"]
    if name == "mix-pass":
        assert reply["kind"] == "compare" and found["TRACK"] in reply["tracks"]
    if name == "markers":
        assert [m["text"] for m in reply["markers"]] == ["love this"]
        assert daw("get", song, "tracks.bass.automation")[0]["param"] == "gain_db"
    if name == "finished-edit":
        assert (work / "edit.wav").exists()
        assert abs(found["TEMPO"] - 120) < 0.5
        assert not (found["SONG_PATH"].with_suffix(".beats.json")).exists()
