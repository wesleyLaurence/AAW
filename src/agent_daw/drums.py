"""The drum groove as notes: `daw transcribe --drums`.

A drum part's kicks, snares and hats, heard from its audio, as notes on the
song's beat map: General MIDI's 36, 38 and 42, played by a Sampler whose pads
are the part's own clearest hit of each. The bars that play the same groove are
found, and the part is written as a looped clip a groove and a clip a fill.

1. Where something hits: the hits `daw samples hits` finds in the kick, snare
   and hats bands, those within TOGETHER of each other one moment.
2. What hits there. A band says where a drum sounds most, not which drum it
   is: a kick's beater click is as loud in the snare's band as a snare can be,
   and a snare's rattle reaches the hats'. So each drum's sound is learned
   from the part itself, as the change in 24 bands over the 90 ms after a hit,
   from the hits only its own band shows, and every moment is read as a sum of
   the drums whose bands have a hit there. The sounds start from the hits only
   one band shows and are then fitted to every moment together (non-negative
   matrix factorization), so a hat on every kick is not learned as the kick's.
   The bands over 300 Hz are read from 5 ms on, after a beater's click.
3. A drum plays at a moment when its band has a hit there and its share is
   within MIN_LEVEL_DB of its loudest hits; its level gives the velocity.
   A moment no drum explains, a tom or a cymbal, is counted, not written.
4. Grooves. Each bar's hits on its sixteenths are its pattern. A groove is
   the hits most of its bars play, and a bar is of it when MATCH of the hits
   either has are shared; the one most bars are near is groove A, then B from
   the bars left, and so on while one holds GROOVE_BARS bars. Bars of one
   groove in a row are one clip looped a bar, two grooves in turn one looped
   two bars, and each note of a groove sits at the median velocity and timing
   of its bars. The other bars are fills, written as played.

Every value is an estimate from decoded audio; nothing here listens.
"""

from __future__ import annotations

import json
import tempfile
from collections import Counter
from pathlib import Path

import numpy as np

from . import beats, hits

DRUMS = ("kick", "snare", "hat")
BAND = {"kick": "kick", "snare": "snare", "hat": "hats"}
NOTE = {"kick": 36, "snare": 38, "hat": 42}
NOTE_NAME = {"kick": "C2", "snare": "D2", "hat": "F#2"}
TOGETHER = 0.030  # hits in two bands this near are one moment
DECAY = 0.050  # how far a band's level falls this long after a hit
CLICK_DB = 15.0  # a hit in the snare's band falling this far in DECAY is a click
QUIET_DB = -15.0  # another band this far under its loudest leaves a drum alone
CANDIDATE_DB = -35.0  # a band's hits this far under its loudest are looked at
MIN_LEVEL_DB = -15.0  # a drum this far under its loudest hits is not heard
DRIFT_MS = 20.0  # a part whose beats move this far from one tempo is warned of
VELOCITY_RANGE_DB = 40.0  # velocity 127 at the loudest hit, 1 this far under
EDGES = np.geomspace(40.0, 16000.0, 25)  # 24 bands
SPLIT_HZ = 300.0  # bands under this from a long window, the rest a short one
LONG, SHORT, HOP = 4096, 512, 128  # STFT samples at 44.1 kHz, scaled to the rate
SPAN = 0.090  # a moment's sound is read over this
AFTER_CLICK = 0.005  # the bands over SPLIT_HZ from this long after the hit
LEARN_ROUNDS = 30  # updates of the sounds learned together; more overfit a busy stem
GROOVE_BARS = 4  # a pattern this many bars share is a groove
MATCH = 0.8  # a bar sharing this much of the hits either has with a groove is it
DURATION = 0.25  # a written hit's length in beats; the pads play it whole


# --- what hits, and when ---------------------------------------------------------


def moments(mono, sr) -> list[dict]:
    """Every moment some band has a hit: its time, each band's level there against
    that band's loudest hit, and how far each falls in the DECAY after."""
    found = []
    for drum in DRUMS:
        low, high = hits.BANDS[BAND[drum]]
        rows, db, step = hits.find(mono, sr, low, high)
        if not rows:
            continue
        top = max(r[1] for r in rows)
        for t, level, rise in rows:
            if level - top < CANDIDATE_DB:
                continue
            i = int(round(t / step))
            peak = float(db[i : i + max(1, int(0.020 / step))].max())
            later = float(db[min(len(db) - 1, i + int(DECAY / step))])
            found.append((t, drum, level - top, rise, peak - later))
    found.sort()
    out = []
    for t, drum, level, rise, fall in found:
        if out and t - out[-1]["first"] < TOGETHER:
            here = out[-1]
            if level > here["bands"].get(drum, -999):
                here["bands"][drum], here["falls"][drum] = level, fall
            if rise > here["rise"]:
                here["seconds"], here["rise"] = t, rise
            continue
        out.append({"seconds": t, "first": t, "rise": rise, "bands": {drum: level}, "falls": {drum: fall}})
    for m in out:
        del m["first"]
    return out


class Sounds:
    """The change in each of 24 bands over SPAN after a time: the low bands from a
    long window, the rest from a short one AFTER_CLICK on, each against the band
    just before the hit."""

    def __init__(self, mono, sr):
        from scipy.signal import stft

        scale = sr / 44100
        self.sr = sr
        self.hop = max(1, round(HOP * scale))
        x = mono.astype(np.float64)
        self.rows = []
        for size in (round(LONG * scale), round(SHORT * scale)):
            if len(x) < size:
                x = np.pad(x, (0, size - len(x)))
            f, _, z = stft(x, sr, nperseg=size, noverlap=size - self.hop, boundary=None, padded=False)
            power = np.abs(z) ** 2
            for low, high in zip(EDGES[:-1], EDGES[1:]):
                if (size == round(LONG * scale)) == (high <= SPLIT_HZ):
                    self.rows.append((size, np.sqrt(power[(f >= low) & (f < high)].sum(0))))
        self.frames = max(1, int(SPAN * sr / self.hop))

    def of(self, seconds):
        out = []
        for size, band in self.rows:
            centre = round((seconds * self.sr - size / 2) / self.hop)
            before = band[max(0, centre - round((size / 2 + 0.005 * self.sr) / self.hop))]
            first = centre + (round(AFTER_CLICK * self.sr / self.hop) if size < LONG * self.sr / 44100 else 0)
            piece = band[max(0, first) : max(0, first) + self.frames]
            if len(piece) < self.frames:
                piece = np.pad(piece, (0, self.frames - len(piece)))
            out.append(np.maximum(piece - before, 0))
        return np.concatenate(out)


def unit(rows):
    rows = np.atleast_2d(rows)
    return rows / (np.linalg.norm(rows, axis=1, keepdims=True) + 1e-12)


def heard(mono, sr) -> dict:
    """Each moment with the drums that play in it and their levels, the drums' own
    clearest moments and how many moments no drum explains."""
    from scipy.optimize import nnls

    found = moments(mono, sr)
    if not found:
        return {"moments": [], "drums": [], "clearest": {}, "unnamed": 0}
    sounds = Sounds(mono, sr)
    x = np.stack([sounds.of(m["seconds"]) for m in found])
    # The first sound of each drum: the moments only its own band shows, loud there.
    # A kick's beater click sounds in the snare's band and is gone in DECAY; a
    # snare's rattle is not, and a kick with one is not a kick alone.
    seeds = {
        "kick": [
            i
            for i, m in enumerate(found)
            if m["bands"].get("kick", -999) >= -6 and m["falls"].get("snare", CLICK_DB) >= CLICK_DB
        ],
        "snare": [
            i
            for i, m in enumerate(found)
            if m["bands"].get("snare", -999) >= -12 and m["bands"].get("kick", -999) < QUIET_DB
        ],
        "hat": [
            i
            for i, m in enumerate(found)
            if m["bands"].get("hat", -999) >= -15
            and m["bands"].get("kick", -999) < QUIET_DB
            and m["bands"].get("snare", -999) < QUIET_DB
        ],
    }
    drums = [d for d in DRUMS if len(seeds[d]) >= 3]
    if not drums:
        return {"moments": [], "drums": [], "clearest": {}, "unnamed": len(found)}
    sound = np.stack([np.median(unit(x[seeds[d]]), axis=0) for d in drums], axis=1)

    # Which drums may sound at each moment: those whose band has a hit there.
    allowed = np.array([[d in m["bands"] for d in drums] for m in found])

    def gains_of(sound):
        g = np.zeros((len(x), len(drums)))
        for i, row in enumerate(x):
            cols = np.flatnonzero(allowed[i])
            if len(cols):
                g[i, cols] = nnls(sound[:, cols], row)[0]
        return g

    # Learned together: every moment is the sum of the drums its bands allow, and
    # the sounds and their levels are fitted to all of them (non-negative matrix
    # factorization, by multiplicative updates). A drum that always sounds with
    # another, a hat on every kick, is then not learned as part of it: the moments
    # it sounds alone say what it is, and the other's sound is what is left.
    sound = sound / (np.linalg.norm(sound, axis=0, keepdims=True) + 1e-12)
    for _ in range(LEARN_ROUNDS):
        gains = gains_of(sound)
        sound = sound * (x.T @ gains) / (sound @ (gains.T @ gains) + 1e-12)
        sound = sound / (np.linalg.norm(sound, axis=0, keepdims=True) + 1e-12)
    gains = gains_of(sound)
    tops = np.array([np.percentile(g[g > 0], 90) if (g > 0).any() else 1.0 for g in gains.T])
    rel = 20 * np.log10(gains / tops + 1e-12)
    fit = np.linalg.norm(x - gains @ sound.T, axis=1) / (np.linalg.norm(x, axis=1) + 1e-12)
    out, unnamed = [], 0
    for i, m in enumerate(found):
        playing = {
            d: float(rel[i, k])
            for k, d in enumerate(drums)
            if d in m["bands"] and rel[i, k] >= MIN_LEVEL_DB
        }
        if playing:
            out.append({"seconds": m["seconds"], "drums": playing})
        elif fit[i] > 0.5 and np.linalg.norm(x[i]) > 0:
            unnamed += 1
    # Each drum's level against its loudest hits; velocity from it.
    for k, d in enumerate(drums):
        levels = [m["drums"][d] for m in out if d in m["drums"]]
        if levels:
            top = float(np.percentile(levels, 99))
            for m in out:
                if d in m["drums"]:
                    m["drums"][d] = min(0.0, m["drums"][d] - top)
    clearest = {}
    for k, d in enumerate(drums):
        others = [j for j in range(len(drums)) if j != k]
        score = [
            (rel[i, k] - max([rel[i, j] for j in others], default=-120) - 20 * fit[i], i)
            for i, m in enumerate(found)
            if d in m["bands"] and rel[i, k] > -3
        ]
        if score:
            clearest[d] = found[max(score)[1]]["seconds"]
    return {"moments": out, "drums": drums, "clearest": clearest, "unnamed": unnamed}


def velocity(level_db: float) -> int:
    return int(np.clip(round(127 + level_db * 127 / VELOCITY_RANGE_DB), 1, 127))


# --- on the beat map --------------------------------------------------------------


class Bars:
    """The beat map's bars as beats from bar 1: where each starts and how many beats
    it holds, continued at the map's bar length before and after it."""

    def __init__(self, report, origin):
        self.per_bar = int(report["beats_per_bar"])
        starts, lengths = {}, Counter()
        for i, b in enumerate(report["beats"]):
            starts.setdefault(b["bar"], i - origin)
            lengths[b["bar"]] += 1
        self.first, self.last = min(starts), max(starts)
        self.starts = starts
        self.lengths = {bar: (n if bar != self.last else max(n, self.per_bar)) for bar, n in lengths.items()}

    def start(self, bar):
        if bar < self.first:
            return self.starts[self.first] - (self.first - bar) * self.per_bar
        if bar > self.last:
            return self.starts[self.last] + self.lengths[self.last] + (bar - self.last - 1) * self.per_bar
        return self.starts[bar]

    def length(self, bar):
        return self.lengths.get(bar, self.per_bar)

    def of(self, beat):
        """The bar a beat from bar 1 falls in."""
        bar = self.first
        if beat < self.start(self.first):
            return self.first - int(np.ceil((self.start(self.first) - beat) / self.per_bar - 1e-9))
        while self.start(bar + 1) <= beat + 1e-9:
            bar += 1
        return bar


def placed(found, report) -> tuple[list[dict], "Bars"]:
    """Each drum's hit as a row: its time, its place on the map, beats from bar 1,
    its bar and sixteenth in it, level and velocity."""
    from .transcription import first_bar

    grid = hits.Grid(report)
    origin = first_bar(report)
    bars = Bars(report, origin)
    rows = []
    for m in found:
        where, index = grid.place(m["seconds"])
        beat = float(index - origin)
        cell = round(beat * 4)
        bar = bars.of(cell / 4)
        for d, level in m["drums"].items():
            rows.append(
                {
                    "drum": d,
                    "note": NOTE_NAME[d],
                    "midi": NOTE[d],
                    "seconds": round(m["seconds"], 4),
                    **where,
                    "beats": round(beat, 4),
                    "_bar": bar,
                    "_cell": cell - round(bars.start(bar) * 4),
                    "level_db": round(level, 1),
                    "velocity": velocity(level),
                }
            )
    # A drum on one sixteenth twice is once, the louder.
    best = {}
    for r in rows:
        key = (r["drum"], r["_bar"], r["_cell"])
        if key not in best or r["level_db"] > best[key]["level_db"]:
            best[key] = r
    rows = sorted(best.values(), key=lambda r: (r["seconds"], DRUMS.index(r["drum"])))
    return rows, bars


# --- grooves and fills ------------------------------------------------------------


def similar(a, b) -> float:
    """How alike two bars' patterns are: the hits they share against all either has."""
    if a[0] != b[0]:
        return 0.0
    both = len(a[1] | b[1])
    return len(a[1] & b[1]) / both if both else 1.0


def grooves(rows, bars) -> dict:
    """Which bars play the same groove: each bar's pattern, the grooves they share,
    the bars of each and how a bar of one differs from it, and the fills.

    A groove is the hits most of its bars play, and a bar is of it when at least
    MATCH of the hits either has are shared: a bar a hit or two from its groove,
    as the hearing misses or adds on a busy stem, is still it. Each groove is
    taken in turn from the bars left, the one most bars are near first."""
    by_bar = {}
    for r in rows:
        by_bar.setdefault(r["_bar"], []).append(r)
    if not by_bar:
        return {"labels": {}, "grooves": {}, "differs": {}, "by_bar": {}}
    first, last = min(by_bar), max(by_bar)
    pattern = {
        bar: (bars.length(bar), frozenset((r["drum"], r["_cell"]) for r in by_bar.get(bar, [])))
        for bar in range(first, last + 1)
    }

    def common(members, size):
        counted = Counter(hit for b in members for hit in pattern[b][1])
        return (size, frozenset(hit for hit, n in counted.items() if n * 2 > len(members)))

    labels, found = {}, {}
    left = {bar for bar in pattern if pattern[bar][1]}
    while left:
        best, members = None, []
        for candidate, _ in Counter(pattern[b] for b in left).most_common(8):
            groove = candidate
            for _ in range(3):
                near = [b for b in left if similar(pattern[b], groove) >= MATCH]
                if not near:
                    break
                groove = common(near, candidate[0])
            near = [b for b in left if similar(pattern[b], groove) >= MATCH]
            if len(near) > len(members) and groove[1]:
                best, members = groove, near
        if best is None or len(members) < GROOVE_BARS:
            break
        name = chr(ord("A") + len(found)) if len(found) < 26 else f"G{len(found) + 1}"
        for b in members:
            labels[b] = name
        found[name] = best
        left -= set(members)
    differs = {}
    for b, name in labels.items():
        extra = pattern[b][1] - found[name][1]
        missing = found[name][1] - pattern[b][1]
        if extra or missing:
            differs[b] = (sorted(extra, key=order), sorted(missing, key=order))
    return {"labels": labels, "grooves": found, "differs": differs, "first": first, "last": last, "by_bar": by_bar}


def order(hit):
    return (hit[1], DRUMS.index(hit[0]))


def where(cell) -> str:
    """A cell of a bar as BEAT.N, its sixteenth in the beat from 1."""
    return f"{cell // 4 + 1}.{cell % 4 + 1}"


def clips(rows, bars, shape) -> list[dict]:
    """The clips the part is written as: a run of one groove looped a bar, two
    grooves in turn looped two bars, and the bars between them as played."""
    labels, first, last = shape["labels"], shape.get("first"), shape.get("last")
    if first is None:
        return []
    out, bar = [], first
    while bar <= last:
        label = labels.get(bar)
        if label is None:
            if not shape["by_bar"].get(bar):
                bar += 1  # a bar with no hit is no clip
                continue
            run = [bar]
            while run[-1] + 1 <= last and labels.get(run[-1] + 1) is None and shape["by_bar"].get(run[-1] + 1):
                run.append(run[-1] + 1)
            out.append({"kind": "fill", "bars": run, "loop_bars": None, "grooves": []})
            bar = run[-1] + 1
            continue
        # One groove again and again, or two in turn: whichever runs longer.
        one = 1
        while labels.get(bar + one) == label:
            one += 1
        other = labels.get(bar + 1)
        two = 0
        if other is not None and other != label:
            while labels.get(bar + two) == (label, other)[two % 2]:
                two += 1
            two -= two % 2
        if two >= 4 and two > one:
            out.append({"kind": "groove", "bars": list(range(bar, bar + two)), "loop_bars": 2, "grooves": [label, other]})
            bar += two
        else:
            out.append({"kind": "groove", "bars": list(range(bar, bar + one)), "loop_bars": 1, "grooves": [label]})
            bar += one
    return out


def groove_notes(shape, name, bars) -> list[dict]:
    """A groove's notes in its first bar: each hit at the median velocity and timing
    its bars play it with."""
    size, cells = shape["grooves"][name]
    members = [b for b, label in shape["labels"].items() if label == name]
    notes = []
    for drum, cell in sorted(cells, key=lambda h: (h[1], DRUMS.index(h[0]))):
        played = [
            r for b in members for r in shape["by_bar"].get(b, []) if r["drum"] == drum and r["_cell"] == cell
        ]
        offset = float(np.median([r["beats"] - bars.start(r["_bar"]) - cell / 4 for r in played])) if played else 0.0
        vel = int(np.median([r["velocity"] for r in played])) if played else 100
        notes.append({"drum": drum, "at": cell / 4 + offset, "velocity": vel})
    return notes


# --- writing ----------------------------------------------------------------------


def cut(mono, sr, at, found_times, path):
    """The part's own hit at `at`, to the next moment or a second on, faded out, as
    a WAV file."""
    import soundfile as sf

    later = [t for t in found_times if t > at + 0.02]
    end = min(at + 1.0, later[0] if later else at + 1.0, len(mono) / sr)
    a, b = max(0, round((at - 0.002) * sr)), max(round((at - 0.002) * sr) + 1, round(end * sr))
    piece = mono[a:b].astype(np.float64).copy()
    fade = min(len(piece), round(0.005 * sr))
    if fade:
        piece[-fade:] *= np.linspace(1, 0, fade)
        piece[: min(len(piece), round(0.0005 * sr))] *= np.linspace(0, 1, min(len(piece), round(0.0005 * sr)))
    peak = float(np.abs(piece).max()) if len(piece) else 0.0
    if peak > 0:
        piece *= 0.9 / peak
    sf.write(path, piece.astype(np.float32), sr, subtype="FLOAT")
    return round(end - at, 3)


def note_rows(notes, scale):
    """Notes as a clip holds them, a hit a sixteenth long, none before the clip's start."""
    out = []
    for n in sorted(notes, key=lambda n: (n["at"], n["drum"])):
        out.append(
            {
                "pitch": NOTE[n["drum"]],
                "at": round(max(0.0, n["at"]) * scale, 6),
                "duration": round(DURATION * scale, 6),
                "velocity": n["velocity"],
            }
        )
    return out


def parts(layout, bars, shape, rows, as_played) -> list[dict]:
    """Each clip the part is written as: its ID, kind and bars, where it starts and
    how long it is in the part's beats from bar 1, its loop, the beats of each of
    its bars and its notes in beats from its start."""
    if as_played:
        layout = [{"kind": "played", "bars": sorted({r["_bar"] for r in rows}), "loop_bars": None, "grooves": []}]
    out = []
    for n, part in enumerate(layout, 1):
        first, last = part["bars"][0], part["bars"][-1]
        start = bars.start(first)
        if part["kind"] == "groove":
            loop_bars = part["loop_bars"]
            notes = []
            for k, name in enumerate(part["grooves"]):
                offset = bars.start(first + k) - start
                notes += [{**g, "at": g["at"] + offset} for g in groove_notes(shape, name, bars)]
            ident = f"groove-{''.join(part['grooves']).lower()}-{n}"
            written_bars = range(first, first + loop_bars)
        else:
            loop_bars = None
            members = set(range(first, last + 1))
            notes = [
                {"drum": r["drum"], "at": r["beats"] - start, "velocity": r["velocity"]}
                for r in rows
                if r["_bar"] in members
            ]
            ident = f"{part['kind'] if part['kind'] == 'fill' else 'drums'}-{n}"
            written_bars = range(first, last + 1)
        out.append(
            {
                "id": ident,
                "kind": part["kind"],
                "bars": [first, last],
                **({"grooves": part["grooves"], "loop_bars": loop_bars} if part["kind"] == "groove" else {}),
                "start": start,
                "length": bars.start(last + 1) - start,
                "loop": None if loop_bars is None else bars.start(first + loop_bars) - start,
                "bar_beats": [bars.length(b) for b in written_bars],
                "notes": notes,
            }
        )
    return out


def batch(track, give, written_parts, scale, bar_one_at, samples, taken=()):
    """The commands that write the part, the clips they make and how many hits
    would come before the song's start. `give` is "track" for a new track with a
    Sampler of the samples' pads, "sampler" for an existing track without an
    instrument, and None to leave the track's instrument."""
    commands = [{"op": "set", "path": f"samples.{sid}", "value": asset} for sid, asset in samples.items()]
    pads = {d: {"sample": f"{track}-{d}"} for d in DRUMS if f"{track}-{d}" in samples}
    sampler = {"sampler": {"pads": pads, "map": [{"notes": NOTE[d], "pad": d} for d in pads]}}
    if give == "track":
        commands.append({"op": "track.add", "id": track, "type": "midi", "instrument": sampler})
    elif give == "sampler":
        commands.append({"op": "instrument.set", "track": track, "instrument": sampler})
    made, skipped, taken = [], 0, set(taken)
    for part in written_parts:
        ident, n = part["id"], 2
        while ident in taken:
            ident, n = f"{part['id']}-{n}", n + 1
        taken.add(ident)
        at = bar_one_at + part["start"] * scale
        if at < -1e-9:
            skipped += len(part["notes"])
            continue
        commands.append(
            {
                "op": "clip.add",
                "track": track,
                "id": ident,
                "at": round(at, 6),
                "length_beats": round(part["length"] * scale, 6),
                **({"loop_beats": round(part["loop"] * scale, 6)} if part["loop"] else {}),
                "notes": note_rows(part["notes"], scale),
            }
        )
        made.append(
            {
                "ref": f"tracks.{track}.clips.{ident}",
                "kind": part["kind"],
                "bars": part["bars"],
                **({"grooves": part["grooves"], "loop_bars": part["loop_bars"]} if "grooves" in part else {}),
                "at": round(at, 4),
                "length_beats": round(part["length"] * scale, 4),
            }
        )
    return commands, made, skipped


def as_text(notes, bars_beats, grid) -> tuple[str, int]:
    """A clip's notes as note text, a row a drum, kick first, a cell of `grid`
    beats and a checked bar line between bars; how many notes fall between
    cells. Each note is on its sixteenth there; its timing stays in the clip."""
    from fractions import Fraction

    step = Fraction(grid)
    cells_in = [int(Fraction(n) / step) for n in bars_beats]
    total = sum(cells_in)
    rows, between = [], 0
    for drum in DRUMS:
        mine = [n for n in notes if n["drum"] == drum]
        if not mine:
            continue
        line = ["."] * total
        for n in mine:
            sixteenth = Fraction(round(n["at"] * 4), 4)
            cell = sixteenth / step
            if cell.denominator != 1:
                between += 1
            cell = min(max(int(round(float(cell))), 0), total - 1)
            line[cell] = NOTE_NAME[drum].lower() + ("" if n["velocity"] == 100 else f"@{n['velocity']}")
        words, at = [], 0
        for k, size in enumerate(cells_in):
            if k:
                words.append("|")
            words += line[at : at + size]
            at += size
        rows.append(" ".join(words))
    return "\n".join(rows), between


def drift_ms(report, rows) -> float:
    """How far the part's beats move from one steady tempo over its hits, in ms:
    how far its notes, placed by beat, move from its audio in a song of one tempo."""
    if not rows:
        return 0.0
    from .transcription import first_bar

    times = np.array([b["seconds"] for b in report["beats"]])
    index = np.arange(len(times)) - first_bar(report)
    steady = report["first_downbeat_seconds"] + index * 60 / report["tempo"]["bpm"]
    first, last = min(r["seconds"] for r in rows), max(r["seconds"] for r in rows)
    inside = (times >= first - 1) & (times <= last + 1)
    return float(np.abs(times - steady)[inside].max() * 1000) if inside.any() else 0.0


def runs(numbers):
    """Bar numbers as [from, to] runs."""
    out = []
    for n in sorted(numbers):
        if out and n == out[-1][1] + 1:
            out[-1][1] = n
        else:
            out.append([n, n])
    return out


def transcribe(
    source: str,
    song: Path | None = None,
    track: str | None = None,
    at: float | None = None,
    beats_of: str | None = None,
    near: float | None = None,
    window: float = 10.0,
    midi_out: Path | None = None,
    text: bool = False,
    grid: str | None = None,
    as_played: bool = False,
    db: Path | None = None,
) -> dict:
    from . import library
    from .transcription import Played, resolve, separated_from

    db = db or library.default_db()
    writes = bool(song is not None or midi_out is not None or text)
    if grid is not None and not text:
        raise ValueError("--grid is the cells of --text")
    if as_played and not writes:
        raise ValueError("--as-played is how the part is written: give it with --text, --midi or --project")
    if midi_out is not None:
        if midi_out.suffix.lower() not in (".mid", ".midi"):
            raise ValueError("--midi is a .mid file")
        if not midi_out.resolve().parent.is_dir():
            raise ValueError(f"--midi: there is no folder {midi_out.resolve().parent}")
    path, label, clip = resolve(source, song, db)
    if clip is None and at is None and song is not None:
        at = 0.0
    if clip is not None and at is not None:
        raise ValueError("An audio clip places its notes where it plays; --at goes with a sample or a file")
    of = library.resolve(db, beats_of) if beats_of else separated_from(path) or path
    try:
        report = library.beat_map(db, of)
    except ValueError as error:
        raise ValueError(f"The hits are placed on a beat map, and {of.name} has none: {error}") from None
    mono, sr = beats.read(path)
    played = Played(clip, len(mono) / sr) if clip is not None else None
    found = heard(mono, sr)
    moments_ = found["moments"]
    left_out = {}
    if played is not None:
        inside = [m for m in moments_ if played.start <= m["seconds"] < played.end]
        left_out["outside_the_clip"] = sum(len(m["drums"]) for m in moments_) - sum(len(m["drums"]) for m in inside)
        moments_ = inside
    rows, bars = placed(moments_, report)
    shape = grooves(rows, bars)
    layout = clips(rows, bars, shape)
    out = {
        "path": str(path),
        "beat_map": report.get("map"),
        **({"beat_map_of": str(of)} if of != path else {}),
        "tempo_bpm": report["tempo"]["bpm"],
        "meter": report["meter"],
        "first_downbeat_seconds": report["first_downbeat_seconds"],
        "odd_bars": report.get("odd_bars", []),
        "drums": {
            d: {
                "note": NOTE_NAME[d],
                "midi": NOTE[d],
                "count": sum(1 for r in rows if r["drum"] == d),
                **({"clearest_seconds": round(found["clearest"][d], 4)} if d in found["clearest"] else {}),
            }
            for d in DRUMS
        },
        "unnamed": found["unnamed"],
        "count": len(rows),
        "grooves": [
            {
                "id": name,
                "bars": runs(b for b, label in shape["labels"].items() if label == name),
                "count": sum(1 for label in shape["labels"].values() if label == name),
                "beats": size,
                "hits": len(cells),
                "differs": [
                    {
                        "bar": b,
                        **({"extra": [f"{d} {where(c)}" for d, c in extra]} if extra else {}),
                        **({"missing": [f"{d} {where(c)}" for d, c in missing]} if missing else {}),
                    }
                    for b, (extra, missing) in sorted(shape["differs"].items())
                    if shape["labels"][b] == name
                ],
            }
            for name, (size, cells) in shape["grooves"].items()
        ],
        "fills": [[part["bars"][0], part["bars"][-1]] for part in layout if part["kind"] == "fill"],
        "left_out": left_out,
    }
    warnings = []
    if not found["drums"]:
        warnings.append("No kick, snare or hat was heard; is this a drum part?")
    else:
        missing = [d for d in DRUMS if d not in found["drums"]]
        if missing:
            warnings.append(f"Too few hits of {', '.join(missing)} to learn its sound; none are written")
    if played is not None and played.loops:
        warnings.append(f"{clip['ref']} loops; the note clips hold its first pass")
    drift = drift_ms(report, rows)
    if writes and drift >= DRIFT_MS:
        low, high = report["tempo"].get("bpm_range") or (report["tempo"]["bpm"],) * 2
        warnings.append(
            f"The part's tempo moves ({low:g}–{high:g} BPM): its notes are placed by its beats, so at one "
            f"tempo they move from its audio by up to {drift:.0f} ms"
        )
    if writes:
        if not rows:
            raise ValueError("No hit is left to write")
        made = written(
            rows, bars, layout, shape, report, label, song, track, at, played, midi_out, text,
            grid or "1/4", as_played, mono, sr, found, path,
        )
        if "before_the_song" in made:
            left_out["before_the_song"] = made.pop("before_the_song")
        out.update(made)
    listed = [{k: v for k, v in r.items() if not k.startswith("_")} for r in rows]
    if near is not None:
        listed = [r for r in listed if abs(r["seconds"] - near) <= window]
        out["near"] = {"seconds": near, "window_seconds": window, "of": len(rows)}
    out["hits"] = listed
    if warnings:
        out["warnings"] = warnings
    out["source"] = (
        "measured from decoded audio: each drum's sound learned from the part's own hits, placed on the beat "
        "map; estimates, not ground truth"
    )
    return out


def written(rows, bars, layout, shape, report, label, song, track, at, played, midi_out, text, grid, as_played, mono, sr, found, path) -> dict:
    """The part as note text, a MIDI file of every hit as played, and in the song:
    the pads cut from its clearest hits, then the clips, by one batch, so the
    song is changed last and once and the part is one edit to undo."""
    from fractions import Fraction

    from . import library
    from .cli import run_engine
    from .model import load
    from .transcription import midi_file

    out = {}
    per_bar = bars.per_bar
    earliest = min(r["beats"] for r in rows)
    pickup = float(per_bar * np.ceil(-earliest / per_bar - 1e-9)) if earliest < 0 else 0.0
    if midi_out is not None:
        target = midi_out.resolve()
        midi_file([{**r, "length_beats": DURATION} for r in rows], pickup, target)
        out["midi"] = {"path": str(target), "bar_one_beat": pickup, "as_played": True}
    written_parts = parts(layout, bars, shape, rows, as_played)
    if text:
        try:
            step = Fraction(grid)
        except (ValueError, ZeroDivisionError):
            step = None
        if step is None or step <= 0 or any((Fraction(b) / step).denominator != 1 for b in {bars.per_bar, *(b for p in written_parts for b in p["bar_beats"])}):
            raise ValueError(f"--grid is a cell's beats that fit a bar, such as 1/4 or 1/2, not {grid}")
        out["text"] = {"grid": grid, "clips": []}
        for part in written_parts:
            shown = [n for n in part["notes"] if n["at"] < sum(part["bar_beats"])]
            body, between = as_text(shown, part["bar_beats"], grid)
            out["text"]["clips"].append(
                {
                    "id": part["id"],
                    "kind": part["kind"],
                    "bars": part["bars"],
                    **({"loop_bars": part["loop_bars"]} if "loop_bars" in part else {}),
                    "text": body,
                    **({"between_cells": between} if between else {}),
                }
            )
    if song is None:
        return out
    project = load(song, False)
    tracks = project.get("tracks", [])
    taken = ()
    if track is None:
        track, n = "drums", 2
        while any(t["id"] == track for t in tracks):
            track, n = f"drums-{n}", n + 1
        give = "track"
    else:
        existing = next((t for t in tracks if t["id"] == track), None)
        if existing is None:
            raise ValueError(f"No track {track}")
        if existing.get("type") != "midi":
            raise ValueError(f"tracks.{track} is not a MIDI track")
        give = "sampler" if existing.get("instrument") is None else None
        taken = {c.get("id") for c in existing.get("clips", [])}
    scale, bar_one_at = 1.0, float(at or 0.0)
    if played is not None:
        scale = (60 / report["tempo"]["bpm"]) / played.per_beat
        bar_one_at = played.beat(report["first_downbeat_seconds"])
    with tempfile.TemporaryDirectory() as scratch:
        cuts = {}
        if give:
            for d, seconds in found["clearest"].items():
                if any(r["drum"] == d for r in rows):
                    if f"{track}-{d}" in project.get("samples", {}):
                        raise ValueError(f"The song already has a sample {track}-{d}; name another track with --track")
                    cuts[d] = Path(scratch) / f"{path.stem}-{d}.wav"
                    cut(mono, sr, seconds, [m["seconds"] for m in found["moments"]], cuts[d])
        _, made, _ = batch(track, give, written_parts, scale, bar_one_at, {}, taken)
        if not made:
            raise ValueError("Every hit comes before the song's start")
        samples = {f"{track}-{d}": library.import_asset(file, Path(song).parent) for d, file in cuts.items()}
    commands, made, skipped = batch(track, give, written_parts, scale, bar_one_at, samples, taken)
    with tempfile.TemporaryDirectory() as scratch:
        file = Path(scratch) / "batch.json"
        file.write_text(json.dumps({"label": f"Drums from {path.name}", "commands": commands}))
        result = run_engine("batch", song, file)
    out["clip"] = {
        "track": track,
        "pads": {
            d: {"note": NOTE_NAME[d], "sample": f"{track}-{d}", "cut_at_seconds": round(found["clearest"][d], 4)}
            for d in cuts
        },
        "clips": made,
        "bar_one_at": round(bar_one_at, 4),
        **({"song_beats_a_beat": round(scale, 4)} if abs(scale - 1) > 1e-4 else {}),
        "project_sha256": result["project_sha256"],
    }
    if skipped:
        out["before_the_song"] = skipped
    return out
