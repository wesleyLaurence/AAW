"""A song's timeline in both units: beats and seconds, and where its sounds are.

Positions in a song are beats and a request names seconds. This converts between
them through the song's tempo map, says where each track's sound starts and ends, gives
and the beat a sound must start on to end on another.
"""

from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path

import soundfile as sf

from .model import TempoMap, load, meter, schedule, tempo_map


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


def played(pad, project, root: Path, tempo=None):
    """How fast a pad plays its sample, and for how many seconds when left to end,
    where the song is at `tempo` (its tempo at beat 0 unless given)."""
    speed = 2 ** (pad["transpose"] / 12)
    if pad["source_bpm"]:
        speed *= (tempo or project["session"]["tempo"]) / pad["source_bpm"]
    end = pad["end_seconds"]
    if end is None:
        end = sf.info(root / project["samples"][pad["sample"]]["path"]).duration
    return speed, (end - pad["start_seconds"]) / speed


def repitch(clip) -> float:
    """How many times as fast as written an audio clip's transpose plays its
    file: one unless it is transposed by repitching."""
    if clip.get("stretch", "repitch") != "repitch":
        return 1.0
    return 2 ** (clip.get("transpose", 0.0) / 12)


def clip_region(track, index, clips, project, root: Path):
    """An audio clip as the engine plays it: from its lead before its beat to where
    it leaves, which is where a clip that follows it starts, and on through its
    fade out. A clip with a tempo of its own plays at the song's tempo where it
    starts: one that crosses a step plays as a part a step (`parts`)."""
    tempo_map = TempoMap.of(project)

    def placed(clip):
        """Its speed, the second its beat is on, and how long before it it starts."""
        at = float(Fraction(str(clip["at"])))
        tempo = tempo_map.tempo_at(at)
        speed = (tempo / clip["source_bpm"] if clip["source_bpm"] else 1.0) * repitch(clip)
        beat = tempo_map.seconds(at)
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


def repetitions(clip, tempo_map: TempoMap) -> list[dict]:
    """An audio clip as the engine plays it: itself, or a looped clip as its
    repetitions, each a copy from the clip's start to the loop's end a loop
    later, the last cut off where the clip ends. A clip that starts into its
    loop plays the rest of it first, from there in its file, without a lead.
    A clip without a tempo of its own in a song whose tempo changes plays as
    much of its file in each repetition as its beats last there."""
    if clip.get("loop_beats") is None:
        return [clip]
    every, length = Fraction(str(clip["loop_beats"])), Fraction(str(clip["length_beats"]))
    into = Fraction(str(clip.get("loop_offset_beats") or 0))
    start, out, at = Fraction(str(clip["at"])), [], Fraction(0)
    while at < length:
        beats = min(every - into, length - at)
        here = float(start + at)
        per_beat = 60 / (clip["source_bpm"] or tempo_map.tempo_at(here)) * repitch(clip)
        source_start = clip["source_start_seconds"] + float(into) * per_beat
        if tempo_map.tempo is None and not clip["source_bpm"]:
            end = source_start + (tempo_map.seconds(here + float(beats)) - tempo_map.seconds(here)) * repitch(clip)
        else:
            end = source_start + float(beats) * per_beat
        if clip["source_end_seconds"] is not None:
            end = min(end, clip["source_end_seconds"])
        out.append({
            **clip, "at": str(start + at), "source_start_seconds": source_start, "source_end_seconds": end,
            "lead_ms": clip["lead_ms"] if into == 0 else 0.0,
            "loop_beats": None, "loop_offset_beats": None, "length_beats": None,
        })
        at += every - into
        into = Fraction(0)
    return out


STEP_JOIN_MS = 5.0


def parts(clip, tempo_map: TempoMap, file_seconds: float) -> list[dict]:
    """A clip with a tempo of its own that crosses steps of the song's tempo,
    as the engine plays it: a part a step, each stretched to its step's
    tempo, the parts crossing over in a few milliseconds before each step."""
    if not clip["source_bpm"] or tempo_map.tempo is not None:
        return [clip]
    at = Fraction(str(clip["at"]))
    per_beat = 60 / clip["source_bpm"] * repitch(clip)
    end = min(clip["source_end_seconds"] if clip["source_end_seconds"] is not None else file_seconds, file_seconds)
    last = float(at) + (end - clip["source_start_seconds"]) / per_beat
    cuts = [b for b, _, _ in tempo_map.steps[1:] if float(at) < float(b) < last]
    if not cuts:
        return [clip]

    def file_at(b):
        return clip["source_start_seconds"] + float(b - at) * per_beat

    out, start = [], at
    for k, cut in enumerate(cuts + [None]):
        first, tail = k == 0, cut is None
        out.append({
            **clip,
            "at": clip["at"] if first else str(start),
            "source_start_seconds": clip["source_start_seconds"] if first else file_at(start),
            "source_end_seconds": clip["source_end_seconds"] if tail else file_at(cut),
            "lead_ms": clip["lead_ms"] if first else STEP_JOIN_MS,
            "fade_in_ms": clip["fade_in_ms"] if first else STEP_JOIN_MS,
            "fade_out_ms": clip["fade_out_ms"] if tail else STEP_JOIN_MS,
        })
        if not tail:
            start = cut
    return out


def track_pads(track) -> dict:
    """A track's pads: the sampler's of a MIDI track; an audio track has none."""
    if track.get("type") == "midi":
        instrument = track["instrument"] or {}
        return (instrument.get("sampler") or {}).get("pads", {})
    return {}


def regions(project, root: Path):
    """Every forward hit and audio clip of the song as a region, by track, in
    time order."""
    rate = project["session"]["sample_rate"]
    tempo_map = TempoMap.of(project)
    pads = {(t["id"], name): pad for t in project["tracks"] for name, pad in track_pads(t).items()}
    lengths, found = {}, {}
    for hit in schedule(project):
        pad = pads[(hit.track, hit.pad)]
        if pad["reverse"]:
            continue
        tempo = tempo_map.tempo_at_seconds(hit.start / rate)
        if (hit.track, hit.pad, tempo) not in lengths:
            lengths[(hit.track, hit.pad, tempo)] = played(pad, project, root, tempo)
        speed, length = lengths[(hit.track, hit.pad, tempo)]
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
    for track in project["tracks"]:
        # A deactivated clip plays nothing, and the one before it leaves as if alone.
        audio = [
            p
            for clip in track.get("audio", [])
            if clip.get("active", True)
            for r in repetitions(clip, tempo_map)
            for p in parts(r, tempo_map, _file_seconds(project, root, r))
        ]
        clips = [clip_region(track["id"], i, audio, project, root) for i in range(len(audio))]
        if clips:
            found[track["id"]] = sorted(found.get(track["id"], []) + clips, key=lambda r: r.start)
    return found


def _file_seconds(project, root: Path, clip) -> float:
    """The length of a clip's file, which only a clip that crosses a step needs."""
    if not clip["source_bpm"]:
        return 0.0
    return sf.info(root / project["samples"][clip["sample"]]["path"]).duration


def clock(seconds: float) -> str:
    """Seconds as minutes and seconds, such as 1:20.500."""
    minutes, rest = divmod(round(seconds * 1000), 60000)
    return f"{minutes}:{rest // 1000:02d}.{rest % 1000:03d}"


def position(beats: float, tempo: "float | TempoMap", meter=(Fraction(4), Fraction(1))) -> dict:
    """A place on the timeline in every form: beats, seconds, and bar and beat from
    1, the beat counted as the time signature counts it (`meter` is the bar and
    the beat it counts, in quarter-note beats, as `model.meter` gives them).
    `tempo` is the song's one tempo or its tempo map."""
    bar, unit = (float(m) for m in meter)
    seconds = tempo.seconds(beats) if isinstance(tempo, TempoMap) else beats * 60 / tempo
    return {
        "beats": round(beats, 6),
        "seconds": round(seconds, 6),
        "time": clock(seconds),
        "bar": int(beats // bar) + 1,
        "beat": round((beats % bar) / unit + 1, 6),
    }


def report(path: Path, seconds=(), beats=(), end_at=None, pad=None, fit=False, tail=0.0, tempo_for=None):
    from .beats import seconds as parse
    from .cli import run_engine

    project = load(path)
    tempo = project["session"]["tempo"]
    tm = tempo_map(project)
    bar = meter(project["session"])
    found = regions(project, path.parent)
    ends = [r.end for hits in found.values() for r in hits]
    sound_ends = tm.beat(max(ends)) if ends else 0.0
    result = {"tempo": tempo}
    if tm.tempo is None:
        result["tempo_steps"] = [{"at": float(b), "bpm": bpm, "seconds": round(s, 6)} for b, bpm, s in tm.steps]
    if fit:
        # The length is the song's content, the next whole bar after the last
        # sound, set by the engine after every edit (D130): there is nothing
        # to fit.
        result["fitted"] = {
            "note": "Nothing to do: the length is the song's content, the next whole bar "
            "after the last sound, and follows every edit"
        }
    length = float(Fraction(str(project["session"]["length_beats"])))
    result["length"] = position(length, tm, bar)
    result["sound_ends"] = position(sound_ends, tm, bar)
    result["tracks"] = {
        track: {
            "first_sound": position(tm.beat(min(r.start for r in hits)), tm, bar),
            "last_sound_ends": position(tm.beat(max(r.end for r in hits)), tm, bar),
            "hits": len(hits),
        }
        for track, hits in found.items()
    }
    if tempo_for is not None:
        # The session's length is in beats, so its seconds follow the tempo.
        if tempo_for <= 0:
            raise ValueError("tempo-for must be more than 0 seconds")
        # With a tempo lane every step is scaled by one factor, the opening
        # tempo's given.
        faster = length * 60 / tempo_for if tm.tempo is not None else tempo * tm.seconds(length) / tempo_for
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
            "the last beat takes more beats at a faster tempo."
            + ("" if tm.tempo is not None else " The song's tempo changes: tempo is its opening tempo, and every step scales by the same change."),
        }
    if seconds:
        result["seconds"] = [position(tm.beat(parse(text)), tm, bar) for text in seconds]
    if beats:
        result["beats"] = [position(float(Fraction(text)), tm, bar) for text in beats]
    if (end_at is None) != (pad is None):
        raise ValueError("--end-at and --pad go together")
    if pad is not None:
        track, _, name = pad.partition(".")
        pads = {t["id"]: track_pads(t) for t in project["tracks"]}
        if name not in pads.get(track, {}):
            raise ValueError(f"Unknown pad: {pad}; name it as TRACK.PAD")
        target = float(Fraction(str(end_at)))
        seconds_long = played(pads[track][name], project, path.parent, tm.tempo_at(target))[1]
        start = tm.beat(tm.seconds(target) - seconds_long)
        long = target - start if tm.tempo is None else seconds_long * tempo / 60
        if target - long < 0:
            raise ValueError(f"{pad} is {long:.3f} beats long and cannot end at beat {target:g}")
        result["end_aligned"] = {
            "pad": pad,
            "length_beats": round(long, 6),
            "length_seconds": round(seconds_long if tm.tempo is None else long * 60 / tempo, 6),
            "ends": position(target, tm, bar),
            "starts": position(target - long, tm, bar),
        }
    return result
