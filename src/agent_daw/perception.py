"""Local render diagnostics. Measurements describe audio; they do not judge taste."""

from __future__ import annotations

import hashlib
import importlib.metadata
import json
import warnings
from itertools import pairwise
from pathlib import Path

import numpy as np
import pyloudnorm as pyln
import soundfile as sf
from scipy.signal import resample_poly, spectrogram, welch

from .engine import schedule
from .model import atomic_text, beat, digest, frame, load, project_hash

VERSION = 1
BANDS = {
    "sub": (20, 60),
    "low": (60, 250),
    "low_mid": (250, 500),
    "mid": (500, 2000),
    "high_mid": (2000, 4000),
    "high": (4000, 8000),
    "air": (8000, 20000),
}


def db(power):
    return float(10 * np.log10(power)) if power > 0 else None


def delta(a, b):
    return float(b - a) if a is not None and b is not None else None


def integrated(x, rate):
    if len(x) < round(0.4 * rate):
        return None
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        value = pyln.Meter(rate).integrated_loudness(x)
    return float(value) if np.isfinite(value) else None


def measure(x, rate):
    """dBFS uses mean-square power across channels, with a unit peak reference."""
    peak = float(np.max(np.abs(x)))
    power = float(np.mean(x * x))
    n = min(len(x), 8192)
    freq, psd = welch(x, rate, nperseg=n, axis=0)
    spectrum = psd.mean(axis=1) * rate / n
    band_power = {
        name: float(spectrum[(freq >= lo) & (freq < hi)].sum())
        for name, (lo, hi) in BANDS.items()
    }
    total = sum(band_power.values())
    correlation = side_fraction = None
    if x.shape[1] == 2:
        centered = x - x.mean(axis=0)
        energy = np.sum(centered * centered, axis=0)
        if np.all(energy > 1e-24):
            correlation = float(
                np.clip(
                    np.sum(centered[:, 0] * centered[:, 1])
                    / np.sqrt(energy[0] * energy[1]),
                    -1,
                    1,
                )
            )
        mid = np.mean(x, axis=1)
        side = (x[:, 0] - x[:, 1]) / 2
        ms = float(np.mean(mid * mid))
        ss = float(np.mean(side * side))
        if ms + ss > 0:
            side_fraction = ss / (ms + ss)
    peak_db = db(peak * peak)
    rms_db = db(power)
    over = resample_poly(x, 4, 1, axis=0)
    return {
        "duration_seconds": len(x) / rate,
        "integrated_lufs": integrated(x, rate),
        "peak_dbfs": peak_db,
        "estimated_true_peak_dbtp": db(float(np.max(np.abs(over))) ** 2),
        "rms_dbfs": rms_db,
        "crest_db": delta(rms_db, peak_db),
        "band_dbfs": {k: db(v) for k, v in band_power.items()},
        "band_fraction": {
            k: v / total if total > 0 else None for k, v in band_power.items()
        },
        "stereo_correlation": correlation,
        "side_energy_fraction": side_fraction,
        "silent": peak <= 1e-6,
        "over_range_samples": int(np.count_nonzero(np.abs(x) >= 1)),
    }


def read_audio(path):
    x, rate = sf.read(path, always_2d=True, dtype="float64")
    if not len(x) or x.shape[1] not in (1, 2) or not np.isfinite(x).all():
        raise ValueError(f"Expected nonempty finite mono/stereo audio: {path}")
    if rate not in (44100, 48000):
        raise ValueError("Perception supports 44100 or 48000 Hz audio")
    return x, rate


def resolve(source):
    source = Path(source).resolve()
    if source.is_file() and source.suffix.lower() == ".json":
        value = json.loads(source.read_text())
        if not isinstance(value, dict):
            raise ValueError("Expected a render report or render pointer JSON object")
        if "directory" in value:
            directory = Path(value["directory"])
            source = directory if directory.is_absolute() else source.parent / directory
        elif "render_id" in value:
            source = source.parent
        else:
            raise ValueError("Expected a render report or render pointer JSON")
    if source.is_dir():
        return source / "mix.wav", source
    if source.name == "mix.wav" and (source.parent / "report.json").is_file():
        return source, source.parent
    return source, None


def context(folder, x, rate):
    if folder is None:
        return None, None, 0, []
    manifest = json.loads((folder / "report.json").read_text())
    if digest(folder / "mix.wav") != manifest["audio_sha256"]:
        raise ValueError("Render mix hash mismatch")
    project = load(folder / "song.snapshot.yaml", verify_assets=False)
    if project_hash(project) != manifest["project_sha256"]:
        raise ValueError("Render snapshot hash mismatch")
    if rate != project.session.sample_rate or len(x) != manifest["mix"]["frames"]:
        raise ValueError("Render audio dimensions do not match its manifest")
    region = manifest["target"].get("source_frames")
    offset = region[0] if region else 0
    expected_end = (
        region[1]
        if region
        else frame(project.session.length_beats, project.session.tempo, rate)
    )
    if offset < 0 or expected_end - offset != len(x):
        raise ValueError("Render timeline does not match its manifest")
    sections = []
    for sec in project.sections:
        start = max(offset, frame(sec.at, project.session.tempo, rate))
        end = min(
            offset + len(x),
            frame(beat(sec.at) + beat(sec.length_beats), project.session.tempo, rate),
        )
        if end > start:
            sections.append(
                {"id": sec.id, "start_frame": start - offset, "end_frame": end - offset}
            )
    return project, manifest, offset, sections


def timeline(x, rate, project, offset):
    # One beat per energy bin, or one second when musical timing is unknown.
    if project:
        tempo = project.session.tempo
        first = int(np.floor(offset / rate * tempo / 60))
        last = int(np.ceil((offset + len(x)) / rate * tempo / 60))
        boundaries = (
            [0]
            + [
                frame(i, tempo, rate) - offset
                for i in range(first + 1, last)
                if 0 < frame(i, tempo, rate) - offset < len(x)
            ]
            + [len(x)]
        )
    else:
        boundaries = list(range(0, len(x), rate)) + [len(x)]
    bins = []
    for start, end in pairwise(boundaries):
        bins.append(
            {
                "start_seconds": (offset + start) / rate,
                "end_seconds": (offset + end) / rate,
                "at_beat": (offset + start) / rate * project.session.tempo / 60
                if project
                else None,
                "rms_dbfs": db(float(np.mean(x[start:end] ** 2))),
            }
        )
    # Ungated K-weighted 3-second windows, at 1-second hops. No padded short windows.
    weighted = x.copy()
    filters = [
        pyln.IIRfilter(4.0, 1 / np.sqrt(2), 1500.0, rate, "high_shelf"),
        pyln.IIRfilter(0.0, 0.5, 38.0, rate, "high_pass"),
    ]
    for filt in filters:
        for ch in range(weighted.shape[1]):
            weighted[:, ch] = filt.apply_filter(weighted[:, ch])
    short = []
    for start in range(0, len(x) - 3 * rate + 1, rate):
        power_db = db(
            float(np.sum(np.mean(weighted[start : start + 3 * rate] ** 2, axis=0)))
        )
        short.append(
            {
                "start_seconds": (offset + start) / rate,
                "end_seconds": (offset + start + 3 * rate) / rate,
                "lufs": power_db - 0.691 if power_db is not None else None,
            }
        )
    return {"energy": bins, "short_term_loudness": short}


def musical_context(project, manifest, offset, count, sections):
    if project is None:
        return None
    rate, tempo = project.session.sample_rate, project.session.tempo
    triggers = schedule(project)
    result = {}
    ranges = [{"id": "whole_render", "start_frame": 0, "end_frame": count}, *sections]
    for track in project.tracks:
        if track.id not in manifest["tracks"]:
            continue
        regions = []
        for region in ranges:
            start, end = offset + region["start_frame"], offset + region["end_frame"]
            hits = [
                tr
                for tr in triggers
                if tr.track == track.id and start <= tr.start < end
            ]
            placements = []
            for clip in track.clips:
                pattern = project.patterns[clip.pattern]
                for repeat in range(clip.repeats):
                    at = beat(clip.at) + repeat * beat(pattern.length_beats)
                    stop = at + beat(pattern.length_beats)
                    if (
                        frame(at, tempo, rate) < end
                        and frame(stop, tempo, rate) > start
                    ):
                        placements.append(clip.pattern)
            regions.append(
                {
                    "id": region["id"],
                    "trigger_count": len(hits),
                    "triggers_per_beat": len(hits)
                    / ((end - start) / rate * tempo / 60),
                    "pattern_occurrences": {
                        p: placements.count(p) for p in sorted(set(placements))
                    },
                }
            )
        result[track.id] = {
            "audible_in_render": not track.mute
            and (not any(t.solo for t in project.tracks) or track.solo),
            "regions": regions,
        }
    return {
        "source": "saved project schedule; triggers are not detected audio onsets; pattern IDs do not establish musical similarity",
        "tracks": result,
    }


def analyze(source):
    audio_path, folder = resolve(source)
    x, rate = read_audio(audio_path)
    project, manifest, offset, sections = context(folder, x, rate)
    report = {
        "schema_version": VERSION,
        "kind": "listen",
        "source": {
            "audio_path": str(audio_path),
            "audio_sha256": digest(audio_path),
            "render_id": manifest["render_id"] if manifest else None,
            "project_sha256": manifest["project_sha256"] if manifest else None,
            "sample_rate": rate,
            "frames": len(x),
            "channels": x.shape[1],
            "start_seconds": offset / rate,
            "tempo": project.session.tempo if project else None,
        },
        "methods": {
            "loudness": "pyloudnorm BS.1770-4 K-weighting, 400 ms absolute/relative gating; null below gate or under 400 ms",
            "short_term": "ungated K-weighted 3-second windows, 1-second hops; omitted under 3 seconds",
            "bands": "Welch power spectrum, Hann window, up to 8192 frames, 50% overlap; mean channel power",
            "band_hz": BANDS,
            "stereo": "Pearson channel correlation; side/(mid+side) energy fraction; null if undefined",
            "silence": "all samples at or below -120 dBFS peak",
            "peak": "4x polyphase oversampling estimate, not certified true peak",
            "null": "undefined or insufficient signal/duration; never a zero measurement",
            "interpretation": "diagnostics only; no musical quality score or masking diagnosis",
        },
        "dependencies": {
            name: importlib.metadata.version(name)
            for name in ("numpy", "scipy", "soundfile", "pyloudnorm", "matplotlib")
        },
        "analysis_code_sha256": digest(Path(__file__)),
        "mix": measure(x, rate),
        "timeline": timeline(x, rate, project, offset),
        "sections": {},
        "tracks": {},
        "musical_context": musical_context(project, manifest, offset, len(x), sections),
    }
    for sec in sections:
        a, b = sec["start_frame"], sec["end_frame"]
        report["sections"][sec["id"]] = {
            "start_seconds": (offset + a) / rate,
            "end_seconds": (offset + b) / rate,
            "audio": measure(x[a:b], rate),
        }
    if folder:
        for track in manifest["tracks"]:
            path = folder / "stems" / f"{track}.wav"
            stem, stem_rate = read_audio(path)
            if stem.shape != x.shape or stem_rate != rate:
                raise ValueError(f"Stem is not aligned with mix: {track}")
            expected_hash = manifest["tracks"][track].get("audio_sha256")
            actual_hash = digest(path)
            if expected_hash and expected_hash != actual_hash:
                raise ValueError(f"Stem hash mismatch: {track}")
            report["tracks"][track] = {
                "kind": manifest["tracks"][track].get("kind", "track"),
                "audio_sha256": actual_hash,
                "hash_verified_against_render": expected_hash is not None,
                "audio": measure(stem, rate),
                "sections": {
                    s["id"]: measure(stem[s["start_frame"] : s["end_frame"]], rate)
                    for s in sections
                },
            }
    return report, x


def plot_listen(report, x, output):
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.figure import Figure

    rate = report["source"]["sample_rate"]
    offset = report["source"]["start_seconds"]
    tempo = report["source"]["tempo"]
    scale = tempo / 60 if tempo else 1
    fig = Figure(figsize=(12, 7), layout="constrained")
    FigureCanvasAgg(fig)
    top, bottom = fig.subplots(2, 1, sharex=True)
    bins = report["timeline"]["energy"]
    top.step(
        [b["start_seconds"] * scale for b in bins] + [bins[-1]["end_seconds"] * scale],
        [b["rms_dbfs"] if b["rms_dbfs"] is not None else -120 for b in bins]
        + [bins[-1]["rms_dbfs"] if bins[-1]["rms_dbfs"] is not None else -120],
        where="post",
    )
    top.set(
        ylabel="RMS dBFS",
        title="Rendered audio energy (silence displayed at −120 dBFS)",
    )
    # Average channel powers: antiphase stereo must not disappear through mono cancellation.
    n = min(len(x), 4096)
    f, t, power = spectrogram(x, rate, nperseg=n, noverlap=n // 2, axis=0)
    power = power.mean(axis=1)
    keep = f >= 20
    db_power = 10 * np.log10(np.maximum(power[keep], 1e-12))
    # pcolormesh needs at least two time centers for useful geometry on tiny inputs.
    if len(t) < 2:
        t = np.array([0, len(x) / rate])
        db_power = np.repeat(db_power, 2, axis=1)
    if np.count_nonzero(keep) >= 2:
        mesh = bottom.pcolormesh(
            (t + offset) * scale,
            f[keep],
            db_power,
            shading="nearest",
            vmin=-100,
            vmax=-20,
            cmap="magma",
        )
        fig.colorbar(mesh, ax=bottom, label="Power spectral density (dB/Hz)")
    else:
        bottom.text(
            0.5,
            0.5,
            "Audio too short for a spectrogram",
            transform=bottom.transAxes,
            ha="center",
        )
    bottom.set(
        yscale="log",
        ylim=(20, rate / 2),
        ylabel="Frequency (Hz)",
        xlabel="Quarter-note beat (zero-based)" if tempo else "Seconds",
    )
    for sec_id, sec in report["sections"].items():
        pos = sec["start_seconds"] * scale
        top.axvline(pos, color="gray", alpha=0.5)
        top.text(
            pos, 0.98, sec_id, transform=top.get_xaxis_transform(), va="top", fontsize=8
        )
    for ax in (top, bottom):
        ax.set_xlim(offset * scale, (offset + len(x) / rate) * scale)
        ax.grid(alpha=0.15)
        if tempo:
            start = int(np.ceil(offset * scale))
            stop = int(np.ceil((offset + len(x) / rate) * scale))
            # Beat grid on short views; bar grid on long views, bounded for full songs.
            step = (
                1
                if stop - start <= 32
                else max(4, 4 * int(np.ceil((stop - start) / 128)))
            )
            for pos in range(start, stop, step):
                ax.axvline(pos, color="gray", alpha=0.15, linewidth=0.5)
    path = output / "overview.png"
    fig.savefig(path, dpi=140)
    return str(path.resolve())


def listen(source, images=True):
    report, audio = analyze(source)
    audio_path, folder = resolve(source)
    root = folder or audio_path.parent
    identity = hashlib.sha256(json.dumps(report, sort_keys=True).encode()).hexdigest()[
        :16
    ]
    output = root / "analysis" / identity
    output.mkdir(parents=True, exist_ok=True)
    report["images"] = [plot_listen(report, audio, output)] if images else []
    report["report_path"] = str((output / "listen.json").resolve())
    atomic_text(
        output / "listen.json", json.dumps(report, indent=2, allow_nan=False) + "\n"
    )
    return report


DB_FIELDS = ("integrated_lufs", "peak_dbfs", "estimated_true_peak_dbtp", "rms_dbfs")


def metric_diff(a, b, gain):
    raw = {
        key: delta(a[key], b[key])
        for key in (
            *DB_FIELDS,
            "crest_db",
            "stereo_correlation",
            "side_energy_fraction",
        )
    }
    raw["band_dbfs"] = {k: delta(a["band_dbfs"][k], b["band_dbfs"][k]) for k in BANDS}
    raw["band_fraction"] = {
        k: delta(a["band_fraction"][k], b["band_fraction"][k]) for k in BANDS
    }
    # Gated LUFS is not strictly gain-invariant near the absolute gate. Only
    # publish analytical offsets for ungated power/peak metrics.
    matched = {
        key: raw[key] + gain if raw[key] is not None and gain is not None else None
        for key in DB_FIELDS
        if key != "integrated_lufs"
    }
    matched["band_dbfs"] = {
        k: v + gain if v is not None and gain is not None else None
        for k, v in raw["band_dbfs"].items()
    }
    return {
        "actual_delta": raw,
        "loudness_matched_delta": matched,
        "silent_before": a["silent"],
        "silent_after": b["silent"],
    }


def compare(before, after, images=True):
    a = listen(before, images=images)
    b = listen(after, images=images)
    gain = delta(b["mix"]["integrated_lufs"], a["mix"]["integrated_lufs"])
    report = {
        "schema_version": VERSION,
        "kind": "compare",
        "before": a["report_path"],
        "after": b["report_path"],
        "direction": "after minus before",
        "matching": {
            "after_gain_db": gain,
            "method": "one global mix-integrated LUFS offset applied analytically to after mix, tracks and sections; no audio rewritten; null if either loudness is undefined; matched gated LUFS omitted because absolute gating may change",
        },
        "mix": metric_diff(a["mix"], b["mix"], gain),
        "sections": {},
        "tracks": {},
        "timeline": [],
        "musical_context": {
            "before": a["musical_context"],
            "after": b["musical_context"],
        },
    }
    same_timeline = all(
        a["source"][k] == b["source"][k]
        for k in ("sample_rate", "frames", "start_seconds", "tempo")
    )
    report["timeline_aligned"] = same_timeline
    report["whole_render_comparison"] = (
        "descriptive whole-file aggregates; only aligned timelines support location-by-location comparison"
    )
    report["section_alignment"] = (
        "same ID and same absolute start/end seconds; other sections are listed as unmatched"
    )
    common = set(a["sections"]) & set(b["sections"])
    aligned = {
        k
        for k in common
        if all(
            a["sections"][k][v] == b["sections"][k][v]
            for v in ("start_seconds", "end_seconds")
        )
    }
    report["unmatched_sections"] = {
        "before": sorted(set(a["sections"]) - aligned),
        "after": sorted(set(b["sections"]) - aligned),
    }
    for name in sorted(aligned):
        sa, sb = a["sections"][name], b["sections"][name]
        report["sections"][name] = {
            "start_seconds": sa["start_seconds"],
            "end_seconds": sa["end_seconds"],
            **metric_diff(sa["audio"], sb["audio"], gain),
        }
    report["unmatched_tracks"] = {
        "before": sorted(set(a["tracks"]) - set(b["tracks"])),
        "after": sorted(set(b["tracks"]) - set(a["tracks"])),
    }
    for name in sorted(set(a["tracks"]) & set(b["tracks"])):
        ta, tb = a["tracks"][name], b["tracks"][name]
        report["tracks"][name] = {
            **metric_diff(ta["audio"], tb["audio"], gain),
            "sections": {
                s: metric_diff(ta["sections"][s], tb["sections"][s], gain)
                for s in sorted(aligned)
            },
        }
    report["musical_context_delta"] = {}
    if a["musical_context"] and b["musical_context"]:
        # Returns have stems but schedule no triggers, so they have no context.
        common = set(a["musical_context"]["tracks"]) & set(b["musical_context"]["tracks"])
        for name in sorted(common):
            ra = a["musical_context"]["tracks"][name]["regions"]
            rb = b["musical_context"]["tracks"][name]["regions"]
            changes = []
            for index, left in enumerate(ra):
                # First entry is the aggregate; named sections follow. Avoid
                # confusing an actual section named whole_render with it.
                candidates = (
                    rb[:1]
                    if index == 0 and same_timeline
                    else rb[1:]
                    if index > 0 and left["id"] in aligned
                    else []
                )
                for right in candidates:
                    if left["id"] == right["id"]:
                        changes.append(
                            {
                                "id": left["id"],
                                "trigger_count_delta": right["trigger_count"]
                                - left["trigger_count"],
                                "triggers_per_beat_delta": right["triggers_per_beat"]
                                - left["triggers_per_beat"],
                                "pattern_occurrences_before": left[
                                    "pattern_occurrences"
                                ],
                                "pattern_occurrences_after": right[
                                    "pattern_occurrences"
                                ],
                            }
                        )
            report["musical_context_delta"][name] = changes
    if same_timeline:
        for ta, tb in zip(a["timeline"]["energy"], b["timeline"]["energy"]):
            change = delta(ta["rms_dbfs"], tb["rms_dbfs"])
            report["timeline"].append(
                {
                    "start_seconds": ta["start_seconds"],
                    "end_seconds": ta["end_seconds"],
                    "at_beat": ta["at_beat"],
                    "rms_delta_db": change,
                    "matched_rms_delta_db": change + gain
                    if change is not None and gain is not None
                    else None,
                }
            )
    output = Path(b["report_path"]).parent / (
        "compare-"
        + hashlib.sha256(Path(a["report_path"]).read_bytes()).hexdigest()[:16]
    )
    output.mkdir(parents=True, exist_ok=True)
    report["images"] = []
    if images and same_timeline:
        report["images"] = [plot_compare(a, b, gain, output)]
    report["report_path"] = str((output / "compare.json").resolve())
    atomic_text(
        output / "compare.json", json.dumps(report, indent=2, allow_nan=False) + "\n"
    )
    return report


def plot_compare(a, b, gain, output):
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.figure import Figure

    fig = Figure(figsize=(12, 6), layout="constrained")
    FigureCanvasAgg(fig)
    axes = fig.subplots(2, 1, sharex=True)
    tempo = a["source"]["tempo"]
    scale = tempo / 60 if tempo else 1
    for ax, matched in zip(axes, (False, True)):
        for report, label, adjustment in (
            (a, "Before", 0),
            (b, "After", gain if matched else 0),
        ):
            if adjustment is None:
                continue
            bins = report["timeline"]["energy"]
            times = [v["start_seconds"] * scale for v in bins] + [
                bins[-1]["end_seconds"] * scale
            ]
            values = [
                v["rms_dbfs"] + adjustment if v["rms_dbfs"] is not None else np.nan
                for v in bins
            ]
            ax.step(times, values + [values[-1]], where="post", label=label)
        ax.set(
            ylabel="RMS dBFS", title="Loudness-matched" if matched else "Actual levels"
        )
        if matched and gain is None:
            ax.set_title("Loudness matching unavailable: undefined integrated loudness")
        for name, sec in a["sections"].items():
            pos = sec["start_seconds"] * scale
            ax.axvline(pos, color="gray", alpha=0.3)
            ax.text(
                pos,
                0.95,
                name,
                transform=ax.get_xaxis_transform(),
                fontsize=8,
                va="top",
            )
        ax.grid(alpha=0.2)
        ax.set_xlim(
            a["source"]["start_seconds"] * scale,
            (a["source"]["start_seconds"] + a["mix"]["duration_seconds"]) * scale,
        )
        ax.legend(loc="lower right")
    axes[-1].set_xlabel("Quarter-note beat (zero-based)" if tempo else "Seconds")
    path = output / "comparison.png"
    fig.savefig(path, dpi=140)
    return str(path.resolve())
