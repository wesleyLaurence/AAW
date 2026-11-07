"""A named deliverable made from a render: WAV, AIFF, FLAC, AAC or MP3, at a stated level.

An export is a render's mix, the whole song or a range of it, under a name the
person chose, in the project's `exports/` folder unless a path says otherwise.
It is what is heard: a muted or solo-muted track, or a deactivated clip, is not
in the render it is cut from. It can change the sample rate and the level by one
gain for the whole file and says what it did; it does not limit, which is the
song's master limiter's work. A file beside it records the render it came from.
"""

import json
import math
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

from fractions import Fraction

from . import perception
from .model import beat, digest, frame, hash_matches, load, meter

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
# The uncompressed formats, by extension: soundfile's name for each and the bit
# depths it writes.
PCM = {".wav": ("WAV", (16, 24, 32)), ".aiff": ("AIFF", (16, 24, 32)), ".aif": ("AIFF", (16, 24, 32)), ".flac": ("FLAC", (16, 24))}
SUBTYPES = {16: "PCM_16", 24: "PCM_24", 32: "FLOAT"}
FORMATS = ".wav, .aiff, .flac, .m4a or .mp3"
# The highest rate the compressed formats are written at.
COMPRESSED_RATE = 48000
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


def position(text: str, song: dict) -> Fraction:
    """A place in the song: beats from its start (`66`, `16.5`, `1/3`), or
    `BAR:BEAT` counted from 1 in the song's meter (`17:1`, `2:2.5`), the beat
    the note the meter counts, an eighth in 6/8."""
    text = str(text).strip()
    if ":" not in text:
        return beat(text)
    bar, _, at = text.partition(":")
    try:
        bar, at = int(bar), beat(at)
    except (ValueError, TypeError):
        raise ValueError(f"A place is beats or BAR:BEAT, such as 66 or 17:1: {text}") from None
    length, unit = meter(song["session"])
    if bar < 1 or at < 1 or (at - 1) * unit >= length:
        raise ValueError(f"{text} is not in a bar of {song['session'].get('time_signature', '4/4')}")
    return (bar - 1) * length + (at - 1) * unit


def place(beats: Fraction, song: dict) -> str:
    """Beats from the song's start as BAR:BEAT in its meter."""
    length, unit = meter(song["session"])
    bar = math.floor(beats / length)
    at = (beats - bar * length) / unit + 1
    text = str(at.numerator) if at.denominator == 1 else f"{float(at):g}"
    return f"{bar + 1}:{text}"


def span(song: dict, start=None, end=None, bars=None, section=None):
    """The beats an export covers, start and end, and whether that is the whole
    song: given as places, as bars `A-B` counted whole from 1, or a section."""
    length = beat(song["session"]["length_beats"])
    if bars is not None:
        first, _, last = str(bars).partition("-")
        try:
            first, last = int(first), int(last or first)
        except ValueError:
            raise ValueError(f"--bars is a bar or two from 1, such as 2-35: {bars}") from None
        if not 1 <= first <= last:
            raise ValueError(f"--bars counts from bar 1, first to last: {bars}")
        bar = meter(song["session"])[0]
        a, b = (first - 1) * bar, last * bar
    elif section is not None:
        found = [s for s in song.get("sections", []) if s["id"] == section]
        if len(found) != 1:
            raise ValueError(f"Unknown or ambiguous section: {section}")
        a = beat(found[0]["at"])
        b = a + beat(found[0]["length_beats"])
    else:
        a = Fraction(0) if start is None else position(start, song)
        b = length if end is None else position(end, song)
    if a < 0:
        raise ValueError("A range starts at the song's start or after it")
    if b <= a:
        raise ValueError("A range ends after it starts")
    if b > length:
        raise ValueError(
            f"The range ends at {place(b, song)}, past the song's end at {place(length, song)} "
            f"(beat {float(length):g}), where nothing is rendered"
        )
    return a, b, a == 0 and b == length


def resample(x, rate: int, to: int):
    """`x` at another sample rate, by a polyphase filter: 48000 to 44100 is up
    147, down 160."""
    from scipy.signal import resample_poly

    g = math.gcd(rate, to)
    return resample_poly(x, to // g, rate // g, axis=0)


def export(
    project: Path,
    to: Path | None = None,
    bits=None,
    bitrate=None,
    gain=None,
    peak=None,
    lufs=None,
    match=None,
    ceiling=-0.1,
    render=None,
    replace=False,
    start=None,
    end=None,
    bars=None,
    section=None,
    sample_rate=None,
):
    song = load(project)
    # A name alone goes in the project's exports folder, as does no name: the
    # song's title, else its folder's while the title is the one it was made
    # with, as a 24-bit WAV.
    exports = project.parent / "exports"
    if to is None:
        title = (song["session"].get("title") or "").strip()
        title = project.parent.name if title in ("", "Untitled") else title
        to = exports / f"{title.replace('/', '-')}.wav"
    elif to.parent == Path("."):
        to = exports / to
    kind = to.suffix.lower()
    if kind not in PCM and kind not in ENCODERS:
        raise ValueError(f"An export is a {FORMATS} file")
    if kind in PCM:
        bits = 24 if bits is None else bits
        if bits not in PCM[kind][1]:
            raise ValueError(f"A {kind} file is {' or '.join(map(str, PCM[kind][1]))} bits")
    elif bits is not None:
        raise ValueError(f"--bits is for .wav, .aiff and .flac; a {kind} file has a bitrate")
    if sample_rate is not None and not 8000 <= sample_rate <= 384000:
        raise ValueError("sample rate must be 8000–384000 Hz")
    if kind in ENCODERS and sample_rate is not None and sample_rate > COMPRESSED_RATE:
        raise ValueError(f"A {kind} file is at most {COMPRESSED_RATE} Hz")
    if to.exists() and not replace:
        raise ValueError(f"{to} exists; pass --replace to write over it")
    if ceiling > 0:
        raise ValueError("ceiling must be at most 0 dBFS")
    ranged = any(v is not None for v in (start, end, bars, section))
    if ranged and render is not None:
        raise ValueError("A range is of the song as it is; --render exports a render whole")
    a, b, whole = span(song, start, end, bars, section)
    folder, manifest, rendered_now = find_render(project, render)
    x, rate = sf.read(folder / "mix.wav", always_2d=True, dtype="float64")
    # The range's frames of the mix, which starts at the song's start.
    tempo = song["session"]["tempo"]
    first, last = (0, len(x)) if whole or render is not None else (frame(a, tempo, rate), frame(b, tempo, rate))
    if last > len(x):
        raise ValueError(f"The render is {len(x)} frames; the range ends at frame {last}")
    x = x[first:last]
    out_rate = sample_rate or rate
    if out_rate > COMPRESSED_RATE and kind in ENCODERS:
        # A song above what AAC and MP3 carry is written at the rate it divides.
        out_rate = 44100 if rate % 44100 == 0 else COMPRESSED_RATE
    if out_rate != rate:
        x = resample(x, rate, out_rate)
    before = measured(x, out_rate)
    if before["peak_dbfs"] is None:
        if whole:
            raise ValueError("The render is silent")
        raise ValueError(f"Nothing is heard from {place(a, song)} to {place(b, song)}: muted tracks and deactivated clips are not exported")

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
        **(before if applied == 0 else measured(y, out_rate)),
        "rendered": before,
    }

    to.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=to.parent, prefix=".", suffix=kind)
    os.close(fd)
    try:
        container = PCM.get(kind, (None,))[0]
        if container and bits == 16:
            # Dither of one step, triangular, the same for the same audio.
            rng = np.random.default_rng(0)
            steps = np.round(y * 32768 + rng.random(y.shape) - rng.random(y.shape))
            steps = np.clip(steps, -32768, 32767).astype(np.int16)
            sf.write(temp, steps, out_rate, subtype="PCM_16", format=container)
            made = {"container": container.lower(), "bits": 16, "dither": "triangular, one step"}
        elif container:
            sf.write(temp, y, out_rate, subtype=SUBTYPES[bits], format=container)
            made = {"container": container.lower(), "bits": bits, **({"float": True} if bits == 32 else {})}
        else:
            kbps = bitrate or BITRATES[kind]
            with tempfile.TemporaryDirectory() as scratch:
                pcm = Path(scratch) / "mix.wav"
                sf.write(pcm, y, out_rate, subtype="PCM_24")
                encoder = encode(pcm, Path(temp), kbps)
            codec = "aac" if kind == ".m4a" else "mp3"
            made = {"container": kind[1:], "codec": codec, "bitrate_kbps": kbps, "encoder": encoder}
        os.replace(temp, to)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)

    # Where in the song the file is, which a render named outright gives whole.
    covered = {"whole_song": whole, "start_seconds": first / rate, "end_seconds": last / rate}
    if render is None:
        covered.update(start_beat=float(a), end_beat=float(b), start=place(a, song), end=place(b, song))
    warnings = []
    if bits != 32 and level["peak_dbfs"] is not None and level["peak_dbfs"] >= 0:
        warnings.append(
            f"The file's peak reached {level['peak_dbfs']:+.2f} dBFS after resampling and was clipped "
            "at full scale; --peak -1 or a 32-bit file leaves room"
        )
    if kind in ENCODERS and level["estimated_true_peak_dbtp"] > -0.99:
        warnings.append(
            f"True peak {level['estimated_true_peak_dbtp']:.2f} dBTP before encoding: a decoder "
            "can clip what a lossy encoder overshoots; --peak -1 leaves room by turning the file "
            "down, and a master limiter with true_peak: true and ceiling_db: -1 holds it in the song"
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
        "sample_rate": out_rate,
        **({"resampled_from": rate} if out_rate != rate else {}),
        "channels": x.shape[1],
        "duration_seconds": len(y) / out_rate,
        "range": covered,
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
