"""Dynamics in detail: how a mix moves.

How far its loudness ranges and where it is loudest, how hard each hit lands
at its own stem, through its group and through the mix, and what each
compressor and limiter took off, a section at a time. A person sets a
threshold, an attack and a ceiling by listening for these; here they are read
from the render and from what the engine reported as it made it.
These are measurements: a kick that sits under a pad may be meant to.
"""

from __future__ import annotations

import numpy as np

from . import aaw_py
from .model import sounded

# A hit's punch is the peak of its first ATTACK_MS against the RMS of the
# BODY_MS that follow, or of what there is before the next hit. Hits that
# start within APART_MS of one another are one hit: a chord, a flam, a roll.
ATTACK_MS = 30
BODY_MS = 200
APART_MS = 50
# A hit whose attack peaks under this at its stem made no sound, and a punch
# is never written past FLOOR_DB: all attack and nothing after it.
SOUNDS = 10 ** (-80 / 20)
FLOOR_DB = 60.0

FIGURES = ("loudness_range_lu", "max_short_term_lufs", "max_momentary_lufs")
REDUCTION = ("max_gain_reduction_db", "mean_gain_reduction_db", "fraction_over_1db_reduction")
PUNCH = ("punch_db", "punch_in_group_db", "punch_in_mix_db")


def rounded(value, places):
    return None if value is None else round(float(value), places) + 0.0


def both(before, after, places=1) -> dict:
    change = None if before is None or after is None else rounded(after - before, places)
    return {"before": before, "after": after, "delta": change}


def loudness(x, rate, spans) -> list[dict]:
    """The loudness range and the loudest 3 s and 400 ms of each span of some
    audio, as (first frame, frame past the last): the analyzer's meter, run
    over saved audio, so the report and the app's window cannot disagree."""
    read = aaw_py.loudness(np.ascontiguousarray(x, dtype=np.float64), rate, spans)
    return [{key: rounded(one[key], 2) for key in FIGURES} for one in read]


def punch(audio, windows) -> tuple[np.ndarray, np.ndarray]:
    """Each window's punch in decibels and its attack's peak, a window being
    a hit's first frame, the end of its attack and the end of its body; a
    punch is nan where the attack holds no sound."""
    out, peaks = np.full(len(windows), np.nan), np.zeros(len(windows))
    for at, (start, mid, end) in enumerate(windows):
        peak = peaks[at] = float(np.abs(audio[start:mid]).max())
        if peak <= 0:
            continue
        body = float(np.sqrt(np.mean(audio[mid:end] ** 2)))
        out[at] = min(20 * np.log10(peak / body), FLOOR_DB) if body > 0 else FLOOR_DB
    return out, peaks


def middle(values) -> float | None:
    """The median of the hits that were measured."""
    values = values[~np.isnan(values)]
    return rounded(np.median(values), 1) if len(values) else None


class Punch:
    """How hard each track's hits land: at its stem, at its group's and in
    the mix. The hits are the song's, what it schedules on the track, so
    nothing is detected; a track of audio clips has none."""

    def __init__(self, x, rate, project, manifest, offset, sections):
        self.x, self.sections = x, sections
        attack, body, apart = (round(ms * rate / 1000) for ms in (ATTACK_MS, BODY_MS, APART_MS))
        tempo = project["session"]["tempo"]
        starts: dict[str, list] = {}
        for track, at, _, _, hit in sounded(project):
            frame = round(at * 60 / tempo * rate) - offset
            if track in manifest["tracks"] and 0 <= frame < len(x):
                # A hit without a note is written as its pad and its transpose.
                starts.setdefault(track, []).append((frame, hit.rsplit(" ", 1)[0] if hit else None))
        self.hits: dict[str, tuple] = {}
        for track, found in starts.items():
            frames, pads = [], []
            for frame, pad in sorted(found, key=lambda one: one[0]):
                if frames and frame - frames[-1] < apart:
                    pads[-1].add(pad)
                else:
                    frames.append(frame)
                    pads.append({pad})
            ends = [min(a + attack + body, b) for a, b in zip(frames, [*frames[1:], len(x)])]
            # The render's end can leave a last hit no body to measure.
            kept = [at for at, (a, end) in enumerate(zip(frames, ends)) if end - a >= apart]
            self.hits[track] = (
                np.array([frames[at] for at in kept], dtype=np.int64),
                [(frames[at], frames[at] + attack, ends[at]) for at in kept],
                [pads[at] - {None} for at in kept],
            )
        self.groups = {name: entry.get("tracks", []) for name, entry in manifest["tracks"].items()}
        self.read: dict[str, dict] = {}

    def add(self, name, stem):
        """A stem: a track's own hits, and those of a group's tracks through
        the group, which come after them in a render."""
        if name in self.hits:
            windows = self.hits[name][1]
            own, peaks = punch(stem, windows)
            # A hit that made no sound at its stem is not one.
            own[peaks < SOUNDS] = np.nan
            mix, _ = punch(self.x, windows)
            mix[np.isnan(own)] = np.nan
            self.read[name] = {"punch_db": own, "punch_in_mix_db": mix}
        for track in self.groups[name]:
            if track in self.read:
                through, _ = punch(stem, self.hits[track][1])
                through[np.isnan(self.read[track]["punch_db"])] = np.nan
                self.read[track]["punch_in_group_db"] = through

    def over(self, read, chosen) -> dict:
        """The hits chosen, counted, with the median of each punch."""
        heard = chosen & ~np.isnan(read["punch_db"])
        return {
            "count": int(heard.sum()),
            **{key: middle(read[key][heard]) for key in PUNCH if key in read},
        }

    def listed(self, name) -> dict | None:
        """`hits` of a track in a listen report, whole, by section and by pad;
        None for a stem the song schedules nothing on."""
        if name not in self.read:
            return None
        frames, _, pads = self.hits[name]
        read = self.read[name]
        out = self.over(read, np.ones(len(frames), dtype=bool))
        out["sections"] = {}
        for s in self.sections:
            found = self.over(read, (frames >= s["start_frame"]) & (frames < s["end_frame"]))
            if found["count"]:
                out["sections"][s["id"]] = found
        # A kit on one track: each drum apart, in the hits that it starts in.
        names = sorted(set().union(*pads))
        if len(names) > 1:
            out["pads"] = {
                pad: self.over(read, np.array([pad in one for one in pads], dtype=bool)) for pad in names
            }
        return out


DEVICES = ("compressor", "limiter")


def effects(manifest, project) -> dict:
    """`effects` of a listen report: every compressor and limiter of the
    render, by channel, with what it took off over the song and in each
    section, as the engine measured while it rendered. `path` is where the
    device is in the song; the ID and the key are read from the snapshot."""
    owners = {
        owner["id"]: (kind, owner)
        for kind in ("tracks", "groups", "returns")
        for owner in project.get(kind, [])
    }

    def chain(entries, written, base) -> list[dict]:
        out = []
        for at, entry in enumerate(entries):
            if entry["type"] not in DEVICES:
                continue
            song = written[at] if at < len(written) else {}
            ident = song.get("id")
            one = {"path": f"{base}.{ident if ident else at}", "type": entry["type"]}
            if ident:
                one["id"] = ident
            if song.get("sidechain"):
                one["keyed_by"] = song["sidechain"]
            if entry.get("bypass"):
                out.append(one | {"bypass": True})
                continue
            one |= reduction(entry)
            if "sections" in entry:
                one["sections"] = {sid: reduction(part) for sid, part in entry["sections"].items()}
            out.append(one)
        return out

    tracks = {}
    for name, entry in manifest["tracks"].items():
        kind, owner = owners.get(name, ("tracks", {}))
        patch = ((owner.get("instrument") or {}).get("synth") or {}).get("effects", [])
        found = chain(
            entry.get("instrument_effects", []), patch, f"tracks.{name}.instrument.synth.effects"
        ) + chain(entry.get("effects", []), owner.get("effects", []), f"{kind}.{name}.effects")
        if found:
            tracks[name] = found
    master = chain(manifest.get("master_effects", []), project["master"]["effects"], "master.effects")
    return {"master": master, "tracks": tracks}


def reduction(entry) -> dict:
    return {
        key: rounded(entry.get(key), 3 if key.startswith("fraction") else 2) for key in REDUCTION
    }


def section_effects(found, section) -> dict:
    """`effects` of one section: each device with what it took off there."""

    def chain(devices):
        return [
            {key: value for key, value in one.items() if key not in (*REDUCTION, "sections")}
            | (one.get("sections", {}).get(section) or {})
            for one in devices
        ]

    return {
        "master": chain(found["master"]),
        "tracks": {name: chain(devices) for name, devices in found["tracks"].items()},
    }


def figures_between(a, b, gain) -> tuple[dict, dict]:
    """The change in the range and the two maxima, as it is and with the
    after matched to the before's loudness: a gain moves the maxima and
    leaves the range where it was. Empty for measurements made before these."""
    raw = {key: both(a[key], b[key], 2)["delta"] for key in FIGURES if key in a and key in b}
    matched = {
        key: None if raw[key] is None or gain is None else rounded(raw[key] + gain, 2)
        for key in FIGURES[1:]
        if key in raw
    }
    return raw, matched


def hits_between(a, b, sections) -> dict | None:
    """A track's hits before and after; None when either render schedules none."""
    if a is None or b is None:
        return None

    def each(before, after) -> dict:
        return {
            "count": {
                "before": before["count"],
                "after": after["count"],
                "delta": after["count"] - before["count"],
            },
            **{
                key: both(before.get(key), after.get(key))
                for key in PUNCH
                if key in before or key in after
            },
        }

    out = each(a, b)
    out["sections"] = {
        s: each(a["sections"][s], b["sections"][s])
        for s in sections
        if s in a["sections"] and s in b["sections"]
    }
    pads = sorted(set(a.get("pads", {})) & set(b.get("pads", {})))
    if pads:
        out["pads"] = {pad: each(a["pads"][pad], b["pads"][pad]) for pad in pads}
    return out


def effects_between(a, b, sections) -> dict | None:
    """`effects` in `daw compare`: each compressor and limiter both renders
    have, by its ID, or by its place among its channel's devices of its type
    when it has none, with what it took off before and after, the largest
    change first, and the paths of those only one render has."""
    if a is None or b is None:
        return None

    def keyed(found) -> dict:
        out = {}
        chains = [("master", found["master"]), *found["tracks"].items()]
        for at, (channel, devices) in enumerate(chains):
            # The master's chain is apart from a track of that name.
            owner = ("master",) if at == 0 else ("tracks", channel)
            unnamed: dict[str, int] = {}
            for one in devices:
                if "id" in one:
                    out[(*owner, "id", one["id"])] = one
                else:
                    nth = unnamed[one["type"]] = unnamed.get(one["type"], 0) + 1
                    out[(*owner, one["type"], nth)] = one
        return out

    def each(before, after) -> dict:
        return {
            key: both(before.get(key), after.get(key), 3 if key.startswith("fraction") else 2)
            for key in REDUCTION
        }

    before, after = keyed(a), keyed(b)
    devices = []
    for key in before.keys() & after.keys():
        was, now = before[key], after[key]
        one = {"path": now["path"], "type": now["type"]}
        if was["path"] != now["path"]:
            one["path_before"] = was["path"]
        if was.get("bypass") or now.get("bypass"):
            one |= {"bypass_before": bool(was.get("bypass")), "bypass_after": bool(now.get("bypass"))}
        one |= each(was, now)
        one["sections"] = {
            s: each(was["sections"][s], now["sections"][s])
            for s in sections
            if s in was.get("sections", {}) and s in now.get("sections", {})
        }
        devices.append(one)
    devices.sort(key=lambda one: (-abs(one[REDUCTION[0]]["delta"] or 0), one["path"]))
    return {
        "devices": devices,
        "added": sorted(after[key]["path"] for key in after.keys() - before.keys()),
        "removed": sorted(before[key]["path"] for key in before.keys() - after.keys()),
    }


METHOD = (
    "loudness_range_lu, max_short_term_lufs and max_momentary_lufs are the analyzer's meter "
    "run over the saved audio: K-weighted power in blocks of 100 ms, momentary over 400 ms "
    "and short-term over 3 s, a block apart; the range is EBU Tech 3342's, the 10th to the "
    "95th percentile of the short-term values over -70 LUFS and within 20 LU under their "
    "mean; null under 3 s, or 400 ms for the momentary, and under -70 LUFS. A hit is what "
    "the song schedules on a track, a pad's trigger or a note, not a detected onset, and "
    f"those that start within {APART_MS} ms of one another are one hit; its punch is the "
    f"peak of its first {ATTACK_MS} ms against the RMS of the {BODY_MS} ms that follow, or "
    "of what there is before the next hit, in decibels and never written past "
    f"{FLOOR_DB:g}; punch_db, punch_in_group_db and punch_in_mix_db are the median over a "
    "track's hits of the same frames read at its stem, at its group's stem and in the mix, "
    "and a hit whose attack peaks under -80 dBFS at its stem is not counted. effects are the "
    "render report's: each compressor's and limiter's gain reduction as the engine measured "
    "it on the frames inside the song, over the whole and over each section; measured, not "
    "judged"
)
