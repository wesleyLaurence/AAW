"""A song's beats and bars as a model trained to hear them has them: Beat This!

The onset measurement in beats.py finds a grid that fits the song's attacks and
scores once, for the whole song, which of its beats starts a bar. A song played
without a click, a half-time groove, a bar of three beats or an edit off the
downbeat goes wrong there. Beat This! (Foscarin, Schlüter and Widmer, ISMIR
2024, MIT) was trained on thousands of annotated songs and says, 50 times a
second, how likely a beat and a downbeat start there; it does not assume a bar's
length. Where `daw separate setup` has installed it, it leads, and the onsets
back it:

1. The model's frames give its beats and its downbeats (`heard`).
2. The pulse (`plan`): the model's tempo, doubled when it is under 70 BPM, or
   under 90 BPM and what starts between its beats starts nearly as strongly as
   what starts on them, as trap's hats and snares do in a 140 song the model
   counts at 70.
   --bpm takes half, the same or double the model's, whichever is nearest.
3. The beats: beats.track measures the onset grid at that tempo. Where its beats
   are the model's, within 40 ms, they are kept, since they are placed to a
   fraction of a millisecond. Where they are not, the model's are taken, each
   moved to the attack within 30 ms of it (`refined`).
4. The bars (`bars`): a path through the beats takes a downbeat where the model
   hears one and keeps the bars one length, the length the model most often
   hears; a bar of any other length costs about two bars' worth of the model's
   word, so one stray downbeat is passed over and a change that holds is
   followed. From a --downbeat time on, bars of that length run from it to the
   next one asked for, whatever the model hears: a correction holds.

The model runs as its own process in the separation environment
(downbeat_runner.py), which prints its frames; everything after that is here, so
it can be tested with frames made up (AAW_DOWNBEATS names a stand-in, and "off"
measures without the model). Every value is an estimate; nothing here listens.
"""

from __future__ import annotations

import json
import os
import shlex
import subprocess
from pathlib import Path

import numpy as np

from . import beats

NAME = "beat-this-final0"
ALWAYS_DOUBLED = 70.0  # the model's tempo under which it is counted twice as fast
MAYBE_DOUBLED = 90.0  # and under which it is, when the points between its beats hold attacks
BETWEEN_STRENGTH = 0.8  # how strongly, against its beats, by the onset envelope
SAME_BEAT = 0.040  # a model's beat this near an onset grid's is the same beat
SAME_SHARE = 0.8  # the share of beats that must be the same for the grid to be kept
SNAP = 0.030  # how far from a model's beat its attack is looked for
BAR_COST = 6.0  # a bar of another length, against -log of the model's odds
FLOOR = 0.05  # no beat is surer than this either way
TIMEOUT = 600


# --- running the model ---------------------------------------------------------


def spec() -> dict:
    from . import separation

    return next(m for m in separation.analysis() if m["name"] == NAME)


def checkpoint() -> Path:
    from . import separation

    return separation.models_dir() / spec()["files"][0]["name"]


def unavailable() -> str | None:
    """Why the model cannot run on this Mac, or None when it can."""
    value = os.environ.get("AAW_DOWNBEATS")
    if value == "off":
        return "AAW_DOWNBEATS is off"
    if value:
        return None
    from . import separation

    if not separation.env_python().is_file():
        return "The separation environment is not installed; daw separate setup installs it with the downbeat model"
    if not any(separation.root().glob("env/lib/python*/site-packages/beat_this")):
        return "The separation environment lacks Beat This!; daw separate setup adds it"
    file = checkpoint()
    if not file.is_file() or file.stat().st_size != spec()["files"][0]["bytes"]:
        return f"The downbeat model is not downloaded; daw separate setup --models {NAME} gets it"
    return None


def frames(path: Path) -> dict:
    """The model's frames for a file: its beat and downbeat logits, 50 a second."""
    from . import separation

    value = os.environ.get("AAW_DOWNBEATS")
    if value:
        command = [*shlex.split(value), "--source", str(path), "--checkpoint", "stand-in"]
    else:
        runner = Path(__file__).with_name("downbeat_runner.py")
        command = [str(separation.env_python()), str(runner), "--source", str(path), "--checkpoint", str(checkpoint())]
    try:
        out = subprocess.run(command, capture_output=True, text=True, timeout=TIMEOUT)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ValueError(f"The downbeat model did not run: {exc}") from None
    lines = [line for line in out.stdout.splitlines() if line.startswith("{")]
    try:
        result = json.loads(lines[-1])
    except (IndexError, json.JSONDecodeError):
        raise ValueError(f"The downbeat model said nothing readable: {out.stderr.strip()[-500:]}") from None
    if out.returncode or "error" in result:
        raise ValueError(f"The downbeat model failed: {result.get('error') or out.stderr.strip()[-500:]}")
    return {
        "fps": float(result["fps"]),
        "beat": np.asarray(result["beat"], dtype=float),
        "downbeat": np.asarray(result["downbeat"], dtype=float),
        "device": result.get("device"),
        "seconds": result.get("seconds"),
    }


# --- what the model heard ------------------------------------------------------


def peaks(logits, fps: float) -> np.ndarray:
    """Times of the frames more likely than not and the likeliest within 60 ms
    either side, neighbors one peak at their middle: the model's own reading."""
    x = np.asarray(logits, dtype=float)
    if not len(x):
        return np.zeros(0)
    padded = np.pad(x, 3, constant_values=-np.inf)
    top = np.lib.stride_tricks.sliding_window_view(padded, 7).max(axis=1)
    at = np.flatnonzero((x == top) & (x > 0))
    groups = np.split(at, np.flatnonzero(np.diff(at) > 1) + 1) if len(at) else []
    return np.array([g.mean() / fps for g in groups])


def heard(found: dict) -> tuple[np.ndarray, np.ndarray]:
    """The model's beats, the less likely of two too close dropped and any it
    passed over in a break put back, and its downbeats, each on its nearest beat."""
    times = peaks(found["beat"], found["fps"])
    times = filled(thinned(times, likelihood(found["beat"], found["fps"], times)))
    downs = peaks(found["downbeat"], found["fps"])
    if len(times):
        downs = np.unique([times[np.argmin(np.abs(times - d))] for d in downs])
    return times, downs


def thinned(times, odds) -> np.ndarray:
    """Beats without the less likely of any two closer than half the usual beat
    there: a stray the model heard between two beats."""
    times, odds = np.asarray(times, dtype=float), np.asarray(odds, dtype=float)
    for _ in range(3):
        if len(times) < 4:
            break
        gaps = np.diff(times)
        close = np.flatnonzero(gaps < 0.5 * beats._smooth(gaps, 9))
        if not len(close):
            break
        drop = set(int(i) if odds[i] < odds[i + 1] else int(i) + 1 for i in close)
        keep = np.array([i not in drop for i in range(len(times))])
        times, odds = times[keep], odds[keep]
    return times


def filled(times) -> np.ndarray:
    """Beats with a gap of two or more beats filled evenly from those either side."""
    times = np.asarray(times, dtype=float)
    if len(times) < 4:
        return times
    gaps = np.diff(times)
    typical = beats._smooth(gaps, 9)
    out = [times[0]]
    for t, gap, usual in zip(times[1:], gaps, typical):
        count = int(round(gap / usual))
        if count >= 2:
            out.extend(out[-1] + gap * np.arange(1, count) / count)
        out.append(t)
    return np.array(out)


def likelihood(logits, fps: float, times) -> np.ndarray:
    """How likely, by the model, each time is: its likeliest frame within 40 ms."""
    logits = np.asarray(logits, dtype=float)
    at = np.round(np.asarray(times) * fps).astype(int)
    reach = np.clip(at[:, None] + np.arange(-2, 3)[None, :], 0, len(logits) - 1)
    return 1 / (1 + np.exp(-logits[reach].max(axis=1)))


def onset(flux, fps: float, times) -> np.ndarray:
    """The onset envelope's peak within 2 frames of each time: how strong an attack starts there."""
    at = np.round(np.asarray(times) * fps).astype(int)
    reach = np.clip(at[:, None] + np.arange(-2, 3)[None, :], 0, len(flux) - 1)
    return np.asarray(flux)[reach].max(axis=1)


def bpm_of(times) -> float:
    return float(60 / np.median(np.diff(times)))


def plan(found: dict, flux, fps: float, hint=None) -> dict:
    """The pulse the map counts: the model's beats as heard, doubled or halved.

    `flux` is the song's onset envelope at `fps` frames a second (beats.py's).
    factor is 1, 2 or 0.5, or None when --bpm asks for a tempo none of those is
    near, which leaves the beats to the onset grid at that tempo.
    """
    times, downs = heard(found)
    heard_bpm = bpm_of(times)
    factor = count(times, flux, fps, hint)
    return {
        "beats": counted(found, times, factor),
        "heard_bpm": heard_bpm,
        "factor": factor,
        "bpm": None if factor is None else heard_bpm * factor,
        "bar": bar_length(times, downs),
    }


def count(times, flux, fps: float, hint=None):
    """How the model's beats are counted: 1, 2 or 0.5, or None for a --bpm none
    of those is near."""
    heard_bpm = bpm_of(times)
    if hint:
        factor = min((0.5, 1.0, 2.0), key=lambda f: abs(np.log(heard_bpm * f / hint)))
        return factor if abs(np.log(heard_bpm * factor / hint)) <= 0.04 else None
    if heard_bpm < ALWAYS_DOUBLED:
        return 2.0
    if heard_bpm < MAYBE_DOUBLED:
        # Trap's hats and snares between the beats a model counts at 70 start as
        # strongly as its beats do; a slow groove's eighths start softer.
        between = onset(flux, fps, (times[:-1] + times[1:]) / 2)
        on = float(np.median(onset(flux, fps, times)))
        return 2.0 if on > 0 and float(np.median(between)) >= BETWEEN_STRENGTH * on else 1.0
    if heard_bpm > beats.MAX_BPM:
        return 0.5
    return 1.0


def counted(found, times, factor, after=None) -> np.ndarray:
    """The model's beats counted by `factor`: with a beat between each two at 2,
    every other at 0.5. `after` is the beat that follows them, if any, so a
    doubled count has its last beat between."""
    if factor == 2.0:
        ends = times if after is None else np.append(times, after)
        return np.sort(np.concatenate([times, (ends[:-1] + ends[1:]) / 2]))
    if factor == 0.5:
        # The half of the beats the downbeats fall on.
        odds = likelihood(found["downbeat"], found["fps"], times)
        return times[int(odds[1::2].sum() > odds[0::2].sum()) :: 2]
    return times


def segmented(found: dict, flux, fps: float, hints=(), asked=()) -> dict | None:
    """The model's beats in segments where their tempo steps (segments.py), or
    None where it keeps one tempo and no step is asked for.

    The model hears a stretch at half its tempo as readily as a whole song, as a
    sparse verse of a 154 song at 77, so its beats step where the song does not.
    Each stretch between its steps is counted: the longest as a song is
    (`count`), each other at half, the same or double, whichever is nearest it.
    A 140 trap song the model hears at 70 that switches to 170 is heard at 85
    there, which a song alone would keep as a slow groove. The steps are then
    those of the counted beats, and those asked for. --bpm gives a tempo for
    each segment in turn, and each stretch of a segment is counted nearest it.

    Gives the counted beats, the beat each segment starts on, and for each
    segment its tempo, whether the model's count is used, how its longest
    stretch was counted, the tempo heard, and whether its step was asked for."""
    from . import segments

    times, _ = heard(found)
    forced = segments.forced_at(times, asked)
    # Steps are read from the beats evened out, so a stretch heard at half time
    # is no step; a step's beat is the heard beat nearest it.
    even = segments.evened(times)
    found_at = sorted({int(np.argmin(np.abs(times - even[j]))) for j in segments.steps_in(even)})
    cuts = segments.joined([j for j in found_at if 0 < j < len(times)], forced, len(times))
    if not cuts:
        return None
    edges = [0, *cuts, len(times)]
    parts = list(zip(edges[:-1], edges[1:]))

    def heard_tempo(k):
        a, b = parts[k]
        return segments.tempo_of(times[a : min(b + 1, len(times))])

    anchor = max(range(len(parts)), key=lambda k: parts[k][1] - parts[k][0])
    a, b = parts[anchor]
    factors = [1.0] * len(parts)
    factors[anchor] = count(times[a:b], flux, fps)
    ref = heard_tempo(anchor) * factors[anchor]
    for k in range(len(parts)):
        if k != anchor:
            options = [f for f in (0.5, 1.0, 2.0) if beats.MIN_BPM <= heard_tempo(k) * f <= beats.MAX_BPM] or [1.0]
            factors[k] = min(options, key=lambda f: abs(np.log(heard_tempo(k) * f / ref)))

    def run():
        """The counted beats, and where each part starts in them."""
        out, at = [], []
        for (a, b), factor in zip(parts, factors):
            at.append(len(out))
            out.extend(counted(found, times[a:b], factor, times[b] if b < len(times) else None))
        return np.array(out), at

    # Neighbours counted at one tempo are one segment, unless a step between
    # them was asked for.
    groups = [[0]]
    for k in range(1, len(parts)):
        same = abs(np.log(heard_tempo(k) * factors[k] / (heard_tempo(k - 1) * factors[k - 1])))
        if same <= np.log(1 + segments.STEP) and parts[k][0] not in forced:
            groups[-1].append(k)
        else:
            groups.append([k])
    if len(groups) == 1:
        return None
    hints = list(hints or [])
    if len(hints) > len(groups):
        raise ValueError(
            f"--bpm gives {len(hints)} tempos and the map has {len(groups)} segments; "
            "give --segment-at for each step"
        )
    for g, group in enumerate(groups):
        if g < len(hints) and hints[g]:
            for k in group:
                factors[k] = count(times[parts[k][0] : parts[k][1]], flux, fps, hints[g])
    beats_out, part_at = run()
    names = {1.0: "as heard", 2.0: "doubled", 0.5: "halved", None: "at --bpm"}
    out = {"beats": beats_out, "starts": [], "bpm": [], "counted": [], "how": [], "heard_bpm": [], "forced": []}
    for g, group in enumerate(groups):
        lo = part_at[group[0]]
        hi = part_at[groups[g + 1][0]] if g + 1 < len(groups) else len(beats_out)
        used = all(factors[k] is not None for k in group)
        longest = max(group, key=lambda k: parts[k][1] - parts[k][0])
        hint = hints[g] if g < len(hints) else None
        out["starts"].append(lo)
        out["bpm"].append(segments.tempo_of(beats_out[lo : min(hi + 1, len(beats_out))]) if used else hint)
        out["counted"].append(used)
        out["how"].append(names[factors[longest]])
        out["heard_bpm"].append(heard_tempo(longest))
        out["forced"].append(g > 0 and parts[group[0]][0] in forced)
    return out


def bar_length(times, downs) -> int:
    """The bar the model most often hears, in its own beats: 4 unless it says otherwise."""
    index = np.searchsorted(times, downs)
    gaps = np.diff(index)
    gaps = gaps[(gaps >= 2) & (gaps <= 12)]
    if len(gaps) < 3:
        return 4
    values, counts = np.unique(gaps, return_counts=True)
    return int(values[np.argmax(counts)])


# --- the beats -----------------------------------------------------------------


def shared(a, b, within=SAME_BEAT) -> float:
    """How many of a's beats have one of b's within `within`, and of b's one of
    a's: the lesser share."""

    def share(x, y):
        if not len(x) or not len(y):
            return 0.0
        j = np.clip(np.searchsorted(y, x), 1, len(y) - 1)
        nearest = np.minimum(np.abs(x - y[j - 1]), np.abs(x - y[j]))
        return float(np.mean(nearest <= within))

    a, b = np.asarray(a, dtype=float), np.asarray(b, dtype=float)
    return min(share(a, b), share(b, a))


def refined(mono, sr, times) -> np.ndarray:
    """Each beat moved to the sharpest attack within 30 ms of it. The model places a
    beat to its 20 ms frame, each on its own, so a beat with no attack near it is
    moved by as much as the others were in the middle, not by its neighbors'."""
    times = np.asarray(times, dtype=float)
    offsets, rises = beats.transients(mono, sr, times, SNAP)
    sharp = rises >= np.log(2)
    if not sharp.any():
        return times
    return times + np.where(sharp, offsets, float(np.median(offsets[sharp])))


def tempo_of(times, mono, sr) -> tuple[np.ndarray, dict]:
    """The tempo of beats the model placed: a line through them, which they are
    when they keep to it, as a song played to a click does."""
    index = np.arange(len(times), dtype=float)
    step, start = np.polyfit(index, times, 1)
    wander = times - (start + step * index)
    steady = float(np.mean(np.abs(wander) <= beats.AGREE_MS / 1000)) >= beats.STEADY_SHARE
    local = 60 / beats._smooth(np.diff(times), 2 * beats.WINDOW + 1)
    offsets, rises = beats.transients(mono, sr, times)
    sharp = rises >= np.log(2)
    if steady:
        times = start + step * index
    found = {
        # A line through a song that changes tempo is no one tempo of it; the beat
        # it keeps most is.
        "bpm": round(float(60 / step if steady else 60 / np.median(np.diff(times))), 4),
        "steady": bool(steady),
        "drift_ms": round(float(np.ptp(wander)) / 2 * 1000, 2),
        "bpm_range": None if steady else [round(float(local.min()), 2), round(float(local.max()), 2)],
        "ambiguous_with": [],
        "transients_fitted": int(sharp.sum()),
        "transients_on_grid": round(float(np.mean(sharp & (np.abs(offsets) <= beats.AGREE_MS / 1000))), 3),
        "fit_spread_ms": None,
    }
    return times, found


# --- the bars ------------------------------------------------------------------


def bars(odds, length: int, forced=()) -> np.ndarray:
    """The beats that start a bar: the path that best agrees with the model's odds
    of a downbeat on each beat, a bar of other than `length` beats costing
    BAR_COST, and every beat in `forced` a downbeat. The song can start anywhere
    in a bar, and its last bar can be cut short."""
    p = np.clip(np.asarray(odds, dtype=float), FLOOR, 1 - FLOOR)
    n, states = len(p), max(2 * length, length + 2)
    on, off = -np.log(p), -np.log(1 - p)
    forced = set(int(i) for i in forced)
    never = 1e12
    # A state is how many beats the bar has had before this one.
    cost = np.full(states, off[0])
    cost[0] = on[0]
    if 0 in forced:
        cost[1:] = never
    turn = np.full(states, BAR_COST)
    turn[length - 1] = 0.0
    back = np.zeros((n, states), dtype=int)
    for i in range(1, n):
        new = np.empty(states)
        new[1:] = cost[:-1] + off[i]
        back[i, 1:] = np.arange(states - 1)
        through = cost + turn
        back[i, 0] = int(np.argmin(through))
        new[0] = through[back[i, 0]] + on[i]
        if i in forced:
            new[1:] = never
        cost = new
    state = int(np.argmin(cost))
    path = [state]
    for i in range(n - 1, 0, -1):
        state = int(back[i, state])
        path.append(state)
    return np.flatnonzero(np.array(path[::-1]) == 0)


def halves(starts, count: int, per_bar: int) -> np.ndarray:
    """Bars of a doubled count cut into bars of `per_bar`: the model's bar of four
    at 70 is two bars of four at 140. Whole bars before the first are added."""
    head = list(range(int(starts[0]) - per_bar, -1, -per_bar))[::-1]
    ends = [*starts[1:], count]
    return np.array(head + [i for s, e in zip(starts, ends) for i in range(int(s), int(e), per_bar)])


def anchored(count: int, anchors, per_bar: int, back=True) -> np.ndarray:
    """Bars of `per_bar` beats from each anchor to the next, and with `back` whole
    bars back from the first."""
    anchors = sorted(set(int(a) for a in anchors))
    head = list(range(anchors[0] - per_bar, -1, -per_bar))[::-1] if back else []
    ends = [*anchors[1:], count]
    return np.array(head + [i for a, e in zip(anchors, ends) for i in range(a, e, per_bar)])


def labels(count: int, starts, per_bar: int) -> list[tuple[int, int, int]]:
    """Each beat's bar, beat and index from the first downbeat. Beats before the
    first downbeat count back from it in bars of `per_bar`: bar 0, then -1."""
    starts = [int(s) for s in starts]
    first = starts[0]
    out = [((i - first) // per_bar + 1, (i - first) % per_bar + 1, i - first) for i in range(first)]
    for bar, (s, e) in enumerate(zip(starts, [*starts[1:], count]), start=1):
        out.extend((bar, i - s + 1, i - first) for i in range(s, e))
    return out


def odd(starts, times, per_bar: int) -> list[dict]:
    """The bars of another length than `per_bar`, but the last, which the song's end can cut."""
    return [
        {"bar": bar, "seconds": round(float(times[s]), 6), "beats": int(e - s)}
        for bar, (s, e) in enumerate(zip(starts[:-1], starts[1:]), start=1)
        if e - s != per_bar
    ]
