"""Offline sampler with explicit block voice state and sample-accurate scheduling."""

from __future__ import annotations
from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path
import hashlib
import json
import os
import platform
import time
import tempfile
import importlib.metadata
import numpy as np
import soundfile as sf
from scipy.signal import resample_poly
from . import __version__
from .effects import Chain
from .model import (
    Project,
    Pad,
    Event,
    beat,
    frame,
    midi,
    load,
    project_hash,
    atomic_text,
    digest,
    save,
)


@dataclass
class Trigger:
    start: int
    track: str
    pad: str
    event: Event
    velocity_scale: float
    cutoff: int | None = None


def schedule(p: Project) -> list[Trigger]:
    triggers = []
    sr, tempo = p.session.sample_rate, p.session.tempo
    for t in p.tracks:
        for c in t.clips:
            pattern = p.patterns[c.pattern]
            for r in range(c.repeats):
                base = beat(c.at) + r * beat(pattern.length_beats)
                for e in pattern.expanded():
                    at = base + beat(e.at)
                    start = frame(at, tempo, sr)
                    cutoff = (
                        frame(at + beat(e.duration), tempo, sr)
                        if e.duration is not None and t.pads[e.pad].mode == "gate"
                        else None
                    )
                    triggers.append(
                        Trigger(start, t.id, e.pad, e, c.velocity_scale, cutoff)
                    )
    triggers.sort(key=lambda e: (e.start, e.track, e.pad))
    # Choke groups are local to a track. The incoming hit releases the preceding voice.
    last = {}
    tracks = {t.id: t for t in p.tracks}
    for tr in triggers:
        pad = tracks[tr.track].pads[tr.pad]
        if pad.choke_group:
            key = (tr.track, pad.choke_group)
            if key in last:
                old = last[key]
                old.cutoff = (
                    min(old.cutoff, tr.start) if old.cutoff is not None else tr.start
                )
            last[key] = tr
    return triggers


def stereo_pan(data, pan):
    if data.shape[1] == 1:
        angle = (pan + 1) * np.pi / 4
        return data * np.array([np.cos(angle), np.sin(angle)])
    # Stereo balance: retain center image, attenuate the opposite channel.
    return data * np.array([min(1, 1 - pan), min(1, 1 + pan)])


@dataclass
class Voice:
    audio: np.ndarray
    attack: int
    release: int
    gate: int | None
    cursor: int = 0

    @property
    def length(self):
        return (
            min(len(self.audio), self.gate + self.release)
            if self.gate is not None
            else len(self.audio)
        )

    def process(self, size):
        stop = min(self.cursor + size, self.length)
        i = np.arange(self.cursor, stop)
        x = self.audio[self.cursor : stop].copy()
        env = np.ones(len(i))
        if self.attack:
            env *= np.minimum(1, i / self.attack)
        # Every natural sample ending fades. A gate releases at the note-off frame.
        natural = min(self.release, self.length)
        if natural:
            env *= np.minimum(1, np.maximum(0, (self.length - 1 - i) / max(1, natural)))
        x *= env[:, None]
        self.cursor = stop
        return x


class Sampler:
    def __init__(self, project: Project, directory: Path):
        self.project = project
        self.directory = directory
        self.original = {}
        self.prepared = {}

    def prepare(self, pad: Pad, event: Event):
        asset = self.project.samples[pad.sample]
        if pad.sample not in self.original:
            audio, sr = sf.read(
                self.directory / asset.path, always_2d=True, dtype="float64"
            )
            if (
                audio.shape[1] not in (1, 2)
                or not len(audio)
                or not np.isfinite(audio).all()
            ):
                raise ValueError(f"{pad.sample}: expected finite mono/stereo audio")
            self.original[pad.sample] = (audio, sr)
        x, source_sr = self.original[pad.sample]
        semitones = pad.transpose + event.transpose
        if event.note:
            semitones += midi(event.note) - midi(asset.root_note)
        speed = 2 ** (semitones / 12)
        if pad.source_bpm:
            speed *= self.project.session.tempo / pad.source_bpm
        key = (
            pad.sample,
            pad.start_seconds,
            pad.end_seconds,
            pad.reverse,
            pad.mono,
            speed,
        )
        if key not in self.prepared:
            start = round(pad.start_seconds * source_sr)
            end = (
                round(pad.end_seconds * source_sr)
                if pad.end_seconds is not None
                else len(x)
            )
            if start >= len(x) or end > len(x):
                raise ValueError(f"{pad.sample}: trim outside sample")
            y = x[start:end]
            if pad.reverse:
                y = y[::-1]
            if pad.mono:
                y = y.mean(axis=1, keepdims=True)
            ratio = Fraction(
                self.project.session.sample_rate / source_sr / speed
            ).limit_denominator(8192)
            if ratio != 1:
                y = resample_poly(y, ratio.numerator, ratio.denominator, axis=0)
            self.prepared[key] = y
        return self.prepared[key]

    def voice(self, pad: Pad, tr: Trigger):
        y = self.prepare(pad, tr.event)
        y = (
            stereo_pan(y, pad.pan)
            * 10 ** (pad.gain_db / 20)
            * min(1, tr.event.velocity / 127 * tr.velocity_scale)
        )
        rate = self.project.session.sample_rate
        return Voice(
            y,
            round(pad.attack_ms * rate / 1000),
            round(pad.release_ms * rate / 1000),
            None if tr.cutoff is None else max(0, tr.cutoff - tr.start),
        )


def render_track(p: Project, track, triggers, sampler, block_size=4096):
    """Sum a track's voices before its inserts, gain and pan."""
    total = frame(p.session.length_beats, p.session.tempo, p.session.sample_rate)
    out = np.zeros((total, 2), dtype=np.float64)
    pending = [x for x in triggers if x.track == track.id]
    active = []
    index = 0
    for start in range(0, total, block_size):
        end = min(start + block_size, total)
        while index < len(pending) and pending[index].start < end:
            tr = pending[index]
            active.append((tr.start, sampler.voice(track.pads[tr.pad], tr)))
            index += 1
        alive = []
        for onset, voice in active:
            offset = max(start, onset)
            chunk = voice.process(end - offset)
            out[offset : offset + len(chunk)] += chunk
            if voice.cursor < voice.length:
                alive.append((onset, voice))
        active = alive
    return out


def metrics(x, rate):
    peak = float(np.max(np.abs(x))) if x.size else 0
    rms = float(np.sqrt(np.mean(x * x))) if x.size else 0
    # Oversampled peak is an estimate, not a certified true-peak meter.
    oversampled = resample_poly(x, 4, 1, axis=0)
    true_peak = float(np.max(abs(oversampled))) if x.size else 0
    return {
        "frames": len(x),
        "duration_seconds": len(x) / rate,
        "sample_rate": rate,
        "channels": 2,
        "peak_dbfs": float(20 * np.log10(max(peak, 1e-12))),
        "estimated_true_peak_dbtp": float(20 * np.log10(max(true_peak, 1e-12))),
        "rms_dbfs": float(20 * np.log10(max(rms, 1e-12))),
        "over_range_samples": int(np.count_nonzero(abs(x) >= 1)),
        "dc_offset": float(x.mean()),
        "finite": bool(np.isfinite(x).all()),
    }


def wav_atomic(path, x, sr, subtype):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, suffix=".wav")
    os.close(fd)
    try:
        sf.write(tmp, x, sr, subtype=subtype)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def render(
    path: Path,
    output: Path | None = None,
    block_size=4096,
    track_id: str | None = None,
    section: str | None = None,
):
    if block_size <= 0:
        raise ValueError("block_size must be positive")
    before = time.perf_counter()
    p = load(path)
    rate = p.session.sample_rate
    if track_id is not None and track_id not in {t.id for t in p.tracks}:
        raise ValueError(f"Unknown track: {track_id}")
    region = None
    if section is not None:
        matches = [s for s in p.sections if s.id == section]
        if len(matches) != 1:
            raise ValueError(f"Unknown or ambiguous section: {section}")
        sec = matches[0]
        region = (
            frame(sec.at, p.session.tempo, rate),
            frame(beat(sec.at) + beat(sec.length_beats), p.session.tempo, rate),
        )
    total = frame(p.session.length_beats, p.session.tempo, rate)
    if total < 1:
        raise ValueError("Session length must be at least one audio frame")
    if region and region[1] <= region[0]:
        raise ValueError("Section length must be at least one audio frame")
    if total > rate * 60 * 15:
        raise ValueError("MVP render limit is 15 minutes")
    # Resolve all selected sources before producing any new artifacts.
    sampler = Sampler(p, path.parent)
    triggers = schedule(p)
    tracks, effect_reports, keys = {}, {}, {}
    needed = (
        {t.id for t in p.tracks}
        if track_id is None
        else {track_id} | p.sidechain_sources(track_id)
    )
    sources = {s for t in p.tracks for s in t.sidechains()}
    any_solo = any(t.solo for t in p.tracks)
    mix = np.zeros((total, 2), dtype=np.float64)
    for track in p.render_order():
        if track.id not in needed:
            continue
        chain = Chain(track.effects, rate)
        x = chain.run(
            render_track(p, track, triggers, sampler, block_size),
            {s: keys[s] for s in track.sidechains()},
            block_size,
        )
        # Sidechain keys are post-insert and pre-fader, so gain and mute never move them.
        if track.id in sources:
            keys[track.id] = x
        if track_id is not None and track.id != track_id:
            continue
        effect_reports[track.id] = chain.report()
        x = stereo_pan(x, track.pan) * 10 ** (track.gain_db / 20)
        if track.mute or (any_solo and not track.solo):
            x[:] = 0
        tracks[track.id] = x
        mix += x
    tracks = {t.id: tracks[t.id] for t in p.tracks if t.id in tracks}
    master = 10 ** (p.session.master_gain_db / 20)
    fade = min(total, round(p.session.end_fade_ms * rate / 1000))
    fader = np.ones(total)
    if fade:
        fader[-fade:] = np.linspace(1, 0, fade)
    envelope = fader * master
    # Track previews are the track's stem, so they omit the master chain.
    master_chain = Chain([] if track_id else p.master.effects, rate)
    if master_chain.devices:
        mix = master_chain.run(mix * master, block_size=block_size)
        mix *= fader[:, None]
    else:
        mix *= envelope[:, None]
    if region:
        mix = mix[region[0] : region[1]]
    report = metrics(mix, rate)
    if not report["finite"] or report["over_range_samples"]:
        raise ValueError(
            f"Unsafe PCM export: peak {report['peak_dbfs']:.2f} dBFS; "
            "lower master_gain_db or add a master limiter"
        )
    fingerprint = project_hash(p)
    engine_hash = hashlib.sha256(
        b"".join(f.read_bytes() for f in sorted(Path(__file__).parent.glob("*.py")))
    ).hexdigest()
    sample_hashes = {n: digest(path.parent / s.path) for n, s in p.samples.items()}
    dependencies = {
        n: importlib.metadata.version(n)
        for n in ["numpy", "scipy", "soundfile", "PyYAML", "pydantic"]
    }
    render_id = hashlib.sha256(
        json.dumps(
            {
                "project": fingerprint,
                "engine": engine_hash,
                "samples": sample_hashes,
                "dependencies": dependencies,
                "track": track_id,
                "section": section,
            },
            sort_keys=True,
        ).encode()
    ).hexdigest()
    output = output or path.parent / "renders" / render_id[:12]
    if output.exists() and any(output.iterdir()):
        report_path = output / "report.json"
        if (
            not report_path.is_file()
            or json.loads(report_path.read_text()).get("render_id") != render_id
        ):
            raise ValueError(
                "Output directory contains a different render; choose a fresh --output directory"
            )
    output.mkdir(parents=True, exist_ok=True)
    wav_atomic(output / "mix.wav", mix, rate, "PCM_24")
    track_reports = {}
    for name, x in tracks.items():
        x *= envelope[:, None]
        if region:
            x = x[region[0] : region[1]]
        # Float stems retain headroom and sum to the bus before master effects.
        wav_atomic(output / "stems" / f"{name}.wav", x, rate, "FLOAT")
        track_reports[name] = {
            "audio_sha256": digest(output / "stems" / f"{name}.wav"),
            "peak_dbfs": float(20 * np.log10(max(float(np.max(abs(x))), 1e-12))),
            "events": sum(tr.track == name for tr in triggers),
            "effects": effect_reports[name],
        }
    manifest = {
        "engine_version": __version__,
        "python": platform.python_version(),
        "platform": platform.platform(),
        "dependencies": dependencies,
        "project_sha256": fingerprint,
        "sample_sha256": sample_hashes,
        "engine_sha256": engine_hash,
        "render_id": render_id,
        "target": {"track": track_id, "section": section, "source_frames": region},
        "project_file": str(path.resolve()),
        "audio_sha256": digest(output / "mix.wav"),
        "render_seconds": time.perf_counter() - before,
        "mix": report,
        "tracks": track_reports,
        "sections": [s.model_dump() for s in p.sections],
        "tail_policy": "truncate at session length with explicit end fade",
        "master_effects": master_chain.report(),
        "stems_sum_to_mix": not master_chain.devices,
        "stem_policy": "post-insert, post-track and post-master gain and fade, before master effects; float WAV, same start and length",
        "sidechain_policy": "key is the source track after its inserts, before its gain, pan, mute and solo",
        "pitch_policy": "bandlimited repitch; pitch changes duration; source_bpm also repitches",
    }
    atomic_text(output / "report.json", json.dumps(manifest, indent=2) + "\n")
    save(p, output / "song.snapshot.yaml")
    latest_name = (
        "latest.json" if not track_id and not section else "latest-preview.json"
    )
    atomic_text(
        path.parent / "renders" / latest_name,
        json.dumps(
            {
                "directory": str(output.resolve()),
                "mix": str((output / "mix.wav").resolve()),
                "project_sha256": fingerprint,
            },
            indent=2,
        )
        + "\n",
    )
    return {
        "directory": str(output.resolve()),
        "mix_path": str((output / "mix.wav").resolve()),
        **manifest,
    }
