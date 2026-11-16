"""Notes from audio: `daw transcribe`.

A part's notes as MIDI on the song's beat map. Spotify's Basic Pitch
(Bittner et al., ICASSP 2022, Apache-2.0) hears them: polyphonic, any
instrument, best on one instrument at a time, which a stem gives. It runs in the
separation environment `daw separate setup` makes (transcribe_runner.py), its
ONNX model on the processor, a few seconds for a song; everything after its
notes is here, so it can be tested with notes made up (AAW_TRANSCRIBER names a
stand-in).

1. The model's notes: each with its start and end in seconds, its pitch, how
   sure the model is it sounds (its mean note activation, 0 to 1) and how far
   its pitch bends.
2. One pitch heard twice in a row with no gap is one note (`joined`).
3. With --line, one note at a time, for a bass line or a lead (`one_line`):
   notes starting together keep the surest, and a weaker one an octave or a
   semitone from it that starts while it sounds is dropped; any other note
   ends the one before.
4. Each note's velocity comes from the part's own level where it starts,
   against the loudest note's: the model's activation says how sure it is, not
   how hard the note was played.
5. Each note is placed on the beat map: bar, beat, its nearest sixteenth and
   how far off it, as `daw samples hits` places hits, and beats counted from
   bar 1, which a note clip, a MIDI file and note text are written in. A part
   `daw separate` made is placed on the source song's beat map, measured on
   the whole song; --beats names another file to measure.

Every value is an estimate; nothing here listens.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import struct
import subprocess
import tempfile
from pathlib import Path

import numpy as np

from . import beats
from .hits import Grid

NAME = "basic-pitch"
TIMEOUT = 600
TOGETHER = 0.050  # notes starting this near each other start together
JOIN_GAP = 0.020  # one pitch again this soon after it ends may be the same note
PIECE = 0.080  # when one of the two is this short: shorter than any sixteenth to 187 BPM
DOUBT = 0.75  # an octave this sure against the note kept is said beside it
SURE = 0.5  # a note this sure is counted as sure
LEVEL_SECONDS = 0.050  # the stretch at a note's start its level is read over
VELOCITY_RANGE_DB = 40.0  # velocity 127 at the loudest note, 1 this far under
TICKS = 960  # a written file's ticks a beat
NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def name(pitch: int) -> str:
    """A MIDI number as a note name, middle C (60) as C4."""
    return f"{NAMES[pitch % 12]}{pitch // 12 - 1}"


# --- running the model ---------------------------------------------------------


def unavailable() -> str | None:
    """Why the model cannot run on this Mac, or None when it can."""
    if os.environ.get("AAW_TRANSCRIBER"):
        return None
    from . import separation

    if not separation.env_python().is_file():
        return "The separation environment is not installed; daw separate setup installs it with Basic Pitch"
    if not separation.package_here("basic-pitch"):
        return "The separation environment lacks Basic Pitch; daw separate setup adds it"
    return None


def heard(path: Path) -> dict:
    """The model's notes for a file, as transcribe_runner.py prints them."""
    from . import separation

    why = unavailable()
    if why:
        raise ValueError(why)
    value = os.environ.get("AAW_TRANSCRIBER")
    if value:
        command = [*shlex.split(value), "--source", str(path)]
    else:
        runner = Path(__file__).with_name("transcribe_runner.py")
        command = [str(separation.env_python()), "-I", str(runner), "--source", str(path)]
    try:
        out = subprocess.run(command, capture_output=True, text=True, timeout=TIMEOUT)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ValueError(f"Basic Pitch did not run: {exc}") from None
    lines = [line for line in out.stdout.splitlines() if line.startswith("{")]
    try:
        result = json.loads(lines[-1])
    except (IndexError, json.JSONDecodeError):
        raise ValueError(f"Basic Pitch said nothing readable: {out.stderr.strip()[-500:]}") from None
    if out.returncode or "error" in result:
        raise ValueError(f"Basic Pitch failed: {result.get('error') or out.stderr.strip()[-500:]}")
    notes = [
        {"start": float(s), "end": float(e), "pitch": int(p), "confidence": float(c), "bend": (int(lo), int(hi))}
        for s, e, p, c, lo, hi in result["notes"]
    ]
    return {"notes": notes, "version": result.get("version"), "device": result.get("device"), "seconds": result.get("seconds")}


# --- what the notes are ------------------------------------------------------------


def joined(notes: list[dict]) -> tuple[list[dict], int]:
    """One pitch heard twice at once is one note, and so is one pitch that starts
    again within JOIN_GAP of where it ended when either piece is shorter than
    PIECE: the model splits a held note where it wavers. A note struck again
    legato also ends where the next starts, and is kept: both pieces are as long
    as the notes played."""
    out, last = [], {}
    count = 0
    for n in sorted(notes, key=lambda n: (n["start"], n["pitch"])):
        before = last.get(n["pitch"])
        overlaps = before is not None and n["start"] < before["end"] - 1e-3
        piece = before is not None and min(before["end"] - before["start"], n["end"] - n["start"]) < PIECE
        if overlaps or (piece and n["start"] <= before["end"] + JOIN_GAP):
            span = (before["end"] - before["start"]) + (n["end"] - n["start"])
            before["confidence"] = (
                before["confidence"] * (before["end"] - before["start"]) + n["confidence"] * (n["end"] - n["start"])
            ) / span if span > 0 else max(before["confidence"], n["confidence"])
            before["end"] = max(before["end"], n["end"])
            before["bend"] = (min(before["bend"][0], n["bend"][0]), max(before["bend"][1], n["bend"][1]))
            count += 1
            continue
        n = dict(n)
        out.append(n)
        last[n["pitch"]] = n
    return out, count


def kin(a: int, b: int) -> bool:
    """Two pitches the model mistakes for each other: an octave apart, or a semitone."""
    return (a - b) % 12 == 0 or abs(a - b) == 1


def one_line(notes: list[dict]) -> tuple[list[dict], int]:
    """One note at a time, for a bass line or a lead; how many notes were dropped."""
    notes = sorted(notes, key=lambda n: n["start"])
    kept, dropped, i = [], 0, 0
    while i < len(notes):
        j = i + 1
        while j < len(notes) and notes[j]["start"] - notes[i]["start"] <= TOGETHER:
            j += 1
        together = notes[i:j]
        chosen = max(together, key=lambda n: n["confidence"])
        best = dict(chosen)
        octaves = [
            n for n in together
            if n is not chosen and n["pitch"] != best["pitch"] and (n["pitch"] - best["pitch"]) % 12 == 0
            and n["confidence"] >= DOUBT * best["confidence"]
        ]
        if octaves:
            best["octave_doubt"] = max(octaves, key=lambda n: n["confidence"])["pitch"]
        kept.append(best)
        dropped += len(together) - 1
        i = j
    out = []
    for n in kept:
        if out and n["start"] < out[-1]["end"]:
            before = out[-1]
            if n["confidence"] < before["confidence"] and kin(n["pitch"], before["pitch"]):
                dropped += 1
                continue
            before["end"] = n["start"]
        out.append(n)
    return out, dropped


def velocities(notes: list[dict], mono: np.ndarray, sr: int) -> None:
    """Each note's velocity from the part's level over its first LEVEL_SECONDS,
    127 at the loudest note's and 1 at VELOCITY_RANGE_DB under it."""
    if not notes:
        return
    levels = []
    for n in notes:
        a = max(0, int(n["start"] * sr))
        b = max(a + 1, min(len(mono), a + int(min(LEVEL_SECONDS, n["end"] - n["start"]) * sr)))
        piece = mono[a:b].astype(np.float64)
        rms = float(np.sqrt(np.mean(piece**2))) if len(piece) else 0.0
        levels.append(20 * np.log10(max(rms, 1e-9)))
    top = max(levels)
    for n, level in zip(notes, levels):
        n["level_db"] = round(level - top, 1)
        n["velocity"] = int(np.clip(round(127 + (level - top) * 127 / VELOCITY_RANGE_DB), 1, 127))


def first_bar(report: dict) -> int:
    """The beat map's index of bar 1's first beat."""
    return next((i for i, b in enumerate(report["beats"]) if b["bar"] == 1 and b["beat"] == 1), 0)


def placed(notes: list[dict], report: dict | None) -> list[dict]:
    """The notes as the reply lists them, each on the beat map where there is one."""
    grid = Grid(report) if report else None
    origin = first_bar(report) if report else 0
    rows = []
    for n in notes:
        row = {
            "pitch": name(n["pitch"]),
            "midi": n["pitch"],
            "seconds": round(n["start"], 4),
            "until_seconds": round(n["end"], 4),
        }
        if grid:
            where, index = grid.place(n["start"])
            end = grid.index(n["end"])
            row.update(where)
            row["beats"] = round(float(index - origin), 4)
            row["length_beats"] = round(float(end - index), 4)
        row["velocity"] = n.get("velocity", 100)
        row["level_db"] = n.get("level_db")
        row["confidence"] = round(n["confidence"], 3)
        if n["bend"] != (0, 0):
            row["bend_cents"] = list(n["bend"])
        if "octave_doubt" in n:
            row["octave_doubt"] = name(n["octave_doubt"])
        rows.append(row)
    return rows


# --- which beat map ------------------------------------------------------------


def separated_from(path: Path) -> Path | None:
    """The song a part `daw separate` made was taken from, by the recipe beside it:
    the part has the song's length to the frame, so the song's beat map is its."""
    folder = path.parent
    if folder.name != "separated":
        return None
    project = folder.parent
    match = re.match(r"^([0-9a-f]{12})-", path.name)
    if not match:
        return None
    for recipe in sorted(folder.glob(f"{match.group(1)}-*.separated.json")):
        try:
            record = json.loads(recipe.read_text())
        except (OSError, ValueError):
            continue
        parts = [Path(p.get("path", "")).name for p in record.get("parts", {}).values()]
        source = project / record.get("source", {}).get("path", "")
        if path.name in parts and source.is_file():
            return source.resolve()
    return None


# --- writing ---------------------------------------------------------------------


def midi_file(rows: list[dict], shift: float, path: Path) -> None:
    """A type 0 MIDI file of the rows at TICKS a beat, each note at its beats from
    bar 1 plus `shift`, with its velocity; no tempo, as the song keeps its own."""
    events = []
    for r in rows:
        start = round((r["beats"] + shift) * TICKS)
        end = max(start + 1, round((r["beats"] + r["length_beats"] + shift) * TICKS))
        events.append((start, 1, 0x90, r["midi"], r["velocity"]))
        events.append((end, 0, 0x80, r["midi"], 0))
    events.sort()
    body, now = bytearray(), 0
    for tick, _, status, pitch, velocity in events:
        body += varlen(tick - now) + bytes([status, pitch, velocity])
        now = tick
    body += varlen(0) + b"\xff\x2f\x00"
    path.write_bytes(b"MThd" + struct.pack(">IHHH", 6, 0, 1, TICKS) + b"MTrk" + struct.pack(">I", len(body)) + body)


def varlen(value: int) -> bytes:
    out = [value & 0x7F]
    value >>= 7
    while value:
        out.append(0x80 | (value & 0x7F))
        value >>= 7
    return bytes(reversed(out))


def slug(text: str) -> str:
    """A track ID from a name: letters, digits, - and _, starting with a letter."""
    words = re.sub(r"[^A-Za-z0-9_-]+", "-", text).strip("-")
    return words if words[:1].isalpha() else f"notes-{words}".strip("-")


# --- the command -----------------------------------------------------------------


def resolve(value: str, song: Path | None, db: Path) -> tuple[Path, str, dict | None]:
    """What SOURCE names: a file, a library sample, or with a song one of its samples
    by ID or an audio clip as tracks.T.audio.N; its path, a name for its notes and
    the clip."""
    from . import library
    from .model import load
    from .separation import CLIP

    if song is not None:
        project = load(song, False)
        samples = project.get("samples", {})
        if value in samples:
            return (song.parent / samples[value]["path"]).resolve(), value, None
        if match := CLIP.match(value):
            track_id, index = match.group(1), int(match.group(2))
            track = next((t for t in project.get("tracks", []) if t["id"] == track_id), None)
            if track is None:
                raise ValueError(f"No track {track_id}")
            clips = track.get("audio") or []
            if index >= len(clips):
                raise ValueError(f"tracks.{track_id} has {len(clips)} audio clips; no clip {index}")
            clip = clips[index]
            return (song.parent / samples[clip["sample"]]["path"]).resolve(), clip["sample"], {
                **clip,
                "ref": value,
                "tempo": float(project["session"].get("tempo", 120)),
            }
    path = library.resolve(db, value)
    return path, path.stem, None


class Played:
    """How an audio clip plays its file, as the engine has it (aaw-host's Span::of
    and the schema's seconds_per_beat): from source_start_seconds on its beat
    `at`, seconds_per_beat of the file a song beat, faster as a repitched
    transpose goes up; to source_end_seconds, the end of its loop's first pass,
    its length_beats or the file's end, whichever comes first."""

    def __init__(self, clip: dict, file_seconds: float):
        tempo = float(clip["tempo"])
        repitched = clip.get("stretch", "repitch") == "repitch"
        transpose = float(clip.get("transpose", 0) or 0)
        source_bpm = float(clip.get("source_bpm") or 0)
        self.at = float(clip["at"])
        self.start = float(clip.get("source_start_seconds", 0) or 0)
        self.per_beat = 60 / (source_bpm or tempo) * (2 ** (transpose / 12) if repitched else 1.0)
        # What it is heard moved by: its transpose, and a repitched tempo of its own
        # played at the song's.
        self.semitones = transpose + (12 * np.log2(tempo / source_bpm) if repitched and source_bpm else 0.0)
        ends = [file_seconds]
        if clip.get("source_end_seconds") is not None:
            ends.append(float(clip["source_end_seconds"]))
        for beats_long in (clip.get("loop_beats"), clip.get("length_beats")):
            if beats_long is not None:
                ends.append(self.start + float(beats_long) * self.per_beat)
        self.end = min(ends)
        self.loops = clip.get("loop_beats") is not None

    def beat(self, seconds: float) -> float:
        """The song's beat where the clip plays a time of its file."""
        return self.at + (seconds - self.start) / self.per_beat


def transcribe(
    source: str,
    song: Path | None = None,
    track: str | None = None,
    at: float | None = None,
    line: bool = False,
    note_range: tuple[int, int] | None = None,
    min_confidence: float = 0.0,
    beats_of: str | None = None,
    near: float | None = None,
    window: float = 10.0,
    midi_out: Path | None = None,
    text: bool = False,
    grid: str | None = None,
    quantize: str | None = None,
    db: Path | None = None,
) -> dict:
    from . import library

    db = db or library.default_db()
    writes = bool(song is not None or midi_out is not None or text)
    if quantize is not None and not writes:
        raise ValueError("--quantize moves the written notes: give it with --text, --midi or --project")
    if grid is not None and not text:
        raise ValueError("--grid is the cells of --text")
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
        problem = None
    except ValueError as error:
        if writes:
            raise ValueError(f"The notes are written on a beat map, and {of.name} has none: {error}") from None
        report, problem = None, str(error)
    found = heard(path)
    mono, sr = beats.read(path)
    played = Played(clip, len(mono) / sr) if clip is not None else None
    left_out = {}
    notes = found["notes"]
    if played is not None:
        # Only what the clip plays, each note at the pitch it is heard at there.
        inside = [n for n in notes if played.start <= n["start"] < played.end]
        left_out["outside_the_clip"] = len(notes) - len(inside)
        notes = [{**n, "end": min(n["end"], played.end)} for n in inside]
    notes, left_out["joined"] = joined(notes)
    if line:
        notes, left_out["not_the_line"] = one_line(notes)
    shift = round(played.semitones) if played is not None else 0
    if shift:
        for n in notes:
            n["pitch"] += shift
            if "octave_doubt" in n:
                n["octave_doubt"] += shift
        heard_at = len(notes)
        notes = [n for n in notes if 0 <= n["pitch"] <= 127]
        if len(notes) < heard_at:
            left_out["outside_midi"] = heard_at - len(notes)
        for n in notes:
            if "octave_doubt" in n and not 0 <= n["octave_doubt"] <= 127:
                del n["octave_doubt"]
    if note_range is not None:
        kept = [n for n in notes if note_range[0] <= n["pitch"] <= note_range[1]]
        left_out["outside_range"] = len(notes) - len(kept)
        notes = kept
    kept = [n for n in notes if n["confidence"] >= min_confidence]
    left_out["under_min_confidence"] = len(notes) - len(kept)
    notes = kept
    velocities(notes, mono, sr)
    rows = placed(notes, report)
    out = {
        "path": str(path),
        "model": {
            "name": NAME,
            "version": found["version"],
            "device": found["device"],
            "seconds": found["seconds"],
        },
        "beat_map": None if report is None else report.get("map"),
        **({"beat_map_of": str(of)} if of != path else {}),
        **({"no_beat_map": problem} if problem else {}),
        "tempo_bpm": None if report is None else report["tempo"]["bpm"],
        "meter": None if report is None else report["meter"],
        "first_downbeat_seconds": None if report is None else report["first_downbeat_seconds"],
        "odd_bars": [] if report is None else report.get("odd_bars", []),
        "line": line,
        "count": len(rows),
        "sure": sum(1 for r in rows if r["confidence"] >= SURE),
        "left_out": left_out,
        "lowest": name(min(n["pitch"] for n in notes)) if notes else None,
        "highest": name(max(n["pitch"] for n in notes)) if notes else None,
    }
    warnings = []
    if played is not None:
        if shift:
            out["transposed_by"] = shift
        if abs(played.semitones - shift) >= 0.1:
            warnings.append(
                f"{clip['ref']} plays the part {played.semitones:+.2f} semitones from as it was heard; "
                f"the notes are moved {shift:+d}"
            )
        if played.loops:
            warnings.append(f"{clip['ref']} loops; the note clip holds its first pass")
    if writes:
        out.update(written(rows, report, label, song, track, at, played, midi_out, text, grid or "1/4", quantize))
    listed = rows
    if near is not None:
        listed = [r for r in rows if r["until_seconds"] >= near - window and r["seconds"] <= near + window]
        out["near"] = {"seconds": near, "window_seconds": window, "of": len(rows)}
    out["notes"] = listed
    if warnings:
        out["warnings"] = warnings
    out["source"] = (
        "heard by Basic Pitch from decoded audio and placed on the beat map; estimates, not ground truth"
    )
    return out


def written(rows, report, label, song, track, at, played, midi_out, text, grid, quantize) -> dict:
    """The notes as note text, a MIDI file and a note clip in the song.

    One MIDI file of them, in the part's own beats with bar 1 a whole number of
    bars in, is read by the Rust daw into a scratch song, quantized there and
    written back out, and the text is read from it. The song itself is changed
    once, at the end, by importing the notes at its own beats, so a step that
    fails leaves it as it was and the clip is one edit to undo.
    """
    from fractions import Fraction

    from .cli import run_engine

    if not rows:
        raise ValueError("No note is left to write")
    per_bar = int(report["beats_per_bar"])
    earliest = min(r["beats"] for r in rows)
    pickup = float(per_bar * np.ceil(-earliest / per_bar - 1e-9)) if earliest < 0 else 0.0
    out = {}
    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        sketch = scratch / "song"
        run_engine("init", sketch)
        sketch = sketch / "song.yaml"
        # The map's beats are the scratch song's: a bar of per_bar of them.
        if per_bar != 4:
            run_engine("set", sketch, "session.time_signature", f"{per_bar}/4")
        file = scratch / f"{slug(label)}-notes.mid"
        midi_file(rows, pickup, file)
        ref = next(p for p in run_engine("midi", "import", sketch, file)["paths"] if ".clips." in p)
        if quantize is not None:
            run_engine("note", "quantize", sketch, ref, "--grid", quantize)
            run_engine("midi", "export", sketch, ref, file)
            rows = [
                {
                    "beats": float(Fraction(str(n["at"]))) - pickup,
                    "length_beats": float(Fraction(str(n["duration"]))),
                    "midi": n["pitch"],
                    "velocity": n["velocity"],
                }
                for n in run_engine("note", "list", sketch, ref)["clips"][0]["notes"]
            ]
        if text:
            listed = run_engine("note", "list", sketch, ref, "--as", "text", "--grid", grid)["clips"][0]
            out["text"] = {
                "grid": grid,
                "from_bar": 1 - round(pickup / per_bar),
                "text": listed["text"],
                "off_grid": len(listed["off_grid"]),
            }
        if midi_out is not None:
            target = midi_out.resolve()
            target.write_bytes(file.read_bytes())
            out["midi"] = {"path": str(target), "bar_one_beat": pickup}
        # Last, so nothing before it can fail with the song changed.
        if song is not None:
            out["clip"] = into_song(rows, song, track, at, played, report, per_bar, file, quantize)
            out.update(out["clip"].pop("left"))
    return out


def into_song(rows, song, track, at, played, report, per_bar, file, quantize) -> dict:
    """The note clip in the song: bar 1 at `at`, or where an audio clip plays the
    part's first downbeat, each beat of the part as many of the song's beats as
    the clip plays it in; a bar early for each bar of pickup, and what would come
    before the song's start left out."""
    from .cli import run_engine

    scale = 1.0
    if played is not None:
        scale = (60 / report["tempo"]["bpm"]) / played.per_beat
        at = played.beat(report["first_downbeat_seconds"])
    rows = [{**r, "beats": r["beats"] * scale, "length_beats": r["length_beats"] * scale} for r in rows]
    earliest = min(r["beats"] for r in rows)
    bar = per_bar * scale
    pickup = bar * float(np.ceil(-earliest / bar - 1e-9)) if earliest < 0 else 0.0
    clip_at = max(0.0, at - pickup)
    shift = at - clip_at
    kept = [r for r in rows if r["beats"] + shift >= -1e-9]
    if not kept:
        raise ValueError("Every note comes before the song's start")
    midi_file(kept, shift, file)
    args = ["midi", "import", song, file, "--at", round(clip_at, 6)]
    if track is not None:
        args += ["--track", track]
    made = run_engine(*args)
    return {
        # A new track's import names the track and the clip; one on a track, the clip.
        "ref": made.get("path") or next(p for p in made["paths"] if ".clips." in p),
        "at": round(clip_at, 4),
        "bar_one_at": round(at, 4),
        "notes": made["notes"],
        **({"song_beats_a_beat": round(scale, 4)} if abs(scale - 1) > 1e-4 else {}),
        **({"quantized_to": quantize} if quantize else {}),
        "project_sha256": made["project_sha256"],
        "left": {"before_the_song": len(rows) - len(kept)} if len(kept) < len(rows) else {},
    }
