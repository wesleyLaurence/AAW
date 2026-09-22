"""JSON-first commands: errors go to stderr with a nonzero exit status."""

import argparse
import json
import sys
import yaml
from pathlib import Path
from . import library
from .model import Project, Session, load, save, project_hash, frame, Pad, Event, Sample
from .engine import render, schedule

DEFAULT_DB = Path(".daw/library.sqlite")


def parser():
    p = argparse.ArgumentParser(
        prog="daw",
        description="Offline sample workstation. Run daw describe for the authoring contract.",
    )
    sub = p.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init")
    init.add_argument("directory", type=Path)
    init.add_argument("--tempo", type=float, default=144)
    init.add_argument("--bars", type=int, default=16)
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
    imp.add_argument("--root-note")
    for name in ["check", "fmt", "inspect"]:
        s = sub.add_parser(name)
        s.add_argument("project", type=Path)
    apply = sub.add_parser(
        "apply",
        help="Validate and atomically replace project JSON fields from a JSON merge patch",
    )
    apply.add_argument("project", type=Path)
    apply.add_argument("patch", type=Path)
    apply.add_argument(
        "--expect",
        required=True,
        help="Project SHA256 from inspect; rejects stale edits",
    )
    ren = sub.add_parser("render")
    ren.add_argument("project", type=Path)
    ren.add_argument("--output", type=Path)
    ren.add_argument("--track")
    ren.add_argument("--section")
    desc = sub.add_parser("describe")
    desc.add_argument(
        "topic", nargs="?", choices=["project", "sampler"], default="project"
    )
    return p


def merge(target, patch):
    if not isinstance(patch, dict):
        return patch
    result = dict(target) if isinstance(target, dict) else {}
    for k, v in patch.items():
        if v is None:
            result.pop(k, None)
        else:
            result[k] = merge(result.get(k), v)
    return result


def execute(a):
    if a.command == "init":
        path = a.directory / "song.yaml"
        if path.exists():
            raise ValueError(f"Project already exists: {path}")
        if a.bars <= 0:
            raise ValueError("bars must be positive")
        project = Project(session=Session(tempo=a.tempo, length_beats=a.bars * 4))
        save(project, path)
        return {"project": str(path.resolve())}
    if a.command == "describe":
        return {
            "schema": Project.model_json_schema()
            if a.topic == "project"
            else {
                "pad": Pad.model_json_schema(),
                "event": Event.model_json_schema(),
                "sample": Sample.model_json_schema(),
            },
            "semantics": {
                "time": "All at/duration/length_beats fields are quarter-note beats. at is zero-based. Use fraction strings for triplets. 4/4 only.",
                "steps": "x = velocity 100, digits 1–9 = scaled velocities, dot = rest. Whitespace and | ignored. grid is beats per cell; 1/4 = sixteenth note.",
                "swing": "0.5 straight, 0.75 maximum; delays odd step cells. Explicit events are unswung.",
                "pitch": "sample.root_note includes octave, e.g. C2. event.note is target pitch. Repitch changes length. No pitch-preserving stretch.",
                "gate": "Gate mode requires event.duration. Voice releases at note-off; it never sustains beyond sample length.",
                "choke": "Pads sharing a choke_group within a track release on the next hit in that group.",
                "mix": "gain_db is dB. pan is -1 left to +1 right. Mono pads use equal-power pan. Stereo pads/tracks use balance. mute wins over solo.",
                "render": "Finite session length, tails truncated with end fade. PCM24 stereo mix and aligned FLOAT stems. Clipping fails export.",
                "editing": "Use inspect SHA with apply --expect. JSON merge patch: objects merge, arrays replace, null deletes. CLI writers acquire a project lock.",
                "limits": "No effects, synths, time stretching, automation, recording or realtime playback.",
            },
        }
    if a.command == "samples":
        if a.action == "scan":
            return library.scan(a.directory, a.db)
        if a.action == "search":
            if not 1 <= a.limit <= 1000:
                raise ValueError("limit must be 1–1000")
            return library.search(
                a.db, a.query, a.category, a.kind, a.key, a.bpm, a.limit
            )
        source = library.resolve(a.db, a.sample)
        if a.action == "inspect":
            return library.inspect(source)
        if a.action == "audition":
            if not 0 < a.seconds <= 60:
                raise ValueError("seconds must be >0 and <=60")
            return library.audition(source, a.output, a.seconds)
        if a.action == "import":
            project = load(a.project)
            if a.id in project.samples:
                raise ValueError(f"Sample ID already exists: {a.id}")
            asset = library.import_asset(source, a.project.parent, a.root_note)
            project.samples[a.id] = asset
            project = Project.model_validate(project.model_dump())
            save(project, a.project)
            return {"sample_id": a.id, **asset.model_dump()}
    project = load(a.project)
    if a.command == "fmt":
        save(project, a.project)
        return {"project": str(a.project), "formatted": True}
    if a.command == "apply":
        if project_hash(project) != a.expect:
            raise ValueError("Stale project revision; inspect and retry")
        updated = Project.model_validate(
            merge(project.model_dump(mode="json"), json.loads(a.patch.read_text()))
        )
        # Verify referenced audio before replacing the authoritative document.
        from .model import digest

        for name, asset in updated.samples.items():
            path = a.project.parent / asset.path
            if not path.is_file():
                raise ValueError(f"Missing asset {name}")
            if asset.sha256 and digest(path) != asset.sha256:
                raise ValueError(f"Changed asset {name}")
        save(updated, a.project)
        return {"project_sha256": project_hash(updated)}
    if a.command == "render":
        return render(a.project, a.output, track_id=a.track, section=a.section)
    triggers = schedule(project)
    return {
        "valid": True,
        "project_sha256": project_hash(project),
        "session": project.session.model_dump(),
        "duration_seconds": frame(
            project.session.length_beats,
            project.session.tempo,
            project.session.sample_rate,
        )
        / project.session.sample_rate,
        "samples": len(project.samples),
        "patterns": len(project.patterns),
        "tracks": [
            {
                "id": t.id,
                "events": sum(e.track == t.id for e in triggers),
                "gain_db": t.gain_db,
                "mute": t.mute,
                "solo": t.solo,
            }
            for t in project.tracks
        ],
        "sections": [s.model_dump() for s in project.sections],
    }


def main():
    args = parser().parse_args()
    try:
        # Advisory lock serializes CLI writers. Raw external edits are outside this contract.
        import contextlib
        import fcntl

        project = getattr(args, "project", None)
        mutation = args.command in ["apply", "fmt"] or (
            args.command == "samples" and args.action == "import"
        )
        with contextlib.ExitStack() as stack:
            if project and mutation:
                f = stack.enter_context((project.parent / ".daw.lock").open("a"))
                fcntl.flock(f, fcntl.LOCK_EX)
            result = execute(args)
        print(json.dumps(result, indent=2, allow_nan=False))
    except (
        ValueError,
        OSError,
        RuntimeError,
        KeyError,
        TypeError,
        yaml.YAMLError,
    ) as exc:
        print(json.dumps({"error": str(exc), "command": args.command}), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
