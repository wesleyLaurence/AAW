"""What a sample sounds like, in numbers: how bright it is, how much low end
it has, how fast it starts, how long it rings, how noisy it is, how loud and
how hard it hits. Each is read from the sound itself, from its first sound
to its last, and a word beside each says where it sits among samples of its
category. These are measurements to choose by; nobody heard them.
"""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np

from . import aaw_py
from .dynamics import ATTACK_MS, BODY_MS, FLOOR_DB, punch

# The measuring code's part of analysis.ANALYZER: a change to this file or to
# what a punch is measures the library again.
SOURCE = Path(__file__).read_bytes() + repr((ATTACK_MS, BODY_MS, FLOOR_DB)).encode()

# The spectrum is the mean power of windows this long, a quarter apart.
WINDOW_SECONDS = 0.085
LOW_HZ, TOP_HZ = 20.0, 20000.0
# Low end is what lies under this.
LOW_END_HZ = 120.0
# Noisiness compares a stretch of the sound with itself from NEAR_SECONDS to
# FAR_SECONDS later, over the sound's first NOISE_SECONDS.
NEAR_SECONDS, FAR_SECONDS = 0.002, 0.04
NOISE_SECONDS = 10.0
# A sound's level is its peak, or the peak a tone as strong as its loudest
# LEVEL_SECONDS would have where that is lower: noise's highest sample is a
# chance, a long way over the rest and anywhere in it. The sound starts
# between these shares of its level, and has rung out once it stays this far
# under it.
LEVEL_SECONDS = 0.01
RISE = (0.1, 0.9)
RUNG_OUT_DB = 20.0
# Loudness is of the loudest stretch this long, which a shorter sound ends in
# silence.
MOMENT_SECONDS = 0.4

FIELDS = (
    "centroid_hz",
    "low_fraction",
    "attack_ms",
    "decay_ms",
    "noisiness",
    "loudness_lufs",
    "punch_db",
)

# sample_words.json holds the words for a number, from the least to the most,
# and the two lines between them: every sample's under `any`, and a category's
# own where the thirds of its samples lie far enough apart to tell by ear.
# daw describe samples prints the file, which the Rust model reads too.
WORDS, LINES = (
    json.loads(Path(__file__).with_name("sample_words.json").read_text())[key]
    for key in ("words", "lines")
)

# Two sounds are a step apart in a field when they differ by this much: half
# an octave of brightness, a doubling of the attack or of the decay, and so
# on. A step is about how far a category's samples spread in the field.
STEPS = {
    "centroid_hz": (lambda hz: np.log2(max(hz, 1.0)), 0.5),
    "low_fraction": (float, 0.15),
    "attack_ms": (lambda ms: np.log2(ms + 1), 1.0),
    "decay_ms": (lambda ms: np.log2(ms + 10), 1.0),
    "noisiness": (float, 0.15),
    "punch_db": (float, 6.0),
}


def spectrum(mono: np.ndarray, sr: int):
    """The mean power of the sound's windows at each frequency."""
    n = 1 << round(np.log2(sr * WINDOW_SECONDS))
    hop = n // 4
    x = np.pad(mono, (n // 2, n // 2))
    count = 1 + (len(x) - n) // hop
    window = np.hanning(n)
    power = np.zeros(n // 2 + 1)
    for first in range(0, count, 256):  # bounded memory for long files
        rows = np.arange(first, min(count, first + 256))
        idx = np.arange(n)[None, :] + hop * rows[:, None]
        power += (np.abs(np.fft.rfft(x[idx] * window, axis=1)) ** 2).sum(axis=0)
    return np.fft.rfftfreq(n, 1 / sr), power / count


def noisiness(mono: np.ndarray, sr: int):
    """How unlike itself the sound is a moment later, from 0 for a tone to
    near 1 for noise. A stretch of 40 ms is held against the stretch that
    starts from 2 to 40 ms after it, and its likeness to the one most like it
    is 1 for a sound that repeats, at any pitch from 25 Hz up, and near 0 for
    noise. That is read every 40 ms, and the readings' mean by the energy in
    each is taken from 1. A spectrum's flatness says how much of it a sound
    fills, so by it a kick is a tone and a hat less noisy the higher it is
    cut; this reads neither the tilt nor the band."""
    far, near = round(sr * FAR_SECONDS), round(sr * NEAR_SECONDS)
    length = 2 * far
    x = mono[: round(NOISE_SECONDS * sr)]
    x = np.pad(x, (0, max(0, length - len(x))))
    frames = np.stack([x[s : s + length] for s in range(0, len(x) - length + 1, far)])
    size = 1 << int(np.ceil(np.log2(length + far)))
    head = np.fft.rfft(frames[:, :far], size)
    cross = np.fft.irfft(np.conj(head) * np.fft.rfft(frames, size), size)[:, : far + 1]
    sq = np.cumsum(np.pad(frames**2, ((0, 0), (1, 0))), axis=1)
    lags = np.arange(far + 1)
    energy = sq[:, lags + far] - sq[:, lags]
    own = energy[:, :1]
    with np.errstate(divide="ignore", invalid="ignore"):
        alike = np.where(own + energy > 0, 2 * cross / (own + energy), 0.0)
    weight = own[:, 0]
    if weight.sum() <= 0:
        return None
    best = float((alike[:, near:].max(axis=1) * weight).sum() / weight.sum())
    return float(np.clip(1 - best, 0, 1)) + 0.0


def measure(x: np.ndarray, sr: int, start: int, end: int, loop: bool) -> dict:
    """The descriptors of the sound in `x`, frames by channels, between its
    first sound and its last. A loop has many hits, so its attack, decay and
    punch are null."""
    sound = x[start:end]
    mono = sound.mean(axis=1)
    freqs, power = spectrum(mono, sr)
    heard = (freqs >= LOW_HZ) & (freqs <= TOP_HZ)
    energy = float(power[heard].sum())
    out = dict.fromkeys(FIELDS)
    if energy > 0:
        out["centroid_hz"] = round(float((freqs[heard] * power[heard]).sum() / energy))
        out["low_fraction"] = round(float(power[heard & (freqs < LOW_END_HZ)].sum() / energy), 3)
    noise = noisiness(mono, sr)
    out["noisiness"] = None if noise is None else round(noise, 3)
    # The loudest 400 ms, by the meter that reads a render, which takes one
    # channel or two: more are read as their sum.
    heard_as = sound if x.shape[1] <= 2 else mono[:, None]
    moment = round(MOMENT_SECONDS * sr)
    padded = np.zeros((max(len(sound), moment), heard_as.shape[1]))
    padded[: len(sound)] = heard_as
    read = aaw_py.loudness(np.ascontiguousarray(padded), sr, [(0, len(padded))])[0]
    loudest = read["max_momentary_lufs"]
    out["loudness_lufs"] = None if loudest is None else round(loudest, 1)
    if loop:
        return out
    heights = np.abs(sound).max(axis=1)
    span = min(len(heights), round(LEVEL_SECONDS * sr))
    power = np.concatenate([[0.0], np.cumsum(heights**2)])
    loudest = float(np.sqrt((power[span:] - power[:-span]).max() / span))
    level = min(float(heights.max()), np.sqrt(2) * loudest)
    rise = [int(np.argmax(heights >= share * level)) for share in RISE]
    out["attack_ms"] = round((rise[1] - rise[0]) / sr * 1000, 1)
    rung = int(np.flatnonzero(heights >= level * 10 ** (-RUNG_OUT_DB / 20))[-1])
    out["decay_ms"] = round((rung - rise[1]) / sr * 1000, 1)
    # As daw listen reads a hit: the peak of its first 30 ms against the RMS
    # of the 200 ms after them, with silence where the sound has ended.
    attack, body = (round(ms * sr / 1000) for ms in (ATTACK_MS, BODY_MS))
    hit = np.zeros((attack + body, x.shape[1]))
    hit[: min(len(sound), len(hit))] = sound[: len(hit)]
    value = punch(hit, [(0, attack, attack + body)])[0][0]
    out["punch_db"] = None if np.isnan(value) else round(float(value), 1)
    return out


def lines(category: str | None) -> dict:
    """The two lines between a field's three words for a category: its own
    where its samples spread enough to tell apart, and every sample's."""
    return {**LINES["any"], **LINES.get(category or "", {})}


def words(sound: dict, category: str | None) -> dict:
    """A word for each number that has one, against the category's lines."""
    at = lines(category)
    out = {}
    for field, names in WORDS.items():
        value = sound.get(field)
        if value is not None:
            low, high = at[field]
            out[field] = names[(value >= low) + (value >= high)]
    return out


def category(sound: dict, loop: bool) -> str | None:
    """The category a single sound measures as, where that is clear: a hat
    or a kick. In a library of 1,673 samples, nine in ten of those these two
    rules chose that had a category in their names had the same one there."""
    if loop or any(sound[f] is None for f in WORDS) or sound["decay_ms"] >= 400:
        return None
    if (
        sound["noisiness"] >= 0.5
        and sound["centroid_hz"] >= 5000
        and sound["low_fraction"] < 0.01
        and sound["attack_ms"] < 20
    ):
        return "hat"
    if (
        sound["low_fraction"] >= 0.5
        and sound["centroid_hz"] < 300
        and sound["attack_ms"] < 15
        and (sound["punch_db"] or 0) >= 4
    ):
        return "kick"
    return None


def distance(a: dict, b: dict) -> float | None:
    """How far apart two sounds are, in steps: the root of the mean square
    of their differences over the fields both have. Loudness is left out,
    since a gain changes it."""
    apart = [
        (scale(a[field]) - scale(b[field])) / step
        for field, (scale, step) in STEPS.items()
        if a.get(field) is not None and b.get(field) is not None
    ]
    return float(np.sqrt(np.mean(np.square(apart)))) if apart else None
