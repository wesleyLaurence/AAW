"""Taking a song apart into its parts on this Mac: `daw separate`.

The models are the ones the Ultimate Vocal Remover community runs, listed in
`separation_models.json` with where each author put it, its size, its hash and
its license; nothing is bundled, and `daw separate setup` downloads a model from
its address and checks it. audio-separator runs them in a Python environment of
its own under the app's data folder, `~/Library/Application Support/AAW/separation`
(or `AAW_DATA_DIR`), made by `uv`, since torch is a gigabyte no checkout or
bundle should carry; `daw` starts it as a process (`separate_runner.py`) and
reads its progress as lines of JSON. Each part is written under `separated/`
in the project as 32-bit float WAV at 44.1 kHz, with the source's length to the
frame, and added to the song as a sample; the recipe beside the parts names the
source, the model, its weights, its license and what was measured, and the same
source through the same model is found there and not separated again.

AAW_SEPARATOR names a command to run in place of the environment's runner, with
the runner's arguments and lines, for tests; with it no model file is required.
"""

from __future__ import annotations

import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
from datetime import datetime, timezone
from fractions import Fraction
from functools import lru_cache
from pathlib import Path

import numpy as np
import soundfile as sf

from . import library
from .model import digest, load, meter, project_hash

SPLITS = {
    "vocals": ("vocals", "instrumental"),
    "stems": ("drums", "bass", "vocals", "other"),
    "stems6": ("drums", "bass", "vocals", "guitar", "piano", "other"),
}
QUALITIES = ("fast", "best")
RATE = 44100
# The folder in the project the parts go to.
FOLDER = "separated"
RIGHTS = (
    "Stems of a recording are copies of it: taking a song apart for practice, study or "
    "a private remix is yours to do, and publishing or selling the parts or a remix "
    "needs the rights to the song. You answer for the audio you separate."
)
MEASURE = (
    "The numbers measure the parts against the source; they do not hear them. A low "
    "residual says the parts add up to the song, not that each part is clean."
)
RUNTIME_MB = 1000


def data_dir() -> Path:
    return Path(os.environ.get("AAW_DATA_DIR", str(Path.home() / "Library/Application Support/AAW")))


def root() -> Path:
    return data_dir() / "separation"


def env_python() -> Path:
    return root() / "env" / "bin" / "python"


def models_dir() -> Path:
    return root() / "models"


def runtime_file() -> Path:
    return root() / "runtime.json"


@lru_cache(maxsize=1)
def manifest() -> dict:
    return json.loads((Path(__file__).parent / "separation_models.json").read_text())


def models() -> list[dict]:
    return manifest()["models"]


def model_named(name: str) -> dict:
    for model in models():
        if model["name"] == name or model["file"] == name:
            return model
    raise ValueError(f"No model named {name}; daw separate models lists them")


def model_for(split: str, quality: str) -> dict:
    if split not in SPLITS:
        raise ValueError(f"split is one of {', '.join(SPLITS)}")
    if quality not in QUALITIES:
        raise ValueError(f"quality is one of {', '.join(QUALITIES)}")
    of_split = [m for m in models() if m["split"] == split]
    for model in of_split:
        if model["quality"] == quality:
            return model
    # A split with one model has it at either quality.
    return of_split[0]


def defaults() -> list[dict]:
    """The models `daw separate setup` downloads unless told which: fast and best of vocals and stems."""
    return [m for m in models() if m["split"] in ("vocals", "stems")]


def mb(count: int) -> int:
    return round(count / 1_000_000)


def note(text: str):
    print(json.dumps({"note": text}), file=sys.stderr, flush=True)


# --- the runtime -----------------------------------------------------------


def stand_in() -> list[str] | None:
    value = os.environ.get("AAW_SEPARATOR")
    return shlex.split(value) if value else None


def runtime() -> dict:
    """Whether the separation environment is here, and what it holds."""
    if stand_in():
        return {"installed": True, "stand_in": os.environ["AAW_SEPARATOR"]}
    status = {"installed": env_python().is_file(), "root": str(root())}
    if runtime_file().is_file():
        status.update(json.loads(runtime_file().read_text()))
    return status


def find_uv() -> Path:
    found = shutil.which("uv")
    for candidate in [found, Path.home() / ".local/bin/uv", Path("/opt/homebrew/bin/uv"), Path.home() / ".cargo/bin/uv"]:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    raise ValueError(
        "Making the separation environment needs uv, which is not on the PATH: install it "
        "with brew install uv, or from https://docs.astral.sh/uv/"
    )


def apple_silicon() -> bool:
    """Whether this Mac has an Apple chip, asked of the system: the checkout's own
    Python may run under Rosetta and call itself x86_64."""
    if platform.system() != "Darwin":
        return False
    try:
        out = subprocess.run(["/usr/sbin/sysctl", "-n", "hw.optional.arm64"], capture_output=True, text=True)
    except OSError:
        return False
    return out.stdout.strip() == "1"


def install_runtime() -> dict:
    """Makes the environment under root() unless it is there; the runtime's status."""
    status = runtime()
    if status["installed"]:
        return {**status, "installed_now": False}
    if platform.system() != "Darwin":
        raise ValueError("daw separate runs on a Mac")
    if not apple_silicon():
        raise ValueError(
            "daw separate needs an Apple silicon Mac: torch ships no wheels for Intel Macs "
            f"new enough for audio-separator {manifest()['runner']['version']}"
        )
    uv = find_uv()
    spec = manifest()["runner"]
    env = root() / "env"
    root().mkdir(parents=True, exist_ok=True)
    note(f"Making the separation environment at {env}: Python and about {RUNTIME_MB} MB of packages")
    started = time.monotonic()
    run_quietly([uv, "venv", "--python", spec["python"], env])
    packages = [f"{spec['package']}[cpu]=={spec['version']}", *spec.get("also", [])]
    run_quietly([uv, "pip", "install", "--python", env_python(), *packages])
    probe = (
        "import json, platform, torch, onnxruntime; from importlib.metadata import version; "
        "print(json.dumps({'audio_separator': version('audio-separator'), 'torch': torch.__version__, "
        "'onnxruntime': version('onnxruntime'), 'python': platform.python_version(), "
        "'machine': platform.machine(), 'mps': torch.backends.mps.is_available()}))"
    )
    out = subprocess.run([str(env_python()), "-c", probe], capture_output=True, text=True)
    if out.returncode:
        raise ValueError(f"The separation environment does not import its packages: {out.stderr.strip()}")
    versions = json.loads(out.stdout)
    for data in spec.get("data", []):
        download(data["url"], models_dir() / data["name"])
    record = {
        "root": str(root()),
        "python": str(env_python()),
        "versions": versions,
        "made": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "seconds": round(time.monotonic() - started, 1),
    }
    runtime_file().write_text(json.dumps(record, indent=2))
    return {"installed": True, "installed_now": True, **record}


def run_quietly(command):
    out = subprocess.run(list(map(str, command)), capture_output=True, text=True)
    if out.returncode:
        raise ValueError(f"{Path(str(command[0])).name} {command[1]} failed: {out.stderr.strip()[-2000:]}")


# --- the models --------------------------------------------------------------


def file_status(spec: dict) -> dict:
    path = models_dir() / spec["name"]
    here = path.is_file()
    matches = None
    if here and "sha256" in spec:
        matches = path.stat().st_size == spec["bytes"] and digest(path) == spec["sha256"]
    return {"name": spec["name"], "bytes": spec["bytes"], "here": here, "matches": matches}


def model_status(model: dict) -> dict:
    files = [file_status(f) for f in model["files"]]
    return {
        "name": model["name"],
        "title": model["title"],
        "family": model["family"],
        "split": model["split"],
        "quality": model["quality"],
        "parts": list(model["parts"].values()),
        "author": model["author"],
        "license": model["license"],
        "license_url": model.get("license_url"),
        "sdr": model.get("sdr", {}),
        **({"note": model["note"]} if "note" in model else {}),
        "mb": mb(sum(f["bytes"] for f in model["files"])),
        "here": all(f["here"] and f["matches"] is not False for f in files),
        "files": files,
        "get": f"daw separate setup --models {model['name']}",
    }


def listing(split: str | None = None) -> dict:
    chosen = [m for m in models() if split is None or m["split"] == split]
    return {
        "runtime": runtime(),
        "folder": str(models_dir()),
        "models": [model_status(m) for m in chosen],
        "defaults": {s: {q: model_for(s, q)["name"] for q in QUALITIES} for s in SPLITS},
        "rights": RIGHTS,
    }


def download(url: str, target: Path, expected: dict | None = None):
    """Fetches `url` to `target` through a temporary file, checked against `expected`'s bytes and sha256."""
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=target.parent, prefix=".", suffix=".download")
    os.close(fd)
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "aaw-daw-separate"})
        with urllib.request.urlopen(request, timeout=60) as response, open(temp, "wb") as out:
            total = int(response.headers.get("Content-Length") or 0)
            got, said = 0, 0
            while block := response.read(1 << 20):
                out.write(block)
                got += len(block)
                if total > 50_000_000 and got - said >= 100_000_000:
                    said = got
                    note(f"{target.name}: {mb(got)} of {mb(total)} MB")
        if expected:
            size = os.path.getsize(temp)
            if size != expected["bytes"]:
                raise ValueError(f"{target.name} came down as {size} bytes, not the {expected['bytes']} the manifest lists")
            if digest(Path(temp)) != expected["sha256"]:
                raise ValueError(f"{target.name} does not match the manifest's hash; nothing was kept")
        os.replace(temp, target)
    except (OSError, urllib.error.URLError) as exc:
        raise ValueError(f"Could not download {url}: {exc}") from None
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def get_model(model: dict) -> dict:
    """Downloads the files of `model` that are missing or wrong; what was done."""
    fetched, kept = [], []
    for spec in model["files"]:
        status = file_status(spec)
        if status["here"] and status["matches"]:
            kept.append(spec["name"])
            continue
        if status["here"]:
            note(f"{spec['name']} is here but does not match the manifest; downloading it again")
        note(f"Downloading {spec['name']} ({mb(spec['bytes'])} MB) from {spec['url']}")
        download(spec["url"], models_dir() / spec["name"], spec)
        fetched.append(spec["name"])
    return {**model_status(model), "downloaded": fetched, "kept": kept}


def setup(names: list[str] | None = None, all_models=False, runtime_only=False) -> dict:
    chosen = models() if all_models else [model_named(n) for n in names] if names else defaults()
    result = {"root": str(root()), "runtime": install_runtime()}
    if not runtime_only:
        wanted = [
            m for m in chosen if not model_status(m)["here"]
        ]
        if wanted:
            note(
                f"Downloading {mb(sum(f['bytes'] for m in wanted for f in m['files']))} MB of models: "
                + ", ".join(m["name"] for m in wanted)
            )
        result["models"] = [get_model(m) for m in chosen]
    result["rights"] = RIGHTS
    return result


def ensure_model(model: dict):
    """Refuses, saying what to run, unless the runtime and the model are here."""
    if stand_in():
        return
    if not runtime()["installed"]:
        raise ValueError(
            f"The separation runtime is not installed. daw separate setup makes a Python "
            f"environment of about {RUNTIME_MB} MB under {root()} and downloads the default "
            f"models, {mb(sum(f['bytes'] for m in defaults() for f in m['files']))} MB; "
            "--runtime-only makes the environment alone, --models NAME picks the models"
        )
    status = model_status(model)
    if status["here"]:
        return
    wrong = [f["name"] for f in status["files"] if f["here"] and f["matches"] is False]
    if wrong:
        raise ValueError(
            f"{', '.join(wrong)} in {models_dir()} does not match the manifest's hash: "
            f"run {status['get']} to download it again"
        )
    raise ValueError(
        f"The model {model['name']} ({model['title']}, {status['mb']} MB, {model['license']}, by "
        f"{model['author']}) is not downloaded: run {status['get']}"
    )


# --- separating ----------------------------------------------------------------


CLIP = re.compile(r"^tracks\.([A-Za-z][A-Za-z0-9_-]*)\.audio[.\[](\d+)\]?$")


def resolve_source(project: dict, project_dir: Path, source: str) -> dict:
    """What `source` names: a sample of the song, an audio clip, or a file imported now.

    Gives the sample's ID, its entry, and the clip's track and index where a clip
    was named; `imported` holds a new sample's entry for the song.
    """
    samples = project.get("samples", {})
    if source in samples:
        return {"sample": source, "entry": samples[source], "clip": None, "imported": None}
    if match := CLIP.match(source):
        track_id, index = match.group(1), int(match.group(2))
        track = next((t for t in project.get("tracks", []) if t["id"] == track_id), None)
        if track is None:
            raise ValueError(f"No track {track_id}")
        clips = track.get("audio") or []
        if index >= len(clips):
            raise ValueError(f"tracks.{track_id} has {len(clips)} audio clips; no clip {index}")
        clip = clips[index]
        return {"sample": clip["sample"], "entry": samples[clip["sample"]], "clip": (track_id, index, clip), "imported": None}
    path = Path(source).expanduser()
    if not path.is_file():
        raise ValueError(
            f"{source} is not a sample of the song, an audio clip (tracks.T.audio.N) or a file"
        )
    entry = library.import_asset(path, project_dir)
    for sample_id, existing in samples.items():
        if existing["path"] == entry["path"]:
            return {"sample": sample_id, "entry": existing, "clip": None, "imported": None}
    sample_id = sample_slug(path.stem, samples)
    return {"sample": sample_id, "entry": entry, "clip": None, "imported": {sample_id: entry}}


def sample_slug(stem: str, taken) -> str:
    """A sample ID from a file's name: a letter first, then letters, digits, _ and -."""
    out = re.sub(r"[^A-Za-z0-9_-]+", "-", stem).strip("-")
    if not out or not out[0].isalpha():
        out = "song-" + out if out else "song"
    out = out.rstrip("-") or "song"
    candidate, n = out, 2
    while candidate in taken:
        candidate, n = f"{out}-{n}", n + 1
    return candidate


def recipe_path(project_dir: Path, source_hash: str, model: dict) -> Path:
    return project_dir / FOLDER / f"{source_hash[:12]}-{model['name']}.separated.json"


def part_path(project_dir: Path, source_hash: str, model: dict, part: str) -> Path:
    return project_dir / FOLDER / f"{source_hash[:12]}-{model['name']}-{part}.wav"


def reusable(project_dir: Path, recipe: Path) -> dict | None:
    """The recipe's record when every part it names is still here as written."""
    if not recipe.is_file():
        return None
    try:
        record = json.loads(recipe.read_text())
    except ValueError:
        return None
    for part in record.get("parts", {}).values():
        file = project_dir / part["path"]
        if not file.is_file() or digest(file) != part["sha256"]:
            return None
    return record


def run_runner(source_file: Path, model: dict, wanted: dict) -> dict:
    """Runs the separation process over `source_file` and returns its last line."""
    command = stand_in() or [
        str(env_python()),
        str(Path(__file__).parent / "separate_runner.py"),
    ]
    command += [
        "--source", str(source_file),
        "--models", str(models_dir()),
        "--model-file", model["file"],
        "--out-parts", json.dumps(wanted),
    ]
    with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as process:
        last, lines = None, []
        for line in process.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except ValueError:
                lines.append(line)
                continue
            if "error" in event:
                process.wait()
                raise ValueError(f"Separation failed: {event['error']}")
            if "stage" in event:
                note(f"separating: {event['stage']}" + (f" ({event['part']})" if "part" in event else ""))
            last = event
        stderr = process.stderr.read()
        code = process.wait()
    if code or not last or not last.get("done"):
        tail = (stderr.strip() or "\n".join(lines)).splitlines()[-8:]
        raise ValueError("Separation failed: " + (" / ".join(tail) or f"the runner exited with {code}"))
    return last


def measured(file: Path) -> dict:
    """A part's length, level and loudness, read from the file."""
    from . import aaw_py

    x, rate = sf.read(file, always_2d=True, dtype="float64")
    peak = float(np.abs(x).max()) if len(x) else 0.0
    read = aaw_py.loudness(np.ascontiguousarray(x), rate, [(0, len(x))])[0] if len(x) else {}
    lufs = read.get("integrated_lufs")
    return {
        "frames": len(x),
        "sample_rate": rate,
        "channels": x.shape[1],
        "duration_seconds": round(len(x) / rate, 4),
        "peak_dbfs": round(float(20 * np.log10(peak)), 2) if peak > 0 else None,
        "integrated_lufs": round(lufs, 2) if lufs is not None else None,
        "silent": peak <= 1e-6,
    }


def separate(
    project_path: Path,
    source: str,
    split: str | None = None,
    quality="best",
    model_name: str | None = None,
    prefix: str | None = None,
    tracks=False,
) -> dict:
    project_dir = project_path.parent
    if model_name:
        model = model_named(model_name)
        if split and split != model["split"]:
            raise ValueError(f"{model['name']} gives the {model['split']} split, not {split}")
    else:
        model = model_for(split or "vocals", quality)
    split = model["split"]
    ensure_model(model)
    # A part made again is a sample whose file changed: read the song past that check.
    project = load(project_path, verify_assets=False)
    found = resolve_source(project, project_dir, source)
    source_file = project_dir / found["entry"]["path"]
    source_hash = found["entry"].get("sha256") or digest(source_file)
    recipe = recipe_path(project_dir, source_hash, model)
    prefix = prefix or found["sample"]
    wanted = {
        stem: {"part": part, "file": str(part_path(project_dir, source_hash, model, part))}
        for stem, part in model["parts"].items()
    }

    record = reusable(project_dir, recipe)
    reused = record is not None
    if record is None:
        library.probe(source_file)
        result = run_runner(source_file, model, wanted)
        parts = {}
        for part, made in result["parts"].items():
            file = Path(made["file"])
            parts[part] = {
                "path": str(file.relative_to(project_dir)),
                "sha256": digest(file),
                **measured(file),
            }
        record = {
            "source": {
                "sample": found["sample"],
                "path": found["entry"]["path"],
                "sha256": source_hash,
                "original_sample_rate": result.get("original_sample_rate"),
            },
            "split": split,
            "model": {
                k: model.get(k) for k in ("name", "title", "family", "file", "author", "license", "license_url")
            },
            "weights": [{"name": f["name"], "sha256": f["sha256"]} for f in model["files"]],
            "runtime": result.get("runtime", {}),
            "parameters": {
                "sample_rate": result.get("sample_rate", RATE),
                "input_gain_db": result.get("input_gain_db"),
                "normalization": "none",
            },
            "residual_db": result.get("residual_db"),
            "seconds": result.get("seconds"),
            "max_rss_mb": mb(result["max_rss_bytes"]) if result.get("max_rss_bytes") else None,
            "parts": parts,
            "made": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        }
        recipe.parent.mkdir(parents=True, exist_ok=True)
        recipe.write_text(json.dumps(record, indent=2))

    # The song takes the parts as one edit, so a running host hears it and undoes it.
    project = load(project_path, verify_assets=False)
    samples = dict(project.get("samples", {}))
    patch, added, ids = {}, [], {}
    new_samples = dict(found["imported"] or {})
    for part, info in record["parts"].items():
        sample_id = f"{prefix}-{part}"
        existing = samples.get(sample_id) or new_samples.get(sample_id)
        if existing and existing["path"] != info["path"]:
            raise ValueError(
                f"Sample ID {sample_id} already names {existing['path']}; pass --id with another prefix"
            )
        ids[part] = sample_id
        if existing is None or existing.get("sha256") != info["sha256"]:
            # New, or made again since the song took it: the song records the file as it is.
            new_samples[sample_id] = {
                **(existing or {}),
                "path": info["path"],
                "sha256": info["sha256"],
                "source": str((project_dir / record["source"]["path"]).resolve()),
            }
            if existing is None:
                added.append(sample_id)
    if new_samples:
        patch["samples"] = new_samples
    placed = place(project, found, ids, record, prefix) if tracks else None
    if placed:
        patch.update(placed["patch"])
    if patch:
        with tempfile.TemporaryDirectory() as scratch:
            file = Path(scratch) / "patch.json"
            file.write_text(json.dumps(patch))
            label = f"Separate {found['sample']} into " + ", ".join(record["parts"])
            from .cli import run_engine

            run_engine("apply", project_path, file, "--expect", project_hash(project), "--label", label)

    reply = {
        "source": {**record["source"], "clip": f"tracks.{found['clip'][0]}.audio.{found['clip'][1]}" if found["clip"] else None,
                   "imported": bool(found["imported"])},
        "split": split,
        "model": record["model"],
        "reused": reused,
        "parts": [
            {"part": part, "sample_id": ids[part], "added": ids[part] in added, **info}
            for part, info in record["parts"].items()
        ],
        "residual_db": record["residual_db"],
        "seconds": record["seconds"],
        "max_rss_mb": record.get("max_rss_mb"),
        "runtime": record["runtime"],
        "recipe": str(recipe.relative_to(project_dir)),
        "notes": [MEASURE],
    }
    if placed:
        reply["tracks"] = placed["tracks"]
        reply["silenced"] = placed["silenced"]
    if not said_rights():
        reply["rights"] = RIGHTS
    return reply


def said_rights() -> bool:
    """Whether the rights line was given once already; marks it given."""
    marker = root() / "rights-said"
    if marker.is_file():
        return True
    try:
        marker.parent.mkdir(parents=True, exist_ok=True)
        marker.write_text(RIGHTS + "\n")
    except OSError:
        pass
    return False


def place(project: dict, found: dict, ids: dict, record: dict, prefix: str) -> dict:
    """The patch that lays the parts on tracks of their own under the source.

    From a clip, each part gets a clip at the clip's place with its fields, and
    the source clip falls silent: its track muted when the clip is alone on it,
    or else its gain at -96 dB. From a sample, each part plays whole from beat
    0, the song grown to the end of the bar it ends in.
    """
    tracks = [dict(t) for t in project.get("tracks", [])]
    taken = {t["id"] for t in tracks} | {g["id"] for g in project.get("groups", [])} | {
        r["id"] for r in project.get("returns", [])
    }
    patch, silenced = {}, None
    if found["clip"]:
        track_id, index, clip = found["clip"]
        at = next(i for i, t in enumerate(tracks) if t["id"] == track_id)
        source_track = tracks[at]
        base = {k: v for k, v in clip.items() if k != "sample"}
        group = source_track.get("group")
        if len(source_track.get("audio") or []) == 1 and not source_track.get("clips") and not source_track.get("pads"):
            source_track["mute"] = True
            silenced = {"track": track_id, "mute": True}
        else:
            clips = [dict(c) for c in source_track["audio"]]
            clips[index]["gain_db"] = -96
            source_track["audio"] = clips
            silenced = {"clip": f"tracks.{track_id}.audio.{index}", "gain_db": -96}
    else:
        at = len(tracks) - 1
        session = project["session"]
        first = next(iter(record["parts"].values()))
        seconds = Fraction(first["frames"], first["sample_rate"])
        beats = seconds * Fraction(str(session["tempo"])) / 60
        bar = meter(session)[0]
        length = Fraction(str(session.get("length_beats", 16)))
        if beats > length:
            grown = -(-beats // bar) * bar
            patch["session"] = {"length_beats": str(grown) if grown.denominator != 1 else int(grown)}
        base = {"at": 0}
        group = None
    new = []
    for part, sample_id in ids.items():
        track_id = f"{prefix}-{part}"
        if track_id in taken:
            raise ValueError(f"A track, group or return named {track_id} is already in the song; pass --id with another prefix")
        track = {"id": track_id, "pads": {}, "clips": [], "audio": [{**base, "sample": sample_id}]}
        if group:
            track["group"] = group
        new.append(track)
    tracks[at + 1 : at + 1] = new
    patch["tracks"] = tracks
    return {
        "patch": patch,
        "tracks": [{"id": t["id"], "clip": f"tracks.{t['id']}.audio.0", "sample": t["audio"][0]["sample"]} for t in new],
        "silenced": silenced,
    }
