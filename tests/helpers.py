"""Driving `daw` from tests: commands, renders and one effect chain on its own."""

import json
import os
import subprocess
import sys
from fractions import Fraction

import numpy as np
import soundfile as sf

from agent_daw.model import save

SR = 48000


def daw(*args):
    """A Rust `daw` command's result. Its error is raised as ValueError."""
    out = subprocess.run(
        [os.environ["AAW_DAW"], *map(str, args)], capture_output=True, text=True
    )
    if out.returncode:
        raise ValueError(json.loads(out.stderr)["error"])
    return json.loads(out.stdout)


def cli(*args):
    """`daw` through the Python entry point: its exit status and JSON."""
    out = subprocess.run(
        [sys.executable, "-m", "agent_daw.cli", *map(str, args)],
        capture_output=True,
        text=True,
    )
    # An error is the last line of stderr; notes may come before it.
    return out.returncode, json.loads(out.stdout if out.returncode == 0 else out.stderr.strip().splitlines()[-1])


def render(path, output=None, block_size=None, track_id=None, section=None):
    args = ["render", path]
    for flag, value in [
        ("--output", output),
        ("--block-size", block_size),
        ("--track", track_id),
        ("--section", section),
    ]:
        if value is not None:
            args += [flag, value]
    return daw(*args)


def stem(directory, name):
    return sf.read(directory / "stems" / f"{name}.wav", always_2d=True)[0]


def tone(freq, seconds=1.0, amp=0.5):
    x = amp * np.sin(2 * np.pi * freq * np.arange(round(seconds * SR)) / SR)
    return np.column_stack([x, x])


def level_db(x):
    return 20 * np.log10(np.sqrt(np.mean(x**2)))


def lanes(automation):
    """Lanes from {effect index: {field: [(at, value[, curve]), ...]}}."""
    return [
        {
            "param": f"effects.{index}.{field}",
            "points": [dict(zip(("at", "value", "curve"), p)) for p in points],
        }
        for index, fields in automation.items()
        for field, points in fields.items()
    ]


def run_chain(directory, effects, x, automation=None, keys=None, tempo=120, block_size=None):
    """Stereo audio through an effect chain alone: (output, effect reports).

    A song plays `x` once at full level on a track with the effects and nothing
    else, so the track's stem is the chain's output as 32-bit floats. `keys` are
    tracks a compressor's sidechain can name. Each call needs its own directory.
    """
    directory.mkdir(parents=True)
    length = str(Fraction(len(x)) * Fraction(str(tempo)) / (60 * SR))
    pad = {"attack_ms": 0, "release_ms": 0}
    tracks = [{"id": "chain", "effects": effects, "automation": lanes(automation or {})}]
    for name in keys or {}:
        tracks.append({"id": name})
    samples = {"chain": x, **(keys or {})}
    for track in tracks:
        name = track["id"]
        sf.write(directory / f"{name}.wav", samples[name], SR, subtype="DOUBLE")
        track["pads"] = {"x": {"sample": name, **pad}}
        track["clips"] = [{"pattern": "once"}]
    song = {
        "session": {
            "tempo": tempo,
            "length_beats": length,
            "master_gain_db": 0,
            "end_fade_ms": 0,
        },
        "samples": {name: {"path": f"{name}.wav"} for name in samples},
        "patterns": {
            "once": {
                "length_beats": length,
                "events": [{"at": 0, "pad": "x", "velocity": 127}],
            }
        },
        "tracks": tracks,
    }
    save(song, directory / "song.yaml")
    report = render(
        directory / "song.yaml", directory / "out", block_size, track_id="chain"
    )
    return stem(directory / "out", "chain"), report["tracks"]["chain"]["effects"]


def beat_song(path, bpm=120.0, bars=24, first=0.137, pickup=0, late=None, seed=0):
    """Writes a song at 44.1 kHz whose grid is known, and returns its beat times.

    A kick on beats one and three, a snare on two and four and a bass note that
    changes each bar, with eighth-note hats from bar 9 and a higher note from bar
    17. The first downbeat is beat `pickup`, `first` seconds in or later. `late`
    gives how many seconds late each beat is, by its number.
    """
    RATE = 44100
    rng = np.random.default_rng(seed)
    count = bars * 4 + pickup
    times = first + 60 / bpm * np.arange(count)
    if late is not None:
        times = times + late(np.arange(count))
    x = np.zeros(round((times[-1] + 60 / bpm + 1) * RATE))

    def add(at, sound):
        start = round(at * RATE)
        sound = sound[: len(x) - start]
        x[start : start + len(sound)] += sound

    def kick():
        t = np.arange(round(0.25 * RATE)) / RATE
        sweep = np.cumsum(50 + 120 * np.exp(-t / 0.02)) / RATE
        return 0.8 * np.sin(2 * np.pi * sweep) * np.exp(-t / 0.08)

    def noise(seconds, decay, level):
        n = round(seconds * RATE)
        return level * rng.standard_normal(n) * np.exp(-np.arange(n) / (decay * RATE))

    def note(hz, seconds, level=0.2):
        t = np.arange(round(seconds * RATE)) / RATE
        shape = np.minimum(1, t / 0.01) * np.minimum(1, (seconds - t) / 0.02)
        return level * (np.sin(2 * np.pi * hz * t) + 0.4 * np.sin(4 * np.pi * hz * t)) * shape

    roots = [55.0, 65.41, 73.42, 49.0]
    for i, at in enumerate(times):
        bar, place = divmod(i - pickup, 4)
        gap = times[i + 1] - at if i + 1 < count else 60 / bpm
        if i < pickup:
            add(at, noise(0.03, 0.004, 0.12))
            continue
        phrase = bar // 8
        add(at, kick() if place in (0, 2) else noise(0.15, 0.03, 0.4))
        if phrase >= 1:
            add(at, noise(0.03, 0.004, 0.12))
            add(at + gap / 2, noise(0.03, 0.004, 0.12))
        root = roots[bar % 4] * (1.5 if phrase % 2 else 1)
        add(at, note(root, gap * 0.9))
        if phrase >= 2:
            add(at, note(root * 4, gap * 0.9, 0.08))
    sf.write(path, 0.8 * x / np.abs(x).max(), RATE, subtype="PCM_16")
    return np.round(times * RATE) / RATE


def chord_song(path=None, chords=(), bpm=100.0, beats_each=4, drums=True, swing=0.5, grid=0.5, bass=True, seed=0):
    """A song at 44.1 kHz whose chords and swing are known: each chord, MIDI
    notes, for `beats_each` beats with its lowest note an octave down under it
    (or the note `bass` gives for every chord when it is a number), and with
    `drums` a kick and a snare on the beats and a hat on every `grid` beats,
    the second of each pair late by `swing` as a pattern's swing is. Writes it
    to `path` if given and returns the samples."""
    RATE = 44100
    rng = np.random.default_rng(seed)
    beat = 60 / bpm

    def tone(midi, seconds, level):
        t = np.arange(round(seconds * RATE)) / RATE
        hz = 440 * 2 ** ((midi - 69) / 12)
        x = sum(0.6 ** k * np.sin(2 * np.pi * hz * (k + 1) * t) for k in range(6) if hz * (k + 1) < RATE / 2)
        return level * x * np.minimum(1, t / 0.01) * np.exp(-1.5 * t)

    parts = []
    for c in chords:
        seconds = beat * beats_each
        x = sum(tone(n, seconds, 0.2) for n in c)
        if bass is not False:
            x = x + tone(min(c) - 12 if bass is True else bass, seconds, 0.3)
        parts.append(x)
    x = np.concatenate(parts) if parts else np.zeros(round(16 * beat * RATE))
    if drums:
        t = np.arange(round(0.2 * RATE)) / RATE
        kick = 0.8 * np.sin(2 * np.pi * np.cumsum(50 + 90 * np.exp(-t * 30)) / RATE) * np.exp(-t * 18)
        snare = 0.4 * rng.standard_normal(len(t)) * np.exp(-t * 25)
        h = np.arange(round(0.04 * RATE)) / RATE
        hat = 0.12 * np.diff(rng.standard_normal(len(h) + 1)) * np.exp(-h * 90)

        def put(sound, at, gain=1.0):
            start = round(at * RATE)
            if 0 <= start < len(x):
                piece = sound[: len(x) - start]
                x[start : start + len(piece)] += gain * piece

        for cell in range(int(len(x) / RATE / beat / grid)):
            at = cell * grid + (grid * (2 * swing - 1) if cell % 2 else 0)
            put(hat, at * beat, 0.6 + 0.4 * rng.random())
        for b in range(int(len(x) / RATE / beat)):
            put(kick if b % 2 == 0 else snare, b * beat)
    x = 0.8 * x / max(float(np.abs(x).max()), 1e-12)
    if path is not None:
        sf.write(path, x, RATE, subtype="PCM_16")
    return x
