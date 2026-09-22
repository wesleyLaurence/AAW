"""Rebuildable SQLite sample index. Filename metadata is explicitly a hint."""

from pathlib import Path
import hashlib
import re
import shutil
import sqlite3
import json
import numpy as np
import soundfile as sf
from . import analysis
from .model import Sample, digest

EXTENSIONS = {".wav", ".aif", ".aiff", ".flac"}


def connect(db: Path):
    db.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(db)
    con.row_factory = sqlite3.Row
    con.execute("""CREATE TABLE IF NOT EXISTS samples (
      id TEXT PRIMARY KEY, path TEXT UNIQUE, name TEXT, pack TEXT,
      duration REAL, sample_rate INTEGER, channels INTEGER, frames INTEGER,
      category TEXT, kind TEXT, bpm_hint INTEGER, key_hint TEXT,
      bytes INTEGER, mtime_ns INTEGER, search_text TEXT)""")
    # Measured values, valid only while the file's size, mtime and analyzer match.
    con.execute("""CREATE TABLE IF NOT EXISTS analysis (
      id TEXT PRIMARY KEY, bytes INTEGER, mtime_ns INTEGER, analyzer TEXT,
      pitched INTEGER, midi REAL, note TEXT, cents REAL, confidence REAL,
      kind TEXT, bpm REAL, report TEXT)""")
    return con


def hints(path: Path):
    text = str(path).lower()
    name = path.stem.lower()
    category = "other"
    for cat, tokens in [
        ("808", ["808"]),
        ("kick", ["kick"]),
        ("snare", ["snare"]),
        ("clap", ["clap"]),
        ("hat", ["hihat", "hi_hat", "hi-hat"]),
        ("bass", ["bass"]),
        ("piano", ["piano"]),
        ("bell", ["bell"]),
        ("brass", ["brass"]),
        ("vocal", ["vocal", "chant"]),
        ("percussion", ["perc"]),
        ("fx", ["fx", "impact", "riser"]),
    ]:
        if any(t in name for t in tokens):
            category = cat
            break
    kind = (
        "one-shot"
        if any(t in text for t in ["one_shot", "oneshot", "one-shot", "drum_hits"])
        else ("loop" if "loop" in text else "unknown")
    )
    bpms = re.findall(r"(?:^|[_\s-])(\d{2,3})(?=[_\s-]|bpm|$)", name)
    bpm = (
        next((int(v) for v in bpms if 60 <= int(v) <= 200), None)
        if kind == "loop"
        else None
    )
    key = re.search(r"(?:^|_)([A-G](?:#|b)?)(min|maj|m)?$", path.stem)
    return category, kind, bpm, "".join(key.groups(default="")) if key else None


def scan(root: Path, db: Path):
    root = root.expanduser().resolve()
    if not root.is_dir():
        raise ValueError(f"Sample directory does not exist: {root}")
    con = connect(db)
    count, skipped, errors, seen = 0, 0, [], set()
    try:
        for p in sorted(root.rglob("*")):
            if p.suffix.lower() not in EXTENSIONS or not p.is_file():
                continue
            p = p.resolve()
            path = str(p)
            seen.add(path)
            stat = p.stat()
            old = con.execute(
                "SELECT bytes,mtime_ns FROM samples WHERE path=?", (path,)
            ).fetchone()
            if (
                old
                and old["bytes"] == stat.st_size
                and old["mtime_ns"] == stat.st_mtime_ns
            ):
                skipped += 1
                continue
            try:
                info = sf.info(p)
                cat, kind, bpm, key = hints(p)
                pack = p.relative_to(root).parts[0]
                sid = hashlib.sha256(path.encode()).hexdigest()[:16]
                con.execute(
                    "INSERT OR REPLACE INTO samples VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                    (
                        sid,
                        path,
                        p.name,
                        pack,
                        info.duration,
                        info.samplerate,
                        info.channels,
                        info.frames,
                        cat,
                        kind,
                        bpm,
                        key,
                        stat.st_size,
                        stat.st_mtime_ns,
                        path.lower(),
                    ),
                )
                count += 1
            except (RuntimeError, ValueError) as e:
                errors.append({"path": path, "error": str(e)})
        for row in con.execute("SELECT path FROM samples").fetchall():
            p = Path(row["path"])
            if p.is_relative_to(root) and row["path"] not in seen:
                con.execute("DELETE FROM samples WHERE path=?", (row["path"],))
        con.execute("DELETE FROM analysis WHERE id NOT IN (SELECT id FROM samples)")
        con.commit()
        return {
            "root": str(root),
            "indexed": count,
            "unchanged": skipped,
            "errors": errors,
            "database": str(db),
        }
    finally:
        con.close()


MEASURED_KINDS = ["one_shot", "loop", "uncertain"]


def search(
    db: Path,
    query="",
    category=None,
    kind=None,
    key=None,
    bpm=None,
    limit=20,
    pitched=None,
    note_range=None,
    measured_kind=None,
    measured_bpm=None,
):
    """Filename search. Measured filters match only samples with a current analysis."""
    con = connect(db)
    clauses, args = [], [analysis.ANALYZER]
    for token in query.lower().split():
        clauses.append("s.search_text LIKE ? ESCAPE '\\'")
        args.append(
            "%"
            + token.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
            + "%"
        )
    for col, value in [
        ("s.category", category),
        ("s.kind", kind),
        ("s.key_hint", key),
        ("s.bpm_hint", bpm),
        ("a.kind", measured_kind),
    ]:
        if value is not None:
            clauses.append(f"{col} = ?")
            args.append(value)
    if pitched is not None:
        clauses.append("a.pitched = ?")
        args.append(int(pitched))
    if note_range is not None:
        clauses.append("a.pitched = 1 AND ROUND(a.midi) BETWEEN ? AND ?")
        args.extend(note_range)
    if measured_bpm is not None:
        clauses.append("ABS(a.bpm - ?) <= 1")
        args.append(measured_bpm)
    sql = """SELECT s.id,s.name,s.pack,s.duration,s.channels,s.category,s.kind,
      s.bpm_hint,s.key_hint,s.path,a.id AS measured_id,a.pitched,a.note,a.cents,
      a.confidence,a.kind AS measured_kind,a.bpm AS measured_bpm
      FROM samples s LEFT JOIN analysis a ON a.id = s.id AND a.bytes = s.bytes
      AND a.mtime_ns = s.mtime_ns AND a.analyzer = ?"""
    if clauses:
        sql += " WHERE " + " AND ".join(clauses)
    sql += " ORDER BY s.name,s.path LIMIT ?"
    args.append(limit)
    try:
        rows = []
        for r in con.execute(sql, args):
            row = dict(r)
            measured = {
                k: row.pop(k)
                for k in [
                    "pitched",
                    "note",
                    "cents",
                    "confidence",
                    "measured_kind",
                    "measured_bpm",
                ]
            }
            if row.pop("measured_id") is None:
                row["measured"] = None
            else:
                measured["pitched"] = bool(measured["pitched"])
                measured["kind"] = measured.pop("measured_kind")
                measured["bpm"] = measured.pop("measured_bpm")
                row["measured"] = measured
            rows.append(row)
        return rows
    finally:
        con.close()


def analyze(db: Path, path: Path, refresh=False):
    """Measure one file, reusing the index cache when the file is indexed and unchanged."""
    path = path.resolve()
    con = connect(db)
    try:
        row = con.execute(
            "SELECT id,bytes,mtime_ns,bpm_hint FROM samples WHERE path=?", (str(path),)
        ).fetchone()
        stat = path.stat()
        if row and not refresh:
            cached = con.execute(
                "SELECT report FROM analysis WHERE id=? AND bytes=? AND mtime_ns=? AND analyzer=?",
                (row["id"], stat.st_size, stat.st_mtime_ns, analysis.ANALYZER),
            ).fetchone()
            if cached:
                return {**json.loads(cached["report"]), "cached": True, "indexed": True}
        hint = row["bpm_hint"] if row else hints(path)[2]
        report = analysis.analyze(path, hint)
        if row and row["bytes"] == stat.st_size and row["mtime_ns"] == stat.st_mtime_ns:
            pitch, rhythm = report["pitch"], report["rhythm"]
            con.execute(
                "INSERT OR REPLACE INTO analysis VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
                (
                    row["id"],
                    stat.st_size,
                    stat.st_mtime_ns,
                    analysis.ANALYZER,
                    int(pitch["pitched"]),
                    pitch["midi"],
                    pitch["note"],
                    pitch["cents"],
                    pitch["confidence"],
                    rhythm["kind"],
                    rhythm["tempo"]["bpm"] if rhythm["tempo"] else None,
                    json.dumps(report),
                ),
            )
            con.commit()
        return {**report, "cached": False, "indexed": bool(row)}
    finally:
        con.close()


def analyze_all(db: Path, refresh=False):
    """Analyze every indexed sample whose cached analysis is missing or stale."""
    con = connect(db)
    try:
        rows = con.execute(
            """SELECT s.path FROM samples s LEFT JOIN analysis a ON a.id = s.id
            AND a.bytes = s.bytes AND a.mtime_ns = s.mtime_ns AND a.analyzer = ?
            WHERE ? OR a.id IS NULL ORDER BY s.path""",
            (analysis.ANALYZER, int(refresh)),
        ).fetchall()
        total = con.execute("SELECT COUNT(*) FROM samples").fetchone()[0]
    finally:
        con.close()
    errors = []
    for row in rows:
        try:
            analyze(db, Path(row["path"]), refresh=True)
        except (RuntimeError, ValueError, OSError) as e:
            errors.append({"path": row["path"], "error": str(e)})
    return {
        "analyzed": len(rows) - len(errors),
        "unchanged": total - len(rows),
        "errors": errors,
        "analyzer": analysis.ANALYZER,
        "database": str(db),
    }


def resolve(db: Path, value: str) -> Path:
    path = Path(value).expanduser()
    if path.is_file():
        return path.resolve()
    con = connect(db)
    try:
        row = con.execute("SELECT path FROM samples WHERE id=?", (value,)).fetchone()
    finally:
        con.close()
    if not row:
        raise ValueError(f"Unknown sample ID or file: {value}")
    return Path(row["path"])


def inspect(path: Path):
    x, sr = sf.read(path, always_2d=True, dtype="float64")
    if not len(x) or not np.isfinite(x).all():
        raise ValueError(f"Empty or nonfinite audio: {path}")
    peak = float(np.max(np.abs(x)))
    rms = float(np.sqrt(np.mean(x * x)))
    mono = x.mean(axis=1)
    n = min(len(mono), sr * 4)
    spectrum = abs(np.fft.rfft(mono[:n] * np.hanning(n))) ** 2
    freqs = np.fft.rfftfreq(n, 1 / sr)
    energy = max(float(spectrum.sum()), 1e-30)
    bands = {
        f"{lo}-{hi}Hz": round(
            float(spectrum[(freqs >= lo) & (freqs < hi)].sum()) / energy, 4
        )
        for lo, hi in [(20, 60), (60, 120), (120, 300), (300, 2000), (2000, 10000)]
    }
    cat, kind, bpm, key = hints(path)
    return {
        "path": str(path),
        "duration": len(x) / sr,
        "sample_rate": sr,
        "channels": x.shape[1],
        "peak_dbfs": 20 * np.log10(max(peak, 1e-12)),
        "rms_dbfs": 20 * np.log10(max(rms, 1e-12)),
        "dc_offset": float(x.mean()),
        "band_energy_fraction": bands,
        "filename_hints": {"category": cat, "kind": kind, "bpm": bpm, "key": key},
        "sha256": digest(path),
    }


def import_asset(source: Path, project_dir: Path, root_note=None):
    checksum = digest(source)
    target = project_dir / "samples" / f"{checksum[:12]}_{source.name}"
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.exists():
        shutil.copy2(source, target)
    if digest(target) != checksum:
        raise ValueError(f"Imported asset mismatch: {target}")
    return Sample(
        path=str(target.relative_to(project_dir)),
        sha256=checksum,
        source=str(source.resolve()),
        root_note=root_note,
    )


def audition(path: Path, output: Path, seconds=8.0):
    info = sf.info(path)
    x, sr = sf.read(path, frames=round(seconds * info.samplerate), always_2d=True)
    if not len(x):
        raise ValueError("Empty audio")
    peak = float(np.max(abs(x)))
    if peak:
        x *= min(1, 0.7 / peak)
    fade = min(round(sr * 0.015), len(x))
    x[-fade:] *= np.linspace(1, 0, fade)[:, None]
    output.parent.mkdir(parents=True, exist_ok=True)
    sf.write(output, x, sr, subtype="PCM_24")
    return {"audio": str(output.resolve()), "duration": len(x) / sr}
