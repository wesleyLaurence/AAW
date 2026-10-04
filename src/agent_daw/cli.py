"""The Python part of `daw`: the sample library, perception, `check`, `timeline`, `joins` and `export`.

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

COMMANDS = ("samples", "listen", "compare", "check", "timeline", "joins", "export")


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


def song_file(path) -> Path:
    """The song file a PROJECT names: the file itself, or song.yaml in a folder.

    A path that is not there is a folder unless it is written as a YAML file, as
    the Rust `daw` takes it.
    """
    path = Path(path)
    written_as_file = path.suffix.lower() in (".yaml", ".yml")
    if path.is_dir() or not (written_as_file or path.exists()):
        return path / "song.yaml"
    return path


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
        description="The sample library, perception, check, timeline, joins and export. Run daw --help for every command.",
    )
    sub = p.add_subparsers(dest="command", required=True)
    samples = sub.add_parser("samples")
    samples.add_argument("--db", type=Path, default=library.default_db())
    ss = samples.add_subparsers(dest="action", required=True)
    scan = ss.add_parser("scan")
    scan.add_argument("directory", type=Path)
    folders = ss.add_parser("folders", help="List, add, refresh or remove shared sample folders")
    folders.add_argument("operation", choices=["list", "add", "remove", "refresh"], nargs="?", default="list")
    folders.add_argument("directory", type=Path, nargs="?")
    search = ss.add_parser("search")
    search.add_argument("query", nargs="?", default="")
    search.add_argument("--folder", type=Path, action="append", help="Search within this folder; repeat for multiple sources")
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
    beats = ss.add_parser(
        "beats",
        help="Beat and downbeat map of a whole song: tempo, every beat, bars and phrases",
    )
    beats.add_argument("sample")
    beats.add_argument("--near", help="List the beats around a time, as seconds or m:ss")
    beats.add_argument(
        "--window", type=float, default=3.0, help="Seconds either side of --near"
    )
    beats.add_argument("--all", action="store_true", help="List every beat")
    beats.add_argument(
        "--bpm", type=float, help="The song's tempo, to settle half or double time"
    )
    beats.add_argument("--downbeat", help="A time whose nearest beat is a downbeat")
    beats.add_argument(
        "--refresh", action="store_true", help="Measure again, without corrections"
    )
    beats.add_argument(
        "--click", type=Path, help="Write part of the song with a click on each beat"
    )
    beats.add_argument(
        "--seconds", type=float, default=20, help="Length of the --click audition"
    )
    inspect = ss.add_parser("inspect")
    inspect.add_argument("sample")
    aud = ss.add_parser("audition")
    aud.add_argument("sample")
    aud.add_argument("--output", type=Path, required=True)
    aud.add_argument("--seconds", type=float, default=8)
    imp = ss.add_parser(
        "import",
        help="Copy a file into the project and add it to the song; .m4a and .mp3 are decoded to WAV",
    )
    imp.add_argument("sample")
    imp.add_argument("--project", type=song_file, required=True)
    imp.add_argument("--id", help="The sample's ID in the song")
    imp.add_argument(
        "--copy-only",
        action="store_true",
        help="Copy the file into the project and print its entry; the song is not changed",
    )
    imp.add_argument(
        "--root-note",
        help="Note with octave, e.g. C2, or auto to use the measured pitch",
    )
    check = sub.add_parser(
        "check", help="inspect, plus root notes against measured pitch and warnings"
    )
    check.add_argument("project", type=song_file)
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
    timeline = sub.add_parser(
        "timeline",
        help="The song in beats and seconds: where its sounds are, conversions, and its length",
    )
    timeline.add_argument("project", type=song_file)
    timeline.add_argument("--seconds", nargs="+", default=[], help="Times, as seconds or m:ss, to give in beats")
    timeline.add_argument("--beats", nargs="+", default=[], help="Beats to give in seconds")
    timeline.add_argument("--end-at", help="A beat for --pad to end on; gives the beat it starts on")
    timeline.add_argument("--pad", metavar="TRACK.PAD")
    timeline.add_argument(
        "--fit", action="store_true", help="Set the session's length to where its sound ends"
    )
    timeline.add_argument("--tail", type=float, default=0, help="Beats to leave after it, with --fit")
    timeline.add_argument(
        "--tempo-for", type=float, metavar="SECONDS", help="The tempo at which the song is this long"
    )
    joins = sub.add_parser(
        "joins",
        help="Check a render's joins between parts of a song, and its length",
    )
    joins.add_argument("source", type=Path, help="A render: its folder, report or pointer")
    joins.add_argument("--limit", type=float, help="The longest the file may be, seconds")
    joins.add_argument(
        "--seconds", type=float, default=4, help="Length of each join's excerpt"
    )
    joins.add_argument("--no-excerpts", action="store_true")
    export = sub.add_parser(
        "export",
        help="Write the song's render as a named WAV, AAC or MP3 file, at a stated level",
    )
    export.add_argument("project", type=song_file)
    export.add_argument("--to", type=Path, required=True, help="NAME.wav, .m4a or .mp3")
    export.add_argument("--bits", type=int, choices=[16, 24], default=24, help="For .wav")
    export.add_argument("--bitrate", type=int, help="kb/s: 256 for .m4a, 320 for .mp3")
    level = export.add_argument_group(
        "level", "one gain for the whole file, held under --ceiling; as rendered by default"
    ).add_mutually_exclusive_group()
    level.add_argument("--gain", type=float, help="A gain in dB")
    level.add_argument("--peak", type=float, help="Put the true peak at this dBTP")
    level.add_argument("--lufs", type=float, help="Reach this integrated loudness")
    level.add_argument("--match", metavar="SAMPLE", help="Be as loud as this sample of the song")
    export.add_argument(
        "--ceiling", type=float, default=-0.1, help="The highest sample peak a gain may reach, dBFS"
    )
    export.add_argument("--render", type=Path, help="A render to export instead of the latest")
    export.add_argument("--replace", action="store_true", help="Write over an existing file")
    return p


def execute(a):
    if a.command in ("listen", "compare"):
        from . import perception

        if a.command == "listen":
            return perception.listen(a.source, images=not a.no_images)
        return perception.compare(a.before, a.after, images=not a.no_images)
    if a.command == "check":
        return check(a.project)
    if a.command == "timeline":
        from . import timeline

        return timeline.report(
            a.project, a.seconds, a.beats, a.end_at, a.pad, a.fit, a.tail, a.tempo_for
        )
    if a.command == "joins":
        from . import joins

        if not 0 < a.seconds <= 30 or (a.limit is not None and a.limit <= 0):
            raise ValueError("seconds must be >0 and <=30, and limit >0")
        return joins.check(a.source, a.limit, a.seconds, not a.no_excerpts)
    if a.command == "export":
        from .export import export

        if a.bitrate is not None and not 64 <= a.bitrate <= 320:
            raise ValueError("bitrate must be 64–320 kb/s")
        return export(
            a.project,
            a.to,
            bits=a.bits,
            bitrate=a.bitrate,
            gain=a.gain,
            peak=a.peak,
            lufs=a.lufs,
            match=a.match,
            ceiling=a.ceiling,
            render=a.render,
            replace=a.replace,
        )
    from . import library
    from .model import midi

    if a.action == "folders":
        if a.operation == "list":
            return library.folders(a.db)
        if a.operation == "refresh":
            return [library.scan(Path(f["path"]), a.db) if f["available"] else
                    {"root": f["path"], "error": "Folder unavailable"} for f in library.folders(a.db)]
        if a.directory is None:
            raise ValueError("folders add/remove requires a directory")
        if a.operation == "add":
            return library.scan(a.directory, a.db)
        return library.remove_folder(a.db, a.directory)
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
            roots=a.folder,
        )
    if a.action == "analyze" and a.all:
        return library.analyze_all(a.db, a.refresh)
    source = library.resolve(a.db, a.sample)
    if a.action == "analyze":
        return library.analyze(a.db, source, a.refresh)
    if a.action == "beats":
        return beat_map(a, source)
    if a.action == "inspect":
        return library.inspect(source)
    if a.action == "audition":
        if not 0 < a.seconds <= 60:
            raise ValueError("seconds must be >0 and <=60")
        return library.audition(source, a.output, a.seconds)
    return import_sample(a, source)


def beat_map(a, source):
    """A song's beat map: its summary, and the beats asked for."""
    from . import beats, library

    if a.bpm is not None and not beats.MIN_BPM <= a.bpm <= beats.MAX_BPM:
        raise ValueError(f"bpm must be {beats.MIN_BPM:g}–{beats.MAX_BPM:g}")
    if a.window <= 0 or not 0 < a.seconds <= 60:
        raise ValueError("window must be >0, and seconds >0 and <=60")
    downbeat = None if a.downbeat is None else beats.seconds(a.downbeat)
    at = None if a.near is None else beats.seconds(a.near)
    report = library.beat_map(a.db, source, a.bpm, downbeat, a.refresh)
    rows = report.pop("beats")
    if a.all:
        report["beats"] = rows
    if at is not None:
        report["near"] = beats.near(rows, at, a.window)
    if a.click:
        # Around the time asked about, or else from just before the first downbeat.
        start = report["first_downbeat_seconds"] - 0.5 if at is None else at - a.seconds / 2
        report["click"] = beats.click(source, rows, a.click, start, a.seconds)
    return report


def import_sample(a, source):
    """Copies a sample into the project and adds it to the song."""
    from . import library
    from .model import load, midi, project_hash

    if a.copy_only:
        # What adds the copy to the song, such as the app, names it then.
        if a.root_note not in (None, "auto"):
            midi(a.root_note)
        project = None
    elif not a.id:
        raise ValueError("--id is required unless --copy-only")
    else:
        project = load(a.project)
        if a.id in project["samples"]:
            raise ValueError(f"Sample ID already exists: {a.id}")
    root_note, measured, asset = a.root_note, None, None
    if root_note == "auto":
        audio = source
        if source.suffix.lower() in library.COMPRESSED:
            # Pitch is measured from the decoded copy.
            asset = library.import_asset(source, a.project.parent)
            audio = a.project.parent / asset["path"]
        measured = library.analyze(a.db, audio)["pitch"]
        if not measured["pitched"]:
            raise ValueError(
                "No reliable pitch measured (confidence "
                f"{measured['confidence']}); inspect the sample and pass --root-note"
            )
        root_note = measured["note"]
    asset = asset or library.import_asset(source, a.project.parent)
    asset["root_note"] = root_note
    if a.copy_only:
        result = dict(asset)
    else:
        # The song takes the sample as an edit, so a running host sees it and can
        # undo it, and an edit made since the read above is not written over.
        with tempfile.TemporaryDirectory() as scratch:
            patch = Path(scratch) / "patch.json"
            patch.write_text(json.dumps({"samples": {a.id: asset}}))
            label = f"Import sample {a.id}"
            sha = project_hash(project)
            run_engine("apply", a.project, patch, "--expect", sha, "--label", label)
        result = {"sample_id": a.id, **asset}
    if "source_sha256" in asset:
        info = library.probe(a.project.parent / asset["path"])
        result["decoded"] = {
            "sample_rate": info.samplerate,
            "channels": info.channels,
            "duration": info.duration,
        }
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
    """Compare each declared root_note with the pitch measured from its asset.

    A sample the engine cannot read is a warning here and not a note to compare.
    """
    from .analysis import compare_root, measure_pitch, PITCH_SECONDS
    from .library import probe
    import soundfile as sf

    report, warnings = {}, []
    for name, sample in project["samples"].items():
        path = root / sample["path"]
        try:
            info = probe(path)
        except ValueError as e:
            warnings.append(f"{name}: {e}; a render will fail")
            continue
        if not sample["root_note"]:
            continue
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
        print(json.dumps(result, indent=2, allow_nan=False), flush=True)
    except BrokenPipeError:
        # A reader that stopped, as `head` does: end quietly, with the status a
        # shell gives a closed pipe, and nothing left to flush at exit.
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        return 141
    except (ValueError, OSError, RuntimeError, KeyError, TypeError) as exc:
        print(json.dumps({"error": str(exc), "command": args.command}), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
