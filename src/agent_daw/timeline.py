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
    """Part of a sample file on a track: where on the timeline, and of the file.

    It is a pad's hit or an audio clip; `part` names the pad, or the clip as
    `audio.N`.
    """

    track: str
    part: str
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


def clip_region(track, index, clips, project, root: Path):
    """An audio clip as the engine plays it: from its lead before its beat to where
    it leaves, which is where a clip that follows it starts, and on through its
    fade out."""
    tempo = project["session"]["tempo"]

    def placed(clip):
        """Its speed, the second its beat is on, and how long before it it starts."""
        speed = tempo / clip["source_bpm"] if clip["source_bpm"] else 1.0
        beat = float(Fraction(str(clip["at"]))) * 60 / tempo
        return speed, beat, min(clip["lead_ms"] / 1000, clip["source_start_seconds"] / speed, beat)

    clip = clips[index]
    speed, beat, lead = placed(clip)
    start = clip["source_start_seconds"] - lead * speed
    end = length = sf.info(root / project["samples"][clip["sample"]]["path"]).duration
    if clip["source_end_seconds"] is not None:
        ends = beat + (clip["source_end_seconds"] - clip["source_start_seconds"]) / speed
        after = [c for i, c in enumerate(clips) if i != index and abs(placed(c)[1] - ends) < 0.0005]
        leaves = placed(after[0])[2] if after else 0.0
        end = min(clip["source_end_seconds"] + (clip["fade_out_ms"] / 1000 - leaves) * speed, length)
    return Region(
        track,
        f"audio.{index}",
        clip["sample"],
        beat - lead,
        (end - start) / speed,
        start,
        speed,
        clip["fade_in_ms"] / 1000,
        clip["fade_out_ms"] / 1000,
    )


def repetitions(clip, tempo) -> list[dict]:
    """An audio clip as the engine plays it: itself, or a looped clip as its
    repetitions, each a copy from the clip's start to the loop's end a loop
    later, the last cut off where the clip ends."""
    if clip.get("loop_beats") is None:
        return [clip]
    every, length = Fraction(str(clip["loop_beats"])), Fraction(str(clip["length_beats"]))
    per_beat = 60 / (clip["source_bpm"] or tempo)
    start, out, at = Fraction(str(clip["at"])), [], Fraction(0)
    while at < length:
        beats = min(every, length - at)
        end = clip["source_start_seconds"] + float(beats) * per_beat
        if clip["source_end_seconds"] is not None:
            end = min(end, clip["source_end_seconds"])
        out.append({**clip, "at": str(start + at), "source_end_seconds": end, "loop_beats": None, "length_beats": None})
        at += every
    return out


def track_pads(track) -> dict:
    """A track's pads: its own, or the sampler's of a MIDI track."""
    if track.get("type") == "midi":
        instrument = track["instrument"] or {}
        return instrument.get("sampler", {}).get("pads", {})
    return track["pads"]


def clip_end(project, clip) -> Fraction:
    """The beat a pattern clip's last repeat or a note clip ends on."""
    if "pattern" in clip:
        length = Fraction(str(project["patterns"][clip["pattern"]]["length_beats"])) * clip["repeats"]
    else:
        length = Fraction(str(clip["length_beats"]))
    return Fraction(str(clip["at"])) + length


def regions(project, root: Path):
    """Every forward hit and audio clip of the song as a region, by track, in
    time order."""
    rate = project["session"]["sample_rate"]
    pads = {(t["id"], name): pad for t in project["tracks"] for name, pad in track_pads(t).items()}
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
    tempo = project["session"]["tempo"]
    for track in project["tracks"]:
        audio = [r for clip in track.get("audio", []) for r in repetitions(clip, tempo)]
        clips = [clip_region(track["id"], i, audio, project, root) for i in range(len(audio))]
        if clips:
            found[track["id"]] = sorted(found.get(track["id"], []) + clips, key=lambda r: r.start)
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
        clips = max(
            (clip_end(project, c) for t in project["tracks"] for c in t["clips"]),
            default=Fraction(0),
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
            for name, pad_ in track_pads(t).items()
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
        pads = {t["id"]: track_pads(t) for t in project["tracks"]}
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
