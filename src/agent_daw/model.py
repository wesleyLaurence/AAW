"""The song model, through the Rust core.

`aaw_py` is the one implementation of the document: its schema, validation,
exact beats, canonical YAML and fingerprints. A song here is plain data, the full
dump of a validated document with every field present, as dicts and lists.
Positions are zero-based quarter-note beats.
"""

from __future__ import annotations
from fractions import Fraction
from pathlib import Path
from typing import NamedTuple
import hashlib
import json
import os
import tempfile
from . import aaw_py

Beat = int | float | str


def beat(value: Beat) -> Fraction:
    """A beat written as an integer, a decimal or a fraction like '1/3', exactly."""
    return Fraction(*aaw_py.beat(value))


def frame(value: Beat | Fraction, tempo: float, rate: int) -> int:
    """The audio frame of a beat: rounded half up, from absolute time."""
    x = value if isinstance(value, Fraction) else beat(value)
    return aaw_py.frame(x.numerator, x.denominator, tempo, rate)


class TempoMap:
    """The song's tempo through time, as the engine has it: `session.tempo`,
    or the steps of the master's tempo lane. Each step holds from its beat
    to the next; everything placed in beats stays on its beat. With one
    tempo, seconds are `beats * 60 / tempo` and beats `seconds * tempo / 60`,
    as they always were."""

    def __init__(self, steps):
        # (beat, bpm, seconds) a step, the first at beat 0.
        self.steps = [(Fraction(b), float(bpm), float(s)) for b, bpm, s in steps]
        self._beats = [float(b) for b, _, _ in self.steps]
        self._seconds = [s for _, _, s in self.steps]

    @classmethod
    def of(cls, project) -> "TempoMap":
        return cls((Fraction(n, d), bpm, s) for n, d, bpm, s in aaw_py.tempo_map(project))

    @classmethod
    def constant(cls, bpm: float) -> "TempoMap":
        return cls([(Fraction(0), bpm, 0.0)])

    @property
    def tempo(self) -> float | None:
        """The one tempo of a song that keeps one; None where it changes."""
        return self.steps[0][1] if len(self.steps) == 1 else None

    @property
    def opening(self) -> float:
        return self.steps[0][1]

    def _at_beat(self, beat: float) -> int:
        i = 0
        while i + 1 < len(self._beats) and self._beats[i + 1] <= beat:
            i += 1
        return i

    def _at_seconds(self, seconds: float) -> int:
        i = 0
        while i + 1 < len(self._seconds) and self._seconds[i + 1] <= seconds:
            i += 1
        return i

    def tempo_at(self, beat: float) -> float:
        return self.steps[self._at_beat(beat)][1]

    def tempo_at_seconds(self, seconds: float) -> float:
        return self.steps[self._at_seconds(seconds)][1]

    def seconds(self, beat: float) -> float:
        """The second a beat falls on."""
        i = self._at_beat(beat)
        return self._seconds[i] + (beat - self._beats[i]) * 60 / self.steps[i][1]

    def beat(self, seconds: float) -> float:
        """The beat a second of the song falls on."""
        i = self._at_seconds(seconds)
        return self._beats[i] + (seconds - self._seconds[i]) * self.steps[i][1] / 60

    def frame(self, value: "Beat | Fraction", rate: int) -> int:
        """The audio frame of a beat, rounded half up from absolute time."""
        x = value if isinstance(value, Fraction) else beat(value)
        steps = [(b.numerator, b.denominator, bpm) for b, bpm, _ in self.steps]
        return aaw_py.map_frame(steps, x.numerator, x.denominator, rate)


def tempo_map(project) -> TempoMap:
    """The song's tempo map."""
    return TempoMap.of(project)


def meter(session: dict) -> tuple[Fraction, Fraction]:
    """A session's bar and the note it counts, both in quarter-note beats: (3, 1)
    in 3/4, (3, 1/2) in 6/8, (7/2, 1/2) in 7/8. 4/4 when the field is absent."""
    above, _, below = str(session.get("time_signature", "4/4")).partition("/")
    unit = Fraction(4, int(below))
    return unit * int(above), unit


def midi(note: str) -> int:
    return aaw_py.midi(note)


def validate(data) -> dict:
    """The full dump of a song given as data. Raises ValueError if it is refused."""
    return aaw_py.validate(data)


def parse(text: str) -> dict:
    """The full dump of a song given as YAML text."""
    return aaw_py.parse(text)


def load(path: Path, verify_assets=True) -> dict:
    """The full dump of the song in a file, checking its samples unless told not to."""
    return aaw_py.load(Path(path), verify_assets)


def load_snapshot(path: Path) -> dict:
    """The full dump of a render's snapshot as it was written: a record of
    what was rendered, never converted or written back (D190). A render of a
    song of patterns, made before they were removed, is refused (D191)."""
    return aaw_py.parse(Path(path).read_text())


def to_yaml(project) -> str:
    return aaw_py.to_yaml(project)


def save(project, path: Path):
    """Validates the song and writes its canonical YAML."""
    aaw_py.save(project, Path(path))


def fingerprints(project) -> list[str]:
    """project_sha256, then the forms earlier engines wrote in their reports."""
    return aaw_py.fingerprints(project)


def project_hash(project) -> str:
    # The saved form: a field left at its default does not change the fingerprint.
    return fingerprints(project)[0]


def hash_matches(project, sha: str) -> bool:
    """Whether sha fingerprints project, also in the forms earlier engines wrote."""
    return sha in fingerprints(project)


class Trigger(NamedTuple):
    """A scheduled hit. cutoff is the frame a gated or choked voice releases on."""

    start: int
    track: str
    pad: str
    cutoff: int | None


def schedule(project) -> list[Trigger]:
    """Every hit in time order, with exact start frames."""
    return [Trigger(*t) for t in aaw_py.schedule(project)]


def sounded(project) -> list[tuple]:
    """What each track plays, as (track, start beat, end beat, pitch, hit): a
    note's pitch as it sounds, a MIDI number, or None with the pad and transpose
    of a hit that plays its sample as it is. Audio clips are not listed."""
    return aaw_py.sounded(project)


def warnings(project, seconds=None) -> list[dict]:
    """What daw check warns about in a valid song: objects with a code, a
    level, a message, the paths of what each is about and, with a place in
    time, its beat. `seconds` is each sample's file length by sample ID."""
    return json.loads(aaw_py.warnings(project, seconds or {}))


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
