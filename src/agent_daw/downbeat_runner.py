"""The process that hears where a song's beats and downbeats are: runs Beat This!
over one file in the separation environment `daw separate setup` makes, and
prints the model's frames.

Nothing here imports agent_daw: this file runs under the separation
environment's Python, not the checkout's. `daw samples beats` starts it as

    python downbeat_runner.py --source FILE --checkpoint CKPT

and reads one line of JSON: {"fps": 50, "beat": [...], "downbeat": [...],
"device": ..., "seconds": ...}, where beat and downbeat are the model's logits
for each frame, how likely a beat and a downbeat start there (above 0 is more
likely than not). What they mean for the song is worked out in downbeats.py, so
it can be tested without the model; an error is {"error": TEXT} and a nonzero
exit status. A stand-in with the same arguments and line serves the tests
(tests/stand_in_downbeats.py).

The model is given the file's mono sum, which it resamples to 22.05 kHz. It runs
on the GPU where torch has one (about 3 s for a 4-minute song, start-up and all,
against 5 s on the processor, with the same frames), or on the device
AAW_DOWNBEAT_DEVICE names.
"""

import argparse
import json
import os
import sys
import time


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    parser.add_argument("--checkpoint", required=True)
    args = parser.parse_args()
    started = time.monotonic()
    try:
        import soundfile as sf
        import torch
        from beat_this.inference import Audio2Frames

        x, sr = sf.read(args.source, dtype="float32", always_2d=True)
        device = os.environ.get("AAW_DOWNBEAT_DEVICE") or ("mps" if torch.backends.mps.is_available() else "cpu")
        frames = Audio2Frames(args.checkpoint, device=device)
        with torch.no_grad():
            beat, downbeat = frames(x.mean(axis=1), sr)
        print(
            json.dumps(
                {
                    "fps": 50,
                    "beat": [round(float(v), 2) for v in beat.cpu().numpy()],
                    "downbeat": [round(float(v), 2) for v in downbeat.cpu().numpy()],
                    "device": device,
                    "seconds": round(time.monotonic() - started, 2),
                }
            )
        )
        return 0
    except Exception as exc:  # the caller reads the error, whatever it was
        print(json.dumps({"error": f"{type(exc).__name__}: {exc}"}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
