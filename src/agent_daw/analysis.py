"""Audio-derived sample measurements: pitch, onsets, tempo and loop/one-shot kind.

Every value is measured from decoded audio. Filename hints are only echoed for
comparison. Pitch is a monophonic estimate; chords and noisy material are reported
with low confidence rather than as facts.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

import numpy as np
import soundfile as sf

from .model import digest

ANALYZER = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()[:12]
NOTES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
MIN_HZ, MAX_HZ = 25.0, 2000.0
PITCH_SECONDS = 10.0  # analysed from the first sound onward
RHYTHM_SECONDS = 120.0
SILENCE_DB = -50.0  # relative to the file's peak
YIN_THRESHOLD = 0.15
VOICED_APERIODICITY = 0.3
MIN_BPM, MAX_BPM = 50.0, 220.0
PREFERRED_BPM = (85.0, 175.0)
BARS = (1, 2, 4, 8, 16, 32, 64, 128)
HOP = 512
N_FFT = 2048


def note_name(midi: float) -> str:
    n = int(round(midi))
    return f"{NOTES[n % 12]}{n // 12 - 1}"


def hz_to_midi(hz):
    return 69 + 12 * np.log2(np.asarray(hz, dtype=float) / 440.0)


def _read(path: Path, seconds: float):
    info = sf.info(path)
    frames = min(info.frames, round(seconds * info.samplerate))
    x, sr = sf.read(path, frames=frames, always_2d=True, dtype="float64")
    if not len(x) or not np.isfinite(x).all():
        raise ValueError(f"Empty or nonfinite audio: {path}")
    return x, sr, info


def _bounds(mono: np.ndarray, sr: int):
    level = np.abs(mono)
    peak = float(level.max())
    if peak <= 10 ** (-120 / 20):
        return None
    active = np.flatnonzero(level >= peak * 10 ** (SILENCE_DB / 20))
    return active[0], active[-1] + 1, peak


def _yin(frames: np.ndarray, sr: int):
    """YIN cumulative-mean-normalized difference per frame; returns (hz, aperiodicity)."""
    max_lag = int(np.ceil(sr / MIN_HZ))
    min_lag = max(2, int(sr / MAX_HZ))
    w = frames.shape[1] - max_lag
    size = 1 << int(np.ceil(np.log2(frames.shape[1] + w)))
    head = np.fft.rfft(frames[:, :w], size)
    full = np.fft.rfft(frames, size)
    cross = np.fft.irfft(np.conj(head) * full, size)[:, : max_lag + 1]
    sq = np.cumsum(np.pad(frames**2, ((0, 0), (1, 0))), axis=1)
    lags = np.arange(max_lag + 1)
    energy = sq[:, lags + w] - sq[:, lags]
    diff = np.maximum(energy[:, :1] + energy - 2 * cross, 0)
    cum = np.cumsum(diff[:, 1:], axis=1)
    cmnd = np.ones_like(diff)
    with np.errstate(divide="ignore", invalid="ignore"):
        cmnd[:, 1:] = np.where(cum > 0, diff[:, 1:] * lags[1:] / cum, 1.0)
    hz, aperiodicity = [], []
    for row in cmnd:
        region = row[min_lag : max_lag + 1]
        below = np.flatnonzero(region < YIN_THRESHOLD)
        if len(below):
            i = below[0]
            while i + 1 < len(region) and region[i + 1] < region[i]:
                i += 1
        else:
            i = int(np.argmin(region))
        tau = i + min_lag
        shift = 0.0
        if 0 < tau < max_lag:
            a, b, c = row[tau - 1], row[tau], row[tau + 1]
            denom = a - 2 * b + c
            if denom > 0:
                shift = 0.5 * (a - c) / denom
        hz.append(sr / (tau + shift))
        aperiodicity.append(float(row[tau]))
    return np.array(hz), np.array(aperiodicity)


def _harmonic_support(x: np.ndarray, sr: int, f0: float) -> float:
    """Strongest level near f0 or 2*f0, in dB relative to the spectrum's peak."""
    spectrum = np.abs(np.fft.rfft(x * np.hanning(len(x))))
    freqs = np.fft.rfftfreq(len(x), 1 / sr)
    top = max(float(spectrum.max()), 1e-30)
    near = [
        spectrum[(freqs >= k * f0 * 0.97) & (freqs <= k * f0 * 1.03)] for k in (1, 2)
    ]
    level = max((float(b.max()) for b in near if len(b)), default=0.0)
    return float(20 * np.log10(max(level, 1e-30) / top))


def measure_pitch(mono: np.ndarray, sr: int) -> dict:
    """Median fundamental over periodic frames. Frames are gated to 30 dB below the loudest."""
    bounds = _bounds(mono, sr)
    result = {
        "pitched": False,
        "frequency_hz": None,
        "midi": None,
        "note": None,
        "cents": None,
        "confidence": 0.0,
        "spread_cents": None,
        "voiced_fraction": 0.0,
        "fundamental_support_db": None,
        "method": "YIN, monophonic, median of periodic frames",
    }
    if bounds is None:
        return result
    start, end, _ = bounds
    x = mono[start : min(end, start + round(PITCH_SECONDS * sr))]
    length = 2 * int(np.ceil(sr / MIN_HZ))
    hop = max(1, sr // 50)
    if len(x) < length:
        return result
    starts = np.arange(0, len(x) - length + 1, hop)
    frames = np.stack([x[s : s + length] for s in starts])
    rms = np.sqrt(np.mean(frames**2, axis=1))
    frames = frames[rms >= rms.max() * 10 ** (-30 / 20)]
    hz, ap = _yin(frames, sr)
    voiced = (ap < VOICED_APERIODICITY) & (hz >= MIN_HZ) & (hz <= MAX_HZ)
    fraction = float(voiced.mean())
    result["voiced_fraction"] = round(fraction, 3)
    if not voiced.any():
        return result
    midis = hz_to_midi(hz[voiced])
    center = float(np.median(midis))
    q1, q3 = np.percentile(midis, [25, 75])
    confidence = fraction * (1 - float(np.median(ap[voiced])))
    confidence *= float(np.clip(1 - (q3 - q1) / 2, 0, 1))  # unstable pitch lowers trust
    f0 = float(440 * 2 ** ((center - 69) / 12))
    support = _harmonic_support(x, sr, f0)
    if support < -30:
        # Periodic but no energy at f0 or 2*f0: a chord or missing fundamental.
        confidence *= 0.25
    nearest = int(round(center))
    result.update(
        pitched=bool(fraction >= 0.5 and confidence >= 0.4),
        fundamental_support_db=round(support, 1),
        frequency_hz=round(f0, 3),
        midi=round(center, 3),
        note=note_name(center),
        cents=round((center - nearest) * 100, 1),
        confidence=round(confidence, 3),
        spread_cents=round(float(q3 - q1) * 100, 1),
    )
    return result


def onsets(mono: np.ndarray, sr: int):
    """Spectral-flux onset times (seconds), normalized strengths and the flux curve."""
    x = np.pad(mono, (N_FFT // 2, N_FFT // 2))
    count = 1 + max(0, (len(x) - N_FFT) // HOP)
    window = np.hanning(N_FFT)
    # A full-scale sinusoid peaks near N_FFT/4 with a Hann window.
    reference = max(float(np.abs(mono).max()), 1e-12) * N_FFT / 4
    flux, previous = np.zeros(count), 0.0
    for first in range(0, count, 1024):  # bounded memory for long files
        rows = np.arange(first, min(count, first + 1024))
        idx = np.arange(N_FFT)[None, :] + HOP * rows[:, None]
        logmag = np.log1p(
            100 * np.abs(np.fft.rfft(x[idx] * window, axis=1)) / reference
        )
        rise = np.diff(
            logmag, axis=0, prepend=np.broadcast_to(previous, logmag[:1].shape)
        )
        flux[rows] = np.maximum(rise, 0).sum(axis=1)
        previous = logmag[-1]
    if flux.max() <= 0:
        return np.array([]), np.array([]), flux
    flux = flux / flux.max()
    threshold = max(0.1, float(flux.mean() + 0.5 * flux.std()))
    spacing = max(1, round(0.05 * sr / HOP))
    peaks = []
    for i in np.flatnonzero(flux >= threshold):
        lo, hi = max(0, i - spacing), min(len(flux), i + spacing + 1)
        if flux[i] == flux[lo:hi].max() and (not peaks or i - peaks[-1] > spacing):
            peaks.append(i)
    peaks = np.array(peaks, dtype=int)
    return peaks * HOP / sr, flux[peaks], flux


def _grid_fit(times, weights, bpm, origin=0.0):
    sixteenth = 15.0 / bpm
    tolerance = min(0.03, sixteenth / 4)
    phase = (times - origin) / sixteenth
    error = np.abs(phase - np.round(phase)) * sixteenth
    return float(np.sum(weights * (error <= tolerance)) / max(np.sum(weights), 1e-12))


def _autocorrelation(flux):
    f = flux - flux.mean()
    size = 1 << int(np.ceil(np.log2(2 * len(f))))
    acf = np.fft.irfft(np.abs(np.fft.rfft(f, size)) ** 2, size)[: len(f)]
    return acf / acf[0] if acf[0] > 0 else np.zeros(len(f))


def _periodicity(acf, sr, bpm):
    lag = 60.0 / bpm * sr / HOP
    values = [acf[round(k * lag)] for k in (1, 2, 4) if round(k * lag) < len(acf)]
    return float(np.mean(values)) if values else 0.0


def _refine(times, weights, bpm, origin):
    """Tempo within ±1.5 BPM minimizing weighted squared distance to a sixteenth grid."""
    trial = np.arange(bpm - 1.5, bpm + 1.5, 0.01)[:, None]
    phase = (times[None, :] - origin) * trial / 15.0
    error = ((phase - np.round(phase)) ** 2 * weights).sum(axis=1)
    return float(trial[int(np.argmin(error)), 0])


def measure_rhythm(mono: np.ndarray, sr: int, bpm_hint=None) -> dict:
    duration = len(mono) / sr
    times, weights, flux = onsets(mono, sr)
    acf = _autocorrelation(flux)
    candidates = []
    for bars in BARS:
        bpm = 60 * 4 * bars / duration
        if MIN_BPM <= bpm <= MAX_BPM:
            candidates.append({"bpm": bpm, "bars": bars, "fits_length": True})
    origin = float(times[0]) if len(times) else 0.0
    if len(times) >= 3:
        # Files not cut to whole bars: periodicity, anchored on the first onset.
        grid = np.arange(MIN_BPM, MAX_BPM + 0.01, 0.5)
        found = float(max(grid, key=lambda b: _periodicity(acf, sr, b)))
        for bpm in (found / 2, found, found * 2):
            bpm = _refine(times, weights, bpm, origin)
            if MIN_BPM <= bpm <= MAX_BPM and all(
                abs(bpm - c["bpm"]) > 0.05 for c in candidates
            ):
                candidates.append({"bpm": bpm, "bars": None, "fits_length": False})
    for c in candidates:
        c["grid_fit"] = (
            _grid_fit(times, weights, c["bpm"], 0.0 if c["fits_length"] else origin)
            if len(times)
            else 0.0
        )
        c["periodicity"] = _periodicity(acf, sr, c["bpm"])
        preferred = PREFERRED_BPM[0] <= c["bpm"] <= PREFERRED_BPM[1]
        # Half/double tempo often fits equally; prefer a common range, an exact
        # whole-bar file length, then periodicity.
        c["score"] = (
            c["grid_fit"]
            + 0.15 * preferred
            + 0.02 * c["fits_length"]
            + 0.05 * c["periodicity"]
        )
    candidates.sort(key=lambda c: -c["score"])
    best = candidates[0] if candidates else None
    decays = False
    if len(mono) >= 4:
        q = len(mono) // 4
        decays = float(np.mean(mono[-q:] ** 2)) < 0.25 * float(np.mean(mono[:q] ** 2))
    if best and len(times) >= 4 and best["grid_fit"] >= 0.75:
        kind, reason = "loop", "several onsets aligned to a sixteenth grid"
    elif len(times) <= 2 and (decays or duration < 1.0):
        kind, reason = "one_shot", "at most two onsets with a decaying level"
    else:
        kind, reason = (
            "uncertain",
            "neither a clear gridded loop nor a single decaying hit",
        )
    tempo = None
    if kind == "loop" and best:
        rivals = []
        for c in candidates[1:]:
            near = [best["bpm"], *rivals]
            if c["grid_fit"] >= best["grid_fit"] - 0.05 and all(
                abs(c["bpm"] - b) > 1 for b in near
            ):
                rivals.append(round(c["bpm"], 2))
        tempo = {
            "bpm": round(best["bpm"], 2),
            "bars": best["bars"],
            "fits_whole_bars": best["fits_length"],
            "grid_fit": round(best["grid_fit"], 3),
            "ambiguous_with": rivals,
            "assumes": "4/4, loop starts on a downbeat"
            if best["fits_length"]
            else "4/4",
        }
    hint = None
    if bpm_hint:
        beats = duration * bpm_hint / 60
        hint = {
            "bpm": bpm_hint,
            "length_in_beats": round(beats, 3),
            "fits_whole_bars": bool(
                abs(beats / 4 - round(beats / 4)) < 0.02 and beats >= 3.9
            ),
            "agrees": bool(tempo and abs(tempo["bpm"] - bpm_hint) <= 0.5),
        }
    return {
        "kind": kind,
        "kind_reason": reason,
        "onset_count": int(len(times)),
        "onsets_seconds": [round(float(t), 4) for t in times[:64]],
        "tempo": tempo,
        "filename_bpm_hint": hint,
    }


def analyze(path: Path, bpm_hint=None) -> dict:
    x, sr, info = _read(path, RHYTHM_SECONDS)
    mono = x.mean(axis=1)
    bounds = _bounds(mono, sr)
    level = {"silent": bounds is None}
    if bounds:
        start, end, peak = bounds
        level.update(
            first_sound_seconds=round(start / sr, 4),
            last_sound_seconds=round(end / sr, 4),
            trailing_silence_seconds=round((len(mono) - end) / sr, 4),
            peak_dbfs=round(20 * np.log10(peak), 2),
        )
    stereo = None
    if x.shape[1] == 2 and bounds:
        left, right = x[:, 0], x[:, 1]
        mid, side = (
            np.mean(((left + right) / 2) ** 2),
            np.mean(((left - right) / 2) ** 2),
        )
        std = float(np.std(left) * np.std(right))
        stereo = {
            "correlation": round(
                float(np.mean((left - left.mean()) * (right - right.mean())) / std), 3
            )
            if std > 1e-18
            else None,
            "side_energy_fraction": round(float(side / (mid + side)), 4)
            if mid + side
            else None,
        }
    return {
        "path": str(path),
        "sha256": digest(path),
        "analyzer": ANALYZER,
        "duration": info.frames / info.samplerate,
        "analyzed_seconds": len(x) / sr,
        "sample_rate": sr,
        "channels": x.shape[1],
        "level": level,
        "stereo": stereo,
        "pitch": measure_pitch(mono, sr),
        "rhythm": measure_rhythm(mono, sr, bpm_hint),
        "source": "measured from decoded audio (mono sum); estimates, not ground truth",
    }


def compare_root(declared: str, pitch: dict) -> dict:
    """Relate a declared root_note to a measured pitch."""
    from .model import midi

    result = {"declared": declared, "measured": pitch["note"], "cents": pitch["cents"]}
    if not pitch["pitched"]:
        result.update(status="unverified", confidence=pitch["confidence"])
        return result
    offset = pitch["midi"] - midi(declared)
    nearest = int(round(offset))
    result["offset_semitones"] = round(offset, 3)
    if nearest == 0:
        result["status"] = "ok" if abs(offset) <= 0.3 else "detuned"
    elif nearest % 12 == 0:
        result["status"] = "octave_mismatch"
    else:
        result["status"] = "note_mismatch"
    if result["status"] == "detuned":
        result["suggested_pad_transpose"] = round(-offset, 2)
    return result
