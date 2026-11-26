"""Where a song's tempo steps, and the beats of each stretch between.

beats.py measures one grid for a whole song, or follows one that wanders slowly.
A song that switches tempo, a beat switch from 140 to 170 or a band that comes
in faster than its riff, is neither. Here its tempo is cut into segments:

1. **Steps in a run of beats** (`steps_in`): at each beat, the span of the two
   bars before against the two bars after. Where they differ by more than
   STEP, the beat between two lines that fit each side best starts a new tempo.
   A fill or a ritardando shorter than two bars is drift, as is a band
   wandering a percent or two, and so is a stretch at half or double the tempo:
   a half-time switch is the same tempo counted twice as slowly, which is
   settled a segment at a time.
2. **The run of beats.** The downbeat model's beats follow a step as they follow
   anything (downbeats.py), evened out first (`evened`) where it hears a
   stretch at half time. Without it the onset grid follows one tempo, so the
   envelope is read in windows of WINDOW seconds for the tempo each holds, an
   octave folded into one (`rough_steps`), and the beats are followed through
   the file by dynamic programming, each beat near its stretch's tempo
   (`followed`).
3. **Each segment measured again** (`measured`): from half a beat before its
   first beat to half a beat before the next segment's, by the same measurement
   a song of one tempo has: a steady grid placed to a fraction of a
   millisecond where the run's beats are its own, or the run's beats, each
   moved to its attack. Neighbours that come out at one tempo are one
   (`merged`), and the segments' beats are one run across the steps
   (`joined_beats`).

A song with one tempo has one segment and is measured as it always was. Every
value is an estimate from decoded audio; nothing here listens.
"""

from __future__ import annotations

import numpy as np

from . import beats

STEP = 0.04  # a change of the beat's length this large, held for two bars, is a step
HOLD = 8  # beats either side of a step it must hold for: two bars of four
WINDOW, HOP = 8.0, 1.0  # seconds of envelope a rough tempo is read from, and how often
BINS = 48  # rough tempos to an octave, each 1.5% from the next
FINE = 8  # tempos scored within each
JUMP = 1.0  # the cost of a rough tempo leaving the one before, against its windows' fit
GUARD = WINDOW  # seconds either side of a rough step where either tempo may hold


# --- where the steps are -------------------------------------------------------


def steps_in(times, hold=HOLD, step=STEP) -> list[int]:
    """The beats a new tempo starts on in a run of beats: each the first beat of
    `hold` or more at a beat `step` longer or shorter than the `hold` before it,
    or than half or twice it. A run of beats that halves or doubles is the same
    tempo counted another way, as a model hears a sparse verse at half time;
    which way a segment is counted is settled for it.

    A tempo here is the span of `hold` beats, not their median: the model places
    a beat to its 20 ms frame, so the length of one beat is a frame either way,
    and the median of a few of them jumps by a frame, which at 150 BPM is 5%."""
    times = np.asarray(times, dtype=float)
    n = len(times) - 1
    if n < 2 * hold:
        return []
    at = np.arange(hold, n - hold + 1)
    before = np.log((times[at] - times[at - hold]) / hold)
    after = np.log((times[at + hold] - times[at]) / hold)
    jump = np.abs(after - before)
    jump = np.minimum(jump, np.abs(jump - np.log(2)))
    found, free = [], jump > np.log(1 + step)
    while free.any():
        best = int(np.argmax(np.where(free, jump, -1)))
        j = int(at[best])
        # The beat it starts on: where a line through the beats before it and one
        # through those after fit them best, near the spans' peak.
        lo, hi = max(j - hold // 2, hold), min(j + hold // 2, n - hold)
        k = min(range(lo, hi + 1), key=lambda k: _two_lines(times, k, hold))
        # Where the beats themselves put it, the change must still be a step.
        moved = abs(np.log((times[k + hold] - times[k]) / (times[k] - times[k - hold])))
        if min(moved, abs(moved - np.log(2))) > np.log(1 + step):
            found.append(k)
        free &= np.abs(at - j) >= hold
    kept = []
    for j in sorted(set(found)):
        if not kept or j - kept[-1] >= hold:
            kept.append(j)
    return kept


def evened(times, share=0.2) -> np.ndarray:
    """A run of beats with any beat the length of two or three of the song's
    quicker common beat cut into them, so a stretch heard at half time reads as
    the same tempo. The quicker common beat is the beat shorter than all but
    `share` of them: a model hears a sparse stretch at half its tempo more often
    than a busy one at double."""
    times = np.asarray(times, dtype=float)
    if len(times) < 3:
        return times
    gaps = np.diff(times)
    unit = float(np.quantile(gaps, share))
    out = [times[0]]
    for t, gap in zip(times[1:], gaps):
        n = int(round(gap / unit))
        if 2 <= n <= 3 and abs(gap / (n * unit) - 1) < 0.25:
            out.extend(out[-1] + gap * np.arange(1, n) / n)
        out.append(t)
    return np.array(out)


def _two_lines(times, k, hold) -> float:
    """How far `hold` beats either side of beat k are from a line each."""
    total = 0.0
    for part in (times[k - hold : k + 1], times[k : k + hold + 1]):
        index = np.arange(len(part), dtype=float)
        fit = np.polyval(np.polyfit(index, part, 1), index)
        total += float(np.sum((part - fit) ** 2))
    return total


def tempo_of(times) -> float:
    """The tempo of a run of beats, from its span."""
    return float(60 * (len(times) - 1) / (times[-1] - times[0]))


def forced_at(times, asked) -> list[int]:
    """The beat nearest each time a step is asked for at."""
    times = np.asarray(times, dtype=float)
    return sorted({int(np.argmin(np.abs(times - t))) for t in asked if 0 < np.argmin(np.abs(times - t))})


def joined(found, forced, count, hold=HOLD) -> list[int]:
    """Steps found and asked for: one asked for stands for any found within `hold` of it."""
    forced = sorted(set(int(f) for f in forced if 0 < f < count))
    kept = [f for f in found if all(abs(f - g) >= hold for g in forced)]
    return sorted([*kept, *forced])


# --- without the model: rough steps from the envelope ---------------------------


def rough_tempos(envelope, fps):
    """For each window of the envelope, how well each tempo of one octave folded
    fits it: (window centers in seconds, tempos, fit by window and tempo)."""
    size, hop = int(WINDOW * fps), int(HOP * fps)
    folded = beats.MIN_BPM * 2 ** (np.arange(BINS) / BINS)
    # Scored finely, since a lag of four beats moves by frames between two of the
    # bins, and each bin takes the best within it.
    fine = beats.MIN_BPM * 2 ** ((np.arange(BINS * FINE) - FINE // 2) / (BINS * FINE))
    starts = np.arange(0, len(envelope) - size + 1, hop)
    # How alike each window is to itself one, two and four beats later
    # (beats._periodicity), at each tempo and at twice it where that is in range.
    lags = np.array([[k * 60 / b * fps for k in (1, 2, 4)] for b in (*fine, *2 * fine)])
    fit = np.zeros((len(starts), BINS))
    for w, s in enumerate(starts):
        acf = beats._autocorrelation(envelope[s : s + size])
        at = np.interp(lags, np.arange(len(acf)), acf)
        score = np.where((lags < len(acf) - 1).all(axis=1), at.mean(axis=1), -np.inf)
        score[len(fine) :][2 * fine > beats.MAX_BPM] = -np.inf
        best = np.fmax(score[: len(fine)], score[len(fine) :])
        fit[w] = best.reshape(BINS, FINE).max(axis=1)
    return (starts + size / 2) / fps, folded, fit


def rough_steps(envelope, fps) -> list[float]:
    """Seconds where the envelope's tempo, an octave folded, steps by more than STEP
    and holds: a few seconds from where the step is."""
    centers, folded, fit = rough_tempos(envelope, fps)
    if len(centers) < 6:
        return []
    top = np.maximum(fit.max(axis=1, keepdims=True), 1e-9)
    cost = 1 - np.clip(fit, 0, None) / top
    # Viterbi around the circle of the octave: a tempo moves a bin a window for
    # free, as a band drifts, and further at JUMP.
    d = np.abs(np.arange(BINS)[:, None] - np.arange(BINS)[None, :])
    d = np.minimum(d, BINS - d)
    move = np.where(d <= 1, 0.0, JUMP)
    total, back = cost[0].copy(), []
    for row in cost[1:]:
        through = total[:, None] + move
        back.append(np.argmin(through, axis=0))
        total = through.min(axis=0) + row
    path = [int(np.argmin(total))]
    for b in reversed(back):
        path.append(int(b[path[-1]]))
    path = np.array(path[::-1])
    # Runs of the path; a step where a run three windows long sits more than STEP
    # from the run before.
    limit = np.log2(1 + STEP) * BINS
    out, ref, w = [], path[0], 1
    while w < len(path):
        off = (path[w : w + 3] - ref + BINS / 2) % BINS - BINS / 2
        if len(off) == 3 and (np.abs(off) > limit).all():
            out.append(float(centers[w]))
            ref = int(np.median(path[w : w + 3]))
            w += 3
            continue
        w += 1
    # Each stretch long enough to measure on its own.
    kept = []
    for t in out:
        if t - (kept[-1] if kept else 0.0) >= 2 * GUARD:
            kept.append(t)
    return [t for t in kept if centers[-1] - t >= GUARD]


def followed(mono, sr, envelope, cuts, period=None) -> np.ndarray:
    """A run of beats over the whole file through rough steps, a few milliseconds
    from where each beat is.

    Each stretch between rough steps away from them is measured as a song is,
    for its tempo. Then the beats are followed by dynamic programming (Ellis,
    "Beat tracking by dynamic programming", 2007): the path through the onset
    envelope whose beats land on the most onset while each beat's length stays
    near its stretch's, either stretch's near a step, so the path follows a band's
    drift and changes tempo where the onsets do. Without a step, `period` is the
    beat's length in seconds."""
    fps = sr / beats.HOP
    duration = len(envelope) / fps
    if not cuts:
        return np.array(_dynamic(envelope, np.full((len(envelope), 2), period * fps))) / fps
    edges = [0.0, *cuts, duration]
    periods = []
    for k in range(len(edges) - 1):
        lo = edges[k] + (GUARD if k else 0.0)
        hi = edges[k + 1] - (GUARD if k < len(edges) - 2 else 0.0)
        _, _, found = _track(mono, sr, envelope, lo, hi, None)
        periods.append(60 / found["bpm"] * fps)
    # The beat lengths allowed at each frame: a stretch's own, and near a rough
    # step either one.
    allowed = np.zeros((len(envelope), 2))
    for k in range(len(periods)):
        lo = int((edges[k] + (GUARD if k else 0.0)) * fps)
        hi = int((edges[k + 1] - (GUARD if k < len(periods) - 1 else 0.0)) * fps)
        allowed[lo:hi] = periods[k]
        if k + 1 < len(periods):
            zone = slice(hi, int((edges[k + 1] + GUARD) * fps))
            allowed[zone] = (periods[k], periods[k + 1])
    allowed[allowed == 0] = periods[-1]
    return np.array(_dynamic(envelope, allowed)) / fps


TIGHTNESS = 100.0  # how much a beat's length leaving its stretch's costs, by its log


def _dynamic(envelope, allowed) -> list[float]:
    """The frames of the best path of beats through `envelope`, each beat's length
    near one of the two in `allowed` at its frame."""
    onset = envelope / max(float(envelope.std()), 1e-12)
    n = len(onset)
    score = onset.copy()
    back = np.full(n, -1)
    longest = int(np.ceil(allowed.max() * 1.3))
    for t in range(1, n):
        best, at = -np.inf, -1
        for p in set(allowed[t]):
            lo, hi = max(0, t - int(np.ceil(p * 1.3))), t - int(p * 0.7)
            if hi <= lo:
                continue
            tau = np.arange(lo, hi)
            cand = score[lo:hi] - TIGHTNESS * np.log((t - tau) / p) ** 2
            i = int(np.argmax(cand))
            if cand[i] > best:
                best, at = float(cand[i]), lo + i
        # A path starts within its first beat; after that a beat follows one.
        if at >= 0 and (best > 0 or t > longest):
            score[t] += best
            back[t] = at
    last_beat = allowed[-1].max()
    tail = np.arange(max(0, n - int(np.ceil(last_beat))), n)
    t = int(tail[np.argmax(score[tail])])
    path = [t]
    while back[path[-1]] >= 0:
        path.append(int(back[path[-1]]))
    return path[::-1]


# --- each segment measured -------------------------------------------------------


def _track(mono, sr, envelope, lo, hi, hint):
    """beats.track over lo to hi seconds of the file: beats there, in the file's seconds."""
    first = max(0, int(lo * sr) // beats.HOP * beats.HOP)
    last = min(len(mono), int(np.ceil(hi * sr)))
    f0, f1 = first // beats.HOP, int(np.ceil(last / beats.HOP))
    times, coarse, found = beats.track(mono[first:last], sr, envelope[f0:f1], hint)
    shift = first / sr
    keep = (times + shift >= lo) & (times + shift < hi)
    return times[keep] + shift, coarse[keep] + shift, found


def bounds(times, starts, duration) -> list[tuple[float, float]]:
    """Each segment's span: from half a beat before its first beat to half a beat
    before the next segment's, the first from 0 and the last to the end."""
    times = np.asarray(times, dtype=float)
    cut = [(times[s - 1] + times[s]) / 2 for s in starts[1:]]
    return list(zip([0.0, *cut], [*cut, duration]))


def measured(mono, sr, envelope, spans, hints, model_beats=None, model_counted=None, label="model"):
    """Each span's beats measured on their own: (times, coarse, tempo report) each.

    With the model's beats, or beats followed one by one (`label`), a span's onset
    grid is kept where its beats are those, and those, moved to their attacks,
    are taken where they are not. `model_counted` says, for each span, whether
    the model's count there is used (False where --bpm asked for a tempo none of
    its counts is near)."""
    from . import downbeats

    out = []
    for k, (lo, hi) in enumerate(spans):
        times, coarse, found = _track(mono, sr, envelope, lo, hi, hints[k])
        found = {**found, "beats_from": "onsets"}
        if model_beats is not None:
            heard = model_beats[(model_beats >= lo) & (model_beats < hi)]
            counted = model_counted[k] if model_counted is not None else True
            agreement = downbeats.shared(heard, times) if counted and len(heard) else None
            found["agreement"] = None if agreement is None else round(agreement, 3)
            if agreement is not None and agreement < downbeats.SAME_SHARE and len(heard) >= 4:
                moved = downbeats.refined(mono, sr, heard) if label == "model" else heard
                times, model_found = downbeats.tempo_of(moved, mono, sr)
                coarse = times.copy()
                found = {**model_found, "beats_from": label, "agreement": round(agreement, 3)}
        out.append((times, coarse, found))
    return out


def merged(mono, sr, envelope, parts, spans, split):
    """Neighbouring segments measured at one tempo made one, and measured again
    over both, unless a step between them was asked for: two stretches can come
    out at one tempo though their beats were counted two ways. `split` holds a
    list for each segment, which loses the second of two merged."""
    parts, spans = list(parts), list(spans)
    split = {**split, **{k: list(v) for k, v in split.items() if k in PER_SEGMENT}}
    seg = split.get("model")
    if seg is not None:
        seg = {**seg, **{k: list(v) for k, v in seg.items() if k in PER_SEGMENT}}
    k = 1
    while k < len(parts):
        a, b = parts[k - 1][2]["bpm"], parts[k][2]["bpm"]
        if not split["forced"][k] and abs(np.log(a / b)) <= np.log(1 + STEP):
            span = (spans[k - 1][0], spans[k][1])
            if seg is None:
                again = measured(mono, sr, envelope, [span], [split["hints"][k - 1]], split.get("followed"), label="followed")
            else:
                counted = [seg["counted"][k - 1] and seg["counted"][k]]
                again = measured(mono, sr, envelope, [span], [split["hints"][k - 1]], seg["beats"], counted)
            parts[k - 1 : k + 1] = again
            spans[k - 1 : k + 1] = [span]
            for lists in (split, seg or {}):
                for key in PER_SEGMENT:
                    if key in lists:
                        del lists[key][k]
            k = max(1, k - 1)
            continue
        k += 1
    if seg is not None:
        split["model"] = seg
    return parts, spans, split


# The lists a split has one entry in for each segment.
PER_SEGMENT = ("starts", "hints", "forced", "bpm", "counted", "how", "heard_bpm")


def joined_beats(parts):
    """The segments' beats as one run, each segment from the beat it starts on:
    a beat too near the one before is dropped, and a gap of two beats or more is
    filled evenly."""
    times, coarse, starts = [], [], []
    for t, c, _ in parts:
        if len(t) == 0:
            starts.append(len(times))
            continue
        if times:
            p = float(np.median(np.diff(t[:8]))) if len(t) > 1 else t[0] - times[-1]
            prev = float(np.median(np.diff(times[-8:]))) if len(times) > 1 else p
            gap = t[0] - times[-1]
            if gap < 0.5 * min(p, prev):
                t, c = t[1:], c[1:]
            elif gap > 1.5 * max(p, prev):
                count = int(round(gap / ((p + prev) / 2)))
                extra = times[-1] + gap * np.arange(1, count) / count
                times.extend(extra)
                coarse.extend(extra)
        starts.append(len(times))
        times.extend(t)
        coarse.extend(c)
    return np.array(times), np.array(coarse), starts


def summary(parts) -> dict:
    """The tempo of a song in segments: the first segment's tempo, not steady, the
    largest drift, and the range of all; a song of one segment has its own."""
    tempos = [f for _, _, f in parts]
    first = tempos[0]
    keys = ("bpm", "steady", "drift_ms", "bpm_range", "ambiguous_with", "transients_fitted",
            "transients_on_grid", "fit_spread_ms")
    if len(tempos) == 1:
        return {k: first[k] for k in keys}
    lows = [(f["bpm_range"] or [f["bpm"], f["bpm"]])[0] for f in tempos]
    highs = [(f["bpm_range"] or [f["bpm"], f["bpm"]])[1] for f in tempos]
    sharp = [f["transients_fitted"] for f in tempos]
    on = [f["transients_on_grid"] * max(n, 1) for f, n in zip(tempos, sharp)]
    spreads = [f["fit_spread_ms"] for f in tempos if f["fit_spread_ms"] is not None]
    return {
        "bpm": first["bpm"],
        "steady": False,
        "drift_ms": max(f["drift_ms"] for f in tempos),
        "bpm_range": [round(float(min(lows)), 2), round(float(max(highs)), 2)],
        "ambiguous_with": first["ambiguous_with"],
        "transients_fitted": int(sum(sharp)),
        "transients_on_grid": round(float(sum(on) / max(sum(max(n, 1) for n in sharp), 1)), 3),
        "fit_spread_ms": max(spreads) if spreads else None,
    }


def ambiguous(bpm, neighbours, own=()) -> list[float]:
    """Half and double a segment's tempo in range, and a neighbour's tempo where the
    segment is at half or double it: a half-time switch is often the same tempo."""
    out = {round(float(b), 2) for b in (bpm / 2, bpm * 2) if beats.MIN_BPM <= b <= beats.MAX_BPM}
    out |= {round(float(b), 2) for b in own}
    for n in neighbours:
        if any(abs(np.log(n / (bpm * f))) <= 0.04 for f in (0.5, 2.0)):
            out.add(round(float(n), 2))
    out = {b for b in out if abs(np.log(b / bpm)) > 0.04}
    return sorted(out)


def report(times, starts, parts, rows, per_bar, set_by) -> list[dict]:
    """The segments as the map gives them: each from the bar line nearest the beat
    its tempo starts on, with its seconds, tempo, steadiness and drift."""
    downs = [i for i, r in enumerate(rows) if r["beat"] == 1]
    out = []
    bpms = [f["bpm"] for _, _, f in parts]
    for k, (s, (_, _, found)) in enumerate(zip(starts, parts)):
        s = min(s, len(rows) - 1)
        at = min(downs, key=lambda d: abs(d - s)) if downs and k else s
        if k and abs(at - s) > per_bar / 2:
            at = s
        neighbours = [bpms[j] for j in (k - 1, k + 1) if 0 <= j < len(bpms)]
        out.append(
            {
                "bar": rows[at]["bar"],
                "seconds": rows[at]["seconds"],
                "beat_seconds": round(float(times[s]), 6),
                "bpm": found["bpm"],
                "steady": found["steady"],
                "drift_ms": found["drift_ms"],
                "bpm_range": found["bpm_range"],
                "ambiguous_with": ambiguous(found["bpm"], neighbours, found.get("ambiguous_with", [])),
                "beats_from": found.get("beats_from", "onsets"),
                "set_by": set_by[k],
            }
        )
    return out
