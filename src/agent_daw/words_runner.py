"""The process that hears the words of a voice: runs Whisper through Apple's MLX
over one recording in the separation environment `daw separate setup` makes,
and prints what it heard.

Nothing here imports agent_daw: this file runs under the separation
environment's Python, not the checkout's. `daw transcribe --words` starts it as

    python words_runner.py --audio FILE.npy --model FOLDER [--spans S,E,S,E]

FILE.npy is the voice as 16 kHz mono float32, decoded by the command, so no
ffmpeg is needed here. FOLDER holds the model's config.json and weights.
--spans reads only those stretches, in seconds, each cut out and read alone,
as a second reading of what is in doubt does; --language "" lets Whisper tell. It prints one line of JSON:

    {"segments": [{"span", "start", "end", "text", "avg_logprob", "no_speech_prob",
      "compression_ratio", "words": [[start, end, word, probability], ...]}],
     "version": ..., "seconds": ...}

span is the index of the span a segment was heard in, 0 for the whole.
Times are seconds in the whole recording. Everything the words mean for the
song is worked out in words.py, so it can be tested without the model; an error
is {"error": TEXT} and a nonzero exit status. A stand-in with the same
arguments and line serves the tests (tests/stand_in_words.py).

The options are the ones the clean-edit sessions settled on (2026-11-08):
each window read on its own (condition_on_previous_text off), which stopped
the model looping a hook it had heard, and a stretch of silence over two
seconds around a word it is unsure of skipped, where it would invent one.
"""

import argparse
import contextlib
import json
import sys
import time

SAMPLE_RATE = 16000
SEED = 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--audio", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--spans", default="")
    parser.add_argument("--language", default="")
    args = parser.parse_args()
    started = time.monotonic()
    try:
        from importlib.metadata import version

        import mlx.core as mx
        import mlx_whisper
        import numpy as np

        audio = np.load(args.audio).astype(np.float32)
        values = [float(v) for v in args.spans.split(",") if v.strip()]
        # Each span is cut out and read alone: Whisper's own clip_timestamps ran on
        # past a clip's end when clips overlapped (2026-11-16).
        spans = list(zip(values[0::2], values[1::2])) or [(0.0, len(audio) / SAMPLE_RATE)]
        segments = []
        for index, (start, end) in enumerate(spans):
            piece = audio[int(start * SAMPLE_RATE) : int(end * SAMPLE_RATE)]
            if len(piece) < SAMPLE_RATE // 10:
                continue
            # Where a window's reading loops, Whisper reads it again sampling at a
            # higher temperature; seeded, the same voice gives the same words.
            mx.random.seed(SEED)
            with contextlib.redirect_stdout(sys.stderr):
                result = mlx_whisper.transcribe(
                    piece,
                    path_or_hf_repo=args.model,
                    language=args.language or None,
                    word_timestamps=True,
                    condition_on_previous_text=False,
                    hallucination_silence_threshold=2,
                    verbose=None,
                )
            for s in result.get("segments", []):
                segments.append(
                    {
                        "span": index,
                        "start": round(float(s["start"]) + start, 3),
                        "end": round(float(s["end"]) + start, 3),
                        "text": s.get("text", "").strip(),
                        "avg_logprob": round(float(s.get("avg_logprob", 0.0)), 4),
                        "no_speech_prob": round(float(s.get("no_speech_prob", 0.0)), 4),
                        "compression_ratio": round(float(s.get("compression_ratio", 0.0)), 3),
                        "words": [
                            [
                                round(float(w["start"]) + start, 3),
                                round(float(w["end"]) + start, 3),
                                w["word"],
                                round(float(w["probability"]), 4),
                            ]
                            for w in s.get("words", [])
                        ],
                    }
                )
        print(
            json.dumps(
                {
                    "segments": segments,
                    "version": version("mlx-whisper"),
                    "seconds": round(time.monotonic() - started, 2),
                    "audio_seconds": round(len(audio) / SAMPLE_RATE, 2),
                }
            )
        )
        return 0
    except Exception as exc:  # noqa: BLE001 - the command reads the reason
        print(json.dumps({"error": f"{type(exc).__name__}: {exc}"}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
