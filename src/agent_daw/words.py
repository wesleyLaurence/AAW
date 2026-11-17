"""Words by bar: `daw transcribe --words`.

The words a voice sings, each with its time, bar and beat on the song's beat
map, as lines. OpenAI's Whisper hears them, run through Apple's MLX on the
Mac's GPU in the separation environment `daw separate setup` makes
(words_runner.py); everything after what it heard is here, so it can be tested
with words made up (AAW_WORDS names a stand-in).

1. The voice: the vocals part `daw separate` made of the song when there is one
   beside it, else the file given, decoded here to 16 kHz mono.
2. Whisper large-v3-turbo reads the whole voice, each 30-second window on its
   own, as the clean-edit sessions settled on (2026-11-08).
3. What is in doubt (`doubts`): a stretch of two seconds or more with no words
   where the voice is loud, which turbo skips (18 s of one rap record's intro,
   9 s of another's second verse); and a line whose words it is unsure of.
4. Each is read again by the larger Whisper large-v3 on clips of 6 to 10
   seconds cut around it (`clips`), never over the whole song, where it writes
   verses that are not there. The second reading replaces the first where the
   first heard nothing or was less sure (`merged`).
5. The words are lines (`lines`), placed on the beat map; the lines that come
   back are `repeats`, the commonest the hook.

What was heard is kept beside the voice as NAME.words.json, so a second call,
and `daw samples form`, which names the chorus by the words that come back with
its music, read it without the model.

Whisper writes down the lead voice; backing voices and ad-libs under it are
mostly missing. Every word is an estimate.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import tempfile
from difflib import SequenceMatcher
from math import gcd
from pathlib import Path

import numpy as np

from . import beats
from .hits import Grid

FIRST = "whisper-large-v3-turbo"  # reads the whole voice
AGAIN = "whisper-large-v3"  # reads the doubtful stretches again
SAMPLE_RATE = 16000
VERSION = 3  # what is read again; a kept reading of another version is read again
TIMEOUT = 1200
FRAME = 0.05  # seconds a level is read over
LOUD_DB = -20.0  # the voice this near its loud frames (99th percentile) is singing
SKIP_SECONDS = 2.0  # a stretch with no words this long ...
LOUD_SECONDS = 1.0  # ... in which the voice is loud this long was skipped
UNSURE = 0.65  # a line whose words' mean probability is under this is in doubt
PAD = 1.5  # a clip read again runs this far either side of what is in doubt
CLIP_LEAST = 6.0  # and is at least this long
CLIP_MOST = 10.0  # and at most this
LINE_GAP = 0.5  # a pause this long ends a line
LONG_LINE = 2.5  # and a line this long ends at a comma
MOST_LINE = 6.0  # a line longer than this is split at its longest pause
LONGEST_WORD = 2.0  # a word Whisper stretches longer ends this long after its start
SAME_LINE = 0.8  # lines whose words are this alike are the same line
LEAST_TOKENS = 3  # a line of fewer words is not counted as coming back
SURE = 0.5  # a word this probable is counted sure
LEAD_ONLY = (
    "Whisper writes down the lead voice: backing vocals, doubles and ad-libs under it are mostly missing."
)


# --- running the model ------------------------------------------------------------


def folder(name: str) -> Path:
    from . import separation

    return separation.models_dir() / name


def model_here(name: str) -> bool:
    """Whether a model's files are downloaded, by their sizes: hashing gigabytes on
    every call would take longer than the reading (setup checks the hashes). With a
    stand-in, every model but the ones AAW_WORDS_WITHOUT names."""
    from . import separation

    if os.environ.get("AAW_WORDS"):
        return name not in os.environ.get("AAW_WORDS_WITHOUT", "").split(",")
    spec = next((m for m in separation.analysis() if m["name"] == name), None)
    if spec is None:
        return False
    for f in spec["files"]:
        path = separation.models_dir() / f["name"]
        if not path.is_file() or path.stat().st_size != f["bytes"]:
            return False
    return True


def unavailable() -> str | None:
    """Why the words cannot be heard on this Mac, or None when they can."""
    if os.environ.get("AAW_WORDS"):
        return None
    from . import separation

    if not separation.env_python().is_file():
        return "The separation environment is not installed; daw separate setup installs it with Whisper"
    if not separation.package_here("mlx-whisper"):
        return "The separation environment lacks mlx-whisper; daw separate setup adds it"
    if not model_here(FIRST):
        return f"Whisper ({FIRST}) is not downloaded; daw separate setup --models {FIRST} gets it"
    return None


def run(audio: Path, model: str, spans: list[tuple[float, float]] | None, language: str) -> dict:
    """What a model hears in the voice at `audio`, as words_runner.py prints it,
    over the whole of it or over `spans` only; `language` "" for Whisper to tell."""
    from . import separation

    value = os.environ.get("AAW_WORDS")
    args = ["--audio", str(audio), "--model", str(folder(model)), "--language", language]
    if spans:
        args += ["--spans", ",".join(f"{a:.3f},{b:.3f}" for a, b in spans)]
    if value:
        command = [*shlex.split(value), *args]
    else:
        runner = Path(__file__).with_name("words_runner.py")
        command = [str(separation.env_python()), "-I", str(runner), *args]
    try:
        out = subprocess.run(command, capture_output=True, text=True, timeout=TIMEOUT)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ValueError(f"Whisper did not run: {exc}") from None
    lines = [line for line in out.stdout.splitlines() if line.startswith("{")]
    try:
        result = json.loads(lines[-1])
    except (IndexError, json.JSONDecodeError):
        raise ValueError(f"Whisper said nothing readable: {out.stderr.strip()[-500:]}") from None
    if out.returncode or "error" in result:
        raise ValueError(f"Whisper failed: {result.get('error') or out.stderr.strip()[-500:]}")
    return result


def voice_audio(mono: np.ndarray, sr: int) -> np.ndarray:
    """The voice at Whisper's 16 kHz."""
    from scipy.signal import resample_poly

    g = gcd(SAMPLE_RATE, sr)
    return resample_poly(mono, SAMPLE_RATE // g, sr // g).astype(np.float32)


# --- what is in doubt ----------------------------------------------------------------


def words_of(segments: list[dict], reading: str) -> list[dict]:
    """A reading's words in time order, each knowing its reading and the span it
    was heard in. Whisper can stretch a word's end over the stretch after it it did
    not read (a rap record's word from 80.3 to 92.3 s, over 11 s of voice),
    so a word ends at most LONGEST_WORD after its start."""
    out = []
    for n, s in enumerate(segments):
        for start, end, word, p in s.get("words", []):
            start, end = float(start), float(end)
            out.append(
                {
                    "word": word,
                    "start": start,
                    "end": min(end, start + LONGEST_WORD),
                    "p": float(p),
                    "reading": reading,
                    "span": int(s.get("span", 0)),
                    "seg": f"{reading}{n}",
                }
            )
    out.sort(key=lambda w: w["start"])
    return out


def levels(audio: np.ndarray) -> np.ndarray:
    """The voice's level in dB over each FRAME, against its loud frames', the 99th
    percentile: a voice that sings a twentieth of the song is still heard by it."""
    n = int(FRAME * SAMPLE_RATE)
    count = max(1, len(audio) // n)
    x = audio[: count * n].astype(np.float64).reshape(count, n)
    db = 10 * np.log10(np.mean(x * x, axis=1) + 1e-12)
    return db - float(np.percentile(db, 99))


def lines(words: list[dict]) -> list[list[dict]]:
    """The words as lines: a new one where Whisper starts a segment, after a pause
    of LINE_GAP, after a word that ends a sentence and after a comma once the line
    is LONG_LINE long. Whisper sometimes leaves a whole verse unpunctuated in one
    segment, so a line still longer than MOST_LINE is split."""
    out = []
    for w in words:
        if out:
            line, prev = out[-1], out[-1][-1]
            ending = prev["word"].rstrip()
            ends = (
                w["seg"] != prev["seg"]
                or w["start"] - prev["end"] >= LINE_GAP
                or ending.endswith((".", "!", "?"))
                or (ending.endswith(",") and prev["end"] - line[0]["start"] >= LONG_LINE)
            )
            if not ends:
                line.append(w)
                continue
        out.append([w])
    return [piece for line in out for piece in shortened(line)]


def shortened(line: list[dict]) -> list[list[dict]]:
    """A line longer than MOST_LINE split, until none is, where both parts are a
    third of MOST_LINE long or more: at its longest pause there, or, the pauses
    alike, nearest its middle."""
    start, end = line[0]["start"], line[-1]["end"]
    if len(line) < 2 or end - start <= MOST_LINE:
        return [line]
    least = MOST_LINE / 3
    middle = (start + end) / 2
    cuts = [i for i in range(1, len(line)) if line[i]["start"] - start >= least and end - line[i - 1]["end"] >= least]
    if not cuts:
        return [line]
    cut = max(cuts, key=lambda i: (round(line[i]["start"] - line[i - 1]["end"], 1), -abs(line[i]["start"] - middle)))
    return shortened(line[:cut]) + shortened(line[cut:])


def doubts(words: list[dict], level_db: np.ndarray | None, duration: float) -> list[dict]:
    """The stretches to read again: the ones of SKIP_SECONDS or more with no words in
    which the voice is loud for LOUD_SECONDS, and the unsure lines; those that touch
    are one. Without a voice's levels (a whole mix, loud with the band where no one
    sings), the unsure lines only."""
    found = []
    edges = [0.0, *[x for w in words for x in (w["start"], w["end"])], duration]
    for a, b in zip(edges[0::2], edges[1::2]) if level_db is not None else ():
        if b - a < SKIP_SECONDS:
            continue
        frames = level_db[int(a / FRAME) : int(b / FRAME)]
        loud = np.flatnonzero(frames >= LOUD_DB)
        if len(loud) * FRAME >= LOUD_SECONDS:
            first, last = a + loud[0] * FRAME, a + (loud[-1] + 1) * FRAME
            found.append({"from": a, "to": b, "loud": (first, min(last, b)), "why": "loud voice with no words"})
    for line in lines(words):
        p = float(np.mean([w["p"] for w in line]))
        if p < UNSURE:
            found.append({"from": line[0]["start"], "to": line[-1]["end"], "why": f"unsure, {p:.2f}"})
    found.sort(key=lambda d: d["from"])
    joined = []
    for d in found:
        if joined and d["from"] <= joined[-1]["to"] + 0.05:
            last = joined[-1]
            last["to"] = max(last["to"], d["to"])
            last["why"] = last["why"] if d["why"] in last["why"] else f"{last['why']}; {d['why']}"
            if "loud" in d:
                last["loud"] = (min(last.get("loud", d["loud"])[0], d["loud"][0]), max(last.get("loud", d["loud"])[1], d["loud"][1]))
        else:
            joined.append(dict(d))
    return [
        {"from": round(d["from"], 3), "to": round(d["to"], 3), "why": d["why"], "clips": clips(d, duration)}
        for d in joined
    ]


def clips(doubt: dict, duration: float) -> list[dict]:
    """The clips one stretch is read again on: what is in doubt (of a skip, its loud
    part) in pieces short enough that each, padded PAD either side, is at most
    CLIP_MOST seconds, and widened about its middle to CLIP_LEAST. Each gives only
    the words in its own piece, its `share`, so where clips overlap a word is
    heard once; the first and last shares reach the stretch's ends."""
    a, b = doubt.get("loud") or (doubt["from"], doubt["to"])
    inner = CLIP_MOST - 2 * PAD
    count = max(1, int(np.ceil((b - a) / inner)))
    out = []
    for i in range(count):
        lo, hi = a + (b - a) * i / count, a + (b - a) * (i + 1) / count
        s, e = lo - PAD, hi + PAD
        if e - s < CLIP_LEAST:
            middle = (s + e) / 2
            s, e = middle - CLIP_LEAST / 2, middle + CLIP_LEAST / 2
        s, e = max(0.0, s), min(duration, e)
        share = [doubt["from"] if i == 0 else lo, doubt["to"] if i == count - 1 else hi]
        out.append({"seconds": [round(s, 3), round(e, 3)], "share": [round(share[0], 3), round(share[1], 3)]})
    return out


def merged(first: list[dict], again: list[dict], regions: list[dict]) -> tuple[list[dict], list[dict]]:
    """The first reading with each doubtful stretch read again where the second
    reading heard words the first did not, or was surer of them; and what was
    done in each."""
    shares = [c["share"] for r in regions for c in r["clips"]]
    again = [w for w in again if w["span"] < len(shares) and shares[w["span"]][0] <= (w["start"] + w["end"]) / 2 <= shares[w["span"]][1]]
    words = list(first)
    done = []
    for r in regions:
        def inside(w):
            return r["from"] <= (w["start"] + w["end"]) / 2 <= r["to"]

        was = [w for w in words if inside(w)]
        now = [w for w in again if inside(w)]
        p_was = float(np.mean([w["p"] for w in was])) if was else 0.0
        p_now = float(np.mean([w["p"] for w in now])) if now else 0.0
        took = bool(now) and (not was or p_now > p_was)
        if took:
            words = [w for w in words if not inside(w)] + now
        done.append(
            {
                "seconds": r["from"],
                "until_seconds": r["to"],
                "why": r["why"],
                "clips": [c["seconds"] for c in r["clips"]],
                "kept": "again" if took else "first",
                "first": text(was),
                "again": text(now),
                "first_probability": round(p_was, 2),
                "again_probability": round(p_now, 2),
            }
        )
    words.sort(key=lambda w: w["start"])
    return words, done


def text(words: list[dict]) -> str:
    return re.sub(r"\s+", " ", "".join(w["word"] if w["word"][:1].isspace() else " " + w["word"] for w in words)).strip()


def tokens(value: str) -> list[str]:
    """The words of a text to compare, lower case without apostrophes, so the
    it's and its, doin' and doin, are one."""
    return re.findall(r"[a-z0-9]+", re.sub(r"['’]", "", value.lower()))


def alike(a: list[str], b: list[str]) -> float:
    return SequenceMatcher(None, a, b, autojunk=False).ratio() if a and b else 0.0


def repeats(rows: list[dict]) -> list[dict]:
    """The lines that come back, as groups of lines whose words are SAME_LINE alike:
    most often first."""
    out = []
    for g in groups_of(rows):
        texts = [rows[i]["text"] for i in g]
        common = max(set(texts), key=lambda s: (texts.count(s), -texts.index(s)))
        out.append(
            {
                "text": common,
                "count": len(g),
                "bars": [rows[i].get("bar") for i in g],
                "seconds": [rows[i]["seconds"] for i in g],
            }
        )
    out.sort(key=lambda g: (-g["count"], -len(tokens(g["text"])), g["seconds"][0]))
    return out


def groups_of(rows: list[dict]) -> list[list[int]]:
    """The indexes of the lines that come back, a list for each line: lines of
    LEAST_TOKENS words or more whose words are SAME_LINE alike."""
    groups = []
    for i, row in enumerate(rows):
        t = tokens(row["text"])
        if len(t) < LEAST_TOKENS:
            continue
        for g in groups:
            if alike(t, g["tokens"]) >= SAME_LINE:
                g["lines"].append(i)
                break
        else:
            groups.append({"tokens": t, "lines": [i]})
    return [g["lines"] for g in groups if len(g["lines"]) >= 2]


# --- what was heard, kept --------------------------------------------------------------


def sidecar(voice: Path) -> Path:
    return voice.with_name(f"{voice.stem}.words.json")


def saved(voice: Path, language: str | None) -> dict | None:
    """The readings kept beside the voice, while the voice and what reads it again
    are unchanged and the second model's absence is not now made good."""
    from .library import digest

    path = sidecar(voice)
    if not path.is_file():
        return None
    try:
        kept = json.loads(path.read_text())
    except (OSError, ValueError):
        return None
    if kept.get("version") != VERSION or kept.get("sha256") != digest(voice):
        return None
    if language is not None and kept.get("language") != language:
        return None
    if kept.get("again_model") is None and kept.get("regions") and model_here(AGAIN):
        return None
    return kept


def kept(voice: Path) -> dict | None:
    """The words kept beside a voice, merged, for `daw samples form`: its words and
    the file; None when none are kept or the voice has changed."""
    found = saved(voice, None)
    if found is None:
        return None
    words, _ = merged(words_of(found["first"], "f"), words_of(found.get("again", []), "a"), found.get("regions", []))
    return {"words": words, "file": str(sidecar(voice))}


def in_library(path: Path, db: Path) -> bool:
    from .library import connect

    con = connect(db)
    try:
        return con.execute("SELECT 1 FROM samples WHERE path=?", (str(path.resolve()),)).fetchone() is not None
    finally:
        con.close()


def hear(voice: Path, language: str, again_wanted: bool, fresh: bool, db: Path, is_part: bool = True) -> tuple[dict, bool]:
    """The readings of the voice: kept, or heard now and kept; and whether they were kept."""
    from .library import digest
    from .model import atomic_text

    if not fresh and (found := saved(voice, language)) is not None:
        # A reading kept with --once is read again for a call that wants the doubts read again.
        if not again_wanted or found.get("again_asked"):
            return found, True
    why = unavailable()
    if why:
        raise ValueError(why)
    mono, sr = beats.read(voice)
    audio = voice_audio(mono, sr)
    duration = len(audio) / SAMPLE_RATE
    with tempfile.TemporaryDirectory(prefix="aaw-words-") as scratch:
        npy = Path(scratch) / "voice.npy"
        np.save(npy, audio)
        first = run(npy, FIRST, None, language)
        level = levels(audio) if is_part else None
        regions = doubts(words_of(first["segments"], "f"), level, duration) if again_wanted else []
        again, again_model = None, None
        if regions and model_here(AGAIN):
            spans = [tuple(c["seconds"]) for r in regions for c in r["clips"]]
            again = run(npy, AGAIN, spans, language)
            again_model = {"name": AGAIN, "version": again.get("version"), "seconds": again.get("seconds")}
    found = {
        "version": VERSION,
        "sha256": digest(voice),
        "language": language,
        "again_asked": again_wanted,
        "first_model": {"name": FIRST, "version": first.get("version"), "seconds": first.get("seconds")},
        "again_model": again_model,
        "duration": round(duration, 3),
        "first": first["segments"],
        "regions": regions,
        "again": again["segments"] if again else [],
    }
    if not in_library(voice, db):
        atomic_text(sidecar(voice), json.dumps(found, indent=1) + "\n")
    return found, False



# --- the command -----------------------------------------------------------------------


def voice_of(path: Path) -> tuple[Path, Path, bool]:
    """The voice to read and the song whose beat map places it: the vocals part
    `daw separate` made of the song at `path` when there is one, or a part given
    itself with its song; and whether it is a part."""
    from .form import parts_of
    from .transcription import separated_from

    song = separated_from(path)
    if song is not None:
        return path, song, True
    parts = parts_of(path)
    if "vocals" in parts:
        return Path(parts["vocals"]).resolve(), path, True
    return path, path, False


def clock(seconds: float) -> str:
    m, s = divmod(max(0.0, seconds), 60)
    return f"{int(m)}:{s:04.1f}"


def lyrics_file(rows: list[dict], out: Path, about: str):
    """The lines as text, a line each with its time and bar, for a person to read."""
    body = [about, "Time is seconds into the song's file; bar is its beat map's.", ""]
    for r in rows:
        mark = "  (read again)" if r.get("read_again") else ""
        bar = f"bar {r['bar']:>3}  " if r.get("bar") is not None else ""
        body.append(f"{clock(r['seconds']):>7}  {bar}{r['text']}{mark}")
    out.write_text("\n".join(body) + "\n")


def transcribe_words(
    source: str,
    song: Path | None = None,
    beats_of: str | None = None,
    near: float | None = None,
    window: float = 10.0,
    lyrics_out: Path | None = None,
    language: str = "en",
    again: bool = True,
    fresh: bool = False,
    db: Path | None = None,
) -> dict:
    from . import library
    from .transcription import Played, resolve

    db = db or library.default_db()
    if lyrics_out is not None and not lyrics_out.resolve().parent.is_dir():
        raise ValueError(f"--lyrics: there is no folder {lyrics_out.resolve().parent}")
    path, _, clip = resolve(source, song, db)
    voice, whole, is_part = voice_of(path)
    of = library.resolve(db, beats_of) if beats_of else whole
    try:
        report = library.beat_map(db, of)
        problem = None
    except ValueError as error:
        report, problem = None, str(error)
    found, was_kept = hear(voice, language, again, fresh, db, is_part)
    first = words_of(found["first"], "f")
    words, done = merged(first, words_of(found.get("again", []), "a"), found.get("regions", []))
    played = None
    if clip is not None:
        mono, sr = beats.read(path)
        played = Played(clip, len(mono) / sr)
        words = [w for w in words if played.start <= w["start"] < played.end]
    grid = Grid(report) if report else None

    def where(seconds):
        if grid is None:
            return {}
        # By the nearest sixteenth: a word sung a few ms before a bar line is in it.
        p, _ = grid.place(seconds)
        bar, beat, n = (int(v) for v in p["sixteenth"].split("."))
        return {"bar": bar, "beat": beat + (n - 1) / 4, "sixteenth": p["sixteenth"], "off_grid_ms": p["off_grid_ms"]}

    rows = []
    for line in lines(words):
        start, end = line[0]["start"], line[-1]["end"]
        row = {"text": text(line), "seconds": round(start, 3), "until_seconds": round(end, 3), **where(start)}
        if grid is not None:
            row["bars"] = [row["bar"], where(max(start, end - 0.05))["bar"]]
        if played is not None:
            row["song_beat"] = round(played.beat(start), 3)
        row["probability"] = round(float(np.mean([w["p"] for w in line])), 2)
        if any(w["reading"] == "a" for w in line):
            row["read_again"] = True
        row["words"] = [
            {
                "word": w["word"].strip(),
                "seconds": round(w["start"], 3),
                "until_seconds": round(w["end"], 3),
                **where(w["start"]),
                "probability": round(w["p"], 2),
            }
            for w in line
        ]
        rows.append(row)
    for d in done:
        if grid is not None:
            d["bars"] = [where(d["seconds"])["bar"], where(max(d["seconds"], d["until_seconds"] - 0.05))["bar"]]
    back = repeats(rows)
    warnings = [LEAD_ONLY]
    if not is_part:
        warnings.append(
            "Read from the whole mix: daw separate PROJECT SONG --split vocals gives a voice part Whisper hears better"
        )
    if again and found.get("regions") and found.get("again_model") is None:
        warnings.append(
            f"{len(found['regions'])} stretches were in doubt and not read again: Whisper ({AGAIN}) is not "
            f"downloaded; daw separate setup --models {AGAIN} gets it"
        )
    skipped = [d for d in done if "no words" in d["why"] and d["kept"] == "first"]
    for d in skipped:
        warnings.append(
            f"From {clock(d['seconds'])} to {clock(d['until_seconds'])} the voice is loud and no words were heard, "
            "even read again: listen there"
        )
    out = {
        "path": str(voice),
        **({"song": str(whole)} if whole != voice else {}),
        "voice_part": is_part,
        "models": {"first": found["first_model"], "again": found.get("again_model")},
        "language": found.get("language") or None,
        "words_file": str(sidecar(voice)) if sidecar(voice).is_file() else None,
        "kept": was_kept,
        "beat_map": None if report is None else report.get("map"),
        **({"no_beat_map": problem} if problem else {}),
        "tempo_bpm": None if report is None else report["tempo"]["bpm"],
        "meter": None if report is None else report["meter"],
        "first_downbeat_seconds": None if report is None else report["first_downbeat_seconds"],
        "word_count": len(words),
        "sure": sum(1 for w in words if w["p"] >= SURE),
        "line_count": len(rows),
        "read_again": done,
        "hook": back[0]["text"] if back else None,
        "repeats": back,
    }
    if lyrics_out is not None:
        about = (
            f"{whole.stem}: words heard by Whisper from {'the voice part' if is_part else 'the mix'}, not checked by ear"
        )
        lyrics_file(rows, lyrics_out, about)
        out["lyrics"] = str(lyrics_out.resolve())
    listed = rows
    if near is not None:
        listed = [r for r in rows if r["until_seconds"] >= near - window and r["seconds"] <= near + window]
        out["near"] = {"seconds": near, "window_seconds": window, "of": len(rows)}
    out["lines"] = listed
    out["warnings"] = warnings
    out["source"] = "heard by Whisper from decoded audio and placed on the beat map; estimates, not ground truth"
    return out
