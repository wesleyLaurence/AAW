"""The key of what a song plays, and what moves a sample into it.

A song declares no key (D6), so its key is measured from what it plays: each
note of the tracks that are heard, by its pitch as it sounds and its length, and
each sample its pads and audio clips play, by the key its audio measures in,
moved by the transpose and the repitch it is played with, for as many beats as
it sounds. A sample counts as one voice. A sample whose audio has no key, a drum
or a noise, adds nothing. The whole is held to the profiles of the keys as a
sample's own classes are (harmony.py).
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import soundfile as sf

from . import harmony
from .model import beat, load, sounded


def _heard(project) -> set[str]:
    """The tracks that are heard: not muted, nor in a muted group, and soloed or
    in a soloed group when anything is soloed."""
    groups = {g["id"]: g for g in project["groups"]}

    def chain(gid):
        while gid:
            yield groups[gid]
            gid = groups[gid]["group"]

    soloing = any(t["solo"] for t in project["tracks"]) or any(g["solo"] for g in groups.values())
    heard = set()
    for t in project["tracks"]:
        if t["mute"] or any(g["mute"] for g in chain(t["group"])):
            continue
        if soloing and not (t["solo"] or any(g["solo"] for g in chain(t["group"]))):
            continue
        heard.add(t["id"])
    return heard


def _repitch(tempo, source_bpm, stretch) -> float:
    """The semitones a sample is moved by playing it at the song's tempo, repitched."""
    if source_bpm and stretch == "repitch":
        return 12 * np.log2(tempo / source_bpm)
    return 0.0


class Samples:
    """The key of each part of a sample file the song plays, measured once."""

    def __init__(self):
        self.keys, self.lengths = {}, {}

    def length(self, path: Path) -> float:
        if path not in self.lengths:
            info = sf.info(path)
            self.lengths[path] = info.frames / info.samplerate
        return self.lengths[path]

    def key(self, path: Path, start: float, end: float | None) -> dict:
        at = (path, round(start, 3), None if end is None else round(end, 3))
        if at not in self.keys:
            with sf.SoundFile(path) as f:
                sr = f.samplerate
                f.seek(min(f.frames, round(start * sr)))
                stop = f.frames if end is None else min(f.frames, round(end * sr))
                frames = max(0, min(stop - f.tell(), round(harmony.SECONDS * sr)))
                x = f.read(frames, dtype="float32", always_2d=True)
            self.keys[at] = harmony.key_of(x.mean(axis=1), sr) if len(x) else harmony.estimate(np.zeros(12))
        return self.keys[at]


def of_song(path: Path) -> dict:
    """The key of what the song plays, and what it was measured from."""
    # A sample whose file is missing is left out, not the song refused.
    project = load(path, verify_assets=False)
    tempo = float(project["session"]["tempo"])
    heard, files = _heard(project), Samples()
    tracks = {t["id"]: t for t in project["tracks"]}
    profile = np.zeros(12)
    notes: dict[str, float] = {}
    played: dict[tuple, dict] = {}

    def sample(track, sample_id, file, start, end, beats, shift):
        if beats <= 0:
            return
        part = played.setdefault(
            (sample_id, start, end, round(shift)),
            {"sample": sample_id, "file": file, "start": start, "end": end, "shift": round(shift), "beats": 0.0, "tracks": set()},
        )
        part["beats"] += beats
        part["tracks"].add(track)

    for track, at, until, pitch, hit in sounded(project):
        if track not in heard:
            continue
        if pitch is not None:
            length = max(until - at, 0.25)
            profile[int(round(pitch)) % 12] += length
            notes[track] = notes.get(track, 0.0) + length
            continue
        pad_id, _, moved = hit.rpartition(" ")
        t = tracks[track]
        pads = t.get("pads") or ((t.get("instrument") or {}).get("sampler") or {}).get("pads") or {}
        pad = pads.get(pad_id)
        if pad is None or pad["sample"] not in project["samples"]:
            continue
        file = path.parent / project["samples"][pad["sample"]]["path"]
        if not file.is_file():
            continue
        start = float(pad["start_seconds"] or 0.0)
        end = pad["end_seconds"]
        seconds = (end if end is not None else files.length(file)) - start
        rate = tempo / pad["source_bpm"] if pad["source_bpm"] else 1.0
        beats = max(seconds, 0) / rate * tempo / 60
        if until > at:
            beats = min(beats, until - at)
        shift = float(moved or 0) + _repitch(tempo, pad["source_bpm"], pad["stretch"])
        sample(track, pad["sample"], file, start, end, beats, shift)
    for t in project["tracks"]:
        if t["id"] not in heard:
            continue
        for clip in t.get("audio") or []:
            if not clip["active"] or clip["sample"] not in project["samples"]:
                continue
            file = path.parent / project["samples"][clip["sample"]]["path"]
            if not file.is_file():
                continue
            start, end = float(clip["source_start_seconds"] or 0.0), clip["source_end_seconds"]
            rate = tempo / clip["source_bpm"] if clip["source_bpm"] else 1.0
            if clip["length_beats"] is not None:
                beats = float(beat(clip["length_beats"]))
            else:
                seconds = (end if end is not None else files.length(file)) - start
                beats = max(seconds, 0) / rate * tempo / 60
            shift = float(clip["transpose"]) + _repitch(tempo, clip["source_bpm"], clip["stretch"])
            sample(t["id"], clip["sample"], file, start, end, beats, shift)

    samples, keyless = [], []
    for part in played.values():
        key = files.key(part["file"], part["start"], part["end"])
        row = {
            "sample": part["sample"],
            "tracks": sorted(part["tracks"]),
            "beats": round(part["beats"], 3),
            "transpose": part["shift"],
        }
        if part["start"] or part["end"] is not None:
            row["seconds"] = [round(part["start"], 3), None if part["end"] is None else round(part["end"], 3)]
        if not key["tonal"]:
            keyless.append(row)
            continue
        moved = harmony.key_name(harmony.parse_key(key["name"])[0] + part["shift"], key["mode"])
        samples.append({**row, "key": key["name"], "heard_in": moved})
        profile += np.roll(np.asarray(key["profile"]), part["shift"]) * part["beats"]
    key = harmony.estimate(profile)
    return {
        "song": str(path),
        "key": key,
        "from": {
            "notes": {track: round(beats, 3) for track, beats in notes.items()},
            "samples": sorted(samples, key=lambda s: -s["beats"]),
            "without_key": sorted(keyless, key=lambda s: -s["beats"]),
        },
        "source": "measured from the song's notes and its samples' audio; an estimate, not ground truth",
    }


def fit(given: str, song_key: dict, analysis: dict, tempo: float) -> dict:
    """What puts a sample in the song's key, and a loop at its tempo: the
    transpose for a sample played at its own pitch, and the repitch that playing
    a loop at the song's tempo would add to it."""
    key = analysis["key"]
    out = {"sample": given, "path": analysis["path"], "key": key["name"] if key else None}
    if key and key["tonal"]:
        out["key_confidence"] = key["confidence"]
    if not song_key["tonal"]:
        out["why"] = "the song has no key yet: nothing it plays measures in one"
    elif not key or not key["tonal"]:
        out["why"] = "the sample has no key: a drum, a noise or a single note"
    else:
        out.update(harmony.transpose_to(key["name"], song_key["name"]))
    tempo_found = analysis["rhythm"].get("tempo")
    if analysis["rhythm"]["kind"] == "loop" and tempo_found:
        out["source_bpm"] = tempo_found["bpm"]
        out["repitch_semitones"] = round(12 * float(np.log2(tempo / tempo_found["bpm"])), 2)
    if analysis["rhythm"].get("swing", {}).get("swing") is not None:
        out["swing"] = {k: analysis["rhythm"]["swing"][k] for k in ("grid", "swing", "feel")}
    return out
