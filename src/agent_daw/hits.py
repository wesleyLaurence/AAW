"""The hits of a song or a stem in one band, measured from its audio.

A hit is where the level in a band rises sharply and peaks: a kick in the low
band, a snare's crack, a hat. The rule is a plain one on the level, since a hit
the band shows is a hit a picture of the band shows: the band is filtered out,
its level followed a millisecond at a time, and every peak that stands far
enough above what came just before it is a hit. Its time is the file's own
sharpest transient in the stretch where the band's level climbs, and a climb
with no transient in it, a swell or two notes beating, is not a hit. Each hit is
placed on the song's beat map by bar and beat and on its nearest sixteenth, and
given its level against the band's loudest hit and the spacing to the next, so
a missing hit or an extra one stands out in the list as in the picture.

Every value is an estimate from decoded audio; nothing here listens.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
from scipy.signal import butter, find_peaks, sosfiltfilt

from . import beats

# Bands named for what tends to sound in them on a drum stem. The kick's is
# under a snare's body and over sub rumble; the snare's is its crack, over a
# kick's click and under most of a hat; the hats' is over both.
BANDS = {
    "kick": (45.0, 90.0),
    "snare": (1500.0, 4000.0),
    "hats": (7000.0, 16000.0),
    "all": (20.0, 20000.0),
}
HOP = 0.001  # the level is followed a millisecond at a time
MIN_GAP = 0.050  # two hits closer than this are one
LOOK_BACK = 0.100  # a hit's rise is from the quietest point this long before its peak
MIN_RISE_DB = 6.0
MIN_LEVEL_DB = -20.0  # against the band's loudest hit
COUNTED_DB = -30.0  # the levels are counted down to here, whatever is listed
STEP_DB = 3.0
ATTACK_SEARCH = 0.150  # the attack is at most this after the level starts to climb
LEAD = 0.025  # a hit's sound can start this long before its loudest attack
BARS_A_ROW = 8  # in the picture
SECONDS_A_ROW = 15.0  # in the picture of a file with no beat map


def band(text) -> tuple[str, float, float]:
    """A band named in BANDS or given as LOW-HIGH in Hz: its name, low and high."""
    text = str(text).strip().lower()
    if text in BANDS:
        return (text, *BANDS[text])
    low, dash, high = text.partition("-")
    try:
        lo, hi = float(low), float(high)
    except ValueError:
        lo = hi = None
    if not dash or lo is None or not 10 <= lo < hi <= 24000:
        names = ", ".join(BANDS)
        raise ValueError(f"band is one of {names}, or LOW-HIGH in Hz such as 45-90, not {text}")
    return (f"{lo:g}-{hi:g}", lo, hi)


def level(mono, sr, low, high):
    """The band's level in dB a millisecond at a time, and that step in seconds.

    The band is filtered forward and back, so it is not moved in time. Its power is
    averaged over a period of its lowest frequency, from 2 to 25 ms, so a low
    band's own cycles do not read as hits.
    """
    nyquist = sr / 2
    high = min(high, 0.95 * nyquist)
    x = mono.astype(np.float64)
    if low > 20 and high < 0.9 * nyquist:
        x = sosfiltfilt(butter(4, [low, high], "bandpass", fs=sr, output="sos"), x)
    elif low > 20:
        x = sosfiltfilt(butter(4, low, "highpass", fs=sr, output="sos"), x)
    elif high < 0.9 * nyquist:
        x = sosfiltfilt(butter(4, high, "lowpass", fs=sr, output="sos"), x)
    width = max(round(sr * min(max(1 / low, 0.002), 0.025)), 1)
    hop = max(round(sr * HOP), 1)
    sums = np.concatenate([[0.0], np.cumsum(x * x)])
    centers = np.arange(0, len(x), hop)
    first = np.clip(centers - width // 2, 0, len(x))
    last = np.clip(centers + width - width // 2, 0, len(x))
    power = (sums[last] - sums[first]) / np.maximum(last - first, 1)
    return 10 * np.log10(power + 1e-20), hop / sr


def find(mono, sr, low, high, min_rise=MIN_RISE_DB):
    """Every hit in the band, in order: (seconds, level in dB, rise in dB), with the
    band's level and its step in seconds."""
    db, step = level(mono, sr, low, high)
    if float(db.max()) <= -150:
        return [], db, step
    back = max(int(round(LOOK_BACK / step)), 1)
    peaks, _ = find_peaks(db, distance=max(int(round(MIN_GAP / step)), 1))
    attack = _Attacks(mono, sr)
    rows = []
    for p in peaks:
        trough = max(0, p - back) + int(np.argmin(db[max(0, p - back) : p + 1]))
        rise = float(db[p] - db[trough])
        if rise < min_rise:
            continue
        # The averaged level starts to climb before the sound does, slowly where
        # something else sounds in the band, and can go on climbing long after,
        # so the attack is looked for over the whole climb: from just before the
        # level gains a tenth of it to its peak.
        # A climb with no attack in it, a swell or two notes beating, is not a hit.
        climb = db[trough : p + 1]
        start = trough + int(np.argmax(climb >= db[trough] + 0.1 * rise))
        first = start * step - 0.005
        last = min(p * step, first + ATTACK_SEARCH)
        at = attack.within(first, max(last, first + 0.004))
        if at is not None:
            rows.append((float(at), float(db[p]), rise))
    # Two peaks whose attacks are this close are one hit, the louder; a quieter
    # sound just before a loud one does not lend it its time.
    kept = []
    for row in sorted(rows):
        if kept and row[0] - kept[-1][0] < MIN_GAP:
            if row[1] > kept[-1][1]:
                kept[-1] = row
            continue
        kept.append(row)
    return kept, db, step


class _Attacks:
    """The sharpest rise in level near a time, much as beats.transients measures
    one: the energy of the signal's first difference over 2 ms, against the 2 ms
    before."""

    def __init__(self, mono, sr):
        self.sr = sr
        self.width = max(2, round(beats.RISE * sr))
        edge = np.diff(mono.astype(np.float64), prepend=mono[:1])
        self.energy = np.concatenate([[0.0], np.cumsum(edge**2)])

    def _rise(self, first, last, share):
        """Each sample from `first` to `last` and how sharply the level rises there,
        against `share` of the loudest energy among them."""
        size = len(self.energy) - 1
        at = np.arange(max(0, first), min(size, last) + 1)
        ends = np.clip(at + self.width, 0, size)
        mids = np.clip(ends - self.width, 0, size)
        starts = np.clip(mids - self.width, 0, size)
        after = self.energy[ends] - self.energy[mids]
        before = self.energy[mids] - self.energy[starts]
        floor = share * max(float(after.max()), 1e-30) if len(at) else 0.0
        return at, np.log((after + floor) / (before + floor + 1e-30) + 1e-30)

    def within(self, first, last):
        """The time of the hit's attack from `first` to `last` seconds, or None when no
        rise there doubles the level."""
        # Which rise: against a tenth of the loudest energy in reach, so a blip in
        # the quiet before a hit, large against nothing, does not outrank it.
        at, rise = self._rise(round(first * self.sr), round(last * self.sr), 0.1)
        if not len(at) or rise.max() < np.log(2):
            return None
        best = int(at[int(np.argmax(rise))])
        # Where it begins: that weighing favors the loudest part of the attack,
        # and a hit's sound can start a little before it, a beater's click
        # before a kick's body. So the sharpest rise as beats.transients weighs
        # it, in the LEAD before, where the sound is within 30 dB of the attack's.
        loud = self.energy[min(best + self.width, len(self.energy) - 1)] - self.energy[best]
        at, rise = self._rise(best - round(LEAD * self.sr), best, 1e-4)
        after = self.energy[np.clip(at + self.width, 0, len(self.energy) - 1)] - self.energy[at]
        rise = np.where(after >= 1e-3 * loud, rise, -np.inf)
        return int(at[int(np.argmax(rise))]) / self.sr


class Grid:
    """A beat map's beats, continued at its tempo before the first and after the last."""

    def __init__(self, report):
        rows = report["beats"]
        self.times = np.array([r["seconds"] for r in rows], dtype=float)
        self.period = 60 / report["tempo"]["bpm"]
        self.per_bar = int(report["beats_per_bar"])
        # How many beats the first beat is from bar 1's first.
        self.first = (rows[0]["bar"] - 1) * self.per_bar + rows[0]["beat"] - 1

    def index(self, seconds):
        """A time as a number of beats from the map's first, a fraction between them."""
        t, last = self.times, len(self.times) - 1
        if seconds <= t[0]:
            return (seconds - t[0]) / self.period
        if seconds >= t[last]:
            return last + (seconds - t[last]) / self.period
        i = int(np.searchsorted(t, seconds, side="right")) - 1
        return i + (seconds - t[i]) / (t[i + 1] - t[i])

    def seconds(self, index):
        t, last = self.times, len(self.times) - 1
        if index <= 0:
            return t[0] + index * self.period
        if index >= last:
            return t[last] + (index - last) * self.period
        i = int(np.floor(index))
        return t[i] + (index - i) * (t[i + 1] - t[i])

    def bar_beat(self, index):
        """Bar, and beat in it from 1 with its fraction, of a beat index."""
        count = self.first + index
        whole = int(np.floor(count))
        return whole // self.per_bar + 1, whole % self.per_bar + 1 + (count - whole)

    def place(self, seconds):
        """Where a time falls: its bar and beat, its nearest sixteenth as BAR.BEAT.N
        and how many ms from that sixteenth it is."""
        index = self.index(seconds)
        bar, beat = self.bar_beat(index)
        sixteenth = round(index * 4) / 4
        s_bar, s_beat = self.bar_beat(sixteenth)
        n = int(round((s_beat - int(s_beat)) * 4)) + 1
        return {
            "bar": int(bar),
            "beat": round(float(beat), 3),
            "sixteenth": f"{s_bar}.{int(s_beat)}.{n}",
            "off_grid_ms": round(float((seconds - self.seconds(sixteenth)) * 1000), 1),
        }, index


def counts(levels):
    """How many hits lie in each 3 dB under the loudest, down to COUNTED_DB."""
    out = []
    top = 0.0
    while top > COUNTED_DB:
        low = top - STEP_DB
        n = sum(1 for v in levels if low < v <= top)
        out.append({"from_db": top, "to_db": low, "hits": n})
        top = low
    return out


def measure(path: Path, low, high, report=None, min_rise=MIN_RISE_DB, min_level=MIN_LEVEL_DB):
    """The hits of a file in a band, each on the beat map `report` when there is one."""
    mono, sr = beats.read(path)
    found, db, step = find(mono, sr, low, high, min_rise)
    loudest = max((f[1] for f in found), default=None)
    rel = [f[1] - loudest for f in found] if found else []
    grid = Grid(report) if report else None
    rows = []
    for (at, lvl, rise), r in zip(found, rel):
        if r < min_level:
            continue
        row = {"seconds": round(at, 4)}
        if grid:
            where, index = grid.place(at)
            row.update(where)
            row["_index"] = index
        row.update(level_db=round(r, 1), rise_db=round(rise, 1))
        rows.append(row)
    for row, after in zip(rows, rows[1:] + [None]):
        row["gap_seconds"] = None if after is None else round(after["seconds"] - row["seconds"], 4)
        if grid:
            row["gap_beats"] = None if after is None else round(after["_index"] - row["_index"], 2)
    for row in rows:
        row.pop("_index", None)
    return {
        "loudest_dbfs": None if loudest is None else round(loudest, 1),
        "levels": counts(rel),
        "count": len(rows),
        "hits": rows,
    }, (mono, sr, db - (loudest if loudest is not None else float(db.max())), step, rows, grid)


def picture(drawn, output: Path, band_name, near=None):
    """The band's level and the file's under it, a row of bars at a time, each hit
    a mark at its level and a line at its time, bar lines numbered."""
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.figure import Figure

    mono, sr, band_db, step, rows, grid = drawn
    hop = max(round(sr * step), 1)
    n = len(band_db)
    padded = np.pad(np.abs(mono), (0, max(0, n * hop - len(mono))))[: n * hop]
    whole = padded.reshape(n, hop).max(axis=1)
    file_db = 20 * np.log10(whole / max(float(whole.max()), 1e-12) + 1e-12)
    duration = len(mono) / sr
    if near is not None:
        spans = [(max(0.0, near[0]), min(duration, near[1]))]
    elif grid:
        bars = grid.per_bar * BARS_A_ROW
        first = grid.seconds(-grid.first)  # bar 1
        start_index = -grid.first - bars * int(np.ceil(max(0.0, first) / (bars * grid.period)))
        spans = []
        i = start_index
        while grid.seconds(i) < duration:
            spans.append((max(0.0, grid.seconds(i)), min(duration, grid.seconds(i + bars))))
            i += bars
    else:
        spans = [(s, min(duration, s + SECONDS_A_ROW)) for s in np.arange(0, duration, SECONDS_A_ROW)]
    spans = [s for s in spans if s[1] > s[0]]
    fig = Figure(figsize=(16, 1.9 * len(spans) + 0.6), layout="constrained")
    FigureCanvasAgg(fig)
    axes = fig.subplots(len(spans), 1, squeeze=False)[:, 0]
    t = np.arange(n) * step
    for ax, (a, b) in zip(axes, spans):
        k = (t >= a) & (t <= b)
        ax.plot(t[k], file_db[k], color="0.75", lw=0.5, label="file peak")
        ax.plot(t[k], band_db[k], color="tab:blue", lw=0.7, label=f"{band_name} band")
        if grid:
            i0, i1 = int(np.floor(grid.index(a))), int(np.ceil(grid.index(b)))
            for i in range(i0, i1 + 1):
                at = grid.seconds(i)
                if not a <= at <= b:
                    continue
                bar, beat = grid.bar_beat(i)
                down = int(beat) == 1
                ax.axvline(at, color="0.35" if down else "0.85", lw=0.8 if down else 0.4, zorder=0)
                if down:
                    ax.text(at, 2.5, str(bar), fontsize=7, va="bottom", ha="left")
        for r in rows:
            if a <= r["seconds"] <= b:
                ax.axvline(r["seconds"], color="tab:red", lw=0.5, alpha=0.6)
                ax.plot(r["seconds"], r["level_db"], "v", color="tab:red", ms=4)
        ax.set_xlim(a, b)
        ax.set_ylim(-60, 6)
        ax.set_ylabel("dB")
    axes[0].set_title(
        f"{band_name} band level against its loudest hit (blue), the file's peak level (grey), "
        "hits (red, marked at their level), bar lines numbered",
        fontsize=9,
    )
    axes[-1].set_xlabel("seconds")
    output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output, dpi=100)
    return str(output.resolve())
