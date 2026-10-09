"""A finer spectrum in numbers: third octaves, a tilt, and each stem's resonances.

Third octaves say where a mix or a stem has its power more closely than seven
bands do, and the tilt says how bright it is as one number. A resonance is a
narrow peak that stays where it is while the notes move, given as a frequency,
a height and a width, which are the three numbers an equalizer's bell takes.
These are measurements: nothing here says a peak should be cut.
"""

from __future__ import annotations

from collections import defaultdict

import numpy as np
from numpy.lib.stride_tricks import sliding_window_view
from scipy import fft
from scipy.ndimage import median_filter
from scipy.signal import find_peaks, get_window

from .analysis import hz_to_midi, note_name
from .model import meter, sounded

# Third octaves at the standard centers, 20 Hz to 20 kHz. The top one ends at
# 20 kHz, where the seven bands do.
EDGES = [10 ** (1.25 + 0.1 * k) for k in range(31)] + [20000.0]
CENTERS = [
    20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800,
    1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000, 12500, 16000, 20000,
]  # fmt: skip
WINDOW = 32768  # frames: the 20 Hz band holds three bins
BAND_OCTAVES = 0.1 * np.log2(10)  # a third octave, as these bands are spaced
TILT_HZ = (50, 10000)
# A band this far under the loudest is fitted as that far under, so the floor
# of a stem with nothing up there does not set its tilt.
TILT_FLOOR_DB = 60.0

# Resonances. A stem's spectrum is taken a span at a time, a bar or as many
# bars as last a second, and smoothed over SMOOTH of an octave, which joins the
# partials of notes a semitone apart and still stands a ring apart from its
# neighbors. A peak stands LEAST_DB over the median of the AROUND octaves about
# it: a median, so that a slope or a filter's corner is not a peak and the
# peak's own height does not lift what it is measured from. A resonance is such
# a peak in the median of the spans, found as a peak of its own within NEAR of
# that frequency in MIN_FRACTION of the spans where the stem sounds. First
# guesses, run on generated audio and one real render, heard by nobody.
PER_OCTAVE = 48  # points of a span's curve to the octave
SMOOTH = 1 / 12
AROUND = 2
LEAST_DB = 5.0
SADDLE_DB = 3.0  # what a peak stands over the dip between it and a taller one
APART = PER_OCTAVE // 6  # two peaks closer than a sixth of an octave are one
NEAR = 0.03
MIN_FRACTION = 0.5
MIN_SPANS = 4  # fewer cannot tell what stays from what passes
SPAN_SECONDS = 1.0
SPAN_WINDOW = 16384
HZ = (40.0, 16000.0)
SOUNDING_DB = 40.0  # a span sounds within this of the stem's loudest
FLOOR_DB = 70.0  # and a peak counts within this of its span's loudest point
SILENT = 1e-12  # power: -120 dBFS
LISTED = 5
# A note's partials that stand apart at this smoothing: up to the sixteenth,
# which is a semitone from the next.
HARMONICS = 16


def windows(audio, size, hop, kind=np.float64, block=64):
    """The audio's power in each bin of each Hann window, the channels'
    averaged and each window's mean taken out, as Welch's method has it: some
    windows at a time, as (their first frames, their powers)."""
    shape = get_window("hann", size).astype(kind)
    scale = 2 / (size * float(np.sum(shape.astype(float) ** 2)) * audio.shape[1])
    starts = np.arange(0, len(audio) - size + 1, max(1, hop))
    for at in range(0, len(starts), block):
        rows = starts[at : at + block]
        power = 0.0
        for channel in range(audio.shape[1]):
            pieces = sliding_window_view(audio[:, channel], size)[rows].astype(kind)
            pieces = (pieces - pieces.mean(axis=1, keepdims=True)) * shape
            found = fft.rfft(pieces, axis=1, workers=-1)
            power = power + (found.real**2 + found.imag**2)
        yield rows, power * scale


def third_octaves(x, rate) -> list:
    """The power in each third octave, dBFS, the channels' averaged; None where
    the audio is too short for two bins of its spectrum to fall in the band, or
    the band is empty."""
    n = min(len(x), WINDOW)
    total, count = 0.0, 0
    for rows, power in windows(x, n, n // 2):
        total, count = total + power.sum(axis=0), count + len(rows)
    at = np.searchsorted(fft.rfftfreq(n, 1 / rate), EDGES)
    # Each band summed by itself: a band far under the loudest is lost in a running sum.
    bands = [float(np.sum(total[a:b])) / count for a, b in zip(at, at[1:])]
    return [
        round(float(10 * np.log10(band)), 2) if b - a >= 2 and band > 0 else None
        for band, a, b in zip(bands, at, at[1:])
    ]


def tilt(levels) -> float | None:
    """The slope of a line through the third octaves from 50 Hz to 10 kHz, in
    dB an octave: 0 for pink noise, +3 for white. None where under half of them
    are within TILT_FLOOR_DB of the loudest: a tone or a narrow sound has none."""
    known = [
        (k * BAND_OCTAVES, level)
        for k, (center, level) in enumerate(zip(CENTERS, levels))
        if TILT_HZ[0] <= center <= TILT_HZ[1] and level is not None
    ]
    if not known:
        return None
    octaves, level = (np.array(v) for v in zip(*known))
    floor = level.max() - TILT_FLOOR_DB
    if np.count_nonzero(level > floor) < 12:
        return None
    return round(float(np.polyfit(octaves, np.maximum(level, floor), 1)[0]), 2)


def figures(hz: float) -> float:
    """A frequency as it is printed: to three figures."""
    return float(f"{hz:.3g}")


def close(a: float, b: float) -> bool:
    return abs(np.log(a / b)) <= np.log(1 + NEAR)


def crossing(values, level) -> float:
    """How far along some values the first at or under a level is, between the
    points; their whole length where none is."""
    under = np.flatnonzero(values <= level)
    if not len(under) or under[0] == 0:
        return float(len(values) - 1)
    at = int(under[0])
    return at - (level - values[at]) / (values[at - 1] - values[at])


class Resonances:
    """Each stem's peaks that stay where they are, for one render."""

    def __init__(self, frames, rate, project, manifest, offset, sections):
        session = project["session"]
        self.rate, self.project, self.manifest = rate, project, manifest
        per_beat = 60 / float(session["tempo"]) * rate
        self.span_beats = float(meter(session)[0])
        while self.span_beats * per_beat < SPAN_SECONDS * rate:
            self.span_beats *= 2
        size = self.span_beats * per_beat
        first = int(np.floor(offset / size + 1e-9))
        last = max(first + 1, int(np.ceil((offset + frames) / size - 1e-9)))
        # Each span on the song's own bars, as (first frame, last, first beat).
        self.spans = []
        for i in range(first, last):
            a, b = max(0, round(i * size) - offset), min(frames, round((i + 1) * size) - offset)
            # A part of a bar at a preview's edge is read when windows fit inside it.
            if b - a >= 2 * SPAN_WINDOW:
                self.spans.append((a, b, i * self.span_beats))
        middle = np.array([(a + b) / 2 for a, b, _ in self.spans])
        self.sections = {
            s["id"]: (middle >= s["start_frame"]) & (middle < s["end_frame"]) for s in sections
        }
        top = min(20000.0, 0.98 * rate / 2)
        self.hz = 20.0 * 2 ** (np.arange(int(np.log2(top / 20.0) * PER_OCTAVE) + 1) / PER_OCTAVE)
        self.ratio: dict[str, np.ndarray] = {}
        self.own: dict[str, np.ndarray] = {}
        self.sounding: dict[str, np.ndarray] = {}
        self.played = None

    def smoothed(self, freq, power) -> np.ndarray:
        """A spectrum's power about each point of the curve: in the point's own
        48th of an octave, then weighted over its neighbors by a triangle
        SMOOTH of an octave wide halfway up, so that a single partial reads
        highest at its own frequency. Power in a constant share of an octave
        leans 3 dB an octave against a density, which a median does not see."""
        step = freq[1] - freq[0]
        edges = np.append(freq - step / 2, freq[-1] + step / 2)
        summed = np.concatenate([[0.0], np.cumsum(power)])
        half = 2 ** (0.5 / PER_OCTAVE)
        cells = np.interp(self.hz * half, edges, summed) - np.interp(self.hz / half, edges, summed)
        reach = round(SMOOTH * PER_OCTAVE)
        weights = reach - np.abs(np.arange(-reach + 1, reach))
        return np.convolve(np.pad(cells, reach - 1, mode="edge"), weights / weights.sum(), mode="valid")

    def add(self, name, audio):
        if not self.spans:
            empty = np.zeros((0, len(self.hz)))
            self.ratio[name], self.own[name], self.sounding[name] = empty, empty > 0, np.zeros(0, bool)
            return
        power = np.array([float(np.mean(audio[a:b] ** 2)) for a, b, _ in self.spans])
        sounding = (power > SILENT) & (power >= power.max(initial=0) * 10 ** (-SOUNDING_DB / 10))
        # Each span's spectrum from the windows that lie wholly inside it.
        first = np.array([a for a, _, _ in self.spans])
        last = np.array([b for _, b, _ in self.spans])
        summed = np.zeros((len(self.spans), SPAN_WINDOW // 2 + 1))
        count = np.zeros(len(self.spans))
        for rows, found in windows(audio, SPAN_WINDOW, SPAN_WINDOW // 4, np.float32):
            span = np.searchsorted(first, rows, side="right") - 1
            inside = (span >= 0) & (rows + SPAN_WINDOW <= last[span])
            for i in np.unique(span[inside]):
                mine = inside & (span == i)
                summed[i] += found[mine].sum(axis=0)
                count[i] += mine.sum()
        freq = fft.rfftfreq(SPAN_WINDOW, 1 / self.rate)
        level = np.full((len(self.spans), len(self.hz)), -300.0)
        for i in np.flatnonzero(sounding):
            level[i] = 10 * np.log10(np.maximum(self.smoothed(freq, summed[i] / count[i]), 1e-30))
        ratio = level - median_filter(level, size=(1, AROUND * PER_OCTAVE + 1), mode="nearest")
        ratio[level < level.max(axis=1, keepdims=True) - FLOOR_DB] = 0
        # Each span's own peaks, as the points of the curve within NEAR of one.
        own = np.zeros(ratio.shape, dtype=bool)
        reach = int(np.ceil(np.log2(1 + NEAR) * PER_OCTAVE))
        for i in np.flatnonzero(sounding):
            at, _ = find_peaks(ratio[i], height=LEAST_DB, prominence=SADDLE_DB)
            for off in range(-reach, reach + 1):
                own[i, np.clip(at + off, 0, ratio.shape[1] - 1)] = True
        self.ratio[name], self.own[name], self.sounding[name] = ratio, own, sounding

    def rows(self, name, section=None) -> np.ndarray:
        """The spans a stem sounds in, of the whole render or of a section."""
        rows = self.sounding[name]
        return rows if section is None else rows & self.sections[section]

    def curve(self, name, section=None) -> np.ndarray | None:
        """A stem's height over its surroundings at each frequency, the median
        of the spans where it sounds; None with too few to tell."""
        rows = self.rows(name, section)
        if rows.sum() < MIN_SPANS:
            return None
        return np.median(self.ratio[name][rows], axis=0)

    def height(self, name, hz, section=None) -> float | None:
        """How far a stem stands over its surroundings at a frequency, listed or not."""
        curve = self.curve(name, section)
        if curve is None:
            return None
        near = np.abs(np.log(self.hz / hz)) <= np.log(1 + NEAR)
        return round(float(curve[near].max()), 1)

    def listed(self, name, section=None) -> dict:
        """`resonances` of a stem: up to LISTED, those the notes do not explain
        first and the most prominent first, and how many more there are."""
        curve = self.curve(name, section)
        if curve is None:
            return {"resonances": None, "resonances_omitted": 0}
        rows = self.rows(name, section)
        found = []
        peaks, _ = find_peaks(curve, height=LEAST_DB, prominence=SADDLE_DB, distance=APART)
        for i in peaks:
            if not HZ[0] <= self.hz[i] <= HZ[1]:
                continue
            present = float(self.own[name][rows, i].mean())
            if present < MIN_FRACTION:
                continue
            # The top between the points of the curve, and the width 3 dB
            # down, or halfway down a peak under 6 dB, which is where an
            # equalizer's bell of that gain counts its q.
            below, top, above = (float(v) for v in curve[i - 1 : i + 2])
            bend = below - 2 * top + above
            hz = float(self.hz[0] * 2 ** ((i + (0.5 * (below - above) / bend if bend else 0)) / PER_OCTAVE))
            down = top - min(top / 2, 3.0)
            left, right = i - crossing(curve[i::-1], down), i + crossing(curve[i:], down)
            octaves = (right - left) / PER_OCTAVE
            found.append(
                {
                    "freq_hz": figures(hz),
                    "note": note_name(float(hz_to_midi(hz))),
                    "prominence_db": round(float(top), 1),
                    "q": round(float(1 / (2 ** (octaves / 2) - 2 ** (-octaves / 2))), 1),
                    "present_fraction": round(present, 2),
                    "could_be_note": self.could_be_note(name, hz, rows),
                }
            )
        found.sort(key=lambda r: (r["could_be_note"] is True, -r["prominence_db"]))
        return {"resonances": found[:LISTED], "resonances_omitted": max(0, len(found) - LISTED)}

    def feeding(self, name) -> list[str]:
        """The tracks whose sound a stem holds: itself, a group's tracks, or
        those that send to a return, with their groups' tracks."""
        entries = self.manifest["tracks"]
        entry = entries[name]
        if entry.get("kind", "track") == "track":
            return [name]
        out = list(entry.get("tracks", []))
        for sender in entry.get("senders", []):
            out += entries.get(sender, {}).get("tracks", []) or [sender]
        return out

    def could_be_note(self, name, hz, rows) -> bool | None:
        """Whether a peak could be what the stem's tracks play, from the song.

        True when nothing they play moves in pitch, so nothing tells a ring
        from the sound's own partials; and when, in MIN_FRACTION of the spans,
        a note they play has one of its first HARMONICS partials there, or a
        hit plays a sample as it is. None when a track plays audio clips, of
        which the song says nothing, or plays nothing at all.
        """
        if self.played is None:
            self.played = defaultdict(list)
            for track, at, until, pitch, hit in sounded(self.project):
                self.played[track].append((at, until, pitch, hit))
        tracks = {t["id"]: t for t in self.project["tracks"]}
        feeding = self.feeding(name)
        events = [e for track in feeding for e in self.played[track]]
        if not events or any(tracks[t].get("audio") for t in feeding if t in tracks):
            return None
        pitches = {round(pitch, 2) for _, _, pitch, _ in events if pitch is not None}
        hits = {hit for _, _, pitch, hit in events if pitch is None}
        if len(pitches) <= 1 and len({hit.rsplit(" ", 1)[0] for hit in hits}) == len(hits):
            return True
        # A partial is the nearest semitone's, give or take two of the spectrum's bins.
        reach = hz * (2 ** (1 / 24) - 1) + 2 * self.rate / SPAN_WINDOW
        partials = np.arange(1, HARMONICS + 1)
        explained = 0
        for i in np.flatnonzero(rows):
            start = self.spans[i][2]
            end = start + self.span_beats
            explained += any(
                (at < end and until > start or start <= at < end)
                and (
                    pitch is None
                    or np.any(np.abs(partials * 440 * 2 ** ((pitch - 69) / 12) - hz) <= reach)
                )
                for at, until, pitch, _ in events
            )
        return bool(explained >= MIN_FRACTION * rows.sum())


def change(before, after):
    return round(after - before, 1) if before is not None and after is not None else None


def between(before: Resonances | None, after: Resonances | None, a, b) -> dict:
    """`resonances` of each stem in `daw compare`: each one either render lists,
    with its height in both, the largest change first."""
    out = {}
    if before is None or after is None:
        return out
    for name in a["tracks"]:
        if name not in b["tracks"]:
            continue
        sides = [a["tracks"][name]["audio"]["resonances"], b["tracks"][name]["audio"]["resonances"]]
        if None in sides:
            out[name] = None
            continue
        rows = []
        for entry in [*sides[0], *sides[1]]:
            if any(close(entry["freq_hz"], row["freq_hz"]) for row in rows):
                continue
            listed = [
                next((e for e in side if close(e["freq_hz"], entry["freq_hz"])), None)
                for side in sides
            ]
            was, now = (
                found["prominence_db"] if found else grid.height(name, entry["freq_hz"])
                for found, grid in zip(listed, (before, after))
            )
            rows.append(
                {
                    "freq_hz": entry["freq_hz"],
                    "note": entry["note"],
                    "before_db": was,
                    "after_db": now,
                    "delta_db": change(was, now),
                    "listed_before": listed[0] is not None,
                    "listed_after": listed[1] is not None,
                }
            )
        out[name] = sorted(rows, key=lambda r: -abs(r["delta_db"] or 0))
    return out


METHOD = {
    "spectrum": "spectrum_db is the power in each of 31 third octaves, dBFS, from a Welch "
    f"spectrum with a Hann window of up to {WINDOW} frames, the channels' powers averaged; "
    "null where the audio is too short for two bins to fall in the band, or the band is empty",
    "third_octave_hz": CENTERS,
    "tilt": f"the slope of a line through spectrum_db from {TILT_HZ[0]} Hz to {TILT_HZ[1]} Hz "
    "in dB an octave: 0 for pink noise, +3 for white; a band more than "
    f"{TILT_FLOOR_DB:g} dB under the loudest is fitted as that far under, and the tilt is "
    "null where over half of them are",
    "resonances": "a stem's spectrum a bar at a time, or as many bars as last a second, "
    "smoothed over a twelfth of an octave; a resonance stands "
    f"{LEAST_DB:g} dB or more over the median of the {AROUND} octaves about it in the median of "
    f"the bars where the stem sounds, and is a peak of its own within {NEAR:.0%} of that "
    f"frequency in {MIN_FRACTION:g} of them; prominence_db is that height, which reads a "
    "little under the gain of a bell that made it; q is the frequency over the width 3 dB "
    "down, or halfway down a peak under 6 dB, and a peak narrower than the smoothing reads "
    "as about 16; could_be_note is read from the song and null where it does not say; null "
    f"with fewer than {MIN_SPANS} such bars; a measurement, not a fault",
}

# The chart's colors: an ink for the mix, eight hues in a fixed order for the
# stems nearest it, and a gray for the rest.
SURFACE, INK, SECONDARY, MUTED = "#fcfcfb", "#0b0b0b", "#52514e", "#898781"
GRID, AXIS, OTHER = "#e1e0d9", "#c3c2b7", "#c3c2b7"
SERIES = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#4a3aa7", "#e34948"]
SPAN_DB = 72.0


def plot(report, path):
    """The third octaves of the mix over those of its stems: the eight stems
    that come nearest the mix in any band each in a color, the rest in gray."""
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.figure import Figure
    from matplotlib.lines import Line2D

    def levels(audio):
        return np.array([np.nan if v is None else v for v in audio["spectrum_db"]])

    mix = levels(report["mix"])
    stems = {name: levels(track["audio"]) for name, track in report["tracks"].items()}
    with np.errstate(invalid="ignore"):
        nearest = {
            name: np.nanmax(level - mix) if np.isfinite(level - mix).any() else -np.inf
            for name, level in stems.items()
        }
    # Colored in the render's order, so a stem keeps its color while the same stems lead.
    leading = set(sorted(stems, key=lambda name: -nearest[name])[: len(SERIES)])
    colors = dict(zip([name for name in stems if name in leading], SERIES))
    fig = Figure(figsize=(12, 6), layout="constrained", facecolor=SURFACE)
    FigureCanvasAgg(fig)
    ax = fig.subplots()
    ax.set_facecolor(SURFACE)
    at = np.arange(len(CENTERS))
    for name, level in stems.items():
        if name not in colors:
            ax.plot(at, level, color=OTHER, linewidth=1.2, zorder=1)
    for name, color in colors.items():
        ax.plot(at, stems[name], color=color, linewidth=2, zorder=2, solid_capstyle="round")
    ax.plot(at, mix, color=INK, linewidth=2.6, zorder=3, solid_capstyle="round")
    top = np.nanmax(mix) if np.isfinite(mix).any() else 0.0
    top = 6 * np.ceil((top + 3) / 6)
    ax.set_ylim(top - SPAN_DB, top)
    ax.set_xlim(0, len(CENTERS) - 1)
    ticks = range(2, len(CENTERS), 3)  # 31.5 Hz and each octave up
    ax.set_xticks(list(ticks), [f"{CENTERS[i]:g}" if CENTERS[i] < 1000 else f"{CENTERS[i] / 1000:g}k" for i in ticks])
    ax.set_xlabel("Third octave (Hz)", color=SECONDARY)
    ax.set_ylabel("Power in the band (dBFS)", color=SECONDARY)
    ax.set_title("Third octaves of the mix and its stems" if stems else "Third octaves", color=INK, loc="left")
    ax.grid(color=GRID, linewidth=0.8)
    ax.set_axisbelow(True)
    ax.tick_params(colors=MUTED, length=0)
    for side, spine in ax.spines.items():
        spine.set_visible(side == "bottom")
        spine.set_color(AXIS)
    handles = [Line2D([], [], color=INK, linewidth=2.6, label="mix")]
    handles += [Line2D([], [], color=color, linewidth=2, label=name) for name, color in colors.items()]
    if len(stems) > len(colors):
        count = len(stems) - len(colors)
        handles.append(Line2D([], [], color=OTHER, linewidth=1.2, label=f"{count} other stem{'s' * (count > 1)}"))
    if stems:
        fig.legend(
            handles=handles, loc="outside lower center", ncols=min(len(handles), 5), frameon=False, labelcolor=INK
        )
    fig.savefig(path, dpi=140)
    return str(path.resolve())
