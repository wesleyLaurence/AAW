"""A downbeat model that stands in for Beat This!, for the tests: the runner's
arguments and line (agent_daw/downbeat_runner.py), with frames made from what a
test says the model hears.

The test writes FILE.heard.json beside the audio: {"beats": [seconds...],
"downbeats": [seconds...]}, or {"fail": "words"} for a model that fails. Each
beat and downbeat is a logit of 6 on its nearest frame, 2 on the frames either
side and -6 elsewhere, 50 frames a second over the file's length. The tests
set AAW_DOWNBEATS to run this in place of the separation environment.
"""

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import soundfile as sf

FPS = 50


def frames(times, count):
    x = np.full(count, -6.0)
    for t in times:
        i = int(round(t * FPS))
        if 0 <= i < count:
            x[max(0, i - 1) : i + 2] = np.maximum(x[max(0, i - 1) : i + 2], 2.0)
            x[i] = 6.0
    return [round(float(v), 2) for v in x]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--checkpoint", required=True)
    a = p.parse_args()
    heard = json.loads(a.source.with_name(a.source.name + ".heard.json").read_text())
    if "fail" in heard:
        print(json.dumps({"error": heard["fail"]}))
        return 1
    count = int(sf.info(a.source).duration * FPS) + 1
    print(
        json.dumps(
            {
                "fps": FPS,
                "beat": frames(heard["beats"], count),
                "downbeat": frames(heard["downbeats"], count),
                "device": "stand-in",
                "seconds": 0.0,
            }
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
