"""Strict, versioned authoring model. Positions are zero-based quarter-note beats."""

from __future__ import annotations
from fractions import Fraction
from pathlib import Path
import hashlib
import json
import os
import re
import tempfile
from typing import Literal
import yaml
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

Beat = int | float | str


def beat(value: Beat) -> Fraction:
    try:
        result = Fraction(str(value))
    except (ValueError, ZeroDivisionError) as exc:
        raise ValueError(
            f"Invalid beat value {value!r}; use a number or fraction like '1/3'"
        ) from exc
    if result < 0:
        raise ValueError("Beat values must be nonnegative")
    return result


def frame(value: Beat | Fraction, tempo: float, rate: int) -> int:
    x = Fraction(value) if isinstance(value, Fraction) else beat(value)
    x = x * 60 * rate / Fraction(str(tempo))
    # Round half up, from absolute time. No cumulative rounding drift.
    return (2 * x.numerator + x.denominator) // (2 * x.denominator)


def midi(note: str) -> int:
    match = re.fullmatch(r"([A-Ga-g])([#b]?)(-?\d+)", note)
    if not match:
        raise ValueError(f"Invalid note {note!r}; use e.g. C2 or F#1")
    key, accidental, octave = match.groups()
    n = (int(octave) + 1) * 12 + {
        "C": 0,
        "D": 2,
        "E": 4,
        "F": 5,
        "G": 7,
        "A": 9,
        "B": 11,
    }[key.upper()]
    n += {"": 0, "#": 1, "b": -1}[accidental]
    if not 0 <= n <= 127:
        raise ValueError(f"Note outside MIDI range: {note}")
    return n


class Strict(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)


class Sample(Strict):
    path: str
    sha256: str | None = Field(default=None, pattern=r"^[0-9a-f]{64}$")
    source: str | None = None
    root_note: str | None = None

    @field_validator("root_note")
    @classmethod
    def note_valid(cls, v):
        if v is not None:
            midi(v)
        return v


class Pad(Strict):
    sample: str
    mode: Literal["one_shot", "gate"] = "one_shot"
    gain_db: float = Field(default=0, ge=-96, le=24)
    pan: float = Field(default=0, ge=-1, le=1)
    transpose: float = Field(default=0, ge=-36, le=36)
    start_seconds: float = Field(default=0, ge=0)
    end_seconds: float | None = Field(default=None, gt=0)
    attack_ms: float = Field(default=0.3, ge=0, le=10000)
    release_ms: float = Field(default=8, ge=0, le=10000)
    choke_group: str | None = None
    reverse: bool = False
    source_bpm: float | None = Field(default=None, ge=20, le=400)
    mono: bool = False

    @model_validator(mode="after")
    def trim_valid(self):
        if self.end_seconds is not None and self.end_seconds <= self.start_seconds:
            raise ValueError("end_seconds must exceed start_seconds")
        return self


class Event(Strict):
    at: Beat
    pad: str
    velocity: int = Field(default=100, ge=1, le=127)
    note: str | None = None
    duration: Beat | None = None
    transpose: float = Field(default=0, ge=-36, le=36)

    @field_validator("at", "duration")
    @classmethod
    def beats_valid(cls, v):
        if v is not None:
            beat(v)
        return v

    @field_validator("note")
    @classmethod
    def note_valid(cls, v):
        if v is not None:
            midi(v)
        return v


class Pattern(Strict):
    length_beats: Beat
    grid: Beat = "1/4"
    steps: dict[str, str] = Field(default_factory=dict)
    events: list[Event] = Field(default_factory=list)
    swing: float = Field(default=0.5, ge=0.5, le=0.75)

    @model_validator(mode="after")
    def timing_valid(self):
        length, grid = beat(self.length_beats), beat(self.grid)
        if length <= 0 or grid <= 0:
            raise ValueError("Pattern length and grid must be positive")
        for pad, row in self.steps.items():
            cells = "".join(row.split()).replace("|", "")
            if any(c not in ".x123456789" for c in cells):
                raise ValueError(f"Invalid step character in {pad}")
            if len(cells) * grid != length:
                raise ValueError(f"Step row {pad} must span length_beats exactly")
        if any(beat(e.at) >= length for e in self.events):
            raise ValueError("Pattern event starts outside pattern")
        return self

    def expanded(self) -> list[Event]:
        result = list(self.events)
        grid = beat(self.grid)
        for pad, row in self.steps.items():
            for i, c in enumerate("".join(row.split()).replace("|", "")):
                if c == ".":
                    continue
                at = i * grid
                if i % 2:
                    at += grid * Fraction(str(2 * self.swing - 1))
                result.append(
                    Event(
                        at=str(at),
                        pad=pad,
                        velocity=100 if c == "x" else round(int(c) * 127 / 9),
                    )
                )
        return result


class Clip(Strict):
    pattern: str
    at: Beat = 0
    repeats: int = Field(default=1, ge=1, le=10000)
    velocity_scale: float = Field(default=1, gt=0, le=2)

    @field_validator("at")
    @classmethod
    def beats_valid(cls, v):
        beat(v)
        return v


class Track(Strict):
    id: str = Field(pattern=r"^[a-zA-Z][a-zA-Z0-9_-]*$")
    gain_db: float = Field(default=0, ge=-96, le=24)
    pan: float = Field(default=0, ge=-1, le=1)
    mute: bool = False
    solo: bool = False
    pads: dict[str, Pad]
    clips: list[Clip] = Field(default_factory=list)


class Section(Strict):
    id: str
    at: Beat
    length_beats: Beat

    @model_validator(mode="after")
    def time_valid(self):
        beat(self.at)
        if beat(self.length_beats) <= 0:
            raise ValueError("Section length must be positive")
        return self


class Session(Strict):
    title: str = "Untitled"
    tempo: float = Field(default=144, ge=20, le=400)
    time_signature: Literal["4/4"] = "4/4"
    sample_rate: Literal[44100, 48000] = 48000
    length_beats: Beat = 16
    master_gain_db: float = Field(default=-6, ge=-96, le=24)
    end_fade_ms: float = Field(default=20, ge=0, le=10000)

    @field_validator("length_beats")
    @classmethod
    def positive(cls, v):
        if beat(v) <= 0:
            raise ValueError("Session length must be positive")
        return v


class Project(Strict):
    schema_version: Literal[1] = 1
    session: Session
    samples: dict[str, Sample] = Field(default_factory=dict)
    patterns: dict[str, Pattern] = Field(default_factory=dict)
    tracks: list[Track] = Field(default_factory=list)
    sections: list[Section] = Field(default_factory=list)

    @model_validator(mode="after")
    def references(self):
        ids = [t.id for t in self.tracks]
        if len(ids) != len(set(ids)):
            raise ValueError("Track IDs must be unique")
        length = beat(self.session.length_beats)
        if len({s.id for s in self.sections}) != len(self.sections):
            raise ValueError("Section IDs must be unique")
        for s in self.sections:
            if beat(s.at) + beat(s.length_beats) > length:
                raise ValueError(f"Section {s.id} exceeds session")
        for t in self.tracks:
            for pad in t.pads.values():
                if pad.sample not in self.samples:
                    raise ValueError(f"{t.id}: unknown sample {pad.sample}")
            for clip in t.clips:
                if clip.pattern not in self.patterns:
                    raise ValueError(f"{t.id}: unknown pattern {clip.pattern}")
                p = self.patterns[clip.pattern]
                if beat(clip.at) + beat(p.length_beats) * clip.repeats > length:
                    raise ValueError(f"{t.id}: clip exceeds session")
                for e in p.expanded():
                    if e.pad not in t.pads:
                        raise ValueError(f"{t.id}: unknown pad {e.pad}")
                    pad = t.pads[e.pad]
                    if e.note and not self.samples[pad.sample].root_note:
                        raise ValueError(
                            f"{pad.sample} needs root_note for pitched events"
                        )
                    if pad.mode == "gate" and (
                        e.duration is None or beat(e.duration) <= 0
                    ):
                        raise ValueError(
                            f"{t.id}.{e.pad}: gated events need positive duration"
                        )
        return self


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def atomic_text(path: Path, text: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=path.parent, prefix="." + path.name)
    try:
        with os.fdopen(fd, "w") as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


class ProjectDumper(yaml.SafeDumper):
    pass


def _represent_mapping(dumper, data):
    flow = (
        ("pad" in data and "at" in data)
        or ("pattern" in data)
        or ("id" in data and "length_beats" in data)
    )
    return dumper.represent_mapping(
        "tag:yaml.org,2002:map", data.items(), flow_style=flow
    )


ProjectDumper.add_representer(dict, _represent_mapping)


def save(project: Project, path: Path):
    data = project.model_dump(mode="json", exclude_none=True, exclude_defaults=True)
    data = {"schema_version": 1, **data}
    atomic_text(
        path,
        yaml.dump(
            data, Dumper=ProjectDumper, sort_keys=False, allow_unicode=True, width=110
        ),
    )


def load(path: Path, verify_assets=True) -> Project:
    p = Project.model_validate(yaml.safe_load(path.read_text()))
    if verify_assets:
        for name, asset in p.samples.items():
            f = (path.parent / asset.path).resolve()
            if not f.is_file():
                raise ValueError(f"Missing sample {name}: {f}")
            if asset.sha256 and digest(f) != asset.sha256:
                raise ValueError(f"Sample content changed: {name}")
    return p


def project_hash(project: Project) -> str:
    return hashlib.sha256(
        json.dumps(project.model_dump(mode="json"), sort_keys=True).encode()
    ).hexdigest()
