"""Overlap between stems: where two put their energy in the same band at the
same moments.

Overlap is what masking needs and is not masking as an ear does it: nothing here
spreads a loud band over its neighbors, tells a tone from noise or changes with
the playback level. A report says which pair, which band, how much of the time
and where; it does not say which of the two should give way.
"""

from __future__ import annotations

from itertools import combinations

import numpy as np
from numpy.lib.stride_tricks import sliding_window_view
from scipy import fft

from . import spectrum
from .model import meter

# Two stems contest a cell when the weaker is within CLOSE_DB of the stronger
# and within HEARD_DB of the stems' sum there, in a cell of the sum that counts:
# within FLOOR_DB of its band's loudest and EMPTY_DB of the loudest in any band,
# and not a skirt: SKIRT_DB under the band beside it at that moment, or that and
# SKIRT_SLOPE_DB more for each band further off. A struck low tone has a skirt
# 40 dB down for octaves above it, which is that tone's edge and not a sound of
# its own; this is a guard against reading it as one, not a model of masking.
# First guesses, run on generated audio and one real render, heard by nobody.
CLOSE_DB = 6.0
HEARD_DB = 9.0
FLOOR_DB = 40.0
EMPTY_DB = 70.0
SKIRT_DB = 12.0
SKIRT_SLOPE_DB = 4.0
LISTED = 5  # pairs `daw listen` prints
# A band is named when it is contested this much of the time either stem sounds
# in it, or this much of the time one of them does: a kick under a held bass is
# contested every time it hits and a quarter of the time the bass sounds.
MIN_FRACTION = 0.25
MIN_FRACTION_OF_ONE = 0.5
MIN_BEATS = 4.0  # and for this long in all
WORST = 3
MIN_CELL_SECONDS = 0.1
SILENT = 1e-14  # power: -140 dBFS

# The third octaves `spectrum_db` is in, from 100 Hz to 20 kHz, and an octave
# each for the two below, three of them taken as one: a cell is too short to
# tell third octaves apart there.
EDGES = [20.0, spectrum.EDGES[4], *spectrum.EDGES[7:]]
BANDS = list(zip(EDGES, EDGES[1:]))


def hz(value: float) -> int:
    """A band's edge as it is printed: whole Hz, to three figures."""
    digits = max(0, len(str(int(value))) - 3)
    return int(round(value, -digits))


def band_hz(first: int, last: int) -> list[int]:
    return [hz(BANDS[first][0]), hz(BANDS[last][1])]


def ratio(count, of) -> float | None:
    return round(float(count) / float(of), 3) if of else None


def runs(flags) -> list[tuple[int, int]]:
    """Each stretch of adjacent true values, as its first and last index."""
    found, start = [], None
    for i, flag in enumerate([*flags, False]):
        if flag and start is None:
            start = i
        elif not flag and start is not None:
            found.append((start, i - 1))
            start = None
    return found


class Grid:
    """Each stem's power on a grid of bands by time, for one render.

    A cell is a sixteenth note, doubled until it is 100 ms or more, on the
    song's own grid. `add` takes each stem; the stems that sum to the mix are
    added up as they come, and their sum's cells are what a stem is heard
    against, so the master's effects do not come into it.
    """

    def __init__(self, frames, rate, project, manifest, offset, sections):
        session = project["session"]
        self.tempo = float(session["tempo"])
        self.project, self.manifest = project, manifest
        # The stems that sum to the mix: all but a grouped track, whose sound
        # is in its group's. A preview of a grouped track alone has only those.
        entries = manifest["tracks"]
        self.summing = {
            name
            for name, entry in entries.items()
            if entry.get("kind", "track") != "track" or not entry.get("group")
        } or set(entries)
        self.cell_beats = 0.25
        while self.cell_beats * 60 / self.tempo < MIN_CELL_SECONDS:
            self.cell_beats *= 2
        per_beat = 60 / self.tempo * rate
        self.cell_frames = self.cell_beats * per_beat
        first = int(np.floor(offset / self.cell_frames + 1e-9))
        last = max(first + 1, int(np.ceil((offset + frames) / self.cell_frames - 1e-9)))
        index = np.arange(first, last)
        self.at = index * self.cell_beats  # each cell's start, in the song's beats
        starts = index * self.cell_frames - offset
        self.bar_beats = float(meter(session)[0])
        self.bars = np.floor(self.at / self.bar_beats + 1e-9).astype(int)
        middle = starts + self.cell_frames / 2
        self.sections = [
            (s["id"], (middle >= s["start_frame"]) & (middle < s["end_frame"]))
            for s in sections
        ]
        # Two windows a cell, each a cell long, centered an eighth and five
        # eighths of the way through it: early, so that a hit on the grid falls
        # in its own cell and 15 dB down in the one before, and a note that
        # stops on the grid 10 dB down in the one after.
        self.window = 2 * int(np.ceil(self.cell_frames / 2))
        self.size = fft.next_fast_len(self.window, real=True)
        centers = np.concatenate([starts + self.cell_frames / 8, starts + 5 * self.cell_frames / 8])
        self.first_frames = np.round(centers).astype(int) + self.window // 2
        freq = fft.rfftfreq(self.size, 1 / rate)
        self.bins = np.searchsorted(freq, EDGES)
        self.shape = np.hanning(self.window).astype(np.float32)
        self.scale = 2 / (self.size * float(np.sum(self.shape.astype(float) ** 2)))
        self.power: dict[str, np.ndarray] = {}
        self.total = None
        self.sum: np.ndarray | None = None
        self.live: np.ndarray | None = None
        self.sounding: dict[str, np.ndarray] = {}

    def cells(self, audio) -> np.ndarray:
        """The audio's power in each cell and band, the channels' averaged."""
        cells = len(self.at)
        out = np.zeros((2 * cells, len(BANDS)))
        for channel in range(audio.shape[1]):
            padded = np.zeros(len(audio) + 2 * self.window, dtype=np.float32)
            padded[self.window : self.window + len(audio)] = audio[:, channel]
            for at in range(0, 2 * cells, 512):  # bounded memory for long songs
                rows = self.first_frames[at : at + 512]
                rows = np.clip(rows, 0, len(padded) - self.window)
                pieces = sliding_window_view(padded, self.window)[rows]
                spectrum = fft.rfft(pieces * self.shape, self.size, axis=1, workers=-1)
                energy = spectrum.real.astype(float) ** 2 + spectrum.imag.astype(float) ** 2
                summed = np.concatenate(
                    [np.zeros((len(rows), 1)), np.cumsum(energy, axis=1)], axis=1
                )
                out[at : at + 512] += summed[:, self.bins[1:]] - summed[:, self.bins[:-1]]
        return (out[:cells] + out[cells:]) * (self.scale / (2 * audio.shape[1]))

    def add(self, name, audio):
        self.power[name] = self.cells(audio)
        if name in self.summing:
            self.total = audio.copy() if self.total is None else self.total + audio

    def summed(self) -> np.ndarray:
        if self.sum is None:
            self.sum = self.cells(self.total)
            self.total = None
        return self.sum

    def heard(self, name) -> np.ndarray:
        """The cells where a stem sounds: within HEARD_DB of the stems' sum, in
        a cell of the sum that is not silence."""
        total = self.summed()
        if self.live is None:
            # The level under which a cell is another band's skirt.
            skirt = np.zeros_like(total)
            for off in range(1, len(BANDS)):
                down = 10 ** (-(SKIRT_DB + SKIRT_SLOPE_DB * (off - 1)) / 10)
                skirt[:, off:] = np.maximum(skirt[:, off:], total[:, :-off] * down)
                skirt[:, :-off] = np.maximum(skirt[:, :-off], total[:, off:] * down)
            self.live = (
                (total > SILENT)
                & (total >= total.max(axis=0) * 10 ** (-FLOOR_DB / 10))
                & (total >= total.max() * 10 ** (-EMPTY_DB / 10))
                & (total >= skirt)
            )
        if name not in self.sounding:
            self.sounding[name] = self.live & (
                self.power[name] * 10 ** (HEARD_DB / 10) >= total
            )
        return self.sounding[name]

    def contest(self, a, b):
        """The cells two stems contest, and those where each sounds."""
        pa, pb = self.power[a], self.power[b]
        close = 10 ** (CLOSE_DB / 10)
        ha, hb = self.heard(a), self.heard(b)
        return ha & hb & (pa * close >= pb) & (pb * close >= pa), ha, hb

    def related(self, a, b) -> bool:
        """Whether one's sound is the other's: a return and what feeds it, a
        group and its tracks."""
        tracks = self.manifest["tracks"]

        def holds(owner, name):
            """Whether a stem's sound is in a channel's: itself, or its group's track."""
            return name == owner or name in tracks.get(owner, {}).get("tracks", [])

        return any(
            other in tracks[one].get("tracks", [])
            or any(
                holds(sender, other) or holds(other, sender)
                for sender in tracks[one].get("senders", [])
            )
            for one, other in ((a, b), (b, a))
        )

    def keyed(self, a, b) -> bool:
        """Whether one has a compressor keyed by the other, read from the song."""
        owners = {
            owner["id"]: owner
            for kind in ("tracks", "groups", "returns")
            for owner in self.project.get(kind, [])
        }
        return any(
            effect["type"] == "compressor"
            and effect.get("sidechain") == key
            and not effect.get("bypass")
            for one, key in ((a, b), (b, a))
            for effect in owners.get(one, {}).get("effects", [])
        )

    def over(self, contested, ha, hb, first, last, cells=slice(None)):
        """A pair's figures over a range of bands, in some of the cells: a
        moment is contested when any band of the range is."""
        bands = slice(first, last + 1)
        both = contested[cells, bands].any(axis=1)
        one, other = ha[cells, bands].any(axis=1), hb[cells, bands].any(axis=1)
        return both.sum(), (one | other).sum(), one.sum(), other.sum()

    def worst(self, contested, ha, hb, first, last, fraction) -> list[dict]:
        """Where a range is contested most: the sections when the song has
        them, and otherwise the runs of bars at least as contested as the whole."""
        found = []
        if self.sections:
            for sid, cells in self.sections:
                count, either, _, _ = self.over(contested, ha, hb, first, last, cells)
                if count:
                    at = self.at[cells]
                    found.append(
                        {
                            "section": sid,
                            "at_beat": float(at[0]),
                            "length_beats": float(at[-1] - at[0] + self.cell_beats),
                            "fraction": ratio(count, either),
                        }
                    )
        else:
            bars = np.unique(self.bars)
            counts = [
                self.over(contested, ha, hb, first, last, self.bars == bar)[:2]
                for bar in bars
            ]
            bad = [c > 0 and c / e >= fraction - 1e-3 for c, e in counts]
            for start, end in runs(bad):
                count = sum(c for c, _ in counts[start : end + 1])
                either = sum(e for _, e in counts[start : end + 1])
                found.append(
                    {
                        "at_beat": float(bars[start] * self.bar_beats),
                        "length_beats": float((bars[end] - bars[start] + 1) * self.bar_beats),
                        "fraction": ratio(count, either),
                    }
                )
        found.sort(key=lambda w: (-w["fraction"], -w["length_beats"], w["at_beat"]))
        return found[:WORST]

    def measured(self, a, b, contest, first, last) -> dict:
        """One pair over one range of bands."""
        contested, ha, hb = contest
        count, either, one, other = self.over(contested, ha, hb, first, last)
        fraction = ratio(count, either)
        marked = contested[:, first : last + 1]
        pa, pb = self.power[a][:, first : last + 1], self.power[b][:, first : last + 1]
        both = self.summed()[marked.any(axis=1), first : last + 1].sum(axis=1)
        return {
            "a": a,
            "b": b,
            "band_hz": band_hz(first, last),
            "fraction": fraction,
            "fraction_of_a": ratio(count, one),
            "fraction_of_b": ratio(count, other),
            "contested_beats": float(count * self.cell_beats),
            "level_dbfs": round(float(10 * np.log10(both.mean())), 2) if count else None,
            "level_difference_db": round(float(np.mean(10 * np.log10(pa[marked] / pb[marked]))), 2)
            if count
            else None,
            "keyed": self.keyed(a, b),
            "worst": self.worst(contested, ha, hb, first, last, fraction) if count else [],
        }

    def ranges(self, a, b, contest=None) -> list[dict]:
        """The ranges of bands a pair contests: each band contested MIN_FRACTION
        of the time either sounds in it, or MIN_FRACTION_OF_ONE of the time one
        of them does, for MIN_BEATS or more, with the bands next to each other
        as one range."""
        contested, ha, hb = contest = contest or self.contest(a, b)
        count, either = contested.sum(axis=0), (ha | hb).sum(axis=0)
        often = (count >= MIN_FRACTION * either) | (
            count >= MIN_FRACTION_OF_ONE * np.minimum(ha.sum(axis=0), hb.sum(axis=0))
        )
        named = often & (count * self.cell_beats >= MIN_BEATS)
        return [self.measured(a, b, contest, first, last) for first, last in runs(named)]

    def summary(self) -> dict:
        """`overlap` in `daw listen`: the pairs contested most of the time."""
        tracks = self.manifest["tracks"]
        order = lambda e: (-e["fraction"], -e["contested_beats"])  # noqa: E731
        found = []
        for a, b in combinations(self.power, 2):
            if self.related(a, b):
                continue
            # A pair is named once, for the bands it contests most of the time.
            most, *rest = sorted(self.ranges(a, b), key=order) or [None]
            if most:
                if rest:
                    most["other_bands_hz"] = sorted(r["band_hz"] for r in rest)
                found.append(most)
        found.sort(key=order)

        def stands_for(name):
            return {name, *tracks[name].get("tracks", [])}

        def covered(entry):
            """A group's pair that one of its tracks' pairs names more closely."""
            low, high = entry["band_hz"]
            return any(
                (other["a"], other["b"]) != (entry["a"], entry["b"])
                and {other["a"], other["b"]} <= stands_for(entry["a"]) | stands_for(entry["b"])
                and {other["a"], other["b"]} & stands_for(entry["a"])
                and {other["a"], other["b"]} & stands_for(entry["b"])
                and any(
                    lo < high and low < hi
                    for lo, hi in [other["band_hz"], *other.get("other_bands_hz", [])]
                )
                for other in found
            )

        found = [entry for entry in found if not covered(entry)]
        return {
            "cell_beats": self.cell_beats,
            "pairs": found[:LISTED],
            "pairs_omitted": max(0, len(found) - LISTED),
        }

    def level(self, name) -> list:
        """A stem's level in each band over the render, dBFS."""
        mean = self.power[name].mean(axis=0)
        return [round(float(10 * np.log10(p)), 2) if p > SILENT else None for p in mean]

    def detail(self, a, b) -> dict:
        """One pair by band and by section: `daw listen --overlap A B`."""
        for name in (a, b):
            if name not in self.power:
                raise ValueError(
                    f"No stem named {name} in this render; its stems are "
                    + ", ".join(self.power)
                )
        if a == b:
            raise ValueError("--overlap takes two different stems")
        contest = contested, ha, hb = self.contest(a, b)
        found = self.ranges(a, b, contest)

        def by_band(cells=slice(None)):
            return [
                ratio(*self.over(contested, ha, hb, band, band, cells)[:2])
                for band in range(len(BANDS))
            ]

        marked = np.where(contested, 10 * np.log10(
            np.maximum(self.power[a], SILENT) / np.maximum(self.power[b], SILENT)
        ), np.nan)
        counts = [self.over(contested, ha, hb, band, band) for band in range(len(BANDS))]
        sections = {}
        for sid, cells in self.sections:
            at = self.at[cells]
            if not len(at):
                continue
            sections[sid] = {
                "at_beat": float(at[0]),
                "length_beats": float(at[-1] - at[0] + self.cell_beats),
                "fraction": by_band(cells),
            }
            for entry in found:
                entry.setdefault("sections", {})[sid] = ratio(
                    *self.over(contested, ha, hb, *self.span(entry), cells)[:2]
                )
        return {
            "a": a,
            "b": b,
            "keyed": self.keyed(a, b),
            "related": self.related(a, b),
            "cell_beats": self.cell_beats,
            "ranges": found,
            "bands_hz": [band_hz(band, band) for band in range(len(BANDS))],
            "whole": {
                "a_dbfs": self.level(a),
                "b_dbfs": self.level(b),
                "fraction": by_band(),
                "fraction_of_a": [ratio(c, one) for c, _, one, _ in counts],
                "fraction_of_b": [ratio(c, other) for c, _, _, other in counts],
                "level_difference_db": [
                    round(float(np.nanmean(marked[:, band])), 2) if c else None
                    for band, (c, _, _, _) in enumerate(counts)
                ],
            },
            "sections": sections,
        }

    def span(self, entry) -> tuple[int, int]:
        """The bands a reported range covers."""
        low, high = entry["band_hz"]
        first = next(i for i in range(len(BANDS)) if hz(BANDS[i][0]) == low)
        last = next(i for i in range(len(BANDS)) if hz(BANDS[i][1]) == high)
        return first, last

    def fraction(self, entry, cells=slice(None)) -> float | None:
        """A reported range's fraction in this render, where it has both stems."""
        if entry["a"] not in self.power or entry["b"] not in self.power:
            return None
        contest = self.contest(entry["a"], entry["b"])
        return ratio(*self.over(*contest, *self.span(entry), cells)[:2])


def change(before, after):
    return round(after - before, 3) if before is not None and after is not None else None


def between(before: Grid | None, after: Grid | None, a, b, aligned) -> dict | None:
    """`overlap` in `daw compare`: each pair either report names, over the bands
    it was named for, before and after, and by section where the two align."""
    if before is None or after is None:
        return None
    named = list(a["overlap"]["pairs"])
    for entry in b["overlap"]["pairs"]:
        # A pair both name is compared once, over the bands the first named.
        if not any((e["a"], e["b"]) == (entry["a"], entry["b"]) for e in named):
            named.append(entry)
    pairs = []
    for entry in named:
        was, now = before.fraction(entry), after.fraction(entry)
        row = {
            "a": entry["a"],
            "b": entry["b"],
            "band_hz": entry["band_hz"],
            "before": was,
            "after": now,
            "delta": change(was, now),
            "keyed_before": before.keyed(entry["a"], entry["b"]) if was is not None else None,
            "keyed_after": after.keyed(entry["a"], entry["b"]) if now is not None else None,
            "sections": {},
        }
        left, right = dict(before.sections), dict(after.sections)
        for sid in aligned:
            was, now = before.fraction(entry, left[sid]), after.fraction(entry, right[sid])
            row["sections"][sid] = {"before": was, "after": now, "delta": change(was, now)}
        pairs.append(row)
    pairs.sort(key=lambda r: -abs(r["delta"] or 0))
    return {"pairs": pairs}


METHOD = (
    "each stem's power in cells of a band by a sixteenth note, never under 100 ms: "
    "third octaves from 89 Hz to 20 kHz and 20-45 and 45-89 Hz below; two stems "
    f"contest a cell when the weaker is within {CLOSE_DB:g} dB of the stronger and "
    f"within {HEARD_DB:g} dB of the stems' sum there, in a cell of the sum within "
    f"{FLOOR_DB:g} dB of its band's loudest and {EMPTY_DB:g} dB of the loudest in "
    f"any band, and not a louder band's skirt, {SKIRT_DB:g} dB under the band beside "
    f"it or {SKIRT_SLOPE_DB:g} dB more for each band further; fraction is the contested "
    "moments over those where either sounds, and fraction_of_a and fraction_of_b over "
    "those where that stem does; a pair is named for the bands it contests "
    f"{MIN_FRACTION:g} of the time either sounds or {MIN_FRACTION_OF_ONE:g} of the time "
    f"one does, for {MIN_BEATS:g} beats or more; overlap of energy, not masking as an "
    "ear does it"
)

# The chart's colors: one hue a stem, a neutral ink where the two contest.
SURFACE, INK, MUTED = "#fcfcfb", "#0b0b0b", "#52514e"
A_COLOR, B_COLOR = "#2a78d6", "#eb6834"


def plot(grid: Grid, a, b, path):
    """Bands up, beats across: each cell in the color of the stem that is
    louder there, and in ink where the two contest it."""
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.colors import ListedColormap
    from matplotlib.figure import Figure
    from matplotlib.patches import Patch

    contested, ha, hb = grid.contest(a, b)
    louder = grid.power[a] >= grid.power[b]
    state = np.zeros(contested.shape)
    state[(ha | hb) & louder] = 1
    state[(ha | hb) & ~louder] = 2
    state[contested] = 3
    # A long song gets a wider picture, so that a cell is still a pixel or two.
    width = float(np.clip(len(grid.at) / 60, 12, 30))
    fig = Figure(figsize=(width, 5), layout="constrained", facecolor=SURFACE)
    FigureCanvasAgg(fig)
    ax = fig.subplots()
    ax.set_facecolor(SURFACE)
    edges = np.append(grid.at, grid.at[-1] + grid.cell_beats)
    ax.pcolormesh(
        edges,
        np.arange(len(BANDS) + 1),
        state.T,
        cmap=ListedColormap([SURFACE, A_COLOR, B_COLOR, INK]),
        vmin=0,
        vmax=3,
        shading="flat",
        rasterized=True,
    )
    ticks = [i for i, edge in enumerate(EDGES) if i % 3 == 2 or i in (0, len(EDGES) - 1)]
    ax.set_yticks(ticks, [f"{hz(EDGES[i]):g}" for i in ticks])
    ax.set_ylabel("Band edge (Hz)", color=MUTED)
    ax.set_xlabel("Quarter-note beat (zero-based)", color=MUTED)
    ax.set_title(f"Overlap of {a} and {b}", color=INK, loc="left")
    for sid, cells in grid.sections:
        if cells.any():
            at = grid.at[cells][0]
            ax.axvline(at, color=MUTED, alpha=0.5, linewidth=0.8)
            ax.text(
                at, 0.985, f" {sid}", transform=ax.get_xaxis_transform(), va="top", fontsize=8, color=MUTED
            )
    ax.tick_params(colors=MUTED)
    for spine in ax.spines.values():
        spine.set_color(MUTED)
        spine.set_alpha(0.4)
    fig.legend(
        handles=[
            Patch(color=A_COLOR, label=f"{a} louder"),
            Patch(color=B_COLOR, label=f"{b} louder"),
            Patch(color=INK, label="contested"),
        ],
        loc="outside lower center",
        ncols=3,
        frameon=False,
        labelcolor=INK,
    )
    fig.savefig(path, dpi=140)
    return str(path.resolve())
