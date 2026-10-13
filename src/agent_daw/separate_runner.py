"""The process that takes a song apart: runs one model of the manifest over one
file through audio-separator, in the separation environment `daw separate
setup` makes, and writes each part where it is told to.

Nothing here imports agent_daw: this file runs under the separation
environment's Python, not the checkout's. `daw separate` starts it as

    python separate_runner.py --source FILE --models DIR --model-file NAME
                              --out-parts JSON

where JSON maps each stem name the model gives to {"part": NAME, "file": PATH}.
Progress is a JSON object a line on stdout, the last one {"done": true, ...}
with the parts' paths and what was measured; an error is {"error": TEXT} and a
nonzero exit status. A stand-in with the same arguments and lines serves the
tests (tests/stand_in_separator.py).

The model is given the source at 44.1 kHz with its peak at -6 dBFS, since
audio-separator scales any stem that passes full scale down to it and would
lose the parts' level against the source; each part is scaled back by the same
gain and cut or padded to the source's length, so the parts line up with the
source and sum against it. A 48 kHz source is resampled to 44.1 kHz once, here,
and its parts stay at 44.1 kHz.
"""

import argparse
import json
import logging
import os
import platform
import resource
import sys
import tempfile
import time
from importlib.metadata import version
from pathlib import Path

import numpy as np
import soundfile as sf

RATE = 44100
# The peak the model is given, under which no stem of a mixed song passes full scale.
INPUT_PEAK = 0.5


def say(**fields):
    print(json.dumps(fields), flush=True)


def fail(message):
    print(json.dumps({"error": message}), flush=True)
    sys.exit(1)


def read_source(path: Path):
    """The file as frames by channels of float64 at 44.1 kHz, and its channel count."""
    x, rate = sf.read(path, always_2d=True, dtype="float64")
    if not len(x):
        fail(f"{path.name} holds no audio")
    if x.shape[1] > 2:
        fail(f"{path.name} has {x.shape[1]} channels; a song is mono or stereo")
    if rate != RATE:
        import librosa

        x = librosa.resample(x.T, orig_sr=rate, target_sr=RATE, res_type="soxr_hq").T
    return np.ascontiguousarray(x), rate


def write_atomic(path: Path, x, rate):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=path.parent, prefix=".", suffix=".wav")
    os.close(fd)
    try:
        sf.write(temp, x, rate, subtype="FLOAT")
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def fit(x, frames, channels):
    """`x` cut or padded to `frames`, with the source's channels."""
    if x.ndim == 1:
        x = x[:, None]
    if x.shape[1] != channels:
        x = np.repeat(x[:, :1], channels, axis=1) if x.shape[1] == 1 else x.mean(axis=1, keepdims=True)
    if len(x) >= frames:
        return x[:frames]
    return np.vstack([x, np.zeros((frames - len(x), channels))])


def db(power, reference):
    if power <= 0:
        return -200.0
    return round(float(10 * np.log10(power / reference)), 2)


def long_convolutions_on_cpu(torch):
    """Runs a one-dimensional convolution on the CPU when the GPU refuses it.

    PyTorch's Metal backend (2.14) refuses a conv1d or conv_transpose1d whose
    output is longer than 65536 frames, which Demucs's first encoder and last
    decoder layer are on a segment of 7.8 s at 44.1 kHz: "Output channels >
    65536 not supported at the MPS device". Those two layers have few channels,
    so they run on the CPU and the rest of the network stays on the GPU.
    """
    import torch.nn.functional as F

    def on_cpu_when_refused(op):
        def run(input, weight, bias=None, *args, **kwargs):
            try:
                return op(input, weight, bias, *args, **kwargs)
            except NotImplementedError:
                if input.device.type != "mps":
                    raise
                moved = op(input.cpu(), weight.cpu(), None if bias is None else bias.cpu(), *args, **kwargs)
                return moved.to(input.device)

        return run

    F.conv1d = on_cpu_when_refused(F.conv1d)
    F.conv_transpose1d = on_cpu_when_refused(F.conv_transpose1d)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--models", type=Path, required=True, help="The folder of downloaded weights")
    p.add_argument("--model-file", required=True, help="audio-separator's name for the model")
    p.add_argument("--out-parts", required=True, help='{"Vocals": {"part": "vocals", "file": PATH}, ...}')
    a = p.parse_args()
    wanted = json.loads(a.out_parts)
    started = time.monotonic()

    if platform.machine() == "arm64":
        # audio-separator puts torch on the GPU only when platform.uname().processor
        # says "arm", and that answer comes from `uname -p`, which says i386 in a
        # process started by an Intel Python under Rosetta, as the checkout's is.
        platform._Processor.get = staticmethod(lambda: "arm")
    from audio_separator.separator import Separator
    import torch

    long_convolutions_on_cpu(torch)
    source, original_rate = read_source(a.source)
    frames, channels = source.shape
    peak = float(np.abs(source).max())
    gain = INPUT_PEAK / peak if peak > 0 else 1.0
    say(stage="loading", model=a.model_file, frames=frames, original_sample_rate=original_rate)
    with tempfile.TemporaryDirectory(prefix="separate-") as scratch:
        scratch = Path(scratch)
        given = scratch / "input.wav"
        # Stereo in, so that every family runs the same path; a mono source gets mono parts.
        sf.write(given, np.repeat(source, 2, axis=1) * gain if channels == 1 else source * gain, RATE, subtype="FLOAT")
        separator = Separator(
            log_level=logging.WARNING,
            model_file_dir=str(a.models),
            output_dir=str(scratch),
            output_format="WAV",
            normalization_threshold=1.0,
            sample_rate=RATE,
            use_soundfile=True,
        )
        separator.load_model(model_filename=a.model_file)
        loaded = time.monotonic()
        device = "mps" if separator.torch_device_mps is not None else "cpu"
        say(stage="separating", device=device, seconds_loading=round(loaded - started, 1))
        names = {stem: f"part-{spec['part']}" for stem, spec in wanted.items()}
        written = separator.separate(str(given), custom_output_names=names)
        separated = time.monotonic()
        written = {Path(f).stem: Path(f) if Path(f).is_absolute() else scratch / f for f in written}
        parts, total = {}, np.zeros_like(source)
        for stem, spec in wanted.items():
            made = written.get(f"part-{spec['part']}")
            if made is None or not made.exists():
                fail(f"The model gave no {stem} part; it wrote {sorted(written)}")
            x = sf.read(made, always_2d=True, dtype="float64")[0] / gain
            x = fit(x, frames, channels)
            total += x
            target = Path(spec["file"])
            write_atomic(target, x.astype(np.float32), RATE)
            parts[spec["part"]] = {
                "file": str(target),
                "peak_dbfs": db(float(np.abs(x).max()) ** 2, 1.0),
            }
            say(stage="written", part=spec["part"], file=str(target))
    source_power = float(np.mean(source * source))
    residual = float(np.mean((source - total) ** 2))
    say(
        done=True,
        parts=parts,
        frames=frames,
        sample_rate=RATE,
        channels=channels,
        original_sample_rate=original_rate,
        input_gain_db=round(float(20 * np.log10(gain)), 2),
        residual_db=db(residual, source_power) if source_power > 0 else None,
        seconds={
            "loading": round(loaded - started, 1),
            "separating": round(separated - loaded, 1),
            "total": round(time.monotonic() - started, 1),
        },
        max_rss_bytes=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        runtime={
            "audio_separator": version("audio-separator"),
            "torch": torch.__version__,
            "onnxruntime": version("onnxruntime"),
            "python": platform.python_version(),
            "machine": platform.machine(),
            "device": device,
        },
    )


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except BaseException as exc:  # noqa: BLE001 - everything the runner cannot do is one line to daw
        fail(f"{type(exc).__name__}: {exc}")
