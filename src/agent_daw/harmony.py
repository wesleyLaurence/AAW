"""The key and the chords of a sample or a song, measured from its audio, and the
key of what a song plays, from its notes and the samples it plays.

Both start from a chromagram: how much of the sound is in each of the twelve
pitch classes, frame by frame. A key is the major or minor key whose profile, how
often each degree of the scale sounds in music in that key (Temperley), the
whole sound's classes are most like. A key and its relative share
their notes and are told apart only by which of them is heard most, so a key is
given with its candidates and with how sure the scale is apart from its tonic.
A chord is the triad, seventh or suspended chord a beat's classes are most like,
followed through the beats so that a chord changes only where the sound does,
more readily on a bar line.

Every value is an estimate from decoded audio or from the song's notes; nothing
here listens.
"""

from __future__ import annotations

import re

import numpy as np

from . import beats as beatmap

FFT, HOP = 8192, 2048
LOW_HZ, HIGH_HZ = 55.0, 2500.0  # the pitch classes are heard from these
BASS_HZ = (40.0, 220.0)  # the bass note of a chord
SECONDS = 600.0  # the most of a file that is read
# The third and fifth harmonics of a note, 19 and 28 semitones above it, at about
# the level they have against the note in a voice or most instruments.
OVERTONES = ((19, 0.5), (28, 0.35))
# Temperley's profiles from the Kostka-Payne corpus: how often each degree of
# the scale sounds in music in a key, from its tonic up. On chords made with
# overtones in every key they found the scale more often than Krumhansl and
# Kessler's.
MAJOR = np.array([0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057, 0.400])
MINOR = np.array([0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133, 0.330])
SHARPS = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
FLATS = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"]
# A key's tonic as a key signature writes it, and the keys whose signature has flats.
MAJOR_TONICS = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"]
MINOR_TONICS = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "G#", "A", "Bb", "B"]
FLAT_MAJORS, FLAT_MINORS = {1, 3, 5, 8, 10}, {0, 2, 3, 5, 7, 10}
# How sharply the candidates' odds fall away with their correlation below the best.
SHARPNESS = 25.0
# A sound has a key when its classes are uneven enough and like a key's profile
# enough: a drum's or a noise's are nearly even.
# One note alone is not a key either.
TONAL_CONTRAST = 0.12
TONAL_CORRELATION = 0.6
# Chords: a quality's intervals from its root, and what it costs against a triad,
# so that a seventh or a suspension is named only where it is heard.
QUALITIES = {
    "": ((0, 4, 7), 0.0),
    "m": ((0, 3, 7), 0.0),
    "7": ((0, 4, 7, 10), 0.04),
    "maj7": ((0, 4, 7, 11), 0.04),
    "m7": ((0, 3, 7, 10), 0.04),
    "dim": ((0, 3, 6), 0.05),
    "sus4": ((0, 5, 7), 0.05),
}
CHANGE = 0.15  # what a change of chord costs within a bar
CHANGE_ON_BAR = 0.05  # and on a bar line
NO_CHORD = 0.35  # a beat no chord is this like is no chord, N
QUIET = 0.05  # a beat this quiet against the song's median is no chord


def _templates(profile_major, profile_minor):
    rows, names = [], []
    for mode, profile in (("major", profile_major), ("minor", profile_minor)):
        for tonic in range(12):
            rows.append(np.roll(profile, tonic))
            names.append((tonic, mode))
    return np.array(rows), names


KEYS, KEY_NAMES = _templates(MAJOR, MINOR)


def key_name(tonic: int, mode: str) -> str:
    tonics = MAJOR_TONICS if mode == "major" else MINOR_TONICS
    return f"{tonics[tonic % 12]} {mode}"


def flats(tonic: int, mode: str) -> bool:
    """Whether a key's signature has flats, so its notes are spelled with them."""
    return tonic % 12 in (FLAT_MAJORS if mode == "major" else FLAT_MINORS)


def scale_of(tonic: int, mode: str) -> int:
    """The tonic of the major key with the same notes: a key and its relative share it."""
    return tonic % 12 if mode == "major" else (tonic + 3) % 12


def relative(tonic: int, mode: str) -> tuple[int, str]:
    return ((tonic + 9) % 12, "minor") if mode == "major" else ((tonic + 3) % 12, "major")


KEY_TEXT = re.compile(
    r"^\s*([A-Ga-g])\s*([#b♯♭]?)\s*(major|maj|minor|min|m|M)?\s*$"
)


def parse_key(text: str) -> tuple[int, str]:
    """A key written as A minor, Am, F#, Bb major, C#min or Ebm: (tonic, mode)."""
    found = KEY_TEXT.match(str(text))
    if not found:
        raise ValueError(f"A key is a note and major or minor, such as A minor, Am, F# or Bbm; not {text}")
    letter, accidental, mode = found.groups()
    tonic = SHARPS.index(letter.upper()) + {"#": 1, "♯": 1, "b": -1, "♭": -1}.get(accidental, 0)
    minor = mode is not None and mode.lower() in ("minor", "min") or mode == "m"
    return tonic % 12, "minor" if minor else "major"


def _fold(sr, low, high):
    """Each spectrum bin's share of the semitone it is nearest, by how near, and
    the semitones as MIDI numbers."""
    freqs = np.fft.rfftfreq(FFT, 1 / sr)
    used = np.flatnonzero((freqs >= low) & (freqs <= min(high, sr / 2)))
    pitch = 12 * np.log2(freqs[used] / 440.0) + 69
    nearest = np.round(pitch).astype(int)
    notes = np.arange(nearest.min(), nearest.max() + 1)
    fold = np.zeros((len(used), len(notes)), dtype=np.float32)
    # A bin between two semitones belongs to neither.
    fold[np.arange(len(used)), nearest - notes[0]] = np.cos(np.pi * (pitch - nearest)) ** 2
    return used, fold, notes


def _classes(notes):
    fold = np.zeros((len(notes), 12), dtype=np.float32)
    fold[np.arange(len(notes)), notes % 12] = 1
    return fold


def _without_overtones(levels):
    """Each semitone's level less what the notes below it put there as their third
    and fifth harmonics, a twelfth and two octaves and a third up: the fifth and
    the third of a note that sounds alone are otherwise heard as notes."""
    out = levels.copy()
    for interval, share in OVERTONES:
        out[:, interval:] -= share * levels[:, :-interval]
    return np.maximum(out, 0)


def chroma(mono, sr):
    """The pitch classes of each frame, from 55 Hz to 2.5 kHz, and the bass's, from
    40 to 220 Hz, less the overtones of the notes below, compressed against the
    loud frames; and frames a second."""
    high, high_fold, high_notes = _fold(sr, LOW_HZ, HIGH_HZ)
    low, low_fold, low_notes = _fold(sr, *BASS_HZ)

    def reduce(power):
        magnitude = np.sqrt(power)
        return np.hstack([magnitude[:, high] @ high_fold, magnitude[:, low] @ low_fold])

    both = beatmap._stft(np.asarray(mono, dtype=np.float32), FFT, HOP, reduce)
    split = len(high_notes)
    tones = _without_overtones(both[:, :split]) @ _classes(high_notes)
    bass = _without_overtones(both[:, split:]) @ _classes(low_notes)
    reference = max(float(np.percentile(tones.sum(axis=1), 99)), 1e-12)
    return np.log1p(100 * tones / reference), np.log1p(100 * bass / reference), sr / HOP


def estimate(profile) -> dict:
    """The key a pitch-class profile is in: the likeliest with its candidates, how
    sure the key and its scale are, and whether the sound has a key at all."""
    profile = np.asarray(profile, dtype=float)
    total = float(profile.sum())
    unit = profile / total if total > 0 else np.full(12, 1 / 12)
    contrast = float(unit.std() / unit.mean())
    if total <= 0 or contrast < 1e-6:
        r = np.zeros(len(KEYS))
    else:
        r = np.array([np.corrcoef(unit, k)[0, 1] for k in KEYS])
    odds = np.exp(SHARPNESS * (r - r.max()))
    odds /= odds.sum()
    order = np.argsort(-r)
    best = int(order[0])
    tonic, mode = KEY_NAMES[best]
    other_tonic, other_mode = relative(tonic, mode)
    pair = odds[best] + odds[KEY_NAMES.index((other_tonic, other_mode))]
    tonal = bool(contrast >= TONAL_CONTRAST and r[best] >= TONAL_CORRELATION)
    measured = {
        "correlation": round(float(r[best]), 3),
        "contrast": round(contrast, 3),
        "profile": [round(float(v), 4) for v in unit],
    }
    if not tonal:
        # Its likeliest key is still a key, but not one it is in.
        return {"tonal": False, "name": None, **measured}
    return {
        "tonal": True,
        "name": key_name(tonic, mode),
        "tonic": key_name(tonic, mode).split()[0],
        "mode": mode,
        "confidence": round(float(odds[best]), 3),
        "relative": key_name(other_tonic, other_mode),
        "scale_confidence": round(float(pair), 3),
        **measured,
        "candidates": [
            {
                "name": key_name(*KEY_NAMES[i]),
                "correlation": round(float(r[i]), 3),
                "confidence": round(float(odds[i]), 3),
            }
            for i in order[:4]
        ],
    }


def key_of(mono, sr) -> dict:
    """The key of a sound, from its whole chromagram."""
    classes, _, _ = chroma(mono, sr)
    return estimate(classes.sum(axis=0) if len(classes) else np.zeros(12))


def _chord_templates():
    rows, labels = [], []
    for name, (intervals, cost) in QUALITIES.items():
        for root in range(12):
            t = np.zeros(12)
            t[[(root + i) % 12 for i in intervals]] = 1.0
            rows.append(t)
            labels.append((root, name, cost))
    rows = np.array(rows)
    rows = rows - rows.mean(axis=1, keepdims=True)
    return rows / np.linalg.norm(rows, axis=1, keepdims=True), labels


CHORDS, CHORD_LABELS = _chord_templates()


def chord_name(root: int, quality: str, flat: bool, bass: int | None = None) -> str:
    names = FLATS if flat else SHARPS
    name = names[root % 12] + quality
    return f"{name}/{names[bass % 12]}" if bass is not None and bass % 12 != root % 12 else name


def chords(mono, sr, times, downbeats, key=None) -> list[dict]:
    """The chords over the beats at `times` (seconds, the last one the end of the
    last beat): a run of beats a chord, each with its first beat's index, how
    many beats it lasts, its name and how like the chord its beats are.
    `downbeats` says which beats start a bar; `key` spells the names."""
    classes, bass, fps = chroma(mono, sr)
    if len(times) < 2:
        return []
    per_beat = beatmap._per_beat(classes, fps, times)
    low = beatmap._per_beat(bass, fps, times)
    level = per_beat.sum(axis=1)
    quiet = level < QUIET * max(float(np.median(level)), 1e-12)
    centered = per_beat - per_beat.mean(axis=1, keepdims=True)
    norm = np.linalg.norm(centered, axis=1, keepdims=True)
    unit = np.where(norm > 1e-12, centered / np.maximum(norm, 1e-12), 0.0)
    costs = np.array([c for _, _, c in CHORD_LABELS])
    like = unit @ CHORDS.T  # beats by chords
    # The last state is no chord, as like any beat as NO_CHORD, and a quiet beat
    # is no chord.
    score = np.hstack([like - costs, np.full((len(like), 1), NO_CHORD)])
    score[quiet, :-1] = -1.0
    states = score.shape[1]
    total, back = score[0].copy(), []
    for i in range(1, len(score)):
        change = CHANGE_ON_BAR if downbeats[i] else CHANGE
        stay = total
        move = total.max() - change
        back.append(np.where(stay >= move, np.arange(states), int(np.argmax(total))))
        total = np.maximum(stay, move) + score[i]
    path = [int(np.argmax(total))]
    for previous in reversed(back):
        path.append(int(previous[path[-1]]))
    path = path[::-1]
    flat = flats(*key) if key else False
    runs, start = [], 0
    for i in range(1, len(path) + 1):
        if i < len(path) and path[i] == path[start]:
            continue
        state = path[start]
        beats = slice(start, i)
        if state == states - 1:
            name, fit = "N", None
        else:
            root, quality, _ = CHORD_LABELS[state]
            intervals = QUALITIES[quality][0]
            heard = low[beats].sum(axis=0)
            low_note = int(np.argmax(heard))
            share = float(heard[low_note] / max(heard.sum(), 1e-12))
            slash = (low_note - root) % 12 in intervals[1:] and share >= 0.2
            name = chord_name(root, quality, flat, low_note if slash else None)
            fit = round(float(like[beats, state].mean()), 3)
        runs.append({"first_beat": start, "beats": i - start, "chord": name, "fit": fit})
        start = i
    return runs


def transpose_to(sample_key: str, song_key: str) -> dict:
    """The semitones, from -6 to 5, that move a sample's notes onto a key's: its
    scale onto the key's scale, so a sample in a key's relative needs none.
    Both keys are names, A minor."""
    tonic, mode = parse_key(sample_key)
    target_tonic, target_mode = parse_key(song_key)
    shift = (scale_of(target_tonic, target_mode) - scale_of(tonic, mode)) % 12
    shift = shift - 12 if shift >= 6 else shift
    moved = key_name(tonic + shift, mode)
    return {
        "transpose": shift,
        "then_key": moved,
        "relation": "same key" if moved == key_name(target_tonic, target_mode) else "relative key",
    }
