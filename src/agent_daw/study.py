"""One analysis of a song: `daw samples study`.

Every measure the reverse-engineer track built, run on a song and the parts
`daw separate` made of it, in the order each needs the last: the beat map, the
key and chords, the tuning, the words, the form, each pitched part's notes, the
drum groove, how each part sounds, and the master. Each measure's reply is kept
in one JSON file with its state, its confidence and what that confidence is of,
and the commands' own pictures and MIDI files beside it, and a markdown summary
of it is written for a person to read. A measure that cannot run says why; one
that fails says how, and the others run on.

Nothing is heard: every figure is the measuring command's estimate.
"""

from __future__ import annotations

import json
import math
import time
import traceback
from datetime import datetime
from pathlib import Path

import numpy as np
import soundfile as sf

from . import beats as beatmap

SCHEMA = 1
MEASURES = ("grid", "harmony", "tuning", "words", "form", "notes", "drums", "sound", "master")
# What each measure is, and the command that gives the same reply on its own.
ABOUT = {
    "grid": ("tempo, meter, beats and bars", "daw samples beats FILE"),
    "harmony": ("key and chords by bar and beat", "daw samples chords FILE"),
    "tuning": ("cents from A 440", None),
    "words": ("the words the voice sings, by bar", "daw transcribe FILE --words"),
    "form": ("sections, which bars are the same music, key changes", "daw samples form FILE"),
    "notes": ("each pitched part's notes", "daw transcribe PART"),
    "drums": ("kicks, snares and hats, grooves and fills", "daw transcribe DRUMS --drums"),
    "sound": ("each part's tone, width, dynamics, pumping and tails", "daw samples sound FILE"),
    "master": ("loudness, peak, spectrum, stereo and translation", "daw listen FILE"),
}
# The pitched parts and whether each is read one note at a time.
PITCHED = (("bass", True), ("other", False), ("guitar", False), ("piano", False), ("vocals", True))
UNSURE_FIT = 0.5  # a chord fitting under this is marked unsure
# The files a study writes into its folder, cleared before it runs again so that
# a measure that no longer runs leaves nothing stale behind.
WRITTEN = ("study.json", "study.md", "form.png", "sound.png", "overview.png", "spectrum.png", "lyrics.txt")


def note(text: str):
    from .separation import note as said

    said(text)


def folder_for(source: Path, out: Path | None) -> tuple[Path, str]:
    """Where the study is written and the song's name: --out, or `study/NAME/` in
    the project whose samples folder holds the song, NAME its sample ID."""
    project = source.parent.parent
    name = source.stem
    if source.parent.name == "samples" and (project / "song.yaml").is_file():
        from .model import load

        try:
            samples = load(project / "song.yaml", False).get("samples", {})
        except Exception:  # an unreadable song still has a file name
            samples = {}
        for sample_id, entry in samples.items():
            if (project / entry.get("path", "")).resolve() == source.resolve():
                name = sample_id
                break
        else:
            # A sample's file is HASH_NAME: the name without its hash.
            head, _, rest = name.partition("_")
            if rest and len(head) == 12:
                name = rest
    if out is not None:
        return out.resolve(), name
    if source.parent.name == "samples" and (project / "song.yaml").is_file():
        safe = "".join(c if c.isalnum() or c in "-_." else "-" for c in name).strip("-.") or "song"
        return (project / "study" / safe).resolve(), name
    raise ValueError(
        f"{source.name} is not in a project's samples folder: give --out DIR, where the study is written"
    )


def plain(value):
    """`value` as JSON can hold it: numpy's numbers as Python's, and a number
    that is not finite as null."""
    if isinstance(value, dict):
        return {str(k): plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(v) for v in value]
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, np.generic):
        value = value.item()
    if isinstance(value, float) and not math.isfinite(value):
        return None
    return value


def ratio(part, whole):
    return round(part / whole, 3) if whole else None


class Study:
    """The measures run one after another, each kept with its state."""

    def __init__(self, source: Path, folder: Path, db: Path, skip: set[str]):
        self.source, self.folder, self.db, self.skip = source, folder, db, skip
        self.measures: dict[str, dict] = {}

    def run(self, name: str, work, confidence=None):
        """Runs one measure: `work()` gives its fields, `confidence(fields)` its
        confidence as (value, of). A ValueError is a reason it could not run;
        anything else is a failure, kept with where it happened."""
        about, command = ABOUT[name]
        entry = {"about": about, **({"same_as": command} if command else {})}
        if name in self.skip:
            self.measures[name] = {"state": "skipped", **entry}
            return None
        note(f"{name}: {about}")
        started = time.monotonic()
        try:
            found = work()
        except ValueError as error:
            self.measures[name] = {"state": "not measured", "why": str(error), **entry}
            return None
        except Exception as error:  # one measure's fault leaves the others to run
            self.measures[name] = {
                "state": "failed",
                "error": f"{type(error).__name__}: {error}",
                "where": traceback.format_exc(limit=-3).strip().splitlines()[-3:],
                **entry,
            }
            return None
        value, of = confidence(found) if confidence else (None, None)
        self.measures[name] = {
            "state": "measured",
            **entry,
            "seconds": round(time.monotonic() - started, 1),
            "confidence": {"value": value, "of": of},
            **found,
        }
        return found

    def done(self, name: str) -> dict | None:
        entry = self.measures.get(name)
        return entry if entry and entry["state"] == "measured" else None


# --- the measures ------------------------------------------------------------------


def grid(report: dict) -> dict:
    keep = (
        "tempo", "meter", "meter_set_by", "beats_per_bar", "odd_bars", "first_beat_seconds",
        "first_downbeat_seconds", "downbeat", "model", "phrases", "swing", "beat_count", "requested",
    )
    from .form import bar_spans

    bars = bar_spans(report)
    return {
        **{k: report[k] for k in keep if k in report},
        "bar_count": len(bars),
        "beat_map": report.get("map"),
        "bars": [
            {"bar": b["bar"], "seconds": round(b["seconds"], 4), "until_seconds": round(b["until"], 4), "beats": b["beats"]}
            for b in bars
        ],
    }


def grid_confidence(found):
    return found.get("downbeat", {}).get("confidence"), (
        "the downbeat's confidence: the model's median odds at the bars' downbeats, or the chosen candidate's"
    )


def camelot(name: str | None) -> str | None:
    """A key's number on the Camelot wheel: C major 8B, A minor 8A."""
    if not name:
        return None
    from .harmony import parse_key

    tonic, mode = parse_key(name)
    major = tonic if mode == "major" else (tonic + 3) % 12
    return f"{(major * 7 % 12 + 7) % 12 + 1}{'B' if mode == 'major' else 'A'}"


def harmony_of(source: Path, report: dict) -> dict:
    from . import harmony

    key, chords = harmony.on_the_map(source, report)
    key.pop("profile", None)
    beats_long = sum(c["beats"] for c in chords if c["fit"] is not None)
    sure = sum(c["beats"] for c in chords if c["fit"] is not None and c["fit"] >= UNSURE_FIT)
    return {
        "key": {**key, "camelot": camelot(key.get("name"))},
        "chords_sure_share": ratio(sure, beats_long),
        "chords": chords,
    }


def harmony_confidence(found):
    return found["key"].get("confidence"), (
        "the key's odds against the other keys (null when the song is not plainly in one); "
        "each chord has its own fit, and chords_sure_share is the share of beats whose chord fits 0.5 or more"
    )


def tuning_of(source: Path, parts: dict) -> dict:
    from . import harmony

    name = "other" if "other" in parts else "mix"
    mono, sr = beatmap.read(parts.get("other", source))
    return {"read_from": name, **harmony.tuning(mono, sr)}


def tuning_confidence(found):
    return found["concentration"], (
        "how alike the partials' distances from their notes are, 0 (anywhere) to 1 (all the same)"
    )


def words_of(source: Path, parts: dict, db: Path, folder: Path, language: str) -> dict:
    from . import words

    if "vocals" not in parts:
        raise ValueError(
            "No voice part beside the song: daw separate PROJECT SONG --split stems makes one, and the words are read from it"
        )
    why = words.unavailable()
    if why:
        raise ValueError(why)
    found = words.transcribe_words(str(source), lyrics_out=folder / "lyrics.txt", language=language, db=db)
    keep = ("path", "models", "language", "words_file", "word_count", "sure", "line_count", "hook", "repeats",
            "read_again", "lines", "warnings", "lyrics")
    return {k: found[k] for k in keep if k in found}


def words_confidence(found):
    return ratio(found["sure"], found["word_count"]), "the share of words Whisper heard with a probability of 0.5 or more"


def form_of(source: Path, report: dict, parts: dict, folder: Path) -> dict:
    from . import form

    found = form.measure(source, report, parts)
    found["picture"] = form.picture(found, folder / "form.png")
    for k in ("_matrix", "_bars", "path", "beat_map", "tempo_bpm", "meter", "first_downbeat_seconds", "source"):
        found.pop(k, None)
    return found


def form_confidence(found):
    return None, (
        "no one figure: named_by says what named the sections, each like how alike a repeat is, "
        "each key_confidence how sure its key is, and checks.warnings what to correct"
    )


def notes_of(parts: dict, silent: list[str], db: Path, folder: Path) -> dict:
    from . import transcription

    why = transcription.unavailable()
    if why:
        raise ValueError(why)
    pitched = [(n, line) for n, line in PITCHED if n in parts]
    if not pitched:
        raise ValueError("No pitched part beside the song: daw separate PROJECT SONG --split stems makes them")
    out, left = {}, {}
    for name, line in pitched:
        if name in silent:
            left[name] = "the part is 30 dB or more under the song: what is in it leaked from the others"
            continue
        note(f"notes: {name}")
        try:
            found = transcription.transcribe(
                str(parts[name]), line=line, midi_out=folder / f"{name}.mid", text=True, db=db
            )
        except ValueError as error:
            left[name] = str(error)
            continue
        out[name] = {
            "path": found["path"],
            "line": line,
            "count": found["count"],
            "sure": found["sure"],
            "lowest": found["lowest"],
            "highest": found["highest"],
            "octave_doubts": sum(1 for n in found["notes"] if "octave_doubt" in n),
            "left_out": found["left_out"],
            "midi": found.get("midi"),
            "text": found.get("text"),
            "warnings": found.get("warnings", []),
            "model": found["model"],
            "notes": found["notes"],
        }
    if not out:
        raise ValueError("; ".join(f"{k}: {v}" for k, v in left.items()) or "No part's notes were heard")
    return {"parts": out, **({"not_read": left} if left else {})}


def notes_confidence(found):
    parts = found["parts"].values()
    return ratio(sum(p["sure"] for p in parts), sum(p["count"] for p in parts)), (
        "the share of notes, all parts together, the model heard with a confidence of 0.5 or more; each part has its own count and sure"
    )


def drums_of(parts: dict, silent: list[str], db: Path, folder: Path) -> dict:
    from . import drums

    if "drums" not in parts:
        raise ValueError("No drums part beside the song: daw separate PROJECT SONG --split stems makes one")
    if "drums" in silent:
        raise ValueError("The drums part is 30 dB or more under the song: the song has no drums")
    found = drums.transcribe(str(parts["drums"]), midi_out=folder / "drums.mid", text=True, db=db)
    keep = ("path", "drums", "unnamed", "count", "grooves", "fills", "text", "midi", "warnings", "hits")
    return {k: found[k] for k in keep if k in found}


def drums_confidence(found):
    return ratio(found["count"], found["count"] + found.get("unnamed", 0)), (
        "the share of the hits found that a kick, snare or hat explains; the rest are toms, cymbals or what no drum's sound fits"
    )


def sound_of(source: Path, report: dict | None, parts: dict, sections: list | None, folder: Path) -> dict:
    from . import sound

    found = sound.measure(source, report, parts, sections, None, None)
    try:
        found["picture"] = sound.picture(found, folder / "sound.png")
    except ValueError:
        pass
    return found


def sound_confidence(found):
    return None, (
        "no one figure: each estimate was checked on renders made with known settings (daw describe beats.sound), "
        "and each eq has error_db, how far its curve is from the part's"
    )


def master_of(source: Path, sections: list | None, folder: Path) -> dict:
    from . import dynamics, perception, spectrum

    report, x, *_ = perception.measured(source)
    rate = report["source"]["sample_rate"]
    mix = dict(report["mix"])
    by_section = []
    whole = mix.get("integrated_lufs")
    for s in sections or []:
        a, b = int(s["seconds"] * rate), int(s["until_seconds"] * rate)
        if b - a < round(0.4 * rate):
            continue
        lufs = perception.integrated(x[a:b], rate)
        figures = dynamics.loudness(x, rate, [(a, b)])[0]
        by_section.append(
            {
                "label": s["label"],
                "name": s.get("name"),
                "bars": s["bars"],
                "integrated_lufs": None if lufs is None else round(lufs, 2),
                "relative_lu": None if lufs is None or whole is None else round(lufs - whole, 1),
                **figures,
            }
        )
    pictures = [perception.plot_listen(report, x, folder), spectrum.plot(report, folder / "spectrum.png")]
    return {
        "mix": {**mix, "third_octave_hz": spectrum.CENTERS},
        "translation": report.get("translation"),
        "sections": by_section,
        "pictures": pictures,
    }


def master_confidence(found):
    return None, "measured, not estimated: loudness to ITU-R BS.1770, an oversampled true peak, the spectrum and the stereo field"


# --- the bars, all measures side by side ----------------------------------------------


def bars_table(measures: dict) -> list[dict]:
    """Each bar from bar 1 with what every measure says of it: its section, its
    chords, the drums' groove, the first words sung in it and the notes each
    part plays there."""
    grid_ = measures.get("grid") or {}
    rows = [{**b} for b in grid_.get("bars", [])]
    if not rows:
        return []
    index = {r["bar"]: r for r in rows}
    form_ = measures.get("form")
    if form_:
        for s in form_.get("sections", []):
            for bar in range(s["bars"][0], s["bars"][1] + 1):
                if bar in index:
                    index[bar]["section"] = s["label"]
                    index[bar]["name"] = s.get("name")
    harmony_ = measures.get("harmony")
    if harmony_:
        for r in rows:
            heard = []
            for c in harmony_["chords"]:
                if c["until_seconds"] <= r["seconds"] + 1e-3 or c["seconds"] >= r["until_seconds"] - 1e-3:
                    continue
                name = c["chord"] if c["fit"] is None or c["fit"] >= UNSURE_FIT else f"{c['chord']}?"
                if not heard or heard[-1] != name:
                    heard.append(name)
            r["chords"] = heard
    drums_ = measures.get("drums")
    if drums_:
        for g in drums_.get("grooves", []):
            for a, b in g["bars"]:
                for bar in range(a, b + 1):
                    if bar in index:
                        index[bar]["groove"] = g["id"]
        for a, b in drums_.get("fills", []):
            for bar in range(a, b + 1):
                if bar in index and "groove" not in index[bar]:
                    index[bar]["groove"] = "fill"
    words_ = measures.get("words")
    if words_:
        for line in words_.get("lines", []):
            if line.get("bar") in index:
                row = index[line["bar"]]
                row["words"] = f"{row['words']} / {line['text']}" if row.get("words") else line["text"]
    notes_ = measures.get("notes")
    if notes_:
        for name, part in notes_["parts"].items():
            for n in part["notes"]:
                if n.get("bar") in index:
                    row = index[n["bar"]]
                    row.setdefault("notes", {})
                    row["notes"][name] = row["notes"].get(name, 0) + 1
                    if name == "bass":
                        row.setdefault("bass", []).append(n["pitch"])
    return rows


# --- the summary ------------------------------------------------------------------------


def clock(seconds) -> str:
    if seconds is None:
        return "?"
    m, s = divmod(max(0.0, float(seconds)), 60)
    return f"{int(m)}:{s:04.1f}"


def num(value, places=1, unit="") -> str:
    if value is None:
        return "—"
    return f"{value:.{places}f}{unit}"


def table(head: list[str], rows: list[list]) -> list[str]:
    out = ["| " + " | ".join(head) + " |", "|" + "---|" * len(head)]
    for r in rows:
        out.append("| " + " | ".join("" if v is None else str(v).replace("|", "\\|").replace("\n", " ") for v in r) + " |")
    return out


def glance(study: dict) -> dict:
    m = study["measures"]
    out = {}
    if (g := m.get("grid", {})).get("state") == "measured":
        out["tempo_bpm"] = g["tempo"]["bpm"]
        out["steady"] = g["tempo"].get("steady")
        out["ambiguous_with"] = g["tempo"].get("ambiguous_with")
        out["tempo_steps"] = [
            {"bar": seg["bar"], "seconds": seg["seconds"], "bpm": seg["bpm"]}
            for seg in (g["tempo"].get("segments") or [])[1:]
        ]
        out["meter"] = g.get("meter")
        out["bars"] = g["bar_count"]
        out["first_downbeat_seconds"] = g.get("first_downbeat_seconds")
        out["downbeat_confidence"] = g.get("downbeat", {}).get("confidence")
    if (h := m.get("harmony", {})).get("state") == "measured":
        key = h["key"]
        out["key"] = key.get("name")
        out["key_confidence"] = key.get("confidence")
        out["camelot"] = key.get("camelot")
        others = [c["name"] for c in key.get("candidates", [])[1:2]]
        out["key_runner_up"] = others[0] if others else None
    if (t := m.get("tuning", {})).get("state") == "measured":
        out["tuning_cents"] = t["cents"]
    if (f := m.get("form", {})).get("state") == "measured":
        out["form"] = f.get("form")
        out["key_changes"] = len(f.get("key_changes", []))
    if (w := m.get("words", {})).get("state") == "measured":
        out["hook"] = w.get("hook")
    if (s := m.get("master", {})).get("state") == "measured":
        mix = s["mix"]
        out["integrated_lufs"] = round(mix["integrated_lufs"], 1) if mix.get("integrated_lufs") is not None else None
        tp = mix.get("estimated_true_peak_dbtp")
        out["true_peak_dbtp"] = None if tp is None else round(tp, 2)
        out["loudness_range_lu"] = mix.get("loudness_range_lu")
    if study.get("parts"):
        out["stems_residual_db"] = study["parts"].get("residual_db")
    out["seconds"] = study["song"]["seconds"]
    return out


def summary(study: dict) -> str:
    song, m, at = study["song"], study["measures"], study["at_a_glance"]
    lines = [
        f"# Study of {song['name']}",
        "",
        f"Source: `{song['path']}` (sha256 {song['sha256'][:12]}, {clock(song['seconds'])}, {song['sample_rate']} Hz). "
        f"Measured {study['made'][:10]} by `daw samples study`.",
        "Every figure is measured, not heard; each measure's confidence is given where it has one. "
        "The whole of each measure is in `study.json` beside this file.",
        "",
        "## At a glance",
        "",
    ]
    said = []
    if "tempo_bpm" in at:
        tempo = f"{at['tempo_bpm']:g} BPM" + (" steady" if at.get("steady") else " not steady")
        if at.get("ambiguous_with"):
            tempo += f" (could be {', '.join(f'{v:g}' for v in at['ambiguous_with'])})"
        for step in at.get("tempo_steps") or []:
            tempo += f", then {step['bpm']:.4g} from bar {step['bar']} ({clock(step['seconds'])})"
        said.append(f"- **Grid:** {tempo}, {at.get('meter')}, {at['bars']} bars from the first downbeat at "
                    f"{num(at.get('first_downbeat_seconds'), 3, ' s')} (downbeat confidence {num(at.get('downbeat_confidence'), 2)})")
    if "key" in at:
        said.append(
            f"- **Key:** {at['key'] or 'not plainly in one'}"
            + (f" ({num(at['key_confidence'], 2)}; runner-up {at['key_runner_up']}), Camelot {at['camelot']}" if at["key"] else "")
        )
    if "tuning_cents" in at:
        said.append(f"- **Tuning:** {at['tuning_cents']:+.1f} cents from A 440")
    if "form" in at:
        said.append(f"- **Form:** {at['form']}; {at['key_changes']} key change(s)")
    if at.get("hook"):
        said.append(f"- **Hook:** “{at['hook']}”")
    if "integrated_lufs" in at:
        said.append(
            f"- **Master:** {num(at['integrated_lufs'], 1, ' LUFS')}, true peak {num(at['true_peak_dbtp'], 2, ' dBTP')}, "
            f"loudness range {num(at['loudness_range_lu'], 1, ' LU')}"
        )
    if study.get("parts"):
        p = study["parts"]
        said.append(
            f"- **Stems:** {', '.join(p['files'])} by {p.get('model') or 'an unknown model'}"
            + (f", residual {p['residual_db']} dB" if p.get("residual_db") is not None else "")
        )
    else:
        said.append(f"- **Stems:** none; {study.get('no_parts')}")
    lines += said or ["Nothing was measured."]
    lines += ["", "## Measures", ""]
    rows = []
    for name in MEASURES:
        e = m[name]
        conf = e.get("confidence", {}).get("value")
        what = e.get("why") or e.get("error") or e["about"]
        rows.append([name, e["state"], num(conf, 2) if conf is not None else "", num(e.get("seconds"), 1), what])
    lines += table(["Measure", "State", "Confidence", "Seconds", "What"], rows)

    if (f := m["form"]).get("state") == "measured":
        lines += ["", "## Sections", "", f"Form: {f.get('form')}. Named by {f.get('named_by')}.", ""]
        loud = {}
        if m["master"].get("state") == "measured":
            loud = {tuple(s["bars"]): s["relative_lu"] for s in m["master"]["sections"]}
        rows = []
        for s in f["sections"]:
            like = s.get("like")
            like_text = ""
            if isinstance(like, dict) and 1 <= like.get("section", 0) <= len(f["sections"]):
                like_text = f"{f['sections'][like['section'] - 1]['label']} ({like.get('how')}"
                like_text += f", {like['semitones']:+d} st)" if like.get("semitones") else ")"
            rows.append([
                s["label"], s.get("name"), f"{s['bars'][0]}–{s['bars'][1]}", clock(s["seconds"]), s["count"],
                f"{s.get('key')} {'sure' if s.get('key_sure') else num(s.get('key_confidence'), 2)}",
                like_text, num(s.get("sung"), 2) if "sung" in s else "", num(s.get("drums"), 2) if "drums" in s else "",
                num(loud.get(tuple(s["bars"])), 1),
            ])
        lines += table(["Label", "Name", "Bars", "Time", "Bars long", "Key", "Like", "Sung", "Drums", "Relative LU"], rows)
        if f.get("key_changes"):
            lines += ["", "Key changes: " + "; ".join(json.dumps(k) for k in f["key_changes"])]
        if f.get("checks", {}).get("warnings"):
            lines += ["", "Checks: " + " ".join(f["checks"]["warnings"])]
        if f.get("silent_parts"):
            lines += ["", f"Silent parts: {', '.join(f['silent_parts'])}."]
        lines += ["", f"Picture: `{Path(f['picture']).name}`."]

    if (h := m["harmony"]).get("state") == "measured":
        key = h["key"]
        lines += ["", "## Harmony", ""]
        if key.get("name"):
            cands = ", ".join(f"{c['name']} {c['confidence']}" for c in key.get("candidates", []))
            lines.append(f"Key {key['name']} ({key['confidence']}; scale {key.get('scale_confidence')}), Camelot {key['camelot']}. Candidates: {cands}.")
        else:
            lines.append("Not plainly in a key (the pitch classes are too even); the chords are named without one.")
        lines.append(f"Chords fitting 0.5 or more: {num((h.get('chords_sure_share') or 0) * 100, 0, '%')} of the beats; `?` marks a chord under 0.5.")
        bars = study.get("bars", [])
        if f.get("state") == "measured" and bars:
            lines.append("")
            by_bar = {b["bar"]: b for b in bars}
            for s in f["sections"]:
                cells = [" ".join(by_bar[b].get("chords", [])) or "—" for b in range(s["bars"][0], s["bars"][1] + 1) if b in by_bar]
                lines.append(f"- {s['label']} {s.get('name') or ''} (bars {s['bars'][0]}–{s['bars'][1]}): | " + " | ".join(cells) + " |")

    if (w := m["words"]).get("state") == "measured":
        lines += ["", "## Words", "", f"{w['line_count']} lines, {w['word_count']} words, {w['sure']} sure."]
        if w.get("hook"):
            hook = next((r for r in w.get("repeats", []) if r["text"] == w["hook"]), {})
            lines.append(f"Hook: “{w['hook']}”, {hook.get('count', '?')} times.")
        lines.append("")
        for line in w["lines"]:
            lines.append(f"- bar {line.get('bar', '?')} ({clock(line['seconds'])}): {line['text']}")
        for warning in w.get("warnings", []):
            lines.append(f"> {warning}")

    if (n := m["notes"]).get("state") == "measured":
        lines += ["", "## Notes", ""]
        rows = []
        for name, p in n["parts"].items():
            rows.append([name, "line" if p["line"] else "all", p["count"], p["sure"], f"{p['lowest']}–{p['highest']}",
                         p["octave_doubts"], f"`{Path(p['midi']['path']).name}`" if p.get("midi") else ""])
        lines += table(["Part", "Read", "Notes", "Sure", "Range", "Octave doubts", "MIDI"], rows)
        for name, why in n.get("not_read", {}).items():
            lines.append(f"- {name} not read: {why}")
        lines += ["", "The bass line is by bar under Bars; every part's notes are in its MIDI file and in `study.json` as note text."]

    if (d := m["drums"]).get("state") == "measured":
        lines += ["", "## Drums", ""]
        counts = ", ".join(f"{k} {v['count']}" for k, v in d["drums"].items())
        lines.append(f"{counts}; {d.get('unnamed', 0)} hits no drum explains.")
        for g in d.get("grooves", []):
            runs = ", ".join(f"{a}–{b}" if a != b else f"{a}" for a, b in g["bars"])
            lines.append(f"- Groove {g['id']}: bars {runs} ({g['count']} bars, {g['hits']} hits)")
        if d.get("fills"):
            lines.append("- Fills: bars " + ", ".join(f"{a}–{b}" if a != b else f"{a}" for a, b in d["fills"]))
        grooves = [c for c in (d.get("text") or {}).get("clips", []) if c["kind"] == "groove"]
        seen = set()
        for c in grooves:
            first = c["id"].split("-")[1]
            if first in seen:
                continue
            seen.add(first)
            lines += ["", f"Groove {first.upper()} as note text (kick, snare, hat rows; sixteenths):", "", "```", c["text"], "```"]
        for warning in d.get("warnings", []):
            lines.append(f"> {warning}")

    if (s := m["sound"]).get("state") == "measured":
        lines += ["", "## Sound", ""]
        rows = []
        for name, p in s["sound"].items():
            tone, width, dyn = p.get("tone") or {}, p.get("width") or {}, p.get("dynamics") or {}
            pump = p.get("pumping") or {}
            tails = p.get("tails") or {}
            echo = tails.get("echoes") or {}
            rows.append([
                name,
                num(tone.get("tilt_db_per_octave"), 1),
                num((tone.get("eq") or {}).get("error_db"), 2),
                num(width.get("side_share"), 2),
                num(dyn.get("crest_db"), 1),
                num(dyn.get("against_mix_db"), 1),
                f"{num(pump.get('depth_db'), 1)} dB, {num(pump.get('release_ms'), 0)} ms" if pump.get("heard") else "",
                num(tails.get("decay_seconds"), 2, " s") if tails.get("decay_seconds") is not None else "",
                (echo.get("nearest") or {}).get("division", "") if echo.get("heard") else "",
            ])
        lines += table(["Part", "Tilt dB/oct", "EQ error dB", "Side share", "Crest dB", "Against mix dB", "Pumping", "Decay", "Echo"], rows)
        if s.get("picture"):
            lines += ["", f"Picture: `{Path(s['picture']).name}`. Each part's eq bands are in `study.json`."]

    if (ms := m["master"]).get("state") == "measured":
        mix = ms["mix"]
        lines += ["", "## Master", ""]
        lines.append(
            f"{num(mix.get('integrated_lufs'), 1, ' LUFS')} integrated, {num(mix.get('max_short_term_lufs'), 1)} short-term max, "
            f"true peak {num(mix.get('estimated_true_peak_dbtp'), 2, ' dBTP')}, range {num(mix.get('loudness_range_lu'), 1, ' LU')}, "
            f"crest {num(mix.get('crest_db'), 1, ' dB')}, tilt {num(mix.get('tilt_db_per_octave'), 2, ' dB/oct')}, "
            f"correlation {num(mix.get('stereo_correlation'), 2)}, side {num(mix.get('side_energy_fraction'), 3)}."
        )
        bands = ", ".join(f"{k} {num(v, 1)}" for k, v in (mix.get("band_dbfs") or {}).items())
        lines.append(f"Bands (dBFS): {bands}.")
        pictures = ", ".join(f"`{Path(p).name}`" for p in ms.get("pictures", []))
        lines.append(f"Pictures: {pictures}.")

    if study.get("bars"):
        lines += ["", "## Bars", ""]
        rows = []
        for b in study["bars"]:
            rows.append([b["bar"], clock(b["seconds"]), b.get("section", ""), " ".join(b.get("chords", [])),
                         " ".join(b.get("bass", [])), b.get("groove", ""), b.get("words", "")])
        lines += table(["Bar", "Time", "Section", "Chords", "Bass", "Drums", "Words"], rows)

    lines += ["", "## Not measured", ""]
    missing = [f"- {k}: {v.get('why') or v.get('error') or v['state']}" for k, v in m.items() if v["state"] != "measured"]
    lines += missing or ["- Every measure ran."]
    lines += [
        "- Always: modes (read from the chords), toms, cymbals and open hats one by one, backing vocals and ad-libs, "
        "compressor and limiter settings, saturation, and which instrument or patch plays a part.",
        "",
    ]
    return "\n".join(lines)


# --- the command ------------------------------------------------------------------------


def run(source: Path, db: Path, out: Path | None = None, skip=(), language: str = "en") -> dict:
    from . import form, library

    skip = set(skip)
    unknown = skip - set(MEASURES)
    if unknown:
        raise ValueError(f"No measure {', '.join(sorted(unknown))}; the measures are {', '.join(MEASURES)}")
    if "grid" in skip:
        raise ValueError("Every measure reads the beat map: grid cannot be skipped")
    folder, name = folder_for(source, out)
    folder.mkdir(parents=True, exist_ok=True)
    for old in [*WRITTEN, *(f"{n}.mid" for n, _ in PITCHED), "drums.mid"]:
        (folder / old).unlink(missing_ok=True)
    started = time.monotonic()
    info = sf.info(str(source))
    parts, recipe = form.split_of(source)
    study = Study(source, folder, db, skip)

    report = None

    def the_grid():
        nonlocal report
        report = library.beat_map(db, source)
        return grid(report)

    study.run("grid", the_grid, grid_confidence)
    if report is None:
        for measure in MEASURES[1:]:
            study.measures.setdefault(measure, {"state": "not measured", "why": "no beat map", "about": ABOUT[measure][0]})
    else:
        study.run("harmony", lambda: harmony_of(source, report), harmony_confidence)
        study.run("tuning", lambda: tuning_of(source, parts), tuning_confidence)
        # Before the form, which names the chorus by the words kept beside the voice.
        study.run("words", lambda: words_of(source, parts, db, folder, language), words_confidence)
        study.run("form", lambda: form_of(source, report, parts, folder), form_confidence)
        form_ = study.done("form") or {}
        silent = form_.get("silent_parts", [])
        sections = form_.get("sections")
        study.run("notes", lambda: notes_of(parts, silent, db, folder), notes_confidence)
        study.run("drums", lambda: drums_of(parts, silent, db, folder), drums_confidence)
        study.run("sound", lambda: sound_of(source, report, parts, sections, folder), sound_confidence)
        study.run("master", lambda: master_of(source, sections, folder), master_confidence)
    whole = {
        "kind": "study",
        "schema_version": SCHEMA,
        "made": datetime.now().astimezone().isoformat(timespec="seconds"),
        "song": {
            "name": name,
            "path": str(source),
            "sha256": library.digest(source),
            "seconds": round(info.duration, 3),
            "sample_rate": info.samplerate,
            "channels": info.channels,
        },
        "parts": None
        if not parts
        else {
            "files": {k: str(v) for k, v in parts.items()},
            "split": (recipe or {}).get("split"),
            "model": ((recipe or {}).get("model") or {}).get("name"),
            "residual_db": (recipe or {}).get("residual_db"),
            "recipe": str((recipe or {}).get("_path")) if recipe else None,
        },
        **(
            {}
            if parts
            else {"no_parts": "daw separate PROJECT SONG --split stems makes the parts; the words, notes, drums and each part's sound are read from them"}
        ),
        "measures": {k: study.measures[k] for k in MEASURES},
    }
    whole["bars"] = bars_table({k: study.done(k) for k in MEASURES})
    whole["at_a_glance"] = glance(whole)
    whole["seconds"] = round(time.monotonic() - started, 1)
    whole = plain(whole)
    (folder / "study.json").write_text(json.dumps(whole, indent=1, allow_nan=False) + "\n")
    (folder / "study.md").write_text(summary(whole))
    return {
        "study": str(folder / "study.json"),
        "summary": str(folder / "study.md"),
        "folder": str(folder),
        "song": whole["song"]["name"],
        "measured": [k for k, v in whole["measures"].items() if v["state"] == "measured"],
        **{
            state.replace(" ", "_"): {k: v.get("why") or v.get("error") or state for k, v in whole["measures"].items() if v["state"] == state}
            for state in ("not measured", "failed", "skipped")
            if any(v["state"] == state for v in whole["measures"].values())
        },
        "at_a_glance": whole["at_a_glance"],
        "files": sorted(p.name for p in folder.iterdir() if p.is_file()),
        "seconds": whole["seconds"],
        "source": "every measure's estimate from decoded audio and the parts daw separate made; nothing heard",
    }
