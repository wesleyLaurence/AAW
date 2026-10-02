"""A song's timeline in both units: beats and seconds, and where its sounds are.

Positions in a song are beats and a request names seconds. This converts between
them at the session's tempo, says where each track's sound starts and ends, gives
the beat a sound must start on to end on another, and fits the session's length to
its content.
"""

import math
from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path

import soundfile as sf

from .model import load, schedule


@dataclass
class Region:
    """A hit that plays part of a sample file: where on the timeline, and of the file."""

    track: str
    pad: str
    sample: str
    start: float  # seconds on the timeline
    length: float  # seconds on the timeline
    source: float  # seconds into the file where it starts
    speed: float  # seconds of the file per second of the timeline
    attack: float
    release: float

    @property
    def end(self):
        return self.start + self.length

    def at(self, source_seconds):
        """Where a time in the file falls on the timeline."""
        return self.start + (source_seconds - self.source) / self.speed


def played(pad, project, root: Path):
    """How fast a pad plays its sample, and for how many seconds when left to end."""
    speed = 2 ** (pad["transpose"] / 12)
    if pad["source_bpm"]:
        speed *= project["session"]["tempo"] / pad["source_bpm"]
    end = pad["end_seconds"]
    if end is None:
        end = sf.info(root / project["samples"][pad["sample"]]["path"]).duration
    return speed, (end - pad["start_seconds"]) / speed


def regions(project, root: Path):
    """Every forward hit of the song as a region, by track, in time order."""
    rate = project["session"]["sample_rate"]
    pads = {(t["id"], name): pad for t in project["tracks"] for name, pad in t["pads"].items()}
    lengths, found = {}, {}
    for hit in schedule(project):
        pad = pads[(hit.track, hit.pad)]
        if pad["reverse"]:
            continue
        if (hit.track, hit.pad) not in lengths:
            lengths[(hit.track, hit.pad)] = played(pad, project, root)
        speed, length = lengths[(hit.track, hit.pad)]
        if hit.cutoff is not None:
            length = min(length, (hit.cutoff - hit.start) / rate + pad["release_ms"] / 1000)
        found.setdefault(hit.track, []).append(
            Region(
                hit.track,
                hit.pad,
                pad["sample"],
                hit.start / rate,
                length,
                pad["start_seconds"],
                speed,
                pad["attack_ms"] / 1000,
                pad["release_ms"] / 1000,
            )
        )
    return found


def clock(seconds: float) -> str:
    """Seconds as minutes and seconds, such as 1:20.500."""
    minutes, rest = divmod(round(seconds * 1000), 60000)
    return f"{minutes}:{rest // 1000:02d}.{rest % 1000:03d}"


def position(beats: float, tempo: float) -> dict:
    """A place on the timeline in every form: beats, seconds, and bar and beat from 1."""
    return {
        "beats": round(beats, 6),
        "seconds": round(beats * 60 / tempo, 6),
        "time": clock(beats * 60 / tempo),
        "bar": int(beats // 4) + 1,
        "beat": round(beats % 4 + 1, 6),
    }


def report(path: Path, seconds=(), beats=(), end_at=None, pad=None, fit=False, tail=0.0, tempo_for=None):
    from .beats import seconds as parse
    from .cli import run_engine

    project = load(path)
    tempo = project["session"]["tempo"]
    found = regions(project, path.parent)
    ends = [r.end for hits in found.values() for r in hits]
    sound_ends = max(ends) * tempo / 60 if ends else 0.0
    result = {"tempo": tempo}
    if fit:
        if tail < 0:
            raise ValueError("tail must be 0 or more beats")
        if not ends:
            raise ValueError("The song has no sound to fit its length to")
        # Whole beats, so the end is a place on the grid. A session holds its
        # clips whole, so it cannot end before the last of them does.
        patterns = project["patterns"]
        clips = max(
            Fraction(str(c["at"])) + Fraction(str(patterns[c["pattern"]]["length_beats"])) * c["repeats"]
            for t in project["tracks"]
            for c in t["clips"]
        )
        wanted = math.ceil(round(sound_ends + tail, 6))
        length = max(wanted, math.ceil(clips))
        if Fraction(str(project["session"]["length_beats"])) != length:
            run_engine("set", path, "session.length_beats", length)
            project = load(path)
        result["fitted"] = {"length_beats": length, "tail_beats": tail}
        if length > wanted:
            result["fitted"]["held_by_clips"] = (
                f"The sound and its tail end by beat {wanted}, but a clip reaches beat "
                f"{float(clips):g}; shorten its pattern to end the song there"
            )
    length = float(Fraction(str(project["session"]["length_beats"])))
    result["length"] = position(length, tempo)
    result["sound_ends"] = position(sound_ends, tempo)
    result["tracks"] = {
        track: {
            "first_sound": position(min(r.start for r in hits) * tempo / 60, tempo),
            "last_sound_ends": position(max(r.end for r in hits) * tempo / 60, tempo),
            "hits": len(hits),
        }
        for track, hits in found.items()
    }
    if tempo_for is not None:
        # The session's length is in beats, so its seconds follow the tempo.
        if tempo_for <= 0:
            raise ValueError("tempo-for must be more than 0 seconds")
        faster = length * 60 / tempo_for
        follows = [
            {"pad": f"{t['id']}.{name}", "stretch": pad_["stretch"], "source_bpm": pad_["source_bpm"]}
            for t in project["tracks"]
            for name, pad_ in t["pads"].items()
            if pad_["source_bpm"]
        ]
        result["tempo_for"] = {
            "seconds": tempo_for,
            "tempo": round(faster, 4),
            "change_percent": round((faster / tempo - 1) * 100, 3),
            "pads_that_follow_tempo": follows,
            "note": "Pads without source_bpm keep their length in seconds, so a sound after "
            "the last beat takes more beats at a faster tempo; fit the length again.",
        }
    if seconds:
        result["seconds"] = [position(parse(text) * tempo / 60, tempo) for text in seconds]
    if beats:
        result["beats"] = [position(float(Fraction(text)), tempo) for text in beats]
    if (end_at is None) != (pad is None):
        raise ValueError("--end-at and --pad go together")
    if pad is not None:
        track, _, name = pad.partition(".")
        pads = {t["id"]: t["pads"] for t in project["tracks"]}
        if name not in pads.get(track, {}):
            raise ValueError(f"Unknown pad: {pad}; name it as TRACK.PAD")
        long = played(pads[track][name], project, path.parent)[1] * tempo / 60
        target = float(Fraction(str(end_at)))
        if target - long < 0:
            raise ValueError(f"{pad} is {long:.3f} beats long and cannot end at beat {target:g}")
        result["end_aligned"] = {
            "pad": pad,
            "length_beats": round(long, 6),
            "length_seconds": round(long * 60 / tempo, 6),
            "ends": position(target, tempo),
            "starts": position(target - long, tempo),
        }
    return result
