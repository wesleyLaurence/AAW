"""The process that hears the notes of a part: runs Spotify's Basic Pitch over one
file in the separation environment `daw separate setup` makes, and prints them.

Nothing here imports agent_daw: this file runs under the separation
environment's Python, not the checkout's. `daw transcribe` starts it as

    python transcribe_runner.py --source FILE

and reads one line of JSON: {"notes": [[start, end, pitch, confidence,
bend_low, bend_high], ...], "version": ..., "device": ..., "seconds": ...}.
start and end are seconds, pitch a MIDI number, confidence the model's note
activation over the note, its mean from 0 to 1 (its onset activation was tried
and is low on a bass's real notes too), and the bends the lowest and highest
pitch over the note against its middle, in cents: how far it slides or wavers.
The model's contour has three bins a semitone, so a bend is a multiple of 33
cents; its middle is left out, since on a part in tune it sits a bin off as
often as on one. Everything the notes mean for the song is worked out in
transcription.py, so it can be tested without the model; an error is
{"error": TEXT} and a nonzero exit status. A stand-in with the same arguments
and line serves the tests (tests/stand_in_transcriber.py).

Basic Pitch is installed without its dependencies, which would put TensorFlow
and an older numpy in the environment: it runs its ONNX model through the
environment's onnxruntime, on the processor. Its pitch bends call
scipy.signal.gaussian, which newer scipy keeps under scipy.signal.windows.
"""

import argparse
import contextlib
import json
import logging
import sys
import time

# Thresholds as Basic Pitch's own defaults; the command filters after.
ONSET = 0.5
FRAME = 0.3
# The shortest note in milliseconds: five frames. Basic Pitch's default, 127.7,
# drops a staccato sixteenth at 117 BPM.
SHORTEST_MS = 58
# Pitch bend bins a semitone, as Basic Pitch measures them.
BINS_PER_SEMITONE = 3


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    args = parser.parse_args()
    started = time.monotonic()
    try:
        logging.disable(logging.WARNING)
        import scipy.signal
        import scipy.signal.windows

        if not hasattr(scipy.signal, "gaussian"):
            scipy.signal.gaussian = scipy.signal.windows.gaussian
        from importlib.metadata import version
        from pathlib import Path

        from basic_pitch import ICASSP_2022_MODEL_PATH
        from basic_pitch.inference import Model, predict

        model = Model(Path(ICASSP_2022_MODEL_PATH).parent / "nmp.onnx")
        # Basic Pitch says what it is doing on stdout, where the one line goes.
        with contextlib.redirect_stdout(sys.stderr):
            _, _, events = predict(
                args.source,
                model,
                onset_threshold=ONSET,
                frame_threshold=FRAME,
                minimum_note_length=SHORTEST_MS,
            )
        notes = []
        for start, end, pitch, amplitude, bends in sorted(events, key=lambda e: (e[0], e[2])):
            bins = sorted(bends or [0])
            middle = bins[len(bins) // 2]
            cents = [(b - middle) * 100 / BINS_PER_SEMITONE for b in bins]
            notes.append(
                [
                    round(float(start), 4),
                    round(float(end), 4),
                    int(pitch),
                    round(float(amplitude), 3),
                    round(min(cents)),
                    round(max(cents)),
                ]
            )
        print(
            json.dumps(
                {
                    "notes": notes,
                    "version": version("basic-pitch"),
                    "device": "cpu",
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
