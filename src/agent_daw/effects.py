"""Insert effects in block-processing form with explicit state and real units.

Each device processes consecutive stereo blocks. Output is identical for any block
partition: filters carry their delay state, dynamics carry detector state, and the
limiter's look-ahead is a declared latency that the chain runner compensates.
"""

from __future__ import annotations
import numpy as np
from scipy.ndimage import maximum_filter1d
from scipy.signal import butter, lfilter, sosfilt
from .model import Compressor, Eq, Filter, Limiter

FIXED = 2**24  # Limiter smoothing sums fixed-point dB so sums are partition-exact.


def peak_db(x):
    with np.errstate(divide="ignore"):
        return 20 * np.log10(np.max(np.abs(x), axis=1))


class Device:
    latency = 0

    def __init__(self, spec):
        self.spec = spec
        self.window = (0, 0)
        self.position = 0
        self.stats = None

    def record(self, reduction):
        """Accumulate gain reduction for output frames inside the aligned render."""
        start, stop = self.window
        a = max(0, start - self.position)
        b = min(len(reduction), stop - self.position)
        self.position += len(reduction)
        if b <= a:
            return
        r = reduction[a:b]
        s = self.stats or {"max": 0.0, "sum": 0.0, "over_1db": 0, "frames": 0}
        s["max"] = max(s["max"], float(r.max()))
        s["sum"] += float(r.sum())
        s["over_1db"] += int(np.count_nonzero(r > 1))
        s["frames"] += len(r)
        self.stats = s

    def report(self):
        result = {"type": self.spec.type, "latency_frames": self.latency}
        if self.stats is not None or isinstance(self, (Dynamics, LimiterDevice)):
            s = self.stats or {"max": 0.0, "sum": 0.0, "over_1db": 0, "frames": 0}
            n = max(1, s["frames"])
            result["max_gain_reduction_db"] = s["max"]
            result["mean_gain_reduction_db"] = s["sum"] / n
            result["fraction_over_1db_reduction"] = s["over_1db"] / n
        return result


class SosDevice(Device):
    def __init__(self, spec, sos):
        super().__init__(spec)
        self.sos = np.asarray(sos, dtype=np.float64)
        self.zi = np.zeros((len(self.sos), 2, 2))

    def process(self, x, key=None):
        if not len(x):
            return x
        y, self.zi = sosfilt(self.sos, x, axis=0, zi=self.zi)
        return y


def filter_sos(spec: Filter, rate):
    btype = "highpass" if spec.mode == "highpass" else "lowpass"
    return butter(
        spec.slope_db_per_octave // 6, spec.cutoff_hz, btype, fs=rate, output="sos"
    )


def band_sos(band, rate):
    """RBJ audio EQ cookbook biquad, normalized to a0 = 1."""
    a = 10 ** (band.gain_db / 40)
    w = 2 * np.pi * band.freq_hz / rate
    cos, alpha = np.cos(w), np.sin(w) / (2 * band.q)
    if band.shape == "bell":
        b = [1 + alpha * a, -2 * cos, 1 - alpha * a]
        d = [1 + alpha / a, -2 * cos, 1 - alpha / a]
    else:
        root = 2 * np.sqrt(a) * alpha
        sign = 1 if band.shape == "low_shelf" else -1
        b = [
            a * ((a + 1) - sign * (a - 1) * cos + root),
            sign * 2 * a * ((a - 1) - sign * (a + 1) * cos),
            a * ((a + 1) - sign * (a - 1) * cos - root),
        ]
        d = [
            (a + 1) + sign * (a - 1) * cos + root,
            -sign * 2 * ((a - 1) + sign * (a + 1) * cos),
            (a + 1) + sign * (a - 1) * cos - root,
        ]
    return [*(v / d[0] for v in b), 1.0, d[1] / d[0], d[2] / d[0]]


class ReleaseHold:
    """y[n] = max(x[n], decay * y[n-1]), vectorized as a running maximum in log form."""

    def __init__(self, release_frames):
        self.log_decay = -1 / max(release_frames, 1e-9)
        self.best = -np.inf
        self.n = 0

    def process(self, x):
        index = self.n + np.arange(len(x), dtype=np.float64)
        self.n += len(x)
        if not len(x):
            return x
        slope = index * self.log_decay
        with np.errstate(divide="ignore"):
            values = np.log(x) - slope
        running = np.maximum.accumulate(np.concatenate([[self.best], values]))[1:]
        self.best = running[-1]
        return np.exp(slope + running)


class Dynamics(Device):
    """Stereo-linked peak compressor: soft-knee static curve, release hold, attack pole."""

    def __init__(self, spec: Compressor, rate):
        super().__init__(spec)
        self.hold = ReleaseHold(spec.release_ms * rate / 1000)
        attack = spec.attack_ms * rate / 1000
        self.alpha = float(np.exp(-1 / attack)) if attack > 0 else 0.0
        self.zi = np.zeros(1)

    def curve(self, level):
        s = self.spec
        over = level - s.threshold_db
        slope = 1 - 1 / s.ratio
        r = np.where(over > 0, over * slope, 0.0)
        if s.knee_db > 0:
            k = s.knee_db
            inside = np.abs(over) <= k / 2
            r = np.where(inside, slope * (over + k / 2) ** 2 / (2 * k), r)
        return np.where(np.isfinite(level), r, 0.0)

    def process(self, x, key=None):
        if not len(x):
            return x
        reduction = self.hold.process(self.curve(peak_db(x if key is None else key)))
        if self.alpha:
            reduction, self.zi = lfilter(
                [1 - self.alpha], [1, -self.alpha], reduction, zi=self.zi
            )
        self.record(reduction)
        return x * (10 ** ((self.spec.makeup_db - reduction) / 20))[:, None]


class LimiterDevice(Device):
    """Look-ahead brickwall limiter on sample peaks.

    Required reduction is maximized over the look-ahead window, held with the
    release, then averaged over the same window in fixed point. Every average
    therefore covers the peak it protects, so no output sample exceeds the ceiling.
    """

    def __init__(self, spec: Limiter, rate):
        super().__init__(spec)
        self.latency = n = max(1, round(spec.lookahead_ms * rate / 1000))
        self.hold = ReleaseHold(spec.release_ms * rate / 1000)
        self.audio = np.zeros((n, 2))
        self.required = np.zeros(n)
        self.held = np.zeros(n, dtype=np.int64)

    def process(self, x, key=None):
        n, m = self.latency, len(x)
        if not m:
            return x
        required = np.maximum(0.0, peak_db(x) - self.spec.ceiling_db)
        extended = np.concatenate([self.required, required])
        start = n - n // 2
        window = maximum_filter1d(extended, n + 1)[start : start + m]
        self.required = extended[-n:]
        held = np.ceil(self.hold.process(window) * FIXED).astype(np.int64)
        extended = np.concatenate([self.held, held])
        sums = np.cumsum(np.concatenate([[0], extended]))
        reduction = (sums[n + 1 :] - sums[:m]) / (n + 1) / FIXED
        self.held = extended[-n:]
        audio = np.concatenate([self.audio, x])
        self.audio = audio[-n:]
        self.record(reduction)
        return audio[:m] * (10 ** (-reduction / 20))[:, None]


def device(spec, rate):
    if isinstance(spec, Filter):
        return SosDevice(spec, filter_sos(spec, rate))
    if isinstance(spec, Eq):
        return SosDevice(spec, [band_sos(b, rate) for b in spec.bands])
    if isinstance(spec, Compressor):
        return Dynamics(spec, rate)
    if isinstance(spec, Limiter):
        return LimiterDevice(spec, rate)
    raise ValueError(f"Unknown effect {spec.type}")


class Delay:
    def __init__(self, frames):
        self.buffer = np.zeros((frames, 2))

    def process(self, x):
        if not len(self.buffer):
            return x
        joined = np.concatenate([self.buffer, x])
        self.buffer = joined[len(joined) - len(self.buffer) :]
        return joined[: len(x)]


class Chain:
    """Serial insert chain. Sidechain keys are delayed by upstream device latency."""

    def __init__(self, specs, rate):
        self.specs = list(specs)
        self.devices = []
        self.latency = 0
        self.slots = []
        for spec in self.specs:
            if spec.bypass:
                self.slots.append(None)
                continue
            d = device(spec, rate)
            d.key_delay = (
                Delay(self.latency) if getattr(spec, "sidechain", None) else None
            )
            self.latency += d.latency
            d.offset = self.latency
            self.devices.append(d)
            self.slots.append(d)

    def process(self, x, keys):
        for d in self.devices:
            key = None
            if d.key_delay is not None:
                key = d.key_delay.process(keys[d.spec.sidechain])
            x = d.process(x, key)
        return x

    def run(self, x, keys=None, block_size=4096):
        """Process a whole timeline in blocks and return it latency-compensated."""
        keys = keys or {}
        total = len(x)
        for d in self.devices:
            d.window = (d.offset, d.offset + total)
            d.position = 0
        if not self.devices:
            return x
        out = []
        for start in range(0, total, block_size):
            stop = min(start + block_size, total)
            out.append(
                self.process(x[start:stop], {k: v[start:stop] for k, v in keys.items()})
            )
        remaining = self.latency
        while remaining > 0:
            n = min(block_size, remaining)
            silence = np.zeros((n, 2))
            out.append(self.process(silence, {k: silence for k in keys}))
            remaining -= n
        return np.concatenate(out)[self.latency : self.latency + total]

    def report(self):
        return [
            {"type": spec.type, "bypass": True} if d is None else d.report()
            for spec, d in zip(self.specs, self.slots)
        ]
