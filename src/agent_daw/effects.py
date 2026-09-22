"""Insert effects in block-processing form with explicit state and real units.

Each device processes consecutive stereo blocks. Output is identical for any block
partition: filters carry their delay state, dynamics carry detector state, and the
limiter's look-ahead is a declared latency that the chain runner compensates. The
delay recurses in chunks no longer than its delay time, and the reverb convolves
fixed partitions aligned to its own input, so neither depends on the caller's blocks.
"""

from __future__ import annotations
import numpy as np
from scipy.fft import irfft, rfft
from scipy.ndimage import maximum_filter1d
from scipy.signal import butter, istft, lfilter, sosfilt, stft
from .model import Compressor, Delay, Eq, Filter, Limiter, Reverb, frame

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


class DelayDevice(Device):
    """Tempo-synced feedback delay: wet[n] = F(input[n - D] + feedback * wet[n - D]).

    F is the optional low/high cut, so every repeat passes through it again and
    darkens. Ping-pong feeds the mono input to the left and crosses the feedback
    between channels. Each chunk of at most D frames depends only on earlier ones.
    """

    def __init__(self, spec: Delay, rate, tempo):
        super().__init__(spec)
        if tempo is None:
            raise ValueError("delay needs the session tempo")
        self.frames = d = max(1, frame(spec.time_beats, tempo, rate))
        self.feedback = spec.feedback_percent / 100
        self.mix = spec.mix_percent / 100
        sections = []
        if spec.lowcut_hz:
            sections.append(
                butter(2, spec.lowcut_hz, "highpass", fs=rate, output="sos")
            )
        if spec.highcut_hz:
            sections.append(
                butter(2, spec.highcut_hz, "lowpass", fs=rate, output="sos")
            )
        self.sos = np.concatenate(sections) if sections else None
        self.zi = np.zeros((len(self.sos), 2, 2)) if sections else None
        self.inputs = np.zeros((d, 2))
        self.wet = np.zeros((d, 2))

    def process(self, x, key=None):
        m, d = len(x), self.frames
        if not m:
            return x
        source = x
        if self.spec.ping_pong:
            source = np.column_stack([x.mean(axis=1), np.zeros(m)])
        inputs = np.concatenate([self.inputs, source])
        wet = np.concatenate([self.wet, np.zeros((m, 2))])
        for i in range(0, m, d):
            stop = min(i + d, m)
            fed = wet[i:stop]
            if self.spec.ping_pong:
                fed = fed[:, ::-1]
            v = inputs[i:stop] + self.feedback * fed
            if self.sos is not None:
                v, self.zi = sosfilt(self.sos, v, axis=0, zi=self.zi)
            wet[d + i : d + stop] = v
        self.inputs, self.wet = inputs[m:], wet[m:]
        return x * (1 - self.mix) + wet[d:] * self.mix


DECAY_60DB = 3 * np.log(10)  # exp(-DECAY_60DB * t / rt60) falls 60 dB at t = rt60


def reverb_ir(spec: Reverb, rate) -> np.ndarray:
    """Seeded synthetic stereo impulse response, energy-normalized per channel.

    Gaussian noise is shaped in the STFT domain so each frequency decays
    exponentially: rt60 is decay_seconds up to damping_hz and falls as 1/f above
    it. A 3 ms raised-cosine onset avoids a click; width mixes a second noise as
    side signal; lowcut is a 12 dB/octave highpass. Predelay prepends silence.
    """
    length = round(1.5 * spec.decay_seconds * rate)  # the undamped tail reaches -90 dB
    noise = np.random.default_rng(spec.seed).standard_normal((2, length))
    freqs, times, z = stft(noise, fs=rate, nperseg=1024, noverlap=768)
    rt60 = spec.decay_seconds * np.minimum(1, spec.damping_hz / np.maximum(freqs, 1))
    z *= np.exp(-DECAY_60DB * times[None, :] / rt60[:, None])
    shaped = istft(z, fs=rate, nperseg=1024, noverlap=768)[1][:, :length]
    onset = min(length, round(0.003 * rate))
    shaped[:, :onset] *= 0.5 - 0.5 * np.cos(np.pi * np.arange(onset) / onset)
    width = spec.width_percent / 100
    ir = np.column_stack([shaped[0] + width * shaped[1], shaped[0] - width * shaped[1]])
    ir = sosfilt(
        butter(2, spec.lowcut_hz, "highpass", fs=rate, output="sos"), ir, axis=0
    )
    ir /= np.sqrt(np.sum(ir**2, axis=0))
    return np.concatenate([np.zeros((round(spec.predelay_ms * rate / 1000), 2)), ir])


class ReverbDevice(Device):
    """Mono-in, stereo-out convolution with reverb_ir.

    Uniformly partitioned overlap-save convolution. Partitions are aligned to the
    device's own input, so output is independent of the caller's block sizes. One
    partition of latency is declared and compensated by the chain.
    """

    def __init__(self, spec: Reverb, rate):
        super().__init__(spec)
        ir = reverb_ir(spec, rate)
        # Larger partitions cost less per frame for long tails; latency is compensated.
        n = int(np.clip(2 ** np.ceil(np.log2(len(ir) / 8)), 4096, 65536))
        count = -(-len(ir) // n)
        padded = np.zeros((count * n, 2))
        padded[: len(ir)] = ir
        self.latency = n
        self.mix = spec.mix_percent / 100
        # Channel-major so every transform runs over contiguous frames.
        self.spectra = rfft(padded.reshape(count, n, 2).transpose(0, 2, 1), 2 * n)
        self.history = np.zeros((count, n + 1), dtype=complex)  # ring of input spectra
        self.head = 0
        self.previous = np.zeros(n)
        self.pending = np.zeros((0, 2))
        self.queue = np.zeros((n, 2))

    def process(self, x, key=None):
        n, count = self.latency, len(self.history)
        pending = np.concatenate([self.pending, x])
        ready = [self.queue]
        blocks = len(pending) // n
        for b in range(blocks):
            block = pending[b * n : (b + 1) * n]
            mono = block.mean(axis=1)
            self.head = (self.head - 1) % count
            self.history[self.head] = rfft(np.concatenate([self.previous, mono]))
            self.previous = mono
            recent = self.history[(self.head + np.arange(count)) % count]
            spectrum = np.einsum("kf,kcf->cf", recent, self.spectra)
            wet = irfft(spectrum, 2 * n)[:, n:].T
            ready.append(block * (1 - self.mix) + wet * self.mix)
        self.pending = pending[blocks * n :]
        out = np.concatenate(ready)
        self.queue = out[len(x) :]
        return out[: len(x)]


def device(spec, rate, tempo=None):
    if isinstance(spec, Filter):
        return SosDevice(spec, filter_sos(spec, rate))
    if isinstance(spec, Eq):
        return SosDevice(spec, [band_sos(b, rate) for b in spec.bands])
    if isinstance(spec, Compressor):
        return Dynamics(spec, rate)
    if isinstance(spec, Limiter):
        return LimiterDevice(spec, rate)
    if isinstance(spec, Delay):
        return DelayDevice(spec, rate, tempo)
    if isinstance(spec, Reverb):
        return ReverbDevice(spec, rate)
    raise ValueError(f"Unknown effect {spec.type}")


class Lag:
    """Fixed sample delay for sidechain keys."""

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

    def __init__(self, specs, rate, tempo=None):
        self.specs = list(specs)
        self.devices = []
        self.latency = 0
        self.slots = []
        for spec in self.specs:
            if spec.bypass:
                self.slots.append(None)
                continue
            d = device(spec, rate, tempo)
            d.key_delay = (
                Lag(self.latency) if getattr(spec, "sidechain", None) else None
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
