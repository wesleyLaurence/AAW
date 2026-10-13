"""A separator that stands in for the model, for the tests: the runner's
arguments and lines (agent_daw/separate_runner.py), with the parts cut from
the source by frequency band, so that they sum to it exactly.

bass is everything under 250 Hz, vocals 1 to 8 kHz, drums 8 kHz and up,
guitar 250 to 500 Hz, piano 500 Hz to 1 kHz; what no asked-for part takes goes
to other, or else to instrumental, or else to the last part asked for. The
tests set AAW_SEPARATOR to run this in place of the separation environment.
"""

import argparse
import json
import math
import sys
import time
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

RATE = 44100
BANDS = {
    "bass": (0, 250),
    "guitar": (250, 500),
    "piano": (500, 1000),
    "vocals": (1000, 8000),
    "drums": (8000, math.inf),
}


def say(**fields):
    print(json.dumps(fields), flush=True)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--models", type=Path, required=True)
    p.add_argument("--model-file", required=True)
    p.add_argument("--out-parts", required=True)
    a = p.parse_args()
    wanted = json.loads(a.out_parts)
    started = time.monotonic()
    if a.model_file == "fail.yaml":
        say(error="The stand-in was asked to fail")
        sys.exit(1)
    x, rate = sf.read(a.source, always_2d=True, dtype="float64")
    original_rate = rate
    if rate != RATE:
        x = resample_poly(x, RATE, rate, axis=0)
    frames, channels = x.shape
    say(stage="loading", model=a.model_file, frames=frames, original_sample_rate=original_rate)
    spectrum = np.fft.rfft(x, axis=0)
    freqs = np.fft.rfftfreq(frames, 1 / RATE)
    parts = [spec["part"] for spec in wanted.values()]
    rest = "other" if "other" in parts else "instrumental" if "instrumental" in parts else parts[-1]
    owner = np.full(len(freqs), parts.index(rest))
    for name, (lo, hi) in BANDS.items():
        if name in parts and name != rest:
            owner[(freqs >= lo) & (freqs < hi)] = parts.index(name)
    say(stage="separating", device="cpu", seconds_loading=0.0)
    made, total = {}, np.zeros_like(x)
    for stem, spec in wanted.items():
        mask = (owner == parts.index(spec["part"]))[:, None]
        part = np.fft.irfft(spectrum * mask, n=frames, axis=0)
        total += part
        target = Path(spec["file"])
        target.parent.mkdir(parents=True, exist_ok=True)
        sf.write(target, part.astype(np.float32), RATE, subtype="FLOAT")
        peak = float(np.abs(part).max())
        made[spec["part"]] = {"file": str(target), "peak_dbfs": round(20 * math.log10(peak), 2) if peak > 0 else -200}
        say(stage="written", part=spec["part"], file=str(target))
    power = float(np.mean(x * x))
    residual = float(np.mean((x - total) ** 2))
    say(
        done=True,
        parts=made,
        frames=frames,
        sample_rate=RATE,
        channels=channels,
        original_sample_rate=original_rate,
        input_gain_db=0.0,
        residual_db=round(10 * math.log10(residual / power), 2) if residual > 0 and power > 0 else -200.0,
        seconds={"loading": 0.0, "separating": round(time.monotonic() - started, 3), "total": round(time.monotonic() - started, 3)},
        max_rss_bytes=100_000_000,
        runtime={"audio_separator": "stand-in", "torch": "none", "device": "cpu", "python": sys.version.split()[0], "machine": "test"},
    )


if __name__ == "__main__":
    main()
