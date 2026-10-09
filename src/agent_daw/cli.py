"""The Python part of `daw`: the sample library, references, perception, `check`, `timeline`, `joins` and `export`.

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

COMMANDS = ("samples", "reference", "listen", "compare", "check", "timeline", "joins", "export")


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
    from . import descriptors, library

    p = argparse.ArgumentParser(
        prog="daw",
        description="The sample library, references, perception, check, timeline, joins and export. Run daw --help for every command.",
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
    search.add_argument(
        "--category",
        help="The category in a sample's name, or the one a sample named for none measures as",
    )
    search.add_argument("--type", dest="kind", choices=["one-shot", "loop", "unknown"])
    search.add_argument("--key")
    search.add_argument("--bpm", type=int)
    search.add_argument("--limit", type=int, default=20)
    search.add_argument(
        "--sort",
        choices=["name", *library.SORTS],
        default="name",
        help="By name, or by length or a measurement with the most first: the "
        "longest, brightest (centroid), most low end (low), slowest attack, longest "
        "decay, noisiest, loudest, hardest hit (punch)",
    )
    search.add_argument("--reverse", action="store_true", help="Turn the order around")
    measured = search.add_argument_group(
        "measured filters", "match only samples analyzed with daw samples analyze"
    )
    pitch = measured.add_mutually_exclusive_group()
    pitch.add_argument("--pitched", action="store_true", default=None)
    pitch.add_argument("--unpitched", dest="pitched", action="store_false")
    measured.add_argument("--note-range", help="Measured root range, e.g. C1-B1")
    measured.add_argument("--measured-type", choices=library.MEASURED_KINDS)
    measured.add_argument("--measured-bpm", type=float, help="Within ±1 BPM")
    measured.add_argument(
        "--measure",
        type=int,
        default=0,
        metavar="N",
        help="First measure N of the matches that are not measured, and keep what was measured",
    )
    sound = search.add_argument_group(
        "sound",
        "what a measured sample is like (daw describe samples). A word keeps the samples "
        "it is true of among their category: "
        + "; ".join(
            " ".join(f"--{word}" for word in names) + f" ({field})"
            for field, names in descriptors.WORDS.items()
        )
        + ". A number keeps those from --min-FIELD to --max-FIELD, FIELD being duration, "
        + ", ".join(
            short if short == field else f"{short} ({field})"
            for short, field in library.SORTS.items()
            if short != "duration"
        )
        + ": --max-decay 300",
    )
    for word in library.WORDS:
        sound.add_argument(
            f"--{word}", dest="words", action="append_const", const=word, help=argparse.SUPPRESS
        )
    for short in library.SORTS:
        for end in ("min", "max"):
            sound.add_argument(
                f"--{end}-{short}", type=float, metavar="X", help=argparse.SUPPRESS
            )
    like = ss.add_parser(
        "like",
        help="The measured samples nearest a chosen one in sound (daw describe samples)",
    )
    like.add_argument("sample", help="A sample's ID or a file, one in a project too")
    like.add_argument("--limit", type=int, default=10)
    like.add_argument(
        "--category",
        help="Compare with this category's samples, or any for every sample; its own unless given",
    )
    like.add_argument("--folder", type=Path, action="append", help="Compare within this folder; repeat for several")
    like.add_argument(
        "--measure",
        type=int,
        default=0,
        metavar="N",
        help="First measure N of the samples to compare with that are not measured",
    )
    analyze = ss.add_parser(
        "analyze",
        help="Measure pitch, onsets, tempo, loop/one-shot and what the sound is like "
        "from audio (daw describe samples)",
    )
    target = analyze.add_mutually_exclusive_group(required=True)
    target.add_argument("sample", nargs="?")
    target.add_argument(
        "--all",
        action="store_true",
        help="Every indexed sample not yet measured, several at a time; says how far "
        "it is on stderr, and a run that is stopped goes on from there",
    )
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
        "--meter",
        help="The song's time signature, such as 3/4; its beats make a bar (4/4 unless given)",
    )
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
        "check",
        help="inspect, plus root notes against measured pitch and warnings, "
        "each with a code (daw describe check)",
    )
    check.add_argument("project", type=song_file)
    listen = sub.add_parser(
        "listen", help="Measure a saved render or WAV; write analysis JSON and images"
    )
    listen.add_argument("source", type=Path)
    listen.add_argument("--no-images", action="store_true")
    listen.add_argument(
        "--section",
        metavar="ID",
        help="One section of the mix and of each stem, with its third octaves, "
        "resonances, hits and what each compressor and limiter took off, in place of "
        "the report (daw describe listen)",
    )
    listen.add_argument(
        "--overlap",
        nargs=2,
        metavar=("A", "B"),
        help="Two of a render's stems, by band and by section, in place of the report "
        "(daw describe listen)",
    )
    listen.add_argument(
        "--write-translation",
        action="store_true",
        help="Write mono.wav and small-speaker.wav beside the analysis: the mix as "
        "translation measures it, for a person to hear (daw describe listen)",
    )
    compare = sub.add_parser(
        "compare",
        help="Compare two renders: actual and loudness-matched deltas; or one "
        "render with a reference, section by section (daw describe reference)",
    )
    compare.add_argument("before", type=Path, help="A render, or with --reference the mix")
    compare.add_argument("after", type=Path, nargs="?")
    compare.add_argument(
        "--reference", metavar="NAME", help="A song saved with daw reference add, in place of AFTER"
    )
    compare.add_argument("--no-images", action="store_true")
    reference = sub.add_parser(
        "reference",
        help="Songs kept as what good sounds like, for daw compare --reference",
    )
    rs = reference.add_subparsers(dest="action", required=True)
    add = rs.add_parser(
        "add", help="Analyze a song, .m4a and .mp3 too, and keep its measurements under NAME"
    )
    add.add_argument("file", type=Path)
    add.add_argument("--name", help="Letters, digits and -; the file's name unless given")
    add.add_argument("--bpm", type=float, help="The song's tempo, to settle half or double time")
    add.add_argument("--downbeat", help="A time whose nearest beat is a downbeat")
    add.add_argument("--meter", help="The song's time signature, such as 3/4; 4/4 unless given")
    add.add_argument("--replace", action="store_true", help="Write over a reference of this name")
    rs.add_parser("list", help="The saved references")
    show = rs.add_parser("show", help="A reference's measurements, whole and by section")
    show.add_argument("name")
    named = rs.add_parser(
        "sections",
        help="A reference's sections; with JSON, names them by hand",
    )
    named.add_argument("name")
    named.add_argument(
        "sections",
        nargs="?",
        help='Such as \'[{"id": "verse", "at": 32}, {"id": "drop", "seconds": "1:02"}]\': '
        "at in the reference's beats from its first downbeat, or seconds; each runs "
        "to the next",
    )
    named.add_argument(
        "--measured", action="store_true", help="Go back to the sections it measured"
    )
    remove = rs.add_parser("remove", help="Forget a reference; the song's file is left")
    remove.add_argument("name")
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
            if a.overlap and a.section:
                raise ValueError("listen takes --overlap A B or --section ID, not both")
            if a.write_translation and (a.overlap or a.section):
                raise ValueError(
                    "--write-translation goes with the whole report, not --overlap or --section"
                )
            if a.overlap:
                return perception.overlap_of(a.source, *a.overlap, images=not a.no_images)
            if a.section:
                return perception.section_of(a.source, a.section, images=not a.no_images)
            return perception.printed(
                perception.listen(a.source, not a.no_images, a.write_translation)
            )
        if (a.after is None) == (a.reference is None):
            raise ValueError(
                "compare takes two renders, or one render and --reference NAME"
            )
        if a.reference is not None:
            from . import reference

            return reference.compare(a.before, a.reference)
        return perception.compare(a.before, a.after, images=not a.no_images)
    if a.command == "reference":
        return reference_command(a)
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
        if a.measure < 0:
            raise ValueError("measure must be 0 or more")
        notes = []
        found = library.search(
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
            sort=a.sort,
            reverse=a.reverse,
            words=a.words or (),
            ranges={
                field: (getattr(a, f"min_{short}"), getattr(a, f"max_{short}"))
                for short, field in library.SORTS.items()
            },
            measure=a.measure,
            notes=notes,
        )
        for note in notes:
            print(json.dumps({"note": note}), file=sys.stderr)
        return found
    if a.action == "like":
        if not 1 <= a.limit <= 1000 or a.measure < 0:
            raise ValueError("limit must be 1–1000, and measure 0 or more")
        source = library.resolve(a.db, a.sample)
        return library.like(a.db, source, a.limit, a.category, a.folder, a.measure)
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


def reference_command(a):
    """`daw reference`: add, list, show, sections and remove."""
    from . import beats, reference

    if a.action == "add":
        if a.bpm is not None and not beats.MIN_BPM <= a.bpm <= beats.MAX_BPM:
            raise ValueError(f"bpm must be {beats.MIN_BPM:g}–{beats.MAX_BPM:g}")
        downbeat = None if a.downbeat is None else beats.seconds(a.downbeat)
        return reference.add(a.file, a.name, a.bpm, downbeat, a.meter, a.replace)
    if a.action == "list":
        return reference.listing()
    if a.action == "show":
        return reference.shown(reference.load(a.name))
    if a.action == "remove":
        return reference.remove(a.name)
    if a.sections is not None and a.measured:
        raise ValueError("sections takes JSON or --measured, not both")
    try:
        spec = None if a.sections is None else json.loads(a.sections)
    except ValueError:
        raise ValueError(
            'Sections are a JSON list such as [{"id": "verse", "at": 32}]'
        ) from None
    return reference.sections(a.name, spec, a.measured)


def beat_map(a, source):
    """A song's beat map: its summary, and the beats asked for."""
    from . import beats, library

    if a.bpm is not None and not beats.MIN_BPM <= a.bpm <= beats.MAX_BPM:
        raise ValueError(f"bpm must be {beats.MIN_BPM:g}–{beats.MAX_BPM:g}")
    if a.window <= 0 or not 0 < a.seconds <= 60:
        raise ValueError("window must be >0, and seconds >0 and <=60")
    downbeat = None if a.downbeat is None else beats.seconds(a.downbeat)
    at = None if a.near is None else beats.seconds(a.near)
    if a.meter is not None:
        beats.bar_of(a.meter)
    report = library.beat_map(a.db, source, a.bpm, downbeat, a.refresh, a.meter)
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
    checked = root_notes(project, path.parent)
    seconds = checked.pop("seconds")
    result.update(checked)
    result["warnings"] += warnings(project, seconds)
    return result


def warning(code, message, path):
    """A warning as `daw check` gives them; daw describe check lists the codes."""
    return {"code": code, "level": "warning", "message": message, "paths": [path]}


def root_notes(project, root):
    """Compare each declared root_note with the pitch measured from its asset.

    A sample the engine cannot read is a warning here and not a note to compare.
    Also gives each readable sample's length in seconds, for the checks of
    audio clips that play to the end of their file.
    """
    from .analysis import compare_root, measure_pitch, PITCH_SECONDS
    from .library import probe
    import soundfile as sf

    report, warnings, seconds = {}, [], {}
    for name, sample in project["samples"].items():
        path = root / sample["path"]
        try:
            info = probe(path)
        except ValueError as e:
            warnings.append(
                warning("sample-unreadable", f"{name}: {e}; a render will fail", f"samples.{name}")
            )
            continue
        seconds[name] = info.frames / info.samplerate
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
                warning(
                    "root-note-mismatch",
                    f"{name}: root_note {entry['declared']} but measured "
                    f"{entry['measured']} ({entry['status']})",
                    f"samples.{name}",
                )
            )
    return {"root_notes": report, "warnings": warnings, "seconds": seconds}


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
