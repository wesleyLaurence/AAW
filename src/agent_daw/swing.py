"""How late a song's or a loop's off-beat notes fall: its swing.

Swing is said as a pattern's `swing` says it: where the second note of a pair
falls within the pair, 0.5 straight, two thirds a triplet feel, 0.75 a dotted
note and its short one. The pair is a beat for swung eighths and half a beat for
swung sixteenths. The onsets of the sound are placed against its beats, and the
ones between beats are read as the second eighth of a beat, or as the second or
fourth sixteenth; which of the two grids the sound is swung on is the one its
off-beat onsets are mostly on.

Every value is an estimate from decoded audio; nothing here listens.
"""

from __future__ import annotations

import numpy as np

from . import beats as beatmap

# Where in a beat each kind of off-beat onset falls. The second eighth falls from
# halfway (straight) to a little under three quarters; the second and fourth
# sixteenths from a quarter and three quarters of the beat (straight) a little
# later. An onset near the beat is on it.
ON_BEAT = 0.08
EIGHTH = (0.42, 0.735)
SIXTEENTHS = ((0.17, 0.42), (0.735, 0.92))
SIXTEENTH_SHARE = 0.25  # of the off-beat onsets' weight, on sixteenths, for 1/16
MIN_ONSETS = 8
AGREE = 0.06  # of a pair: an onset this near its kind's middle is one of them
CLUSTER = 0.4  # of a kind's weight, near its middle, for it to have a swing
STRAIGHT = 0.52  # under this a groove is straight
TRIPLET = 0.02  # this near two thirds it is a triplet feel
SPACING = 0.03  # seconds between onsets at the least
REFINE = 0.012  # how far from its peak an onset's attack is looked for, seconds


def onsets(mono, sr):
    """Each onset's time in seconds, at its attack, and its strength from 0 to 1."""
    power, tops = beatmap.band_power(mono, sr)
    flux, _ = beatmap.onset_envelopes(power, tops)
    if not len(flux) or flux.max() <= 0:
        return np.zeros(0), np.zeros(0)
    fps = sr / beatmap.HOP
    envelope = flux / flux.max()
    reach = max(1, round(SPACING * fps))
    padded = np.pad(envelope, reach)
    windows = np.lib.stride_tricks.sliding_window_view(padded, 2 * reach + 1)
    local = windows.max(axis=1)
    # Against what is around it over half a second, so a quiet hat between loud
    # kicks is an onset and the tail of a crash is not.
    around = max(1, round(0.25 * fps))
    level = np.convolve(envelope, np.ones(2 * around + 1) / (2 * around + 1), mode="same")
    peaks = np.flatnonzero(
        (envelope == local) & (envelope >= 0.04) & (envelope >= 1.5 * level)
    )
    if not len(peaks):
        return np.zeros(0), np.zeros(0)
    times = peaks / fps
    moved, _ = beatmap.transients(mono, sr, times, REFINE)
    return times + moved, envelope[peaks]


def _median(values, weights):
    order = np.argsort(values)
    values, weights = values[order], weights[order]
    middle = np.cumsum(weights) - weights / 2
    return float(np.interp(weights.sum() / 2, middle, values))


def _quartiles(values, weights):
    order = np.argsort(values)
    values, weights = values[order], weights[order]
    middle = (np.cumsum(weights) - weights / 2) / weights.sum()
    return np.interp([0.25, 0.75], middle, values)


def _side(values, weights, pair_seconds):
    """The swing of one kind of off-beat onset: where they cluster within the
    pair. Onsets far from the cluster, such as an attack in a held chord, are not
    counted, and a kind whose onsets do not mostly cluster has no swing."""
    if len(values) < MIN_ONSETS:
        return {"swing": None, "onsets": int(len(values))}, 0.0
    center = _median(values, weights)
    agree = np.abs(values - center) <= AGREE
    if agree.sum() < MIN_ONSETS or weights[agree].sum() < CLUSTER * weights.sum():
        return {"swing": None, "onsets": int(agree.sum())}, 0.0
    values, weights = values[agree], weights[agree]
    swing = _median(values, weights)
    low, high = _quartiles(values, weights)
    return {
        "swing": round(swing, 3),
        "onsets": int(len(values)),
        "spread": round(float(high - low), 3),
        "late_ms": round((swing - 0.5) * pair_seconds * 1000, 1),
    }, float(weights.sum())


def measure(mono, sr, beat_times) -> dict:
    """The swing of a sound against its beats, `beat_times` in seconds."""
    beat_times = np.asarray(beat_times, dtype=float)
    times, strength = onsets(mono, sr)
    result = {
        "grid": None,
        "swing": None,
        "feel": None,
        "eighths": {"swing": None, "onsets": 0},
        "sixteenths": {"swing": None, "onsets": 0},
    }
    if len(beat_times) < 3 or not len(times):
        return {**result, "why": "too few beats or onsets"}
    index = np.searchsorted(beat_times, times, side="right") - 1
    inside = (index >= 0) & (index < len(beat_times) - 1)
    times, strength, index = times[inside], strength[inside], index[inside]
    length = beat_times[index + 1] - beat_times[index]
    place = (times - beat_times[index]) / length
    weight = np.sqrt(strength)
    # The beats are placed at the attacks of what is on them; what is measured
    # here is placed the same way, and the onsets on the beat say how far the two
    # differ, which is taken off every place.
    on = (place < ON_BEAT) | (place > 1 - ON_BEAT)
    if on.sum() >= MIN_ONSETS:
        place = place - _median(np.where(place > 0.5, place - 1, place)[on], weight[on])
    beat = float(np.median(np.diff(beat_times)))
    eighth = (place >= EIGHTH[0]) & (place < EIGHTH[1])
    sixteenth = np.zeros(len(place), dtype=bool)
    for low, high in SIXTEENTHS:
        sixteenth |= (place >= low) & (place < high)
    result["eighths"], eighths = _side(place[eighth], weight[eighth], beat)
    result["sixteenths"], sixteenths = _side(
        np.where(place < 0.5, 2 * place, 2 * place - 1)[sixteenth], weight[sixteenth], beat / 2
    )
    on16 = sixteenths > 0 and sixteenths >= SIXTEENTH_SHARE * (eighths + sixteenths)
    chosen = "sixteenths" if on16 else "eighths"
    swing = result[chosen]["swing"]
    if swing is None:
        return {**result, "why": "too few onsets between the beats"}
    if swing < STRAIGHT:
        feel = "straight"
    elif abs(swing - 2 / 3) <= TRIPLET:
        feel = "triplet"
    else:
        feel = "swung"
    grid = 0.25 if chosen == "sixteenths" else 0.5
    return {
        **result,
        "grid": "1/16" if grid == 0.25 else "1/8",
        "swing": swing,
        "feel": feel,
        "pattern": {"grid": grid, "swing": float(np.clip(round(swing, 2), 0.5, 0.75))},
        "delay_beats": round((2 * swing - 1) * grid, 4),
    }
