"""Runs a device of the person's own: the offline tier of the device spec.

The host calls this with the device's folder, the input it rendered, where
to write the output and a params file: {"sample_rate", "tempo", "frames",
"fields": {name: value}, "lanes": {name: path}, "notes": [[start, end,
pitch, velocity], ...]}, a lane's path a raw file of one little-endian
float32 a frame. An effect's entry (device.py) defines process(audio,
sample_rate, fields) and returns the output as an array; an instrument's
(kind: instrument in its manifest) defines play(notes, frames, sample_rate,
fields), given the track's notes and no input. A MIDI effect's (kind: midi)
defines transform(notes, fields, tempo), given notes in beats, and the
runner writes the notes it returns to the output as JSON, {"notes":
[[start, end, pitch, velocity], ...]}. With --check the device runs once on
a short noise, an instrument or a MIDI effect on the notes given, instead
and the reply says what came back. Errors go to stderr as {"error": ...}
with status 1.
"""

import argparse
import importlib.util
import json
import sys
import traceback
from pathlib import Path

import numpy as np


def fail(message):
    print(json.dumps({"error": message}), file=sys.stderr)
    sys.exit(1)


NOTE = np.dtype([("start", np.int64), ("end", np.int64), ("pitch", np.int64), ("velocity", np.int64)])


def notes_of(params):
    """The notes as an instrument receives them: a structured array of
    start, end, pitch and velocity, a row a note."""
    rows = [tuple(int(x) for x in n) for n in params.get("notes", [])]
    return np.array(rows, dtype=NOTE)


BEATS = np.dtype([("start", np.float64), ("end", np.float64), ("pitch", np.int64), ("velocity", np.int64)])


def beat_notes_of(params):
    """The notes as a MIDI effect receives them: a structured array of
    start and end in beats, pitch and velocity, a row a note."""
    rows = [(float(s), float(e), int(p), int(v)) for s, e, p, v in params.get("notes", [])]
    return np.array(rows, dtype=BEATS)


def transformed(function, notes, fields, tempo):
    """What a MIDI effect's transform returned, as rows of [start, end,
    pitch, velocity]: from a structured array with those fields, or from
    rows of four numbers."""
    try:
        out = function(notes, fields, tempo)
    except Exception:  # noqa: BLE001 - whatever the device raised is the report
        fail(f"transform raised:\n{traceback.format_exc(limit=4)}")
    if out is None:
        fail("transform returned None; it returns the notes to play, an empty list for none")
    if isinstance(out, np.ndarray) and out.dtype.names:
        missing = [f for f in ("start", "end", "pitch") if f not in out.dtype.names]
        if missing:
            fail(f"transform returned an array without {', '.join(missing)}")
        velocity = out["velocity"] if "velocity" in out.dtype.names else np.full(len(out), 100)
        rows = list(zip(out["start"], out["end"], out["pitch"], velocity))
    else:
        try:
            rows = [tuple(r) for r in out]
        except TypeError:
            fail(f"transform returned {type(out).__name__}, not notes")
    notes_out = []
    for k, row in enumerate(rows):
        if len(row) != 4:
            fail(f"note {k} is {len(row)} values; a note is (start, end, pitch, velocity)")
        try:
            start, end, pitch, velocity = (float(x) for x in row)
        except (TypeError, ValueError):
            fail(f"note {k} is not four numbers: {row!r}")
        notes_out.append([start, end, pitch, velocity])
    return notes_out


def load(folder, entry, function="process", signature="process(audio, sample_rate, fields)"):
    """The device's function, from its file, with its folder importable."""
    file = folder / entry
    if not file.is_file():
        fail(f"{file} is not there")
    spec = importlib.util.spec_from_file_location(f"aaw_device_{folder.name.replace('-', '_')}", file)
    module = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(folder))
    try:
        spec.loader.exec_module(module)
    except Exception:  # noqa: BLE001 - whatever the device raised is the report
        fail(f"{file.name} did not load:\n{traceback.format_exc(limit=4)}")
    found = getattr(module, function, None)
    if not callable(found):
        fail(f"{file.name} defines no {signature}")
    return found


def fields_of(params, frames):
    """The fields as the device receives them: values, and a lane's values
    a frame as a (frames, 1) array."""
    fields = dict(params.get("fields", {}))
    for name, path in params.get("lanes", {}).items():
        values = np.fromfile(path, dtype="<f4").astype(np.float64)
        if len(values) != frames:
            fail(f"the lane on {name} has {len(values)} values for {frames} frames")
        fields[name] = values.reshape(-1, 1)
    return fields


def run(function, *args, name="process"):
    try:
        out = function(*args)
    except Exception:  # noqa: BLE001
        fail(f"{name} raised:\n{traceback.format_exc(limit=4)}")
    try:
        out = np.asarray(out, dtype=np.float64)
    except Exception:  # noqa: BLE001
        fail(f"{name} returned {type(out).__name__}, not an array of audio")
    if out.ndim == 1:
        out = out.reshape(-1, 1)
    if out.ndim != 2 or out.shape[1] not in (1, 2) or out.shape[0] == 0:
        fail(f"{name} returned an array of shape {out.shape}; (frames, channels) with one or two channels is expected")
    if not np.all(np.isfinite(out)):
        fail(f"{name} returned samples that are not numbers")
    return out


def main(argv=None):
    parser = argparse.ArgumentParser(prog="device_runner")
    parser.add_argument("--device", required=True, type=Path)
    parser.add_argument("--params", required=True, type=Path)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check", action="store_true")
    a = parser.parse_args(argv)
    params = json.loads(a.params.read_text())
    manifest = a.device / "device.yaml"
    entry, kind = "device.py", "effect"
    # The entry and the kind as the manifest names them, read without a YAML
    # library: a line each, `entry: NAME` and `kind: KIND`.
    if manifest.is_file():
        for line in manifest.read_text().splitlines():
            if line.startswith("entry:"):
                entry = line.split(":", 1)[1].strip().strip("'\"") or entry
            if line.startswith("kind:"):
                kind = line.split(":", 1)[1].split("#")[0].strip().strip("'\"") or kind
    if kind == "midi":
        transform = load(a.device, entry, "transform", "transform(notes, fields, tempo)")
        notes = beat_notes_of(params)
        out = transformed(transform, notes, dict(params.get("fields", {})), float(params.get("tempo", 120.0)))
        if a.check:
            pitches = [int(n[2]) for n in out]
            print(json.dumps({"ok": True, "kind": "midi", "notes_in": int(len(notes)), "notes_out": len(out),
                              "pitches": [min(pitches), max(pitches)] if pitches else [],
                              "first": out[:8]}))
            return
        a.output.write_text(json.dumps({"notes": out}))
        print(json.dumps({"ok": True, "notes_in": int(len(notes)), "notes_out": len(out)}))
        return
    sample_rate = int(params["sample_rate"])
    if kind == "instrument":
        play = load(a.device, entry, "play", "play(notes, frames, sample_rate, fields)")
        frames = int(params["frames"])
        notes = notes_of(params)
        out = run(play, notes, frames, sample_rate, fields_of(params, frames), name="play")
        if a.check:
            print(json.dumps({"ok": True, "notes": int(len(notes)), "frames_in": frames, "frames_out": int(out.shape[0]),
                              "channels": int(out.shape[1]), "peak": float(np.max(np.abs(out)))}))
            return
        import soundfile as sf

        sf.write(a.output, out.astype(np.float32), sample_rate, subtype="FLOAT")
        print(json.dumps({"ok": True, "notes": int(len(notes)), "frames_out": int(out.shape[0]), "channels": int(out.shape[1])}))
        return
    process = load(a.device, entry)
    if a.check:
        frames = int(params.get("frames", sample_rate // 2))
        rng = np.random.default_rng(1)
        audio = rng.uniform(-0.25, 0.25, size=(frames, 2))
        out = run(process, audio, sample_rate, fields_of(params, frames))
        print(json.dumps({"ok": True, "frames_in": frames, "frames_out": int(out.shape[0]), "channels": int(out.shape[1]),
                          "peak": float(np.max(np.abs(out)))}))
        return
    import soundfile as sf

    audio, rate = sf.read(a.input, always_2d=True, dtype="float64")
    if rate != sample_rate:
        fail(f"the input is at {rate} Hz, the song at {sample_rate}")
    out = run(process, audio, sample_rate, fields_of(params, audio.shape[0]))
    sf.write(a.output, out.astype(np.float32), sample_rate, subtype="FLOAT")
    print(json.dumps({"ok": True, "frames_in": int(audio.shape[0]), "frames_out": int(out.shape[0]), "channels": int(out.shape[1])}))


if __name__ == "__main__":
    main()
