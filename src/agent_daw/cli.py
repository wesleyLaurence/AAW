"""JSON-first commands: errors go to stderr with a nonzero exit status."""

import argparse
import json
import sys
import yaml
from pathlib import Path
from . import library
from .model import (
    Project,
    Session,
    load,
    save,
    project_hash,
    frame,
    midi,
    Pad,
    Event,
    Sample,
    Filter,
    Eq,
    Compressor,
    Limiter,
)
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
    desc = sub.add_parser("describe")
    desc.add_argument(
        "topic",
        nargs="?",
        choices=["project", "sampler", "effects"],
        default="project",
    )
    return p


EFFECT_SEMANTICS = {
    "order": "Effects run top to bottom. Track inserts come before track gain and pan. master.effects follow master_gain_db and precede the end fade.",
    "filter": "Butterworth highpass or lowpass. slope_db_per_octave 12/24/36/48 is the order times 6 dB. No resonance control.",
    "eq": "RBJ biquad bands in series. bell uses q as bandwidth; shelves use q as shelf slope (0.71 is maximally flat). gain_db at freq_hz.",
    "compressor": "Stereo-linked sample-peak detector, soft knee of knee_db centered on threshold_db. attack_ms smooths onset; release_ms is the time for reduction to fall by a factor of e. makeup_db is static.",
    "sidechain": "compressor.sidechain names another track. Its key is that track after its own inserts, before its gain, pan, mute and solo, so a muted kick still ducks the bass. Cycles and self-sidechains are rejected. Master compressors cannot use a sidechain.",
    "limiter": "Look-ahead brickwall on sample peaks: no output sample exceeds ceiling_db. lookahead_ms is compensated latency. Estimated true peak can still exceed the ceiling slightly; leave margin below 0 dBFS.",
    "bypass": "bypass: true keeps an effect in the document without processing, for A/B renders with daw compare.",
    "stems": "Stems are post-insert, post-track and post-master gain and fade, before master effects. Without master effects they sum to the mix; report.stems_sum_to_mix says which.",
    "previews": "render --track renders that track plus its sidechain sources and omits master effects, matching its stem. Section previews include the master chain.",
    "report": "Render reports list each effect with latency_frames; dynamics add max and mean gain reduction and the fraction of frames reduced over 1 dB.",
    "limits": "No reverb, delay, saturation, sends, groups or automation yet. Effect parameters are static for the whole render.",
}


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
    if a.command in ("listen", "compare"):
        from . import perception

        if a.command == "listen":
            return perception.listen(a.source, images=not a.no_images)
        return perception.compare(a.before, a.after, images=not a.no_images)
    if a.command == "init":
        path = a.directory / "song.yaml"
        if path.exists():
            raise ValueError(f"Project already exists: {path}")
        if a.bars <= 0:
            raise ValueError("bars must be positive")
        project = Project(session=Session(tempo=a.tempo, length_beats=a.bars * 4))
        save(project, path)
        return {"project": str(path.resolve())}
    if a.command == "describe" and a.topic == "effects":
        return {
            "schema": {
                m.model_fields["type"].annotation.__args__[0]: m.model_json_schema()
                for m in (Filter, Eq, Compressor, Limiter)
            },
            "semantics": EFFECT_SEMANTICS,
        }
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
                "pitch": "sample.root_note includes octave, e.g. C2. event.note is target pitch. Repitch changes length. No pitch-preserving stretch. daw samples analyze measures pitch; import --root-note auto uses it; check warns when a declared root disagrees with the audio.",
                "gate": "Gate mode requires event.duration. Voice releases at note-off; it never sustains beyond sample length.",
                "choke": "Pads sharing a choke_group within a track release on the next hit in that group.",
                "mix": "gain_db is dB. pan is -1 left to +1 right. Mono pads use equal-power pan. Stereo pads/tracks use balance. mute wins over solo.",
                "render": "Finite session length, tails truncated with end fade. PCM24 stereo mix and aligned FLOAT stems. Clipping fails export.",
                "editing": "Use inspect SHA with apply --expect. JSON merge patch: objects merge, arrays replace, null deletes. CLI writers acquire a project lock.",
                "effects": "tracks[].effects and master.effects are serial insert chains; see daw describe effects.",
                "limits": "No reverb, delay, sends, groups, synths, time stretching, automation, recording or realtime playback.",
            },
        }
    if a.command == "samples":
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
        if a.action == "import":
            project = load(a.project)
            if a.id in project.samples:
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
            project.samples[a.id] = asset
            project = Project.model_validate(project.model_dump())
            save(project, a.project)
            result = {"sample_id": a.id, **asset.model_dump()}
            if measured:
                result["measured_pitch"] = measured
                if abs(measured["cents"]) > 10:
                    result["suggested_pad_transpose"] = round(
                        -measured["cents"] / 100, 2
                    )
            return result
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
    result = {
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
                "effects": [e.type for e in t.effects],
                "sidechain": t.sidechains(),
            }
            for t in project.tracks
        ],
        "master_effects": [e.type for e in project.master.effects],
        "sections": [s.model_dump() for s in project.sections],
    }
    if a.command == "check":
        result.update(root_notes(project, a.project.parent))
    return result


def root_notes(project, root):
    """Compare each declared root_note with the pitch measured from its asset."""
    from .analysis import compare_root, measure_pitch, PITCH_SECONDS
    import soundfile as sf

    report, warnings = {}, []
    for name, sample in project.samples.items():
        if not sample.root_note:
            continue
        path = root / sample.path
        info = sf.info(path)
        x, sr = sf.read(
            path,
            frames=min(info.frames, round((PITCH_SECONDS + 5) * info.samplerate)),
            always_2d=True,
            dtype="float64",
        )
        entry = compare_root(sample.root_note, measure_pitch(x.mean(axis=1), sr))
        report[name] = entry
        if entry["status"] != "ok":
            warnings.append(
                f"{name}: root_note {entry['declared']} but measured "
                f"{entry['measured']} ({entry['status']})"
            )
    return {"root_notes": report, "warnings": warnings}


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
