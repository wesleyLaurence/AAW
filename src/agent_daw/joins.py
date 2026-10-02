"""Checks of a rendered edit: each join between two parts of a song, and the length.

A join is where a track goes from one part of a sample file to another part of the
same file, as an edit of a song does. For each one this measures whether the beat
carries across it, whether the splice shows as a step in the waveform, and how the
level changes, and writes a short excerpt for a person to hear. Whether a join
sounds natural is theirs to judge; nothing here listens.
"""

import json
from pathlib import Path

import numpy as np
import soundfile as sf

from . import beats, perception
from .model import digest, load
from .timeline import Region, regions

BEATS = 8  # beats measured either side of a join
SLIP_MS = 1.0  # a beat this far from where it should be across a join is flagged
MEASURED_SLIP_MS = 3.0  # the same, for transients, whose attacks differ by instrument
STEP_RATIO = 2.0  # a step at a splice this many times the largest around it is flagged
LEVEL_DB = 3.0


def find(project, root: Path):
    """The joins of a song: pairs of regions, the one leaving and the one entering.

    Two regions in a row on a track make a join when they are different parts of
    the same file (two audio clips, or two pads), each a beat or more, and the
    second starts as the first ends, within their fades. A drum hit after another
    is not one, nor is a loop played again.
    """
    beat = 60 / project["session"]["tempo"]
    joins = []
    for hits in regions(project, root).values():
        for a, b in zip(hits, hits[1:]):
            overlap = a.end - b.start
            if (
                a.sample == b.sample
                and a.part != b.part
                and min(a.length, b.length) >= beat
                and -0.05 <= overlap <= max(a.release, b.attack) + 0.05
            ):
                joins.append((a, b))
    return sorted(joins, key=lambda pair: pair[1].start)


def saved_map(path: Path):
    """The beat map kept beside a sample, if it is of this audio."""
    sidecar = path.with_name(f"{path.stem}.beats.json")
    if not sidecar.is_file():
        return None
    saved = json.loads(sidecar.read_text())
    return saved if saved.get("sha256") == digest(path) else None


def on_grid(a: Region, b: Region, song_beats, beat):
    """From the song's beat map: where its beats fall either side of a join.

    The last beat the leaving region plays before its fade and the first the
    entering one plays are placed on the timeline, and the time between them is
    held against a whole number of session beats.
    """
    times = np.array([row["seconds"] for row in song_beats])
    fade_starts = a.source + (a.length - a.release) * a.speed
    before = np.flatnonzero((times >= a.source) & (times <= fade_starts + 0.001))
    after = np.flatnonzero(times >= b.source - 0.0005)
    if not len(before) or not len(after) or times[after[0]] > b.source + b.length * b.speed:
        return None
    i, j = int(before[-1]), int(after[0])
    left, entered = a.at(times[i]), b.at(times[j])
    count = round((entered - left) / beat)

    def off(t):
        return (t - round(t / beat) * beat) * 1000

    return {
        "before_ms": round(off(left), 3),
        "after_ms": round(off(entered), 3),
        "interval_error_ms": round((entered - left - count * beat) * 1000, 3),
        "session_beats_between": count,
        "source_beats_skipped": (j - i) - count,
        "leaves_at": {k: song_beats[i][k] for k in ("seconds", "bar", "beat")},
        "enters_at": {k: song_beats[j][k] for k in ("seconds", "bar", "beat")},
        "enters_after_fade_in_ms": round(((times[j] - b.source) / b.speed - b.attack) * 1000, 3),
    }


def measured(mono, rate, a: Region, b: Region, beat, song_beats=None):
    """From the track's audio: how far the transients either side sit from session beats.

    Transients are looked for at the song's beats where its map gives them, and at
    session beats otherwise. Attacks differ by instrument, so a difference of a
    few milliseconds can be a change of sound and not a slip.
    """

    def expected(region, first):
        if song_beats is None:
            k = np.arange(np.ceil(region.start / beat - 1e-6), np.floor(region.end / beat) + 1)
            times = k * beat
        else:
            source = np.array([row["seconds"] for row in song_beats])
            times = region.at(source)
        inside = times[(times >= region.start - 0.001) & (times <= region.end - region.release)]
        return inside[:BEATS] if first else inside[inside < b.start][-BEATS:]

    result = {}
    for side, times in (("before", expected(a, False)), ("after", expected(b, True))):
        result[f"{side}_ms"], result[f"{side}_transients"] = None, 0
        if not len(times):
            continue
        offsets, rises = beats.transients(mono, rate, times)
        sharp = rises >= np.log(2)
        if sharp.sum() >= 2:
            landed = times[sharp] + offsets[sharp]
            away = (landed - np.round(landed / beat) * beat) * 1000
            result[f"{side}_ms"] = round(float(np.median(away)), 3)
            result[f"{side}_transients"] = int(sharp.sum())
    if result["before_ms"] is None or result["after_ms"] is None:
        result["interval_error_ms"] = None
    else:
        result["interval_error_ms"] = round(result["after_ms"] - result["before_ms"], 3)
    return result


def edge_step(steps, frame, rate):
    """The step in the waveform where a region starts or stops, and the largest
    within 3 ms either side of it.

    A fade brings a region in from nothing and out to nothing, so its edges add no
    step. An edge without one jumps by whatever the audio is there, which stands
    out from the steps around it unless a transient of the music is there too.
    """
    reach = round(0.003 * rate)
    here = steps[max(frame - 3, 0) : frame + 2]
    near = np.concatenate([steps[max(frame - reach, 0) : max(frame - 3, 0)], steps[frame + 2 : frame + reach]])
    return (float(here.max()) if len(here) else 0.0), (float(near.max()) if len(near) else 0.0)


def check(source, limit=None, seconds=4.0, excerpts=True):
    """The joins and the length of a full render, as a report."""
    audio_path, folder = perception.resolve(source)
    if folder is None:
        raise ValueError("Join checks need a render: its folder, report or pointer")
    manifest = json.loads((folder / "report.json").read_text())
    target = manifest.get("target") or {}
    if target.get("track") or target.get("section"):
        raise ValueError("Join checks need a render of the whole song")
    if digest(audio_path) != manifest["audio_sha256"]:
        raise ValueError("Render mix hash mismatch")
    project = load(folder / "song.snapshot.yaml", verify_assets=False)
    song_file = Path(manifest.get("project_file", ""))
    root = song_file.parent if song_file.is_file() else folder.parent.parent
    mix, rate = sf.read(audio_path, always_2d=True, dtype="float64")
    tempo = project["session"]["tempo"]
    beat = 60 / tempo

    # The length, with everything in it, and where the sound ends.
    duration = len(mix) / rate
    level = np.abs(mix).max(axis=1)
    sounding = np.flatnonzero(level >= max(float(level.max()), 1e-12) * 10 ** (-60 / 20))
    length = {
        "seconds": duration,
        "sound_ends_seconds": round(float(sounding[-1] + 1) / rate, 3) if len(sounding) else 0.0,
        "limit_seconds": limit,
        "within_limit": None if limit is None else bool(duration <= limit),
    }
    if limit is not None and duration > limit:
        length["over_by_seconds"] = round(duration - limit, 3)

    stems, maps, report = {}, {}, []
    for number, (a, b) in enumerate(find(project, root), 1):
        if a.track not in stems:
            stems[a.track] = sf.read(folder / "stems" / f"{a.track}.wav", always_2d=True, dtype="float64")[0]
        stem = stems[a.track]
        if a.sample not in maps:
            maps[a.sample] = saved_map(root / project["samples"][a.sample]["path"])
        song_beats = maps[a.sample]["beats"] if maps[a.sample] else None

        # The fade: from where either region starts changing level to where both stop.
        fade = (min(b.start, a.end - a.release), max(a.end, b.start + b.attack))
        steps = np.abs(np.diff(stem, axis=0)).max(axis=1)
        edges = [edge_step(steps, round(t * rate), rate) for t in (b.start, a.end)]
        inside, around = max(edges, key=lambda e: e[0] / e[1] if e[1] > 0 else 0.0)

        def rms(start, stop):
            part = stem[max(round(start * rate), 0) : max(round(stop * rate), 0)]
            return perception.db(float(np.mean(part * part))) if len(part) else None

        levels = (rms(max(fade[0] - 1, a.start), fade[0]), rms(fade[1], min(fade[1] + 1, b.end)))
        grid = on_grid(a, b, song_beats, beat) if song_beats else None
        heard = measured(stem.mean(axis=1), rate, a, b, beat, song_beats)
        entry = {
            "join": number,
            "track": a.track,
            "sample": a.sample,
            "parts": [a.part, b.part],
            "at_seconds": round(b.start, 6),
            "at_beats": round(b.start / beat, 4),
            "source_seconds": {
                "leaves": round(a.source + a.length * a.speed, 6),
                "enters": round(b.source, 6),
            },
            "fade": {
                "seconds": round(fade[1] - fade[0], 6),
                "out_ms": round(a.release * 1000, 3),
                "in_ms": round(b.attack * 1000, 3),
                "overlap_ms": round((a.end - b.start) * 1000, 3),
            },
            "grid": grid,
            "measured": heard,
            "step": {
                "at_edge": inside,
                "around": around,
                "ratio": round(inside / around, 3) if around > 0 else None,
            },
            "level": {
                "before_dbfs": None if levels[0] is None else round(levels[0], 2),
                "after_dbfs": None if levels[1] is None else round(levels[1], 2),
                "change_db": None if None in levels else round(levels[1] - levels[0], 2),
            },
        }
        flags = []
        if grid is None:
            flags.append(
                f"No beat map of {a.sample}: run daw samples beats on it to check the beat across the join"
            )
        else:
            if abs(grid["interval_error_ms"]) > SLIP_MS:
                flags.append(f"The beat slips {grid['interval_error_ms']:+.2f} ms across the join")
            if grid["source_beats_skipped"] % 4:
                flags.append(
                    f"{grid['source_beats_skipped']} beats of the song are skipped, not whole bars"
                )
            if grid["enters_after_fade_in_ms"] < 0:
                flags.append("The first beat after the join starts inside the fade in")
        slip = heard["interval_error_ms"]
        if slip is not None and abs(slip) > MEASURED_SLIP_MS:
            flags.append(f"Transients sit {slip:+.2f} ms differently after the join")
        if around > 0 and inside / around > STEP_RATIO:
            flags.append(
                f"A step where a part starts or stops is {inside / around:.1f} times the largest around it: a possible click"
            )
        change = entry["level"]["change_db"]
        if change is not None and abs(change) > LEVEL_DB:
            flags.append(f"The level changes by {change:+.1f} dB across the join")
        entry["flags"] = flags
        if excerpts:
            out = folder / "joins" / f"join-{number:02d}.wav"
            out.parent.mkdir(exist_ok=True)
            middle = (fade[0] + fade[1]) / 2
            start = max(round((middle - seconds / 2) * rate), 0)
            sf.write(out, mix[start : start + round(seconds * rate)], rate, subtype="PCM_24")
            entry["excerpt"] = {"audio": str(out.resolve()), "from_seconds": start / rate}
        report.append(entry)
    return {
        "render": {
            "directory": str(folder),
            "render_id": manifest.get("render_id"),
            "project_sha256": manifest["project_sha256"],
        },
        "tempo": tempo,
        "length": length,
        "joins": report,
        "flagged": [entry["join"] for entry in report if entry["flags"]],
        "methods": {
            "join": "two audio clips in a row on a track, or two hits of different pads, that play the same sample, each a beat or more, the second starting as the first ends",
            "grid": "the sample's saved beat map placed through each region; interval_error_ms is the time between the last beat before the join and the first after it, against whole session beats",
            "measured": f"the sharpest rise within 40 ms of up to {BEATS} beats either side, on the track's stem; the median distance from session beats, after less before",
            "step": "the sample-to-sample change of the stem where the entering part starts and where the leaving part stops, against the largest within 3 ms either side; the edge that stands out more is given",
            "level": "RMS of the stem over up to one second before the fade and after it",
            "flags": f"beat slip over {SLIP_MS} ms; skipped beats not whole bars; first beat inside the fade in; transients over {MEASURED_SLIP_MS} ms apart; step ratio over {STEP_RATIO}; level change over {LEVEL_DB} dB",
        },
    }
