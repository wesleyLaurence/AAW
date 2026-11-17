"""A Whisper that stands in for the real one, for the tests: the runner's arguments
and line (agent_daw/words_runner.py), with the words a test says each model hears.

AAW_WORDS_SCRIPT names a JSON file: {MODEL: [segment, ...]}, MODEL the model
folder's name (whisper-large-v3-turbo, whisper-large-v3), each segment
{"text", "words": [[start, end, word, probability], ...]} in the whole voice's
seconds, or {"fail": TEXT} for a model that fails. Over --spans, each span gives
the words whose middle is in it, as the runner reads a clip cut out. Each call
is written as a line of JSON to the script's file with .calls added.
"""

import argparse
import json
import os
import sys
from pathlib import Path


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--audio", required=True)
    p.add_argument("--model", type=Path, required=True)
    p.add_argument("--spans", default="")
    p.add_argument("--language", default="")
    a = p.parse_args()
    script = Path(os.environ["AAW_WORDS_SCRIPT"])
    heard = json.loads(script.read_text())
    values = [float(v) for v in a.spans.split(",") if v.strip()]
    spans = list(zip(values[0::2], values[1::2]))
    with open(f"{script}.calls", "a") as calls:
        calls.write(json.dumps({"model": a.model.name, "spans": spans, "language": a.language}) + "\n")
    said = heard.get(a.model.name, [])
    if isinstance(said, dict) and "fail" in said:
        print(json.dumps({"error": said["fail"]}))
        return 1
    segments = []
    for index, (start, end) in enumerate(spans or [(float("-inf"), float("inf"))]):
        for s in said:
            words = [w for w in s["words"] if start <= (w[0] + w[1]) / 2 <= end]
            if words:
                segments.append({"span": index, "start": words[0][0], "end": words[-1][1], "text": s["text"], "words": words})
    print(json.dumps({"segments": segments, "version": "stand-in", "seconds": 0.0}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
