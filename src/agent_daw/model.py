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
