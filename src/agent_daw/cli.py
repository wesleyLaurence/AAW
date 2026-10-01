"""The Python part of `daw`: the sample library, perception and `check`.

Every other command belongs to the Rust `daw` and is passed on to it, so `uv run
daw` and the Rust binary are one command. Results are JSON on stdout; errors go
to stderr as JSON with a nonzero exit status.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

DEFAULT_DB = Path(".daw/library.sqlite")
COMMANDS = ("samples", "listen", "compare", "check")


def engine() -> Path:
    """The Rust `daw`: AAW_DAW, or the release build of this checkout."""
    override = os.environ.get("AAW_DAW")
    checkout = Path(__file__).resolve().parents[2]
    binary = Path(override) if override else checkout / "engine/target/release/daw"
    if not binary.is_file():
        raise RuntimeError(
            f"The Rust daw is not at {binary}; run `cargo build --release` in "
            f"{checkout / 'engine'} or set AAW_DAW"
        )
    return binary


def run_engine(*args):
    """A Rust `daw` command's result, or its error as ValueError."""
    out = subprocess.run(
        [str(engine()), *map(str, args)], capture_output=True, text=True
    )
    if out.returncode:
        try:
            message = json.loads(out.stderr)["error"]
        except (ValueError, KeyError, TypeError):
            message = out.stderr.strip() or f"daw {args[0]} failed"
        raise ValueError(message)
    return json.loads(out.stdout)


def parser():
    from . import library

    p = argparse.ArgumentParser(
        prog="daw",
        description="The sample library, perception and check. Run daw --help for every command.",
    )
    sub = p.add_subparsers(dest="command", required=True)
    samples = sub.add_parser("samples")
    samples.add_argument("--db", type=Path, default=DEFAULT_DB)
    ss = samples.add_subparsers(dest="action", required=True)
    scan = ss.add_parser("scan")
    scan.add_argument("directory", type=Path)
    search = ss.add_parser("search")
    search.add_argument("query", nargs="?", default="")
    search.add_argument("--category")
    search.add_argument("--type", dest="kind", choices=["one-shot", "loop", "unknown"])
    search.add_argument("--key")
    search.add_argument("--bpm", type=int)
    search.add_argument("--limit", type=int, default=20)
    measured = search.add_argument_group(
        "measured filters", "match only samples analyzed with daw samples analyze"
    )
    pitch = measured.add_mutually_exclusive_group()
    pitch.add_argument("--pitched", action="store_true", default=None)
    pitch.add_argument("--unpitched", dest="pitched", action="store_false")
    measured.add_argument("--note-range", help="Measured root range, e.g. C1-B1")
    measured.add_argument("--measured-type", choices=library.MEASURED_KINDS)
    measured.add_argument("--measured-bpm", type=float, help="Within ±1 BPM")
    analyze = ss.add_parser(
        "analyze", help="Measure pitch, onsets, tempo and loop/one-shot from audio"
    )
    target = analyze.add_mutually_exclusive_group(required=True)
    target.add_argument("sample", nargs="?")
    target.add_argument("--all", action="store_true", help="Every indexed sample")
    analyze.add_argument("--refresh", action="store_true", help="Ignore the cache")
    inspect = ss.add_parser("inspect")
    inspect.add_argument("sample")
    aud = ss.add_parser("audition")
    aud.add_argument("sample")
    aud.add_argument("--output", type=Path, required=True)
    aud.add_argument("--seconds", type=float, default=8)
    imp = ss.add_parser("import")
    imp.add_argument("sample")
    imp.add_argument("--project", type=Path, required=True)
    imp.add_argument("--id", required=True)
    imp.add_argument(
        "--root-note",
        help="Note with octave, e.g. C2, or auto to use the measured pitch",
    )
    check = sub.add_parser(
        "check", help="inspect, plus root notes against measured pitch and warnings"
    )
    check.add_argument("project", type=Path)
    listen = sub.add_parser(
        "listen", help="Measure a saved render or WAV; write analysis JSON and images"
    )
    listen.add_argument("source", type=Path)
    listen.add_argument("--no-images", action="store_true")
    compare = sub.add_parser(
        "compare", help="Compare two renders: actual and loudness-matched deltas"
    )
    compare.add_argument("before", type=Path)
    compare.add_argument("after", type=Path)
    compare.add_argument("--no-images", action="store_true")
    return p


def execute(a):
    if a.command in ("listen", "compare"):
        from . import perception

        if a.command == "listen":
            return perception.listen(a.source, images=not a.no_images)
        return perception.compare(a.before, a.after, images=not a.no_images)
    if a.command == "check":
        return check(a.project)
    from . import library
    from .model import midi

    if a.action == "scan":
        return library.scan(a.directory, a.db)
    if a.action == "search":
        if not 1 <= a.limit <= 1000:
            raise ValueError("limit must be 1–1000")
        note_range = None
        if a.note_range:
            low, sep, high = a.note_range.partition("-")
            if not sep:
                raise ValueError("note-range must look like C1-B1")
            note_range = (midi(low), midi(high))
            if note_range[0] > note_range[1]:
                raise ValueError("note-range low note exceeds high note")
        return library.search(
            a.db,
            a.query,
            a.category,
            a.kind,
            a.key,
            a.bpm,
            a.limit,
            pitched=a.pitched,
            note_range=note_range,
            measured_kind=a.measured_type,
            measured_bpm=a.measured_bpm,
        )
    if a.action == "analyze" and a.all:
        return library.analyze_all(a.db, a.refresh)
    source = library.resolve(a.db, a.sample)
    if a.action == "analyze":
        return library.analyze(a.db, source, a.refresh)
    if a.action == "inspect":
        return library.inspect(source)
    if a.action == "audition":
        if not 0 < a.seconds <= 60:
            raise ValueError("seconds must be >0 and <=60")
        return library.audition(source, a.output, a.seconds)
    return import_sample(a, source)


def import_sample(a, source):
    """Copies a sample into the project and adds it to the song."""
    from . import library
    from .model import load, project_hash

    project = load(a.project)
    if a.id in project["samples"]:
        raise ValueError(f"Sample ID already exists: {a.id}")
    root_note, measured = a.root_note, None
    if root_note == "auto":
        measured = library.analyze(a.db, source)["pitch"]
        if not measured["pitched"]:
            raise ValueError(
                "No reliable pitch measured (confidence "
                f"{measured['confidence']}); inspect the sample and pass --root-note"
            )
        root_note = measured["note"]
    asset = library.import_asset(source, a.project.parent, root_note)
    # The song takes the sample as an edit, so a running host sees it and can
    # undo it, and an edit made since the read above is not written over.
    with tempfile.TemporaryDirectory() as scratch:
        patch = Path(scratch) / "patch.json"
        patch.write_text(json.dumps({"samples": {a.id: asset}}))
        label = f"Import sample {a.id}"
        sha = project_hash(project)
        run_engine("apply", a.project, patch, "--expect", sha, "--label", label)
    result = {"sample_id": a.id, **asset}
    if measured:
        result["measured_pitch"] = measured
        if abs(measured["cents"]) > 10:
            result["suggested_pad_transpose"] = round(-measured["cents"] / 100, 2)
    return result


def check(path):
    """`daw inspect`, each root note against its measured pitch, and warnings."""
    from .model import load, warnings

    project = load(path)
    result = run_engine("inspect", path)
    result.update(root_notes(project, path.parent))
    result["warnings"] += warnings(project)
    return result


def root_notes(project, root):
    """Compare each declared root_note with the pitch measured from its asset."""
    from .analysis import compare_root, measure_pitch, PITCH_SECONDS
    import soundfile as sf

    report, warnings = {}, []
    for name, sample in project["samples"].items():
        if not sample["root_note"]:
            continue
        path = root / sample["path"]
        info = sf.info(path)
        x, sr = sf.read(
            path,
            frames=min(info.frames, round((PITCH_SECONDS + 5) * info.samplerate)),
            always_2d=True,
            dtype="float64",
        )
        entry = compare_root(sample["root_note"], measure_pitch(x.mean(axis=1), sr))
        report[name] = entry
        if entry["status"] != "ok":
            warnings.append(
                f"{name}: root_note {entry['declared']} but measured "
                f"{entry['measured']} ({entry['status']})"
            )
    return {"root_notes": report, "warnings": warnings}


def main():
    argv = sys.argv[1:]
    if not argv or argv[0] not in COMMANDS:
        # Not Python's: the Rust daw takes this process's place.
        try:
            binary = str(engine())
            os.execv(binary, [binary, *argv])
        except (RuntimeError, OSError) as exc:
            command = argv[0] if argv else None
            print(json.dumps({"error": str(exc), "command": command}), file=sys.stderr)
            return 1
    args = parser().parse_args(argv)
    try:
        result = execute(args)
        print(json.dumps(result, indent=2, allow_nan=False))
    except (ValueError, OSError, RuntimeError, KeyError, TypeError) as exc:
        print(json.dumps({"error": str(exc), "command": args.command}), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
