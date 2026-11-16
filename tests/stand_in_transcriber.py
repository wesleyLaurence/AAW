"""A note model that stands in for Basic Pitch, for the tests: the runner's
arguments and line (agent_daw/transcribe_runner.py), with the notes a test says
the model hears.

The test writes FILE.notes.json beside the audio: {"notes": [[start, end, pitch,
confidence, bend_low, bend_high], ...]}, or {"fail": "words"} for a model that
fails. The tests set AAW_TRANSCRIBER to run this in place of the separation
environment.
"""

import argparse
import json
import sys
from pathlib import Path


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    a = p.parse_args()
    heard = json.loads(a.source.with_name(a.source.name + ".notes.json").read_text())
    if "fail" in heard:
        print(json.dumps({"error": heard["fail"]}))
        return 1
    print(json.dumps({"notes": heard["notes"], "version": "stand-in", "device": "stand-in", "seconds": 0.0}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
