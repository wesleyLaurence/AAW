"""Strict, versioned authoring model. Positions are zero-based quarter-note beats."""

from __future__ import annotations
from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path
import hashlib
import json
import os
import re
import tempfile
from typing import Annotated, Literal
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


ID = r"^[a-zA-Z][a-zA-Z0-9_-]*$"


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


class Filter(Strict):
    type: Literal["filter"]
    id: str | None = Field(default=None, pattern=ID)
    mode: Literal["highpass", "lowpass"]
    cutoff_hz: float = Field(ge=10, le=20000)
    slope_db_per_octave: Literal[12, 24, 36, 48] = 12
    bypass: bool = False


class EqBand(Strict):
    shape: Literal["bell", "low_shelf", "high_shelf"]
    freq_hz: float = Field(ge=20, le=20000)
    gain_db: float = Field(ge=-24, le=24)
    q: float = Field(default=0.71, ge=0.1, le=18)


class Eq(Strict):
    type: Literal["eq"]
    id: str | None = Field(default=None, pattern=ID)
    bands: list[EqBand] = Field(min_length=1, max_length=16)
    bypass: bool = False


class Compressor(Strict):
    type: Literal["compressor"]
    id: str | None = Field(default=None, pattern=ID)
    threshold_db: float = Field(ge=-60, le=0)
    ratio: float = Field(default=4, ge=1, le=20)
    attack_ms: float = Field(default=10, ge=0, le=500)
    release_ms: float = Field(default=120, ge=1, le=5000)
    knee_db: float = Field(default=6, ge=0, le=24)
    makeup_db: float = Field(default=0, ge=-24, le=24)
    sidechain: str | None = None
    bypass: bool = False


class Limiter(Strict):
    type: Literal["limiter"]
    id: str | None = Field(default=None, pattern=ID)
    ceiling_db: float = Field(default=-1, ge=-24, le=-0.1)
    release_ms: float = Field(default=60, ge=1, le=2000)
    lookahead_ms: float = Field(default=3, ge=0.5, le=20)
    bypass: bool = False


class Delay(Strict):
    type: Literal["delay"]
    id: str | None = Field(default=None, pattern=ID)
    time_beats: Beat
    feedback_percent: float = Field(default=35, ge=0, le=95)
    lowcut_hz: float | None = Field(default=None, ge=10, le=20000)
    highcut_hz: float | None = Field(default=None, ge=10, le=20000)
    ping_pong: bool = False
    mix_percent: float = Field(default=100, ge=0, le=100)
    bypass: bool = False

    @model_validator(mode="after")
    def delay_valid(self):
        if not 0 < beat(self.time_beats) <= 16:
            raise ValueError("delay time_beats must be greater than 0 and at most 16")
        if self.lowcut_hz and self.highcut_hz and self.lowcut_hz >= self.highcut_hz:
            raise ValueError("delay lowcut_hz must be below highcut_hz")
        return self


class Reverb(Strict):
    type: Literal["reverb"]
    id: str | None = Field(default=None, pattern=ID)
    decay_seconds: float = Field(default=1.5, ge=0.1, le=12)
    predelay_ms: float = Field(default=10, ge=0, le=250)
    damping_hz: float = Field(default=6000, ge=500, le=20000)
    lowcut_hz: float = Field(default=100, ge=20, le=2000)
    width_percent: float = Field(default=100, ge=0, le=100)
    mix_percent: float = Field(default=100, ge=0, le=100)
    seed: int = Field(default=0, ge=0, le=2**32 - 1)
    bypass: bool = False


Effect = Annotated[
    Filter | Eq | Compressor | Limiter | Delay | Reverb, Field(discriminator="type")
]


def sidechains(effects) -> list[str]:
    return [e.sidechain for e in effects if isinstance(e, Compressor) and e.sidechain]


class Point(Strict):
    """An automation breakpoint. curve shapes the segment from here to the next point."""

    at: Beat
    value: float
    curve: Literal["linear", "hold"] = "linear"

    @field_validator("at")
    @classmethod
    def beats_valid(cls, v):
        beat(v)
        return v


class Lane(Strict):
    """Automation for one parameter. It overrides the static value for the whole song."""

    param: str
    points: list[Point] = Field(min_length=1, max_length=10000)

    @model_validator(mode="after")
    def points_ordered(self):
        times = [beat(p.at) for p in self.points]
        if any(b < a for a, b in zip(times, times[1:])):
            raise ValueError(f"{self.param}: automation points must be in time order")
        if any(a == c for a, c in zip(times, times[2:])):
            raise ValueError(f"{self.param}: at most two points may share a position")
        return self


# Continuous parameters a lane may target, with the domain values interpolate in.
# Frequencies and q move in equal ratios per beat; dB, pan and percent move linearly.
CHANNEL_PARAMS = {"gain_db": "linear", "pan": "linear"}
EFFECT_PARAMS = {
    "filter": {"cutoff_hz": "log"},
    "eq": {"freq_hz": "log", "gain_db": "linear", "q": "log"},
    "compressor": {"threshold_db": "linear", "makeup_db": "linear"},
    "delay": {"feedback_percent": "linear", "mix_percent": "linear"},
    "reverb": {"mix_percent": "linear"},
}


def bounds(model, field) -> tuple[float, float]:
    lo, hi = -float("inf"), float("inf")
    for m in model.model_fields[field].metadata:
        lo = max(lo, getattr(m, "ge", getattr(m, "gt", lo)))
        hi = min(hi, getattr(m, "le", getattr(m, "lt", hi)))
    return lo, hi


class Send(Strict):
    to: str
    gain_db: float = Field(default=0, ge=-96, le=12)
    pre_fader: bool = False


class Track(Strict):
    id: str = Field(pattern=ID)
    gain_db: float = Field(default=0, ge=-96, le=24)
    pan: float = Field(default=0, ge=-1, le=1)
    mute: bool = False
    solo: bool = False
    pads: dict[str, Pad]
    clips: list[Clip] = Field(default_factory=list)
    effects: list[Effect] = Field(default_factory=list, max_length=32)
    sends: list[Send] = Field(default_factory=list, max_length=16)
    automation: list[Lane] = Field(default_factory=list, max_length=64)

    def sidechains(self) -> list[str]:
        return sidechains(self.effects)


class Return(Strict):
    """A return bus: the sum of track sends through its own chain, then to master."""

    id: str = Field(pattern=ID)
    gain_db: float = Field(default=0, ge=-96, le=24)
    pan: float = Field(default=0, ge=-1, le=1)
    mute: bool = False
    effects: list[Effect] = Field(default_factory=list, max_length=32)
    automation: list[Lane] = Field(default_factory=list, max_length=64)

    def sidechains(self) -> list[str]:
        return sidechains(self.effects)


class Master(Strict):
    effects: list[Effect] = Field(default_factory=list, max_length=32)
    automation: list[Lane] = Field(default_factory=list, max_length=64)


@dataclass(frozen=True)
class Target:
    """A resolved lane target. key is canonical: effects are addressed by index."""

    kind: Literal["channel", "send", "effect"]
    field: str
    domain: str
    low: float
    high: float
    send: str | None = None
    effect: int | None = None
    band: int | None = None

    @property
    def name(self) -> str:
        """Parameter name within its effect, e.g. cutoff_hz or bands.0.gain_db."""
        return self.field if self.band is None else f"bands.{self.band}.{self.field}"

    @property
    def key(self) -> str:
        if self.kind == "send":
            return f"sends.{self.send}.{self.field}"
        if self.kind == "effect":
            return f"effects.{self.effect}.{self.name}"
        return self.field


def target(owner: Track | Return | Master, param: str) -> Target:
    """Resolve gain_db, pan, sends.RETURN.gain_db, effects.REF.FIELD or
    effects.REF.bands.N.FIELD, where REF is an effect id or zero-based index."""
    parts = param.split(".")
    if isinstance(owner, Master):
        if parts == ["gain_db"]:
            return Target(
                "channel", "gain_db", "linear", *bounds(Session, "master_gain_db")
            )
    elif len(parts) == 1 and parts[0] in CHANNEL_PARAMS:
        return Target("channel", parts[0], "linear", *bounds(type(owner), parts[0]))
    if isinstance(owner, Track) and parts[0] == "sends" and len(parts) == 3:
        if parts[2] != "gain_db":
            raise ValueError(f"{param}: only a send's gain_db can be automated")
        if parts[1] not in {s.to for s in owner.sends}:
            raise ValueError(f"{param}: no send to {parts[1]}")
        return Target(
            "send", "gain_db", "linear", *bounds(Send, "gain_db"), send=parts[1]
        )
    if parts[0] == "effects" and len(parts) >= 3:
        ids = [e.id for e in owner.effects]
        if parts[1].isdigit() and int(parts[1]) < len(ids):
            index = int(parts[1])
        elif parts[1] in ids:
            index = ids.index(parts[1])
        else:
            raise ValueError(f"{param}: no effect with id or index {parts[1]}")
        spec = owner.effects[index]
        allowed = EFFECT_PARAMS.get(spec.type, {})
        band, model = None, type(spec)
        if spec.type == "eq":
            if len(parts) != 5 or parts[2] != "bands" or not parts[3].isdigit():
                raise ValueError(
                    f"{param}: address eq bands as effects.REF.bands.N.FIELD"
                )
            band, model = int(parts[3]), EqBand
            if band >= len(spec.bands):
                raise ValueError(f"{param}: eq has {len(spec.bands)} bands")
        elif len(parts) != 3:
            raise ValueError(f"{param}: expected effects.REF.FIELD")
        field = parts[-1]
        if field not in allowed:
            raise ValueError(
                f"{param}: {spec.type} {field} cannot be automated; "
                f"automatable: {', '.join(allowed) or 'none'}"
            )
        return Target(
            "effect",
            field,
            allowed[field],
            *bounds(model, field),
            effect=index,
            band=band,
        )
    raise ValueError(
        f"{param}: unknown automation target; use gain_db"
        + ("" if isinstance(owner, Master) else ", pan")
        + (", sends.RETURN.gain_db" if isinstance(owner, Track) else "")
        + " or effects.REF.FIELD"
    )


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
    returns: list[Return] = Field(default_factory=list)
    sections: list[Section] = Field(default_factory=list)
    master: Master = Field(default_factory=Master)

    def render_order(self) -> list[Track]:
        """Tracks with sidechain sources first; otherwise document order."""
        done, order = set(), []
        while len(order) < len(self.tracks):
            for t in self.tracks:
                if t.id not in done and all(s in done for s in t.sidechains()):
                    done.add(t.id)
                    order.append(t)
                    break
        return order

    def senders(self, return_id: str) -> list[str]:
        """Tracks with a send to return_id, in document order."""
        return [t.id for t in self.tracks if any(s.to == return_id for s in t.sends)]

    def sidechain_sources(self, channel_id: str) -> set[str]:
        """Every track whose audio feeds a track or return's detectors, directly or not."""
        tracks = {t.id: t for t in self.tracks}
        channel = tracks.get(channel_id) or next(
            r for r in self.returns if r.id == channel_id
        )
        found, stack = set(), list(channel.sidechains())
        while stack:
            s = stack.pop()
            if s not in found:
                found.add(s)
                stack.extend(tracks[s].sidechains())
        return found

    @model_validator(mode="after")
    def references(self):
        ids = [t.id for t in self.tracks]
        return_ids = [r.id for r in self.returns]
        if len(ids) != len(set(ids)):
            raise ValueError("Track IDs must be unique")
        if len(ids + return_ids) != len(set(ids + return_ids)):
            raise ValueError("Return IDs must be unique and distinct from track IDs")
        for t in [*self.tracks, *self.returns]:
            for source in t.sidechains():
                if source in return_ids:
                    raise ValueError(
                        f"{t.id}: sidechain must name a track, not return {source}"
                    )
                if source not in ids:
                    raise ValueError(f"{t.id}: unknown sidechain track {source}")
                if source == t.id:
                    raise ValueError(f"{t.id}: a track cannot sidechain itself")
        for t in self.tracks:
            targets = [s.to for s in t.sends]
            for to in targets:
                if to not in return_ids:
                    raise ValueError(f"{t.id}: send to unknown return {to}")
            if len(targets) != len(set(targets)):
                raise ValueError(f"{t.id}: at most one send per return")
        visiting, visited = set(), set()
        graph = {t.id: t.sidechains() for t in self.tracks}

        def visit(node):
            if node in visiting:
                raise ValueError(f"Sidechain cycle through track {node}")
            if node not in visited:
                visiting.add(node)
                for source in graph[node]:
                    visit(source)
                visiting.remove(node)
                visited.add(node)

        for node in graph:
            visit(node)
        if any(getattr(e, "sidechain", None) for e in self.master.effects):
            raise ValueError("Master compressor cannot use a sidechain")
        chains = [*self.tracks, *self.returns, self.master]
        for owner, e in ((c, e) for c in chains for e in c.effects):
            if isinstance(e, Delay):
                seconds = beat(e.time_beats) * 60 / Fraction(str(self.session.tempo))
                if not Fraction(1, 1000) <= seconds <= 10:
                    raise ValueError(
                        f"{getattr(owner, 'id', 'master')}: delay time must be "
                        f"1 ms to 10 s at the session tempo, not {float(seconds):.4g} s"
                    )
        length = beat(self.session.length_beats)
        for owner in chains:
            name = getattr(owner, "id", "master")
            ids = [e.id for e in owner.effects if e.id]
            if len(ids) != len(set(ids)):
                raise ValueError(f"{name}: effect IDs must be unique")
            keys = set()
            for lane in owner.automation:
                try:
                    t = target(owner, lane.param)
                except ValueError as exc:
                    raise ValueError(f"{name}: {exc}") from None
                if t.key in keys:
                    raise ValueError(f"{name}: more than one lane for {lane.param}")
                keys.add(t.key)
                for point in lane.points:
                    if beat(point.at) > length:
                        raise ValueError(
                            f"{name}: {lane.param} point at {point.at} is after the session end"
                        )
                    if not t.low <= point.value <= t.high:
                        raise ValueError(
                            f"{name}: {lane.param} value {point.value} outside "
                            f"{t.low:g} to {t.high:g}"
                        )
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
        or "type" in data
        or "shape" in data
        or ("to" in data and set(data) <= {"to", "gain_db", "pre_fader"})
        or ("at" in data and "value" in data)
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
