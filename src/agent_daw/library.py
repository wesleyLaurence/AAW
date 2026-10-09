"""Rebuildable SQLite sample index. Filename metadata is explicitly a hint."""

from pathlib import Path
from contextlib import closing
import hashlib
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import json
from concurrent.futures import ProcessPoolExecutor, as_completed
import numpy as np
import soundfile as sf
from . import analysis, descriptors
from .model import digest

EXTENSIONS = {".wav", ".aif", ".aiff", ".flac"}
# Compressed files an import decodes into the project; the engine reads PCM only.
COMPRESSED = {".m4a", ".mp3"}
# The programs that decode them, in the order tried, each with its arguments
# before the source and before the target. Each writes 32-bit float WAV at the
# file's own rate and channels, which keeps peaks above full scale, and leaves out
# the encoder's padding before the audio. afconvert, which macOS has, also leaves
# out the padding after AAC; ffmpeg keeps that, about 15 ms.
DECODERS = {
    "afconvert": ("-f WAVE -d LEF32", ""),
    "ffmpeg": (
        "-v error -nostdin -y -i",
        "-map 0:a:0 -map_metadata -1 -fflags +bitexact -flags:a +bitexact"
        " -c:a pcm_f32le -f wav",
    ),
}


def default_db() -> Path:
    """One library for the app and CLI, independent of the open project."""
    if override := os.environ.get("AAW_LIBRARY"):
        return Path(override).expanduser()
    data = Path(os.environ.get("AAW_DATA_DIR", str(Path.home() / "Library/Application Support/AAW")))
    return data / "library.sqlite"


def folders(db: Path):
    with closing(connect(db)) as con, con:
        return [{"path": row[0], "available": Path(row[0]).is_dir()}
                for row in con.execute("SELECT path FROM folders ORDER BY path")]


def remove_folder(db: Path, root: Path):
    """Forget a source without touching any original or imported audio."""
    root = str(root.expanduser().resolve())
    with closing(connect(db)) as con, con:
        con.execute("DELETE FROM folders WHERE path=?", (root,))
        remaining = [Path(r[0]) for r in con.execute("SELECT path FROM folders")]
        for row in con.execute("SELECT path FROM samples").fetchall():
            path = Path(row[0])
            if path.is_relative_to(root) and not any(path.is_relative_to(r) for r in remaining):
                con.execute("DELETE FROM samples WHERE path=?", (str(path),))
        con.execute("DELETE FROM analysis WHERE id NOT IN (SELECT id FROM samples)")
    return folders(db)


def connect(db: Path):
    db.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(db)
    con.row_factory = sqlite3.Row
    con.execute("CREATE TABLE IF NOT EXISTS folders (path TEXT PRIMARY KEY)")
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
            if p.suffix.lower() not in EXTENSIONS | COMPRESSED or not p.is_file():
                continue
            # Keep a file symlink under its registered source for scope/removal.
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
                if p.suffix.lower() in COMPRESSED:
                    with tempfile.TemporaryDirectory(prefix="aaw-index-") as temp:
                        decoded = Path(temp) / "audio.wav"
                        decode(p, decoded)
                        info = sf.info(decoded)
                else:
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
        con.execute("INSERT OR IGNORE INTO folders VALUES (?)", (str(root),))
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
# What a search sorts by beside the name, most first: a sample's length, or
# one of its measurements under a short name.
SORTS = {"duration": "duration", **{f.rsplit("_", 1)[0]: f for f in descriptors.FIELDS}}
WORDS = {word: field for field, names in descriptors.WORDS.items() for word in names}

ROWS = """SELECT s.id,s.name,s.pack,s.duration,s.channels,s.category,s.kind,
  s.bpm_hint,s.key_hint,s.path,a.id AS measured_id,a.pitched,a.note,a.cents,
  a.confidence,a.kind AS measured_kind,a.bpm AS measured_bpm,
  json_extract(a.report, '$.sound') AS sound,
  json_extract(a.report, '$.measured_category') AS measured_category
  FROM samples s LEFT JOIN analysis a ON a.id = s.id AND a.bytes = s.bytes
  AND a.mtime_ns = s.mtime_ns AND a.analyzer = ?"""
# A sample named for no category is of the one it measures as.
OF_CATEGORY = (
    "(s.category = ? OR (s.category = 'other' AND "
    "json_extract(a.report, '$.measured_category') = ?))"
)


def listed(r) -> dict:
    """A sample as a search lists it: what its name says, and under `measured`
    what its audio does, or null for a sample that was not analyzed."""
    row = dict(r)
    measured = {
        "pitched": bool(row.pop("pitched")),
        "note": row.pop("note"),
        "cents": row.pop("cents"),
        "confidence": row.pop("confidence"),
        "kind": row.pop("measured_kind"),
        "bpm": row.pop("measured_bpm"),
        "category": row.pop("measured_category"),
    }
    sound = row.pop("sound")
    if row.pop("measured_id") is None:
        row["measured"] = None
        return row
    sound = json.loads(sound) if sound else dict.fromkeys(descriptors.FIELDS)
    among = row["category"] if row["category"] != "other" else measured["category"]
    row["measured"] = {
        **measured,
        **sound,
        "words": descriptors.words(sound, among),
        "words_among": among or "any",
    }
    return row


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
    roots=None,
    sort="name",
    reverse=False,
    words=(),
    ranges=None,
    measure=0,
    notes=None,
):
    """Filename search. Measured filters match only samples with a current analysis.

    `sort` is `name` or one of SORTS, which lists the most first; `reverse`
    turns either around. `words` keeps the samples each word is true of, and
    `ranges` those whose field lies between a least and a most, either of
    which may be None. A sort or filter by a measurement leaves out what was
    not measured, says how many in `notes`, and refuses when that is all of
    them; `measure` measures that many of them first.
    """
    clauses, args = [], [analysis.ANALYZER]
    if roots:
        # Prefixes include a separator: /drums must never also match /drums-old.
        scopes = []
        for root in roots:
            prefix = str(Path(root).expanduser().resolve()).rstrip(os.sep) + os.sep
            scopes.append("substr(s.path, 1, ?) = ?")
            args.extend([len(prefix), prefix])
        clauses.append("(" + " OR ".join(scopes) + ")")
    for token in query.lower().split():
        clauses.append("s.search_text LIKE ? ESCAPE '\\'")
        args.append(
            "%"
            + token.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
            + "%"
        )
    if category is not None:
        clauses.append(OF_CATEGORY)
        args.extend([category, category])
    for col, value in [
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
    sql = ROWS + (" WHERE " + " AND ".join(clauses) if clauses else "")
    sql += " ORDER BY s.name,s.path"
    field = SORTS.get(sort)
    ranges = {f: r for f, r in (ranges or {}).items() if r != (None, None)}
    by_sound = bool(words) or field in descriptors.FIELDS or any(f != "duration" for f in ranges)
    if not (by_sound or ranges or field or measure):
        # By name alone, the index does all of it.
        con = connect(db)
        try:
            if reverse:
                sql = sql.replace("s.name,s.path", "s.name DESC,s.path DESC")
            return [listed(r) for r in con.execute(sql + " LIMIT ?", [*args, limit])]
        finally:
            con.close()

    def found():
        con = connect(db)
        try:
            return [listed(r) for r in con.execute(sql, args)]
        finally:
            con.close()

    rows = found()
    waiting = [r["path"] for r in rows if r["measured"] is None]
    if measure and waiting:
        measure_files(db, waiting[:measure], "search")
        rows = found()
        waiting = [r["path"] for r in rows if r["measured"] is None]
    if by_sound:
        if waiting and len(waiting) == len(rows):
            raise ValueError(
                f"None of the {len(rows)} matching samples is measured, and this search "
                "sorts or filters by a measurement: run `daw samples analyze --all` once "
                "for the library, or add --measure N to measure the first N of them here"
            )
        if waiting and notes is not None:
            notes.append(
                f"{len(waiting)} of the {len(rows)} matching samples are not measured and "
                "were left out: `daw samples analyze --all` measures the library, and "
                "--measure N the first N of these"
            )
        rows = [r for r in rows if r["measured"] is not None]

    def value(row, name):
        return row["duration"] if name == "duration" else row["measured"][name]

    for name, (least, most) in ranges.items():
        rows = [
            r
            for r in rows
            if value(r, name) is not None
            and (least is None or value(r, name) >= least)
            and (most is None or value(r, name) <= most)
        ]
    for word in words:
        rows = [r for r in rows if r["measured"]["words"].get(WORDS[word]) == word]
    if field:
        rows = [r for r in rows if value(r, field) is not None]
        # The rows are by name, which a sort that keeps their order leaves ties in.
        rows.sort(key=lambda r: value(r, field), reverse=not reverse)
    elif reverse:
        rows.reverse()
    return rows[:limit]


def like(db: Path, path: Path, limit=10, category=None, roots=None, measure=0):
    """The measured samples nearest a chosen one in sound, nearest first.

    They are those of its category: the one given, the one its name says, or
    the one it measures as, and every sample where it has none. A loop is
    held against loops and a single sound against single sounds. Each has
    its `distance` in steps (descriptors.STEPS).
    """
    report = analyze(db, path)
    con = connect(db)
    try:
        own = con.execute(
            "SELECT category FROM samples WHERE path=?", (str(path.resolve()),)
        ).fetchone()
    finally:
        con.close()
    named = own["category"] if own else hints(path)[0]
    among = category or (named if named != "other" else report["measured_category"])
    if among == "any":
        among = None

    if all(value is None for value in report["sound"].values()):
        raise ValueError(f"{path.name} is silent, so nothing is like it")

    def found():
        rows = search(db, category=among, roots=roots, limit=10**9)
        return [r for r in rows if Path(r["path"]) != path.resolve()]

    rows = found()
    waiting = [r["path"] for r in rows if r["measured"] is None]
    if measure and waiting:
        measure_files(db, waiting[:measure], "like")
        rows = found()
        waiting = [r["path"] for r in rows if r["measured"] is None]
    loop = report["rhythm"]["kind"] == "loop"
    near = []
    for row in rows:
        if row["measured"] is None or (row["measured"]["kind"] == "loop") != loop:
            continue
        distance = descriptors.distance(report["sound"], row["measured"])
        if distance is not None:
            near.append({**row, "distance": round(distance, 2)})
    if not near and waiting:
        raise ValueError(
            f"None of the {len(waiting)} samples to compare with is measured: run "
            "`daw samples analyze --all` once for the library, or add --measure N to "
            "measure the first N of them here"
        )
    near.sort(key=lambda r: r["distance"])
    return {
        "sample": {
            "path": str(path.resolve()),
            "category": named,
            "measured": {
                "kind": report["rhythm"]["kind"],
                "category": report["measured_category"],
                **report["sound"],
                "words": descriptors.words(report["sound"], among),
                "words_among": among or "any",
            },
        },
        "among": among or "any",
        "compared": len(near),
        "not_measured": len(waiting),
        "like": near[:limit],
    }


def keep(con, sample, size, mtime_ns, report):
    """Saves the measurements of the file of that size and time beside its
    index entry."""
    pitch, rhythm = report["pitch"], report["rhythm"]
    con.execute(
        "INSERT OR REPLACE INTO analysis VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            sample,
            size,
            mtime_ns,
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
            keep(con, row["id"], stat.st_size, stat.st_mtime_ns, report)
        return {**report, "cached": False, "indexed": bool(row)}
    finally:
        con.close()


def measured(job):
    """One indexed file's measurements, or why it has none, in a worker."""
    path, hint = job
    try:
        stat = Path(path).stat()
        return path, (stat.st_size, stat.st_mtime_ns), analysis.analyze(Path(path), hint), None
    except (RuntimeError, ValueError, OSError) as e:
        return path, None, None, str(e)


# Fewer files than this are measured one after another: a worker takes
# longer to start than they take to measure.
TOGETHER = 16


def measure_files(db: Path, paths, said="analyze"):
    """Measures indexed files, several at a time, keeping each as it is done,
    so a run that is stopped keeps what it had measured and the next one goes
    on from there. Says how far it is on stderr every two seconds. Returns
    the files that could not be measured."""
    con = connect(db)
    errors, done, last = [], 0, time.monotonic()
    try:
        rows = {}
        for first in range(0, len(paths), 500):
            some = paths[first : first + 500]
            marks = ",".join("?" * len(some))
            for row in con.execute(
                f"SELECT id,path,bytes,mtime_ns,bpm_hint FROM samples WHERE path IN ({marks})", some
            ):
                rows[row["path"]] = row
        jobs = [(path, rows[path]["bpm_hint"]) for path in paths if path in rows]
        workers = min(8, os.cpu_count() or 1)
        pool = ProcessPoolExecutor(workers) if len(jobs) >= TOGETHER and workers > 1 else None
        try:
            if pool:
                results = (f.result() for f in as_completed([pool.submit(measured, j) for j in jobs]))
            else:
                results = map(measured, jobs)
            for path, stat, report, error in results:
                row = rows[path]
                if error:
                    errors.append({"path": path, "error": error})
                elif (row["bytes"], row["mtime_ns"]) == stat:
                    keep(con, row["id"], *stat, report)
                done += 1
                if time.monotonic() - last >= 2 and done < len(jobs):
                    last = time.monotonic()
                    print(f"{said}: {done} of {len(jobs)}", file=sys.stderr, flush=True)
        finally:
            if pool:
                pool.shutdown(cancel_futures=True)
    finally:
        con.close()
    return sorted(errors, key=lambda e: e["path"])


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
    started = time.monotonic()
    errors = measure_files(db, [row["path"] for row in rows])
    return {
        "analyzed": len(rows) - len(errors),
        "unchanged": total - len(rows),
        "errors": errors,
        "seconds": round(time.monotonic() - started, 1),
        "analyzer": analysis.ANALYZER,
        "database": str(db),
    }


def beat_map(db: Path, path: Path, bpm=None, downbeat=None, refresh=False, meter=None):
    """A file's beat map, measured once and kept beside it as NAME.beats.json.

    The saved map is used while the audio and the measuring code are unchanged.
    `bpm`, `downbeat` and `meter` correct it and are kept with it, so later calls
    give the corrected map; `refresh` measures again without them. Nothing is
    written beside a file in the library index, whose map is measured on each call.
    """
    from . import beats
    from .model import atomic_text

    path = path.resolve()
    sidecar = path.with_name(f"{path.stem}.beats.json")
    sha, asked = digest(path), {"bpm": None, "downbeat": None, "meter": None}
    if sidecar.is_file() and not refresh:
        saved = json.loads(sidecar.read_text())
        if saved.get("sha256") == sha and saved.get("analyzer") == beats.ANALYZER:
            if bpm is None and downbeat is None and meter is None:
                return {"path": str(path), **saved, "map": str(sidecar), "cached": True}
            asked = {**asked, **saved["requested"]}
    asked = {
        "bpm": asked["bpm"] if bpm is None else bpm,
        "downbeat": asked["downbeat"] if downbeat is None else downbeat,
        "meter": asked["meter"] if meter is None else meter,
    }
    report = beats.measure(path, asked["bpm"], asked["downbeat"], asked["meter"])
    report = {**report, "sha256": sha, "requested": asked}
    con = connect(db)
    try:
        row = con.execute("SELECT 1 FROM samples WHERE path=?", (str(path),))
        indexed = row.fetchone()
    finally:
        con.close()
    if indexed:
        return {**report, "map": None, "cached": False}
    # The map goes where its audio goes, so it does not name where that is.
    kept = {k: v for k, v in report.items() if k != "path"}
    atomic_text(sidecar, json.dumps(kept, indent=1) + "\n")
    return {**report, "map": str(sidecar), "cached": False}


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


def probe(path: Path):
    """The format of a file the engine can play: mono or stereo, read by libsndfile."""
    try:
        info = sf.info(path)
    except RuntimeError as e:
        raise ValueError(f"Cannot read {path.name} as audio: {e}") from None
    if info.channels not in (1, 2) or not info.frames:
        raise ValueError(
            f"{path.name} has {info.channels} channels and {info.frames} frames; "
            "samples are mono or stereo audio"
        )
    return info


def decode(source: Path, target: Path, decoder=None):
    """Writes a compressed file as WAV at `target` and returns the decoder that did."""
    names = [decoder] if decoder else [n for n in DECODERS if shutil.which(n)]
    if not names:
        raise ValueError(
            f"Decoding {source.name} needs afconvert, which macOS has, or ffmpeg on the PATH"
        )
    fd, temp = tempfile.mkstemp(dir=target.parent, prefix=".", suffix=".wav")
    os.close(fd)
    failures = []
    try:
        for name in names:
            before, after = (part.split() for part in DECODERS[name])
            paths = str(source.resolve()), os.path.abspath(temp)
            command = [name, *before, paths[0], *after, paths[1]]
            out = subprocess.run(command, capture_output=True, text=True)
            if out.returncode == 0:
                probe(Path(temp))
                os.replace(temp, target)
                return name
            said = (out.stderr or out.stdout).strip().splitlines()
            failures.append(f"{name}: {said[-1] if said else 'failed'}")
        raise ValueError(
            f"Could not decode {source.name} ({'; '.join(failures)}). A copy-protected "
            "file, such as an Apple Music download, cannot be read"
        )
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def import_asset(source: Path, project_dir: Path, root_note=None):
    """Puts a file in the project's samples and returns its entry for the song.

    A compressed file is decoded there once, as WAV named by the original's hash,
    and the entry records that hash as `source_sha256`. Any other file is copied
    as it is. A file the engine cannot read is refused.
    """
    checksum = digest(source)
    compressed = source.suffix.lower() in COMPRESSED
    name = f"{source.stem}.wav" if compressed else source.name
    target = project_dir / "samples" / f"{checksum[:12]}_{name}"
    asset = {
        "path": str(target.relative_to(project_dir)),
        "sha256": checksum,
        "source": str(source.resolve()),
        "root_note": root_note,
    }
    if compressed:
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists():
            decode(source, target)
        return {**asset, "sha256": digest(target), "source_sha256": checksum}
    probe(source)
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.exists():
        shutil.copy2(source, target)
    if digest(target) != checksum:
        raise ValueError(f"Imported asset mismatch: {target}")
    return asset


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
