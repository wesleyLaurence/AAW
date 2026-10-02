"""A named deliverable made from a render: WAV, AAC or MP3, at a stated level.

An export is a render's mix under a name the person chose, in a folder they keep.
It can change the level by one gain for the whole file and says what it did; it
does not limit, which is the song's master limiter's work. A file beside it
records the render it came from.
"""

import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

from . import perception
from .model import digest, hash_matches, load

# The programs that encode each format, in the order tried, with their arguments.
ENCODERS = {
    ".m4a": {
        "afconvert": "-f m4af -d aac -b {bits_per_second} {source} {target}",
        "ffmpeg": "-v error -nostdin -y -i {source} -c:a aac -b:a {kbps}k -f ipod {target}",
    },
    ".mp3": {
        "ffmpeg": "-v error -nostdin -y -i {source} -c:a libmp3lame -b:a {kbps}k -f mp3 {target}",
        "lame": "--silent -b {kbps} {source} {target}",
    },
}
BITRATES = {".m4a": 256, ".mp3": 320}
LEVEL = ("integrated_lufs", "peak_dbfs", "estimated_true_peak_dbtp")


def find_render(project: Path, render=None):
    """The folder and report of the render to export, and whether it was made now.

    A render named by the caller is taken as it is. Otherwise it is the song's
    latest full render, made again here when the song has changed since.
    """
    from .cli import run_engine

    def read(source):
        mix, folder = perception.resolve(source)
        if folder is None or not mix.is_file():
            raise ValueError(f"Not a render: {source}")
        manifest = json.loads((folder / "report.json").read_text())
        target = manifest.get("target") or {}
        if target.get("track") or target.get("section"):
            raise ValueError("An export is of a whole mix, not a track or a section")
        if digest(mix) != manifest["audio_sha256"]:
            raise ValueError("Render mix hash mismatch")
        return folder, manifest

    if render is not None:
        return *read(render), False
    pointer = project.parent / "renders" / "latest.json"
    if pointer.is_file():
        folder, manifest = read(pointer)
        if hash_matches(load(project), manifest["project_sha256"]):
            return folder, manifest, False
    run_engine("render", project)
    return *read(pointer), True


def measured(x, rate):
    return {k: v for k, v in perception.measure(x, rate).items() if k in LEVEL}


def encode(source: Path, target: Path, kbps: int):
    """Encodes a WAV as the format of `target`'s name; returns the encoder that did."""
    encoders = ENCODERS[target.suffix.lower()]
    names = [name for name in encoders if shutil.which(name)]
    if not names:
        raise ValueError(
            f"Encoding {target.suffix} needs one of {', '.join(encoders)} on the PATH"
        )
    failures = []
    for name in names:
        fields = {"source": "\0", "target": "\1", "kbps": kbps, "bits_per_second": kbps * 1000}
        places = {"\0": str(source), "\1": str(target)}
        command = [name, *(places.get(a, a) for a in encoders[name].format(**fields).split())]
        out = subprocess.run(command, capture_output=True, text=True)
        if out.returncode == 0 and target.is_file() and target.stat().st_size:
            return name
        said = (out.stderr or out.stdout).strip().splitlines()
        failures.append(f"{name}: {said[-1] if said else 'failed'}")
    raise ValueError(f"Could not encode {target.name} ({'; '.join(failures)})")


def export(
    project: Path,
    to: Path,
    bits=24,
    bitrate=None,
    gain=None,
    peak=None,
    lufs=None,
    match=None,
    ceiling=-0.1,
    render=None,
    replace=False,
):
    kind = to.suffix.lower()
    if kind != ".wav" and kind not in ENCODERS:
        raise ValueError("An export is a .wav, .m4a or .mp3 file")
    if to.exists() and not replace:
        raise ValueError(f"{to} exists; pass --replace to write over it")
    if ceiling > 0:
        raise ValueError("ceiling must be at most 0 dBFS")
    song = load(project)
    folder, manifest, rendered_now = find_render(project, render)
    x, rate = sf.read(folder / "mix.wav", always_2d=True, dtype="float64")
    before = measured(x, rate)
    if before["peak_dbfs"] is None:
        raise ValueError("The render is silent")

    # One gain for the whole file, held under the ceiling.
    policy, target, wanted = "as rendered", None, 0.0
    if gain is not None:
        policy, wanted = "gain", gain
    elif peak is not None:
        policy, target = "peak", {"true_peak_dbtp": peak}
        wanted = peak - before["estimated_true_peak_dbtp"]
    elif lufs is not None or match is not None:
        if before["integrated_lufs"] is None:
            raise ValueError("The render is too short or too quiet to have a loudness")
        policy, target = "loudness", {"integrated_lufs": lufs}
        if match is not None:
            if match not in song["samples"]:
                raise ValueError(f"Unknown sample: {match}")
            path = project.parent / song["samples"][match]["path"]
            source, source_rate = sf.read(path, always_2d=True, dtype="float64")
            lufs = perception.integrated(source, source_rate)
            if lufs is None:
                raise ValueError(f"Sample {match} is too short or too quiet to have a loudness")
            policy, target = "match", {"sample": match, "integrated_lufs": lufs}
        wanted = lufs - before["integrated_lufs"]
    applied = wanted if policy == "as rendered" else min(wanted, ceiling - before["peak_dbfs"])
    y = x * 10 ** (applied / 20)
    level = {
        "policy": policy,
        "target": target,
        "gain_db": round(applied, 3),
        "held_back_db": round(wanted - applied, 3),
        "ceiling_dbfs": ceiling,
        **(before if applied == 0 else measured(y, rate)),
        "rendered": before,
    }

    to.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=to.parent, prefix=".", suffix=kind)
    os.close(fd)
    try:
        if kind == ".wav" and bits == 16:
            # Dither of one step, triangular, the same for the same audio.
            rng = np.random.default_rng(0)
            steps = np.round(y * 32768 + rng.random(y.shape) - rng.random(y.shape))
            steps = np.clip(steps, -32768, 32767).astype(np.int16)
            sf.write(temp, steps, rate, subtype="PCM_16", format="WAV")
            made = {"container": "wav", "bits": 16, "dither": "triangular, one step"}
        elif kind == ".wav":
            sf.write(temp, y, rate, subtype="PCM_24", format="WAV")
            made = {"container": "wav", "bits": 24}
        else:
            kbps = bitrate or BITRATES[kind]
            with tempfile.TemporaryDirectory() as scratch:
                pcm = Path(scratch) / "mix.wav"
                sf.write(pcm, y, rate, subtype="PCM_24")
                encoder = encode(pcm, Path(temp), kbps)
            codec = "aac" if kind == ".m4a" else "mp3"
            made = {"container": kind[1:], "codec": codec, "bitrate_kbps": kbps, "encoder": encoder}
        os.replace(temp, to)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)

    warnings = []
    if kind != ".wav" and level["estimated_true_peak_dbtp"] > -0.99:
        warnings.append(
            f"True peak {level['estimated_true_peak_dbtp']:.2f} dBTP before encoding: a decoder "
            "can clip what a lossy encoder overshoots; --peak -1 leaves room"
        )
    if level["held_back_db"] > 0.05:
        warnings.append(
            f"The level asked for needs {level['held_back_db']:.2f} dB more than the ceiling "
            "allows; a master limiter in the song makes room"
        )
    record = {
        "file": to.name,
        "sha256": digest(to),
        "format": made,
        "sample_rate": rate,
        "channels": x.shape[1],
        "duration_seconds": len(x) / rate,
        "level": level,
        "render": {
            k: manifest.get(k) for k in ("render_id", "project_sha256", "audio_sha256", "engine_sha256")
        },
        "warnings": warnings,
    }
    # Beside the file, without local paths: what it is and which render it came from.
    note = to.with_name(to.name + ".json")
    note.write_text(json.dumps(record, indent=1) + "\n")
    return {
        **record,
        "file": str(to.resolve()),
        "record": str(note.resolve()),
        "render": {**record["render"], "directory": str(folder), "rendered_now": rendered_now},
    }
