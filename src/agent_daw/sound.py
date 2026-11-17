"""How a song sounds, part by part: `daw samples sound`.

Each part `daw separate` made of a song, and the song itself, is read for what a
producer sets when mixing it:

1. **Tone.** Its spectrum in third octaves against a flat one, pink noise, and
   the equalizer that turns flat into it, as the bands the `eq` effect takes:
   shelves, bells and the passes where it ends. With `--against` a part made
   in a session, the bands that turn that part into this one.
2. **Width.** How much of it is in the sides, whole and section by section,
   with the lows, the mids and the highs apart.
3. **Pumping.** Its level dipping each time the kick hits and coming back:
   the depth, when the dip is deepest and how long it takes to come back,
   which are a sidechained compressor's reduction and release. Read against
   the same part a half kick later, where nothing ducks it.
4. **Tails.** Where the part stops, as a voice at a phrase's end: how long
   what is left takes to fall 60 dB, which is a reverb's decay, and how far
   under the sound it starts; and repeats of the last notes in time, which
   are a delay's: their spacing in beats and how much each repeat falls.
5. **Dynamics.** How far its peaks stand over its level, how far its level
   moves, and how much of its time it spends near its loudest: the marks a
   compressor or a limiter leaves.

Each is an estimate from decoded audio, checked on renders the project made
with known settings; nothing here listens, and a separated part carries what
the separation left in it.
"""

from __future__ import annotations

import warnings
from pathlib import Path

import numpy as np
from scipy.signal import butter, sosfiltfilt

from . import beats as beatmap
from . import hits, spectrum

# --- tone -----------------------------------------------------------------------

FIT_HZ = (25.0, 16000.0)  # the third octaves the equalizer is fitted over
FLOOR_DB = 30.0  # a part plays in the third octaves within this of its loudest
EDGE_BANDS = 2  # and this many either side say where it ends
EDGE_WEIGHT = 0.3
MAX_BANDS = 8
GOOD_DB = 0.5  # the fit stops when its error is this small
GAIN_DB = 0.1  # or a band improves it by less than this
QS = (0.5, 1.0, 2.0, 4.0)
MOST_GAIN_DB = 15.0  # a band moves no more than this, and is no narrower or wider than
WIDEST_Q, NARROWEST_Q = 0.3, 10.0
SHELF_HZ = (500.0, 2000.0)  # a high shelf over the first, a low one under the second
GAINS = np.arange(-MOST_GAIN_DB, MOST_GAIN_DB + 0.01, 0.5)
SLOPES = (12, 24)

# --- width ----------------------------------------------------------------------

WIDTH_BANDS = {"low": (20.0, 250.0), "mid": (250.0, 4000.0), "high": (4000.0, 20000.0)}

# --- pumping --------------------------------------------------------------------

PUMP_BEFORE = (-0.060, -0.015)  # a part's level before a kick, seconds from it
DIP_WITHIN = 0.200  # the dip is looked for this long after the kick
PUMP_SPAN = 0.600  # and the part followed this long, or to the next kick
LEAST_GAP = 0.150  # kicks closer than this to the next are not read
SOUNDING_DB = -30.0  # a part this far under its loudest is not playing
LEAST_DIP = 1.0  # a dip under this many dB is no pumping
OVER_CONTROL = 1.0  # and it is this much deeper than half a kick later
LEAST_KICKS = 8
MIX_BAND = (300.0, 2000.0)  # the song's own dip is read over the kick's body, under the hats

# --- tails ----------------------------------------------------------------------

TAIL_BAND = (200.0, 6000.0)  # the level a tail is read in
STEP = 0.010  # the level followed this often for tails, seconds
BEFORE_END = 0.300  # an end follows this long of the part sounding
FALL_DB = 6.0  # and the level falling this far within
FALL_WITHIN = 0.150
LEAST_REST = 0.400  # and the part not coming back for this long
LONGEST_TAIL = 3.0
RISE_DB = 6.0  # a tail ends where the level rises this far over its lowest
LEAST_TAIL = 0.250  # a decay is fitted over at least this long
LEAST_FALL_DB = 5.0  # and this far
OVER_FLOOR_DB = 6.0  # down to this far over the part's floor
LEAST_ENDS = 3
AGREE_RATIO = 1.6  # a tail's decays agree when the middle half are within this ratio
DEEPEST_DB = 50.0  # a tail is fitted down to this far under the sound
LEAST_STRAIGHT_DB = 3.0  # and its decay is a line to within this
ECHO_LAGS = (0.100, 2.000)  # the spacing of repeats looked for
ECHO_WINDOW = 0.500  # the start of a tail compared with what it may repeat
ECHO_LEAST_STEPS = 10  # steps of a tail over the floor, at least, to compare
ECHO_ALIKE = 0.5  # a tail this alike, as a correlation, to the stretch a spacing before
ECHO_OVER = 0.3  # and this much more than at the typical spacing, repeats it
ECHO_QUIETER_DB = 3.0  # when the repeat is at least this much quieter
DIVISIONS = {
    "1/16": 0.25, "1/8 triplet": 1 / 3, "1/8": 0.5, "dotted 1/8": 0.75, "1/4 triplet": 2 / 3,
    "1/4": 1.0, "dotted 1/4": 1.5, "1/2": 2.0,
}  # fmt: skip

# --- dynamics -------------------------------------------------------------------

SHORT = 0.050  # the short-term level's window
CREST_WINDOW = 0.400
NEAR_TOP_DB = 3.0

EPS = 1e-20


def db(power):
    return 10 * np.log10(np.asarray(power, dtype=np.float64) + EPS)


def r(value, places=1):
    return None if value is None or not np.isfinite(value) else round(float(value), places) + 0.0


def read_stereo(path: Path):
    import soundfile as sf

    x, sr = sf.read(path, always_2d=True, dtype="float32")
    if not len(x) or not np.isfinite(x).all():
        raise ValueError(f"Empty or nonfinite audio: {path}")
    if x.shape[1] > 2:
        x = x[:, :2]
    return x, sr


# --- tone -----------------------------------------------------------------------


def response(shape: str, freq: float, gain: float, q: float, hz: np.ndarray, sr=48000.0, slope=12):
    """An `eq` band's level in dB at `hz`, as the engine's biquads make it."""
    w0 = 2 * np.pi * freq / sr
    cos, alpha = np.cos(w0), np.sin(w0) / (2 * q)
    a = 10 ** (gain / 40)
    z = np.exp(-1j * 2 * np.pi * hz / sr)

    def section(b, den):
        return (b[0] + b[1] * z + b[2] * z**2) / (den[0] + den[1] * z + den[2] * z**2)

    if shape == "bell":
        h = section([1 + alpha * a, -2 * cos, 1 - alpha * a], [1 + alpha / a, -2 * cos, 1 - alpha / a])
    elif shape in ("low_shelf", "high_shelf"):
        root = 2 * np.sqrt(a) * alpha
        s = 1 if shape == "low_shelf" else -1
        b = [
            a * ((a + 1) - s * (a - 1) * cos + root),
            s * 2 * a * ((a - 1) - s * (a + 1) * cos),
            a * ((a + 1) - s * (a - 1) * cos - root),
        ]
        den = [
            (a + 1) + s * (a - 1) * cos + root,
            -s * 2 * ((a - 1) + s * (a + 1) * cos),
            (a + 1) + s * (a - 1) * cos - root,
        ]
        h = section(b, den)
    else:
        # A pass of slope/6 poles, Butterworth: one second-order section a 12 dB.
        order = slope // 6
        h = np.ones_like(z)
        for k in range(order // 2):
            qk = 1 / (2 * np.cos(np.pi * (2 * k + 1) / (2 * order)))
            al = np.sin(w0) / (2 * qk)
            if shape == "highpass":
                b = [(1 + cos) / 2, -(1 + cos), (1 + cos) / 2]
            else:
                b = [(1 - cos) / 2, 1 - cos, (1 - cos) / 2]
            h = h * section(b, [1 + al, -2 * cos, 1 - al])
    return 20 * np.log10(np.abs(h) + 1e-12)


def banded(shape, freq, gain, q, hz, slope=12) -> np.ndarray:
    """A band's level in dB over each third octave about `hz`, its power
    averaged across the band, as the spectrum is measured."""
    spread = 2.0 ** (np.linspace(-1, 1, 7) * spectrum.BAND_OCTAVES / 2)
    at = np.outer(hz, spread).ravel()
    power = 10 ** (response(shape, freq, gain, q, at, slope=slope) / 10)
    return 10 * np.log10(power.reshape(len(hz), -1).mean(axis=1))


def curve_of(levels) -> tuple[np.ndarray, np.ndarray, np.ndarray, tuple]:
    """The third octaves fitted, as (their centers, levels in dB against the
    median of those it plays in, weights, the range it plays in). It plays in
    the third octaves within FLOOR_DB of its loudest; the EDGE_BANDS outside
    that range on either side are kept, counting less, for where it ends."""
    centers = np.array(spectrum.CENTERS, dtype=float)
    keep = (centers >= FIT_HZ[0]) & (centers <= FIT_HZ[1])
    level = np.array([np.nan if v is None else v for v in levels], dtype=float)
    keep &= np.isfinite(level)
    if keep.sum() < 8:
        return centers[:0], level[:0], level[:0], (None, None)
    hz, level = centers[keep], level[keep]
    playing = np.flatnonzero(level > level.max() - FLOOR_DB)
    first, last = int(playing[0]), int(playing[-1])
    kept = slice(max(first - EDGE_BANDS, 0), last + EDGE_BANDS + 1)
    inside = np.zeros(len(hz), bool)
    inside[first : last + 1] = True
    weight = np.where(inside, 1.0, EDGE_WEIGHT)[kept]
    hz, level = hz[kept], level[kept]
    return hz, level - np.median(level[inside[kept]]), weight, (float(centers[keep][first]), float(centers[keep][last]))


def candidates(hz):
    """Every band the fit may add, with its response at `hz`: a table of rows."""
    rows, table = [], []
    # Corners and centers on sixth octaves, between the bands' centers too.
    for f in np.geomspace(hz[0], hz[-1], 2 * len(hz) - 1):
        f = float(round(f, -1 if f < 1000 else -2)) if f >= 100 else float(round(f))
        for q in QS:
            for g in GAINS:
                if g == 0:
                    continue
                rows.append(("bell", f, g, q, None))
                table.append(banded("bell", f, g, q, hz))
        for shape in ("low_shelf", "high_shelf"):
            # A low shelf is under 2 kHz and a high one over 500 Hz: with the
            # overall gain free, each is the other turned over.
            if (shape == "low_shelf" and f > SHELF_HZ[1]) or (shape == "high_shelf" and f < SHELF_HZ[0]):
                continue
            for g in GAINS:
                if g == 0:
                    continue
                rows.append((shape, f, g, 0.71, None))
                table.append(banded(shape, f, g, 0.71, hz))
        for shape in ("highpass", "lowpass"):
            if (shape == "highpass" and f > 2000) or (shape == "lowpass" and f < 100):
                continue
            for slope in SLOPES:
                rows.append((shape, f, 0.0, 0.71, slope))
                table.append(banded(shape, f, 0.0, 0.71, hz, slope=slope))
    return rows, np.array(table)


_TABLES: dict = {}


def fit(hz, target, weight, span=(None, None)) -> tuple[list[dict], float, float]:
    """Bands, one at a time, each the one from the table that most lowers the
    weighted error of what is left, and then all of them moved together to
    the least error; with the overall gain, as (bands, gain_db, error_db)."""
    if not len(hz):
        return [], 0.0, None
    key = tuple(np.round(hz, 1))
    if key not in _TABLES:
        _TABLES[key] = candidates(hz)
    rows, table = _TABLES[key]
    # Bells and shelves within the range the part plays in; passes anywhere.
    low, high = span[0] or hz[0], span[1] or hz[-1]
    allowed = np.array([shape in ("highpass", "lowpass") or low <= f <= high for shape, f, *_ in rows])
    rows = [row for row, ok in zip(rows, allowed) if ok]
    table = table[allowed]
    w = weight / weight.sum()

    def error(left):
        # The overall gain is free: what is left is taken about its weighted mean.
        mean = (left * w).sum(axis=-1, keepdims=True)
        return np.sqrt((((left - mean) ** 2) * w).sum(axis=-1)), mean[..., 0]

    def total(bands):
        return sum((banded(s, f, g, q, hz, slope or 12) for s, f, g, q, slope in bands), np.zeros(len(hz)))

    def polish(bands):
        """The bands' frequencies, gains and widths moved together to the
        least error, from where the table put them."""
        from scipy.optimize import least_squares

        x0, lo, hi = [], [], []
        for shape, f, g, q, slope in bands:
            if shape in ("highpass", "lowpass"):
                x0.append(np.log(f)), lo.append(np.log(20.0)), hi.append(np.log(20000.0))
            else:
                floor_hz = max(low, SHELF_HZ[0]) if shape == "high_shelf" else low
                top_hz = min(high, SHELF_HZ[1]) if shape == "low_shelf" else high
                x0.append(np.log(f)), lo.append(np.log(min(floor_hz, f))), hi.append(np.log(max(top_hz, f)))
                x0 += [g, np.log(q)]
                lo += [-MOST_GAIN_DB, np.log(WIDEST_Q)]
                hi += [MOST_GAIN_DB, np.log(NARROWEST_Q)]

        def unpack(x):
            out, k = [], 0
            for shape, f, g, q, slope in bands:
                if shape in ("highpass", "lowpass"):
                    out.append((shape, float(np.exp(x[k])), 0.0, 0.71, slope))
                    k += 1
                else:
                    out.append((shape, float(np.exp(x[k])), float(x[k + 1]), float(np.exp(x[k + 2])), slope))
                    k += 3
            return out

        sw = np.sqrt(w)

        def residual(x):
            left = target - total(unpack(x))
            return (left - (left * w).sum()) * sw

        x0 = np.clip(x0, np.array(lo) + 1e-9, np.array(hi) - 1e-9)
        found = least_squares(residual, x0, bounds=(lo, hi), max_nfev=200)
        return unpack(found.x)

    bands: list[tuple] = []
    now, _ = error(target)
    barred = np.zeros(len(rows), bool)
    while len(bands) < MAX_BANDS and now > GOOD_DB:
        tried, _ = error(target - total(bands) - table)
        tried[barred] = np.inf
        best = int(np.argmin(tried))
        if not np.isfinite(tried[best]) or now - tried[best] < GAIN_DB:
            break
        moved = polish([*bands, rows[best]])
        # A band moved onto one of its shape already there says nothing new:
        # it is not tried again, and the next best is.
        new = moved[-1]
        if any(b[0] == new[0] and abs(np.log2(b[1] / new[1])) < 1 / 6 for b in moved[:-1]):
            barred[best] = True
            continue
        bands = moved
        now, _ = error(target - total(bands))
    _, gain = error(target - total(bands))
    out = []
    for shape, f, g, q, slope in bands:
        band = {"shape": shape, "freq_hz": r(f, 0)}
        if shape in ("highpass", "lowpass"):
            band["slope_db_per_octave"] = slope
        else:
            band["gain_db"] = r(g)
            band["q"] = r(q, 2)
        out.append(band)
    order = {"highpass": 0, "low_shelf": 1, "bell": 2, "high_shelf": 3, "lowpass": 4}
    out.sort(key=lambda b: (order[b["shape"]], b["freq_hz"]))
    return out, float(gain), float(now)


def tone(levels, against=None) -> dict:
    """A part's tone: its third octaves, its tilt, the range it plays in, and
    the equalizer that turns flat, or `against`'s third octaves, into it."""
    hz, level, weight, span = curve_of(levels)
    out = {
        "third_octaves_db": [None if v is None else r(v) for v in levels],
        "tilt_db_per_octave": spectrum.tilt(levels),
        "plays_hz": None if span[0] is None else [r(span[0], 0), r(span[1], 0)],
    }
    if not len(hz):
        out["eq"] = None
        return out
    target = level
    if against is not None:
        hz_b, level_b, weight_b, span_b = curve_of(against)
        common = np.intersect1d(hz, hz_b)
        if len(common) < 8:
            out["eq"] = None
            out["eq_why"] = "the two play in too few of the same third octaves to compare"
            return out
        target = level[np.isin(hz, common)] - level_b[np.isin(hz_b, common)]
        weight = np.minimum(weight[np.isin(hz, common)], weight_b[np.isin(hz_b, common)])
        hz = common
        span = (max(span[0], span_b[0]), min(span[1], span_b[1]))
    bands, gain, error = fit(hz, target, weight, span)
    out["eq"] = {
        "from": "pink noise, flat in third octaves" if against is None else "the part given with --against",
        "bands": bands,
        "error_db": r(error, 2),
    }
    return out


def eq_curve(bands, hz, sr=48000.0) -> np.ndarray:
    total = np.zeros(len(hz))
    for b in bands:
        total += response(
            b["shape"], b["freq_hz"], b.get("gain_db", 0.0), b.get("q", 0.71), np.asarray(hz, float), sr,
            b.get("slope_db_per_octave", 12),
        )
    return total


# --- width ----------------------------------------------------------------------


def stereo(x) -> dict:
    """The side's share of the power and the channels' correlation."""
    if x.shape[1] < 2:
        return {"side_share": 0.0, "correlation": 1.0}
    left, right = x[:, 0].astype(np.float64), x[:, 1].astype(np.float64)
    mid, side = (left + right) / 2, (left - right) / 2
    pm, ps = float(np.mean(mid * mid)), float(np.mean(side * side))
    if pm + ps <= 1e-14:
        return {"side_share": None, "correlation": None}
    el, er = float(np.sum(left * left)), float(np.sum(right * right))
    corr = float(np.sum(left * right) / np.sqrt(el * er)) if el > 0 and er > 0 else None
    return {"side_share": r(ps / (pm + ps), 3), "correlation": r(corr, 3)}


def width(x, sr, sections) -> dict:
    out = stereo(x)
    if x.shape[1] == 2:
        by_band = {}
        for name, (low, high) in WIDTH_BANDS.items():
            high = min(high, 0.45 * sr)
            sos = butter(4, [low, high], "bandpass", fs=sr, output="sos") if low > 20 else butter(4, high, "lowpass", fs=sr, output="sos")
            y = sosfiltfilt(sos, x.astype(np.float64), axis=0)
            by_band[name] = stereo(y)["side_share"]
        out["side_share_by_band"] = by_band
    if sections:
        rows = []
        for s in sections:
            piece = x[int(s["seconds"] * sr) : int(s["until_seconds"] * sr)]
            if not len(piece):
                continue
            rows.append({"section": s["label"], "name": s.get("name"), "bars": s["bars"], **stereo(piece)})
        out["by_section"] = rows
    return out


# --- level ----------------------------------------------------------------------


def short_levels(mono, sr, size=SHORT) -> np.ndarray:
    n = max(int(sr * size), 1)
    count = len(mono) // n
    if not count:
        return np.zeros(0)
    power = np.mean(np.square(mono[: count * n].astype(np.float64)).reshape(count, n), axis=1)
    return db(power)


def dynamics(mono, sr) -> dict:
    """How far the peaks stand over the level, how far the level moves, and
    how much of the time it is near its loudest, over the stretches it sounds."""
    levels = short_levels(mono, sr)
    if not len(levels) or levels.max() < -100:
        return {"sounding": False}
    top = float(np.percentile(levels, 99))
    sounding = levels > top + SOUNDING_DB
    n = max(int(sr * CREST_WINDOW), 1)
    crests = []
    for i in range(0, len(mono) - n + 1, n):
        piece = mono[i : i + n].astype(np.float64)
        rms = np.sqrt(np.mean(piece * piece))
        if rms > 0 and 20 * np.log10(rms) > top + SOUNDING_DB:
            crests.append(20 * np.log10(np.abs(piece).max() / rms))
    heard = levels[sounding]
    p95 = float(np.percentile(heard, 95))
    return {
        "sounding": True,
        "crest_db": r(np.median(crests)) if crests else None,
        "level_range_db": r(p95 - np.percentile(heard, 10)),
        "near_top_share": r(float(np.mean(heard >= p95 - NEAR_TOP_DB)), 2),
        "peak_dbfs": r(20 * np.log10(np.abs(mono).max() + 1e-12)),
        "level_dbfs": r(10 * np.log10(np.mean(np.power(10.0, heard / 10)))),
    }


# --- pumping --------------------------------------------------------------------


def follow(mono, sr, low=40.0, high=16000.0):
    """The level a millisecond at a time, as `daw samples hits` follows a band."""
    return hits.level(mono, sr, low, high)


def duck_curve(env, step, times, ends, top) -> tuple[np.ndarray, np.ndarray, list]:
    """The level after each time against the level just before it, the median
    over the times the part sounds at, as (seconds from the time, dB), and each
    time's own deepest dip."""
    pre = (int(round(PUMP_BEFORE[0] / step)), int(round(PUMP_BEFORE[1] / step)))
    span = int(round(PUMP_SPAN / step))
    lead = int(round(0.05 / step))
    rows, dips = [], []
    for t, end in zip(times, ends):
        i = int(round(t / step))
        if i + pre[0] < 0 or i + span >= len(env):
            continue
        ref = float(np.median(env[i + pre[0] : i + pre[1]]))
        if ref < top + SOUNDING_DB:
            continue
        row = np.full(lead + span, np.nan)
        stop = min(span, int((end - t) / step))
        row[: lead + stop] = env[i - lead : i + stop] - ref
        rows.append(row)
        dips.append(float(np.nanmin(row[lead : lead + int(DIP_WITHIN / step)])))
    if not rows:
        return np.zeros(0), np.zeros(0), []
    rows = np.array(rows)
    counted = np.sum(np.isfinite(rows), axis=0)
    with warnings.catch_warnings():
        # A moment no kick reached that far is NaN in every row, and stays NaN.
        warnings.simplefilter("ignore", RuntimeWarning)
        median = np.nanmedian(rows, axis=0)
    median[counted < max(3, len(rows) // 2)] = np.nan
    return (np.arange(-lead, span) * step), median, dips


def pumping(mono, sr, kicks: np.ndarray, beat: float | None, band=(40.0, 16000.0)) -> dict:
    """The part's level dipping each time the kick hits: the dip's depth, when
    it is deepest, how long it takes to come most of the way back, and the
    same read half a kick later, where a sidechain does not duck."""
    kicks = np.sort(np.asarray(kicks, dtype=float))
    if len(kicks) < LEAST_KICKS:
        return {"heard": False, "why": f"{len(kicks)} kicks; at least {LEAST_KICKS} are needed"}
    env, step = follow(mono, sr, *band)
    top = float(np.percentile(env, 99))
    nxt = np.append(kicks[1:], kicks[-1] + PUMP_SPAN)
    keep = nxt - kicks >= LEAST_GAP
    times, ends = kicks[keep], nxt[keep]
    tau, curve, dips = duck_curve(env, step, times, ends, top)
    halves = times + (ends - times) / 2
    controls_end = ends
    _, control, _ = duck_curve(env, step, halves, controls_end, top)
    if not len(dips) or not np.isfinite(curve).any():
        return {"heard": False, "why": "the part is not playing where the kick hits"}
    after = (tau >= 0) & (tau <= DIP_WITHIN) & np.isfinite(curve)
    k = np.flatnonzero(after)[int(np.argmin(curve[after]))]
    depth = -float(curve[k])
    control_after = control[: len(after)][after] if len(control) else np.zeros(0)
    control_depth = -float(np.nanmin(control_after)) if len(control_after) and np.isfinite(control_after).any() else 0.0
    out = {
        "depth_db": r(max(depth, 0.0)),
        "at_ms": r(tau[k] * 1000, 0),
        "half_a_kick_later_db": r(max(control_depth, 0.0)),
        "kicks_read": len(dips),
        "share_dipping": r(float(np.mean(np.array(dips) <= -max(depth, LEAST_DIP) / 2)), 2),
    }
    heard = depth >= LEAST_DIP and depth - max(control_depth, 0.0) >= OVER_CONTROL
    out["heard"] = bool(heard)
    if heard:
        # The release: from the deepest point to where the dip is a factor e
        # shallower, as a compressor's release is set.
        rest = curve[k:]
        back = np.flatnonzero(np.isfinite(rest) & (rest >= -depth / np.e))
        if len(back):
            out["release_ms"] = r(back[0] * step * 1000, 0)
        else:
            out["release_ms"] = None
            out["release_why"] = "the part does not come back before the next kick"
        if beat:
            out["release_beats"] = None if out["release_ms"] is None else r(out["release_ms"] / 1000 / beat, 2)
    out["curve"] = {
        "ms": [r(t * 1000, 0) for t in tau[:: int(round(0.01 / step))]],
        "db": [r(v) for v in curve[:: int(round(0.01 / step))]],
    }
    return out


# --- tails ----------------------------------------------------------------------


def tail_level(mono, sr):
    """The level in TAIL_BAND every STEP, in dB."""
    x = sosfiltfilt(butter(4, [TAIL_BAND[0], min(TAIL_BAND[1], 0.45 * sr)], "bandpass", fs=sr, output="sos"), mono.astype(np.float64))
    n = int(round(STEP * sr))
    count = len(x) // n
    power = np.mean(np.square(x[: count * n]).reshape(count, n), axis=1)
    # Each step over twice its length, so a level does not jump between steps.
    smooth = np.convolve(power, np.ones(3) / 3, mode="same")
    return db(smooth)


def ends_of(level) -> list[int]:
    """Where the part stops: the level falls FALL_DB within FALL_WITHIN after
    BEFORE_END of sounding, and stays 3 dB under where it was for LEAST_REST,
    so a gap between syllables is not an end."""
    before = int(round(BEFORE_END / STEP))
    within = int(round(FALL_WITHIN / STEP))
    rest = int(round(LEAST_REST / STEP))
    top = float(np.percentile(level, 99))
    out, i = [], before
    while i < len(level) - within - 1:
        here = level[i]
        if (
            here > top + SOUNDING_DB / 1.5
            and np.min(level[i - before : i + 1]) > top + SOUNDING_DB
            and here >= np.max(level[i : i + 4]) - 0.5
            and level[i + within] <= here - FALL_DB
            and np.max(level[i + within : i + within + rest]) < here - 3.0
        ):
            # The end is where the sound stops: the steepest step of the fall,
            # not a dip between syllables before it.
            i += max(int(np.argmin(np.diff(level[i : i + within + 2]))), 0)
            out.append(i)
            # What follows until the part plays again, back near where it
            # stopped, is this end's tail, and its repeats are not ends.
            back = np.flatnonzero(level[i + within :] >= here - 3.0)
            i += within + (int(back[0]) if len(back) else len(level))
            continue
        i += 1
    return out


def decay_of(level, i, floor) -> dict | None:
    """A tail's decay from the end at `i`: a fast fall, the sound stopping,
    then a slower one, the space it was in; fitted as two lines, the second's
    slope the decay. None when the tail is too short or does not fall far."""
    stop = min(len(level), i + int(round(LONGEST_TAIL / STEP)))
    seg = level[i:stop]
    if len(seg) < 10:
        return None
    lowest = np.minimum.accumulate(seg)
    rises = np.flatnonzero(seg > lowest + RISE_DB)
    if len(rises):
        seg = seg[: rises[0]]
    # Down to the part's floor, and no further than DEEPEST_DB under the sound:
    # a tail is read as a reverb's is measured, over its first tens of dB.
    under = np.flatnonzero((seg <= floor + OVER_FLOOR_DB) | (seg <= seg[0] - DEEPEST_DB))
    if len(under):
        seg = seg[: under[0] + 1]
    n = len(seg)
    least = int(round(LEAST_TAIL / STEP))
    if n < least + 3:
        return None
    t = np.arange(n) * STEP
    best = None
    # The decay is the longer part: the sound stopping takes the first half at most.
    for knee in range(2, min(n - least, n // 2 + 1)):
        a = np.polyfit(t[: knee + 1], seg[: knee + 1], 1) if knee >= 2 else None
        b = np.polyfit(t[knee:], seg[knee:], 1)
        if b[0] >= 0:
            continue
        err = np.sum((np.polyval(b, t[knee:]) - seg[knee:]) ** 2)
        if a is not None:
            err += np.sum((np.polyval(a, t[: knee + 1]) - seg[: knee + 1]) ** 2)
        if best is None or err < best[0]:
            best = (err, knee, b)
    if best is None:
        return None
    err, knee, b = best
    fall = float(seg[knee] - seg[-1])
    if fall < LEAST_FALL_DB and (n - knee) * STEP < 2 * LEAST_TAIL:
        return None
    residual = seg[knee:] - np.polyval(b, t[knee:])
    if np.sqrt(np.mean(residual**2)) > LEAST_STRAIGHT_DB:
        return None
    return {
        "seconds": i * STEP,
        "decay_seconds": float(-60 / b[0]),
        "under_db": float(seg[0] - np.polyval(b, t[knee])),
        "knee_seconds": float(knee * STEP),
        "length_seconds": float(n * STEP),
    }


def tail_span(level, i) -> int:
    """How long the tail after the end at `i` runs, in steps: until the part
    plays again, its level rising RISE_DB over the lowest it fell to."""
    stop = min(len(level), i + int(round(LONGEST_TAIL / STEP)))
    seg = level[i:stop]
    rises = np.flatnonzero(seg > np.minimum.accumulate(seg) + RISE_DB)
    return int(rises[0]) if len(rises) else len(seg)


def echoes(level, ends, beat: float | None) -> dict:
    """Repeats of what played before each end, inside its tail.

    A delay plays the last notes again a spacing later: the tail just after
    the end is a copy, quieter, of the level the spacing before it, its
    syllables' rises and falls in the same places. So for each spacing the
    first ECHO_WINDOW of each tail is compared with the stretch that spacing
    earlier, as how the level moves step to step, and the spacing they are
    most alike at, over the ends, is the delay's. A reverb's tail falls
    smoothly and is like nothing before it. Then the next repeat is the same
    a spacing later, quieter by what each repeat falls."""
    lo, hi = (int(round(v / STEP)) for v in ECHO_LAGS)
    top = float(np.percentile(level, 99))
    move = np.diff(level, prepend=level[0])
    scores = np.zeros(hi + 1)
    weights = np.zeros(hi + 1)
    read = 0
    for i in ends:
        n = tail_span(level, i)
        if n < lo or i - hi < 0:
            continue
        read += 1
        for lag in range(lo, hi + 1):
            w = min(lag, int(round(ECHO_WINDOW / STEP)), n)
            tail = slice(i, i + w)
            heard = level[tail] > top + SOUNDING_DB - 20
            if heard.sum() < ECHO_LEAST_STEPS:
                continue
            a, b = move[i - lag : i - lag + w][heard], move[tail][heard]
            if a.std() < 1e-6 or b.std() < 1e-6:
                continue
            scores[lag] += float(np.corrcoef(a, b)[0, 1]) * heard.sum()
            weights[lag] += heard.sum()
    if read < LEAST_ENDS or not (weights[lo:] > 0).any():
        return {"heard": False, "ends_read": read, "why": f"{read} ends with a tail of {ECHO_LAGS[0]} s or more; at least {LEAST_ENDS} are needed"}
    with np.errstate(all="ignore"):
        alike = np.where(weights > 0, scores / np.maximum(weights, 1), np.nan)
    span = alike[lo:]
    k = int(np.nanargmax(span))
    lag = (lo + k) * STEP
    typical = float(np.nanmedian(span))
    out = {
        "ends_read": read,
        "spacing_seconds": r(lag, 2),
        "alike": r(span[k], 2),
        "typical": r(typical, 2),
    }
    alike_enough = span[k] >= ECHO_ALIKE and span[k] - typical >= ECHO_OVER
    if beat:
        beats_ = lag / beat
        name, value = min(DIVISIONS.items(), key=lambda kv: abs(np.log(kv[1] / beats_)))
        out["spacing_beats"] = r(beats_, 3)
        out["nearest"] = {"division": name, "beats": r(value, 3), "off_ms": r((lag - value * beat) * 1000, 0)}
    # The first repeat's level against what it repeats, and each repeat's
    # against the one before: the medians over the ends.
    steps = int(round(lag / STEP))
    w = min(steps, int(round(ECHO_WINDOW / STEP)))
    first, after = [], []
    for i in ends:
        n = tail_span(level, i)
        if n < w or i - steps < 0:
            continue
        first.append(float(np.median(level[i : i + w]) - np.median(level[i - steps : i - steps + w])))
        if n >= steps + w:
            after.append(float(np.median(level[i + steps : i + steps + w]) - np.median(level[i : i + w])))
    first_db = float(np.median(first)) if first else None
    # A repeat is quieter than what it repeats: a part that only goes on
    # playing the same figure at the spacing is as loud as before.
    heard = alike_enough and first_db is not None and first_db <= -ECHO_QUIETER_DB
    out["heard"] = bool(heard)
    if alike_enough and not heard:
        out["why"] = "the tail is like what came before, as loud: the part plays on, not a repeat"
    if heard:
        out["first_repeat_db"] = r(first_db)
        if after:
            each = float(np.median(after))
            out["falls_each_db"] = r(-each)
            out["feedback_percent"] = r(100 * 10 ** (min(each, 0.0) / 20), 0)
    return out


def tails(mono, sr, beat: float | None) -> dict:
    level = tail_level(mono, sr)
    if not len(level) or level.max() < -100:
        return {"ends": 0}
    floor = float(np.percentile(level, 5))
    found = ends_of(level)
    decays = [d for d in (decay_of(level, i, floor) for i in found) if d is not None]
    out: dict = {"ends": len(found), "decays_fitted": len(decays)}
    if len(decays) >= LEAST_ENDS:
        values = np.array([d["decay_seconds"] for d in decays])
        values = values[(values > 0) & (values < 30)]
        if len(values) >= LEAST_ENDS:
            out["decay_seconds"] = r(np.median(values), 2)
            q1, q3 = np.percentile(values, 25), np.percentile(values, 75)
            out["decay_spread_seconds"] = [r(q1, 2), r(q3, 2)]
            # The ends agree when half of them are within a ratio of each other.
            out["consistent"] = bool(q3 <= AGREE_RATIO * q1)
            out["under_db"] = r(np.median([d["under_db"] for d in decays]))
            out["at_seconds"] = [r(d["seconds"], 2) for d in decays[:12]]
    else:
        out["why"] = f"{len(decays)} of {len(found)} ends fell long enough to read; at least {LEAST_ENDS} are needed"
    out["echoes"] = echoes(level, found, beat)
    return out


# --- the song --------------------------------------------------------------------


def kicks_of(parts: dict, source: Path) -> tuple[np.ndarray, str]:
    """The kick's hits: from the drums part, as `daw transcribe --drums` hears
    them, or from the song's kick band."""
    if "drums" in parts:
        from . import drums

        mono, sr = beatmap.read(parts["drums"])
        found = drums.heard(mono, sr)
        times = [m["seconds"] for m in found["moments"] if "kick" in m["drums"]]
        if len(times) >= LEAST_KICKS:
            return np.array(times), "the drums part's kicks"
    mono, sr = beatmap.read(source)
    low, high = hits.BANDS["kick"]
    rows, _, _ = hits.find(mono, sr, low, high)
    loud = max((row[1] for row in rows), default=0.0)
    return np.array([row[0] for row in rows if row[1] >= loud - 12]), "the song's kick band"


def part(path: Path, sections, kicks, beat, is_drums: bool, against=None) -> dict:
    x, sr = read_stereo(path)
    mono = x.mean(axis=1)
    levels = spectrum.third_octaves(x.astype(np.float64), sr)
    out = {
        "dynamics": dynamics(mono, sr),
        "tone": tone(levels, against),
        "width": width(x, sr, sections),
    }
    if not out["dynamics"].get("sounding"):
        return {"sounding": False}
    if is_drums:
        out["pumping"] = {"heard": False, "why": "the drums are what the kick is in"}
    elif kicks is not None:
        out["pumping"] = pumping(mono, sr, kicks, beat)
    out["tails"] = tails(mono, sr, beat)
    return out


def measure(source: Path, report: dict | None, parts: dict, sections=None, only=None, against: Path | None = None) -> dict:
    """The sound of `source` and of each of its parts."""
    beat = 60 / report["tempo"]["bpm"] if report else None
    kicks, kicks_from = kicks_of(parts, source)
    against_levels = against_dynamics = None
    if against is not None:
        xa, sra = read_stereo(against)
        against_levels = spectrum.third_octaves(xa.astype(np.float64), sra)
        against_dynamics = dynamics(xa.mean(axis=1), sra)
        if not against_dynamics.get("sounding"):
            raise ValueError(f"{against} is silent")
    names = {"mix": source, **parts}
    if only:
        missing = [n for n in only if n not in names]
        if missing:
            raise ValueError(f"No part {', '.join(missing)}; the parts are {', '.join(names)}")
        names = {n: names[n] for n in only}
    out = {}
    mix_level = None
    for name, path in names.items():
        found = part(path, sections, kicks, beat, name == "drums" or name == "mix", against_levels)
        if name == "mix":
            found.pop("pumping", None)
            mix_level = found.get("dynamics", {}).get("level_dbfs")
        if against_dynamics is not None and found.get("dynamics", {}).get("sounding"):
            # What the part given has to change to be this one: positive where
            # this one is more.
            found["dynamics"]["against"] = {
                k: r(found["dynamics"][k] - against_dynamics[k], 2 if k == "near_top_share" else 1)
                for k in ("crest_db", "level_range_db", "near_top_share", "level_dbfs")
                if found["dynamics"].get(k) is not None and against_dynamics.get(k) is not None
            }
        out[name] = found
    if mix_level is not None:
        for name, found in out.items():
            level = found.get("dynamics", {}).get("level_dbfs")
            if name != "mix" and level is not None:
                found["dynamics"]["against_mix_db"] = r(level - mix_level)
    kicks_out = {"count": int(len(kicks)), "from": kicks_from}
    if parts and len(kicks) >= LEAST_KICKS:
        # The song itself, over the kick's body: a part's dip the mix shares is
        # in the record; one it does not share may be the separation's.
        mono, sr = beatmap.read(source)
        in_mix = pumping(mono, sr, kicks, beat, MIX_BAND)
        kicks_out["the_mix_dips"] = {"band_hz": list(MIX_BAND), **in_mix}
        for found in out.values():
            pump = found.get("pumping")
            if pump and pump.get("heard"):
                pump["in_the_mix"] = bool(in_mix.get("heard"))
    return {"kicks": kicks_out, "sound": out}


# --- the picture -----------------------------------------------------------------


def picture(found: dict, output: Path) -> str:
    """A row a part: its tone against flat with the fitted equalizer's curve,
    its level around the kick with the mix's, and its width by section."""
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.figure import Figure

    parts = [(n, p) for n, p in found["sound"].items() if p.get("tone")]
    if not parts:
        raise ValueError("nothing sounds to draw")
    fig = Figure(figsize=(15, 2.8 * len(parts) + 0.6), layout="constrained")
    FigureCanvasAgg(fig)
    axes = fig.subplots(len(parts), 3, squeeze=False, gridspec_kw={"width_ratios": [2.2, 1.4, 1.6]})
    centers = np.array(spectrum.CENTERS, float)
    mix_dip = found.get("kicks", {}).get("the_mix_dips")
    for row, (name, p) in zip(axes, parts):
        tone_ax, pump_ax, width_ax = row
        levels = np.array([np.nan if v is None else v for v in p["tone"]["third_octaves_db"]], float)
        hz, _, _, span = curve_of(p["tone"]["third_octaves_db"])
        if len(hz):
            shown = levels - np.nanmedian(levels[(centers >= span[0]) & (centers <= span[1])])
            tone_ax.semilogx(centers, shown, color="#2a78d6", lw=1.6, label=f"{name} against flat")
            eq = p["tone"].get("eq")
            if eq and eq["bands"]:
                fine = np.geomspace(20, 20000, 400)
                curve = eq_curve(eq["bands"], fine)
                inside = hz[(hz >= span[0]) & (hz <= span[1])]
                at = np.interp(np.log(inside), np.log(fine), curve)
                curve = curve - np.median(at)
                tone_ax.semilogx(fine, curve, color="#eb6834", lw=1.2, ls="--", label=f"eq, {len(eq['bands'])} bands, off {eq['error_db']} dB")
            tone_ax.axvspan(span[0], span[1], color="#888", alpha=0.08)
        tone_ax.set_xlim(20, 20000)
        tone_ax.set_ylim(-40, 25)
        tone_ax.axhline(0, color="#888", lw=0.5)
        tone_ax.set_ylabel(f"{name}\ndB", fontsize=9)
        tone_ax.legend(fontsize=7, loc="lower left")
        tone_ax.tick_params(labelsize=7)
        pump = p.get("pumping") or {}
        if pump.get("curve"):
            ms, db_ = (np.array([np.nan if v is None else v for v in pump["curve"][k]], float) for k in ("ms", "db"))
            color = "#e34948" if pump.get("heard") else "#2a78d6"
            pump_ax.plot(ms, db_, color=color, lw=1.4, label=f"{name}: {pump.get('depth_db')} dB" + (" ducked" if pump.get("heard") else ""))
        if mix_dip and mix_dip.get("curve"):
            ms, db_ = (np.array([np.nan if v is None else v for v in mix_dip["curve"][k]], float) for k in ("ms", "db"))
            pump_ax.plot(ms, db_, color="#888", lw=1, ls=":", label="the mix, 300 Hz–2 kHz")
        pump_ax.axvline(0, color="#888", lw=0.5)
        pump_ax.axhline(0, color="#888", lw=0.5)
        pump_ax.set_ylim(-15, 6)
        pump_ax.set_xlabel("ms from the kick" if row is axes[-1] else "", fontsize=8)
        pump_ax.tick_params(labelsize=7)
        if pump.get("curve") or (mix_dip and mix_dip.get("curve")):
            pump_ax.legend(fontsize=7, loc="lower right")
        else:
            pump_ax.text(0.5, 0.5, (pump.get("why") or "no kick read")[:48], ha="center", va="center", fontsize=7, transform=pump_ax.transAxes)
        by = p["width"].get("by_section") or []
        if by:
            width_ax.bar(range(len(by)), [s["side_share"] or 0 for s in by], color="#1baf7a")
            width_ax.set_xticks(range(len(by)), [s["section"] for s in by], fontsize=6)
        width_ax.set_ylim(0, 0.6)
        width_ax.set_ylabel("side share", fontsize=8)
        width_ax.tick_params(labelsize=7)
        tails_ = p.get("tails") or {}
        said = []
        if tails_.get("decay_seconds"):
            said.append(f"tail decays in {tails_['decay_seconds']} s" + ("" if tails_.get("consistent") else ", ends disagree"))
        echo = tails_.get("echoes") or {}
        if echo.get("heard"):
            said.append(f"repeats every {echo.get('nearest', {}).get('division', echo['spacing_seconds'])}, {echo.get('feedback_percent')}%")
        if said:
            width_ax.set_title("; ".join(said), fontsize=8)
    axes[0][0].set_title("Tone against flat, with the eq that makes it", fontsize=9)
    axes[0][1].set_title("Level around the kick", fontsize=9)
    output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output, dpi=90)
    return str(output.resolve())
