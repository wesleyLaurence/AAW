"""References: a song the person names as what good sounds like, analyzed once
and compared with a mix section by section.

A reference is kept in the workspace library as its measurements, with the path
and hash of the person's file and no copy of its audio. A comparison reports
differences, which way and by how much; it does not say which side is right.
"""

from __future__ import annotations

import datetime
import hashlib
import json
import os
import shutil
import tempfile
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from pathlib import Path

import numpy as np

from . import beats, library, perception
from .model import atomic_text, beat, digest

VERSION = 1
# A band holding less than this share of a side's power is empty there, and has
# no level to compare: 70 dB under the whole.
EMPTY_SHARE = 1e-7
SILENCE_DB = -50.0  # relative to the file's peak, as the beat map takes it
# The least difference an observation is made of. Chosen as what a person
# mixing would act on, and not run on a set of real songs.
NOTABLE = {
    "integrated_lufs": 1.0,
    "estimated_true_peak_dbtp": 1.0,
    "band_db": 3.0,
    "crest_db": 2.0,
    "side_energy_fraction": 0.05,
    "relative_lu": 1.5,
    "span_lu": 2.0,
}
LISTED = 4  # sections named in one observation


def workspace() -> Path:
    """The workspace: AAW_WORKSPACE, or ~/Music/AAW."""
    override = os.environ.get("AAW_WORKSPACE")
    return Path(override).expanduser() if override else Path.home() / "Music/AAW"


def directory() -> Path:
    """Where references are: library/references in the workspace."""
    return workspace() / "library" / "references"


def slug(name: str) -> str:
    """A reference's folder name: lower-case letters, digits and `-`, as a patch's."""
    out = ""
    for c in str(name).lower():
        if c.isascii() and c.isalnum():
            out += c
        elif out and not out.endswith("-"):
            out += "-"
    return out.rstrip("-")


def named(name: str) -> str:
    key = slug(name)
    if not key:
        raise ValueError(f"A reference's name has letters or digits: {name}")
    return key


def load(name: str) -> dict:
    """A saved reference, as `daw reference add` wrote it."""
    key = named(name)
    path = directory() / key / "reference.json"
    if not path.is_file():
        known = ", ".join(r["name"] for r in listing()) or "none"
        raise ValueError(
            f"No reference named {key} (saved: {known}); add one with "
            "daw reference add FILE --name NAME"
        )
    return json.loads(path.read_text())


def store(reference: dict):
    path = directory() / reference["name"] / "reference.json"
    atomic_text(path, json.dumps(reference, indent=1, allow_nan=False) + "\n")


@contextmanager
def readable(source: Path):
    """The file as audio libsndfile reads: itself, or a decoded copy that is
    removed afterwards. Gives the path and the decoder that made it, if any."""
    if source.suffix.lower() not in library.COMPRESSED:
        yield source, None
        return
    with tempfile.TemporaryDirectory(prefix="aaw-reference-") as scratch:
        target = Path(scratch) / "audio.wav"
        yield target, library.decode(source, target)


def sounding(x, rate):
    """Where the file's sound starts and ends, in seconds."""
    level = np.abs(x).max(axis=1)
    peak = float(level.max())
    if peak <= 10 ** (-120 / 20):
        raise ValueError("A reference has sound; this file is silent")
    active = np.flatnonzero(level >= peak * 10 ** (SILENCE_DB / 20))
    return float(active[0] / rate), float((active[-1] + 1) / rate)


def index_of(row, per_bar) -> int:
    """A beat's number in the reference's own beats: 0 on its first downbeat."""
    return (row["bar"] - 1) * per_bar + row["beat"] - 1


def beat_seconds(rows, per_bar, at: float) -> float:
    """The time of a beat of the map, between its beats and past either end."""
    index = np.array([index_of(r, per_bar) for r in rows], dtype=float)
    seconds = np.array([r["seconds"] for r in rows])
    if at <= index[0]:
        return float(seconds[0] + (at - index[0]) * (seconds[1] - seconds[0]))
    if at >= index[-1]:
        return float(seconds[-1] + (at - index[-1]) * (seconds[-1] - seconds[-2]))
    return float(np.interp(at, index, seconds))


def seconds_beat(rows, per_bar, at: float) -> float:
    """The beat of the map at a time."""
    index = np.array([index_of(r, per_bar) for r in rows], dtype=float)
    seconds = np.array([r["seconds"] for r in rows])
    if at <= seconds[0]:
        return float(index[0] + (at - seconds[0]) / (seconds[1] - seconds[0]))
    if at >= seconds[-1]:
        return float(index[-1] + (at - seconds[-1]) / (seconds[-1] - seconds[-2]))
    return float(np.interp(at, seconds, index))


def phrase_starts(beat_map) -> list[dict]:
    """The measured sections' starts: each phrase of the beat map, named for its bar."""
    return [
        {"id": f"bar-{p['bar']}", "seconds": p["seconds"], "bar": p["bar"]}
        for p in beat_map["phrases"]
    ]


def requested_starts(spec, beat_map) -> list[dict]:
    """Sections named by hand, as starts in seconds. Each is {"id", "at"} in the
    reference's own beats or {"id", "seconds"} as seconds or m:ss."""
    if not isinstance(spec, list) or not spec:
        raise ValueError(
            'Sections are a JSON list such as [{"id": "verse", "at": 32}, '
            '{"id": "drop", "seconds": "1:02"}]'
        )
    starts = []
    for row in spec:
        if not isinstance(row, dict) or not isinstance(row.get("id"), str) or not row["id"]:
            raise ValueError(f"A section is an object with an id: {json.dumps(row)}")
        extra = set(row) - {"id", "at", "seconds"}
        if extra or ("at" in row) == ("seconds" in row):
            raise ValueError(
                f"Section {row['id']} takes at, in the reference's beats, or seconds, "
                "and nothing else; it runs to the next section's start"
            )
        if "seconds" in row:
            start = beats.seconds(row["seconds"])
        elif beat_map is None:
            raise ValueError(
                f"Section {row['id']} is in beats and this reference has no beat map; "
                "give seconds"
            )
        else:
            rows, per_bar = beat_map["beats"], beat_map["beats_per_bar"]
            start = beat_seconds(rows, per_bar, float(beat(row["at"])))
        starts.append({"id": row["id"], "seconds": start})
    ids = [s["id"] for s in starts]
    if len(set(ids)) != len(ids):
        raise ValueError("Section IDs are each used once")
    return starts


def label(sections):
    """Each section's `label`: where its loudness sits between the reference's
    quietest and loudest sections, by thirds at least 1 LU wide."""
    loud = [s["audio"]["integrated_lufs"] for s in sections]
    known = [v for v in loud if v is not None] or [0.0]
    top, bottom = max(known), min(known)
    third = max((top - bottom) / 3, 1.0)
    for section, value in zip(sections, loud):
        if value is None:
            section["label"] = None
        elif top - value <= third:
            section["label"] = "high"
        elif value - bottom < third:
            section["label"] = "low"
        else:
            section["label"] = "mid"


def measured(x, rate, starts, end, whole, beat_map) -> list[dict]:
    """The sections that start at `starts`, each running to the next and the last
    to `end`, with what `daw listen` measures of each but its true peak."""
    starts = sorted(starts, key=lambda s: s["seconds"])
    edges = [s["seconds"] for s in starts] + [end]
    frames = []
    for start, a, b in zip(starts, edges, edges[1:]):
        first, last = max(0, round(a * rate)), min(len(x), round(b * rate))
        if last - first < 2:
            raise ValueError(
                f"Section {start['id']} at {a:.2f} s holds no audio: sections are in "
                f"order, apart, and inside the file's {len(x) / rate:.2f} s"
            )
        frames.append((first, last))
    audios = perception.measure_each([x[a:b] for a, b in frames], rate, true_peak=False)
    sections = []
    for start, (first, last), audio in zip(starts, frames, audios):
        section = {"id": start["id"], "start_seconds": first / rate, "end_seconds": last / rate}
        if beat_map:
            rows, per_bar = beat_map["beats"], beat_map["beats_per_bar"]
            at = seconds_beat(rows, per_bar, first / rate)
            section["at"] = round(at, 3)
            section["length_beats"] = round(seconds_beat(rows, per_bar, last / rate) - at, 3)
            section["bar"] = int(np.floor(round(at, 3) / per_bar)) + 1
        section["relative_lu"] = perception.delta(
            whole["integrated_lufs"], audio["integrated_lufs"]
        )
        section["audio"] = audio
        sections.append(section)
    label(sections)
    return sections


def in_use(reference) -> list[dict]:
    """The sections a comparison uses: those named by hand, or else the measured ones."""
    return reference["named"] if reference["named"] is not None else reference["phrases"]


def shown(reference) -> dict:
    """A reference as the commands print it."""
    source = Path(reference["source"]["path"])
    out = {k: v for k, v in reference.items() if k not in ("phrases", "named")}
    out["sections_from"] = "request" if reference["named"] is not None else "measurement"
    out["sections"] = in_use(reference)
    out["source"] = {**reference["source"], "available": source.is_file()}
    out["path"] = str(directory() / reference["name"])
    return out


def add(file, name=None, bpm=None, downbeat=None, meter=None, replace=False) -> dict:
    """Analyzes a song and keeps it as a reference under NAME."""
    source = Path(file).expanduser().resolve()
    if not source.is_file():
        raise ValueError(f"No file at {source}")
    key = named(name if name is not None else source.stem)
    folder = directory() / key
    if (folder / "reference.json").exists() and not replace:
        raise ValueError(
            f"A reference named {key} is saved; give another --name, or --replace"
        )
    if meter is not None:
        beats.bar_of(meter)
    with readable(source) as (audio, decoder), ThreadPoolExecutor(1) as beside:
        x, rate = perception.read_audio(audio)
        first, last = sounding(x, rate)
        # The beat map reads the file itself, so it is made while the whole is measured.
        mapping = beside.submit(beats.measure, audio, bpm, downbeat, meter)
        whole = perception.measure(x, rate)
        entry = {
            "path": str(source),
            "sha256": digest(source),
            "sample_rate": rate,
            "frames": len(x),
            "channels": x.shape[1],
            "duration_seconds": len(x) / rate,
            "sound_seconds": [first, last],
        }
        if decoder:
            entry.update(decoder=decoder, decoded_sha256=digest(audio))
        try:
            beat_map, problem = mapping.result(), None
        except ValueError as e:
            # Music without a beat is still a reference, as one whole, unless
            # the person said where its beats are.
            beat_map, problem = None, str(e).replace(str(audio), source.name)
            if bpm is not None or downbeat is not None:
                raise ValueError(problem) from None
    phrases = []
    if beat_map:
        beat_map = {k: v for k, v in beat_map.items() if k not in ("path", "source")}
        beat_map["requested"] = {"bpm": bpm, "downbeat": downbeat, "meter": meter}
        rows = beat_map["beats"]
        end = min(last, rows[-1]["seconds"] + 60 / beat_map["tempo"]["bpm"])
        phrases = measured(x, rate, phrase_starts(beat_map), end, whole, beat_map)
    reference = {
        "schema_version": VERSION,
        "kind": "reference",
        "name": key,
        "added_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "source": entry,
        "analysis_code_sha256": digest(Path(perception.__file__)),
        "beat_map": None
        if beat_map is None
        else {k: v for k, v in beat_map.items() if k not in ("beats", "phrases", "duration", "sample_rate")},
        "beat_map_problem": problem,
        "mix": whole,
        "phrases": phrases,
        "named": None,
    }
    if folder.exists():
        shutil.rmtree(folder)
    if beat_map:
        atomic_text(folder / "beats.json", json.dumps(beat_map, indent=1) + "\n")
    store(reference)
    return shown(reference)


def listing() -> list[dict]:
    """Every saved reference, by name, in a line each."""
    out = []
    for path in sorted(directory().glob("*/reference.json")):
        try:
            r = json.loads(path.read_text())
            out.append(
                {
                    "name": r["name"],
                    "source": r["source"]["path"],
                    "available": Path(r["source"]["path"]).is_file(),
                    "duration_seconds": r["source"]["duration_seconds"],
                    "bpm": r["beat_map"]["tempo"]["bpm"] if r["beat_map"] else None,
                    "integrated_lufs": r["mix"]["integrated_lufs"],
                    "sections": [s["id"] for s in in_use(r)],
                    "added_at": r["added_at"],
                }
            )
        except (ValueError, KeyError, TypeError) as e:
            out.append({"name": path.parent.name, "problem": f"Not a reference: {e}"})
    return out


def sections(name, spec=None, measured_again=False) -> dict:
    """A reference's sections; with `spec`, names them by hand, which measures
    the person's file again; `measured_again` goes back to the measured ones."""
    reference = load(name)
    if measured_again:
        reference["named"] = None
        store(reference)
    elif spec is not None:
        source = Path(reference["source"]["path"])
        if not source.is_file() or digest(source) != reference["source"]["sha256"]:
            raise ValueError(
                f"The file {reference['name']} was made from is not at {source} as it "
                "was; add it again with daw reference add FILE --name "
                f"{reference['name']} --replace"
            )
        map_path = directory() / reference["name"] / "beats.json"
        beat_map = json.loads(map_path.read_text()) if reference["beat_map"] else None
        starts = requested_starts(spec, beat_map)
        with readable(source) as (audio, _):
            x, rate = perception.read_audio(audio)
        end = reference["source"]["sound_seconds"][1]
        reference["named"] = measured(x, rate, starts, end, reference["mix"], beat_map)
        store(reference)
    out = shown(reference)
    return {k: out[k] for k in ("name", "beat_map", "sections_from", "sections")}


def remove(name) -> dict:
    """Forgets a reference. The person's file is not touched."""
    reference = load(name)
    shutil.rmtree(directory() / reference["name"])
    return {"removed": reference["name"], "source": reference["source"]["path"]}


def share_db(audio, band):
    """A band's level in dBFS, or None where the band is empty of the file's power."""
    share = audio["band_fraction"][band]
    return None if share is None or share < EMPTY_SHARE else audio["band_dbfs"][band]


def balance(mix, ref) -> dict:
    """Each band's level in the mix against the reference's, with the median
    difference across the bands taken out: a mix that is louder or quieter all
    over reads as zero, and one band moved reads as that band's amount."""
    raw = {
        band: perception.delta(share_db(ref, band), share_db(mix, band))
        for band in perception.BANDS
    }
    known = [v for v in raw.values() if v is not None]
    if len(known) < 3:
        return {band: None for band in raw}
    middle = float(np.median(known))
    return {band: None if v is None else v - middle for band, v in raw.items()}


def summary(audio, whole=None) -> dict:
    """What a comparison shows of one side: of a section when `whole` is the
    file's measurements, and otherwise of the file."""
    out = {"integrated_lufs": audio["integrated_lufs"]}
    if whole is None:
        out["estimated_true_peak_dbtp"] = audio["estimated_true_peak_dbtp"]
    else:
        out["relative_lu"] = perception.delta(
            whole["integrated_lufs"], audio["integrated_lufs"]
        )
    for key in ("crest_db", "stereo_correlation", "side_energy_fraction"):
        out[key] = audio[key]
    out["band_fraction"] = audio["band_fraction"]
    return out


def difference(mix, ref, mix_audio, ref_audio) -> dict:
    """Mix minus reference, for two summaries and the measurements under them."""
    out = {
        key: perception.delta(ref[key], mix[key])
        for key in mix
        if key != "band_fraction"
    }
    out["band_db"] = balance(mix_audio, ref_audio)
    out["band_fraction"] = {
        band: perception.delta(ref["band_fraction"][band], mix["band_fraction"][band])
        for band in perception.BANDS
    }
    return out


def pairs(mix_sections, ref_sections):
    """Which of the reference's sections each of the mix's is compared with, and
    how they were matched: by ID where the two share any, otherwise by loudness,
    the loudest against the loudest and the quietest against the quietest."""
    ours = {s["id"]: s for s in mix_sections}
    theirs = {s["id"]: s for s in ref_sections}
    shared = [i for i in ours if i in theirs]
    if shared:
        return "id", [(ours[i], theirs[i]) for i in shared]

    def by_loudness(rows):
        heard = [s for s in rows if s["audio"]["integrated_lufs"] is not None]
        return sorted(heard, key=lambda s: -s["audio"]["integrated_lufs"])

    left, right = by_loudness(mix_sections), by_loudness(ref_sections)
    if not left or not right:
        return "none", []
    step = (len(right) - 1) / (len(left) - 1) if len(left) > 1 else 0
    matched = [(s, right[round(i * step)]) for i, s in enumerate(left)]
    order = {s["id"]: n for n, s in enumerate(mix_sections)}
    return "loudness", sorted(matched, key=lambda p: order[p[0]["id"]])


def signed(value, unit="dB") -> str:
    return f"{value:+.1f} {unit}"


def listed(rows) -> str:
    """Sections and their amounts, the largest first, in one clause."""
    rows = sorted(rows, key=lambda r: -abs(r[1]))
    text = ", ".join(f"{name} {amount}" for name, _, amount in rows[:LISTED])
    more = len(rows) - LISTED
    return text + (f" and {more} more" if more > 0 else "")


def observations(report) -> list[str]:
    """The largest differences in words, the largest first: which way and by how
    much, not what to do about it."""
    found = []  # (how many times its threshold, text)
    whole, parts = report["whole"], report["sections"]
    name = report["reference"]["name"]

    def note(size, limit, text):
        found.append((abs(size) / limit, text))

    def against(sid, part):
        other = part["reference_section"]
        return sid if other == sid else f"{sid} against {other}"

    def across(key, limit, read, phrase, amount=signed):
        """One observation for a measurement over the whole mix and its sections."""
        over_all = read(whole)
        rows = [
            (against(sid, part), read(part), amount(read(part)))
            for sid, part in parts.items()
            if read(part) is not None and abs(read(part)) >= limit
        ]
        notable = over_all is not None and abs(over_all) >= limit
        if not notable and not rows:
            return
        size = abs(over_all) if notable else max(abs(v) for _, v, _ in rows)
        text = f"{key}: "
        if notable:
            text += f"{phrase(over_all)} over the whole mix"
        if rows:
            text += ("; by section, " if notable else "by section, ") + listed(rows)
        note(size, limit, text + ".")

    d = whole["delta"]["integrated_lufs"]
    if d is not None and abs(d) >= NOTABLE["integrated_lufs"]:
        note(
            d,
            NOTABLE["integrated_lufs"],
            f"loudness: the mix is {abs(d):.1f} LU {'louder' if d > 0 else 'quieter'} "
            f"than {name}, {whole['mix']['integrated_lufs']:.1f} against "
            f"{whole['reference']['integrated_lufs']:.1f} LUFS.",
        )
    d = whole["delta"]["estimated_true_peak_dbtp"]
    if d is not None and abs(d) >= NOTABLE["estimated_true_peak_dbtp"]:
        note(
            d,
            NOTABLE["estimated_true_peak_dbtp"],
            f"true peak: the mix's is {whole['mix']['estimated_true_peak_dbtp']:.1f} dBTP "
            f"against {name}'s {whole['reference']['estimated_true_peak_dbtp']:.1f}.",
        )
    for band, (low, high) in perception.BANDS.items():
        across(
            f"{band} ({low}–{high} Hz)",
            NOTABLE["band_db"],
            lambda p, band=band: p["delta"]["band_db"][band],
            lambda v: f"{abs(v):.1f} dB {'above' if v > 0 else 'below'} the reference's balance",
        )
        for side, other in (("mix", "reference"), ("reference", "mix")):
            mine = whole[side]["band_fraction"][band]
            share = whole[other]["band_fraction"][band]
            if mine is not None and share is not None and mine < EMPTY_SHARE <= share:
                note(
                    2,
                    1,
                    f"{band} ({low}–{high} Hz): the {side} has nothing there, under "
                    f"-70 dB of its power, where the {other} has "
                    f"{10 * np.log10(share):.0f} dB of its own.",
                )
    across(
        "crest",
        NOTABLE["crest_db"],
        lambda p: p["delta"]["crest_db"],
        lambda v: f"{abs(v):.1f} dB {'more' if v > 0 else 'less'} between peak and "
        "average than the reference",
    )
    across(
        "side energy",
        NOTABLE["side_energy_fraction"],
        lambda p: p["delta"]["side_energy_fraction"],
        lambda v: f"{abs(v) * 100:.0f} points {'more' if v > 0 else 'less'} of the power "
        "in the sides than the reference",
        lambda v: f"{v * 100:+.0f} points",
    )
    rows = [
        (
            against(sid, part),
            part["delta"]["relative_lu"],
            f"{signed(part['mix']['relative_lu'], 'LU')} from the mix's loudness where "
            f"the reference's is {signed(part['reference']['relative_lu'], 'LU')} from "
            "its own",
        )
        for sid, part in parts.items()
        if part["delta"]["relative_lu"] is not None
        and abs(part["delta"]["relative_lu"]) >= NOTABLE["relative_lu"]
    ]
    if rows:
        size = max(abs(d) for _, d, _ in rows)
        note(size, NOTABLE["relative_lu"], f"contour: {listed(rows)}.")
    span = report["contour"]
    if span["delta"] is not None and abs(span["delta"]) >= NOTABLE["span_lu"]:
        note(
            span["delta"],
            NOTABLE["span_lu"],
            f"contour: the mix's sections are within {span['mix_span_lu']:.1f} LU of each "
            f"other, and {name}'s within {span['reference_span_lu']:.1f} LU.",
        )
    return [text for _, text in sorted(found, key=lambda f: -f[0])]


def span(rows):
    """How far apart the quietest and loudest of some sections are, in LU."""
    loud = [s["audio"]["integrated_lufs"] for s in rows]
    loud = [v for v in loud if v is not None]
    return max(loud) - min(loud) if len(loud) > 1 else None


def compare(source, name) -> dict:
    """A render or an audio file against a saved reference: the whole of each,
    then section by section."""
    reference = load(name)
    audio_path, folder = perception.resolve(source)
    x, rate = perception.read_audio(audio_path)
    project, manifest, offset, bounds = perception.context(folder, x, rate)
    with ThreadPoolExecutor(1) as beside:
        whole = beside.submit(perception.measure, x, rate)
        parts = [x[b["start_frame"] : b["end_frame"]] for b in bounds]
        audios = perception.measure_each(parts, rate, true_peak=False)
        mix = whole.result()
    mix_sections = [
        {
            "id": b["id"],
            "start_seconds": (offset + b["start_frame"]) / rate,
            "end_seconds": (offset + b["end_frame"]) / rate,
            "audio": audio,
        }
        for b, audio in zip(bounds, audios)
    ]
    ref_sections = in_use(reference)
    how, matched = pairs(mix_sections, ref_sections)
    ours, theirs = summary(mix), summary(reference["mix"])
    report = {
        "schema_version": VERSION,
        "kind": "reference_comparison",
        "direction": "mix minus reference",
        "mix": {
            "audio_path": str(audio_path),
            # A render's was checked against its report when the render was read.
            "audio_sha256": manifest["audio_sha256"] if manifest else digest(audio_path),
            "render_id": manifest.get("render_id") if manifest else None,
            "sample_rate": rate,
            "start_seconds": offset / rate,
            "duration_seconds": len(x) / rate,
            "tempo": project["session"]["tempo"] if project else None,
        },
        "reference": {
            "name": reference["name"],
            "path": str(directory() / reference["name"] / "reference.json"),
            "source": reference["source"]["path"],
            "source_sha256": reference["source"]["sha256"],
            "duration_seconds": reference["source"]["duration_seconds"],
            "bpm": reference["beat_map"]["tempo"]["bpm"] if reference["beat_map"] else None,
            "sections_from": "request" if reference["named"] is not None else "measurement",
        },
        "whole": {
            "mix": ours,
            "reference": theirs,
            "delta": difference(ours, theirs, mix, reference["mix"]),
        },
        "matching": {
            "sections": how,
            "method": {
                "id": "sections with the same ID on both sides",
                "loudness": "no IDs in common: the sections of each side in order of "
                "integrated loudness, the mix's loudest against the reference's loudest, "
                "its quietest against the quietest and those between spread evenly; name "
                "the reference's sections with daw reference sections to match by ID",
                "none": "a side has no sections with a loudness: the whole files alone",
            }[how],
        },
        "sections": {},
        "unmatched_sections": {
            "mix": sorted({s["id"] for s in mix_sections} - {a["id"] for a, _ in matched}),
            "reference": sorted(
                {s["id"] for s in ref_sections} - {b["id"] for _, b in matched}
            ),
        },
        "contour": {
            "mix_span_lu": span(mix_sections),
            "reference_span_lu": span(ref_sections),
        },
        "methods": {
            "measurements": "as daw listen makes them, of each whole file and each section",
            "band_db": "each band's level, mix minus reference, with the median of the "
            "seven differences taken out, so overall level reads as zero and one band "
            "moved reads as its amount; null where a side holds under -70 dB of its "
            "power in the band, or fewer than three bands compare",
            "relative_lu": "a section's integrated loudness minus its file's",
            "label": "low, mid or high: where a section's loudness sits between the "
            "reference's quietest and loudest sections",
            "observations": "differences of at least "
            + ", ".join(f"{v:g} {k}" for k, v in NOTABLE.items())
            + "; measured, not judged: a mix can differ from its reference on purpose",
            "limits": "no stems, keys, notes or arrangement are compared; a reference "
            "at another tempo or length is compared by section, not bar by bar",
        },
    }
    report["contour"]["delta"] = perception.delta(
        report["contour"]["reference_span_lu"], report["contour"]["mix_span_lu"]
    )
    for a, b in matched:
        left, right = summary(a["audio"], mix), summary(b["audio"], reference["mix"])
        report["sections"][a["id"]] = {
            "reference_section": b["id"],
            "mix": {k: a[k] for k in ("start_seconds", "end_seconds")} | left,
            "reference": {k: b[k] for k in ("start_seconds", "end_seconds", "label")} | right,
            "delta": difference(left, right, a["audio"], b["audio"]),
        }
    report["observations"] = observations(report)
    root = folder or audio_path.parent
    identity = hashlib.sha256(
        json.dumps(report, sort_keys=True).encode()
    ).hexdigest()[:16]
    output = root / "analysis" / identity / f"reference-{reference['name']}.json"
    report["report_path"] = str(output.resolve())
    atomic_text(output, json.dumps(report, indent=2, allow_nan=False) + "\n")
    return report
