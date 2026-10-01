"""Beat and downbeat map of a whole song, measured from its audio.

A song made to a click has one tempo, so the map is first a steady grid: a tempo
and a first beat, found from the onset envelope of the whole file and then fitted
to the transients near its beats, which places it to a fraction of a millisecond.
How far the song's beats stray from that grid is measured, and a song that leaves
it is followed beat by beat. Which beat of four is the downbeat is scored from what
tends to happen there and reported with its alternatives, since it is the least
certain part. 4/4 is assumed.

Every value is an estimate from decoded audio; nothing here listens.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

import numpy as np
import soundfile as sf

ANALYZER = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()[:12]
MIN_BPM, MAX_BPM = 60.0, 200.0
# Half and double time fit the same onsets, and audio alone does not settle which
# a song is in. The faster is taken, up to 180 BPM, when most of its beats have a
# transient on the grid: its beats include the slower one's, where the slower
# one's can fall on the backbeat. At twice a song's tempo every other beat falls
# between its beats, where what sounds is seldom on the grid.
PREFERRED_BPM = (90.0, 180.0)
ON_GRID_SHARE = 0.7
N_FFT, HOP = 1024, 256  # the onset envelope: a frame every 6 ms at 44.1 kHz
CHROMA_FFT, CHROMA_HOP = 4096, 1024
SILENCE_DB = -50.0  # relative to the file's peak
SEARCH = 0.040  # how far from a grid beat its transient is looked for, seconds
RISE = 0.002  # the time a transient's rise is measured over, seconds
# A song is steady when, window by window, its onsets stay within a few frames of
# one grid. Instruments differ by some milliseconds in where their attacks sit, so
# a tighter limit would call a change of instrument a change of tempo.
STEADY_FRAMES = 3
STEADY_SHARE = 0.8
AGREE_MS = 10.0  # a transient this near a grid beat is on it
WINDOW = 8  # beats in a window when following a tempo that drifts
PHRASE = 16  # beats either side of a downbeat compared for a phrase change


def read(path: Path):
    """A file's mono sum as float32, and its sample rate."""
    with sf.SoundFile(path) as f:
        blocks = [
            block.mean(axis=1)
            for block in f.blocks(blocksize=1 << 18, dtype="float32", always_2d=True)
        ]
        sr = f.samplerate
    mono = np.concatenate(blocks) if blocks else np.zeros(0, dtype=np.float32)
    if not len(mono) or not np.isfinite(mono).all():
        raise ValueError(f"Empty or nonfinite audio: {path}")
    return mono, sr


def _stft(mono, n_fft, hop, reduce):
    """`reduce` of each frame's power spectrum; frame i is centered on sample i * hop."""
    x = np.pad(mono, n_fft // 2)
    count = 1 + (len(x) - n_fft) // hop
    window = np.hanning(n_fft).astype(np.float32)
    rows = []
    for first in range(0, count, 4096):  # bounded memory for long files
        index = np.arange(first, min(count, first + 4096))
        frames = x[np.arange(n_fft)[None, :] + hop * index[:, None]]
        rows.append(reduce(np.abs(np.fft.rfft(frames * window, axis=1)) ** 2))
    return np.concatenate(rows).astype(np.float64)


def band_power(mono, sr):
    """Power per frame in about 32 bands that widen upward, and each band's top in Hz."""
    edges = np.unique(np.round(np.geomspace(1, N_FFT // 2, 33)).astype(int))
    power = _stft(
        mono,
        N_FFT,
        HOP,
        lambda p: np.add.reduceat(p[:, : edges[-1]], edges[:-1], axis=1),
    )
    return power, edges[1:] * sr / N_FFT


def chroma(mono, sr):
    """Power per frame in each of the twelve pitch classes, from 55 Hz to 2 kHz."""
    freqs = np.fft.rfftfreq(CHROMA_FFT, 1 / sr)
    used = (freqs >= 55) & (freqs <= 2000)
    classes = np.round(12 * np.log2(freqs[used] / 440)).astype(int) % 12
    fold = np.zeros((int(used.sum()), 12), dtype=np.float32)
    fold[np.arange(len(classes)), classes] = 1
    return _stft(mono, CHROMA_FFT, CHROMA_HOP, lambda p: p[:, used] @ fold)


def onset_envelopes(power, tops):
    """Spectral flux per frame: of every band, and of the bands up to 160 Hz."""
    level = np.log1p(1000 * np.sqrt(power / max(float(power.max()), 1e-30)))
    rise = np.maximum(np.diff(level, axis=0, prepend=level[:1]), 0)
    return rise.sum(axis=1), rise[:, tops <= 160].sum(axis=1)


def _unit(x):
    mean = float(x.mean())
    return x / mean if mean > 0 else np.zeros_like(x)


def _autocorrelation(x):
    f = x - x.mean()
    size = 1 << int(np.ceil(np.log2(2 * len(f))))
    acf = np.fft.irfft(np.abs(np.fft.rfft(f, size)) ** 2, size)[: len(f)]
    return acf / acf[0] if acf[0] > 0 else np.zeros(len(f))


def _periodicity(acf, fps, bpm):
    """How alike the envelope is to itself one, two and four beats later."""
    lags = [k * 60 / bpm * fps for k in (1, 2, 4)]
    lags = [lag for lag in lags if lag < len(acf) - 1]
    return float(np.mean(np.interp(lags, np.arange(len(acf)), acf))) if lags else 0.0


def _fold(envelope, period, phases):
    """The envelope summed over every beat of a period, for each first frame in `phases`."""
    count = int((len(envelope) - 1 - phases.max()) // period) + 1
    at = phases[:, None] + period * np.arange(count)[None, :]
    return np.interp(at, np.arange(len(envelope)), envelope).sum(axis=1) / count


def tempo_options(envelope, fps, hint=None):
    """The tempos whose beat fits the envelope's periodicity, the likeliest first.

    Half and double time fit the same onsets, so both are options where they are
    in range; a hint leaves the one nearest it, or itself if none is near.
    """
    acf = _autocorrelation(envelope)
    grid = np.arange(MIN_BPM, MAX_BPM + 0.01, 0.25)
    found = float(max(grid, key=lambda b: _periodicity(acf, fps, b)))
    options = [b for b in (found / 2, found, found * 2) if MIN_BPM <= b <= MAX_BPM]
    if hint:
        options = [min(options, key=lambda b: abs(np.log(b / hint)))]
        if abs(np.log(options[0] / hint)) > 0.04:
            options = [float(hint)]
    scores = {
        b: _periodicity(acf, fps, b)
        + 0.15 * (PREFERRED_BPM[0] <= b <= PREFERRED_BPM[1])
        for b in options
    }
    return sorted(options, key=lambda b: -scores[b])


def find_period(envelope, fps, bpm):
    """The beat's period in frames near a tempo, to a small fraction of a frame."""
    # The spectrum of the envelope has a line at the beat's rate and its multiples;
    # their sum places the rate far more finely than a lag of whole frames does.
    size = 1 << int(np.ceil(np.log2(8 * len(envelope))))
    spectrum = np.abs(np.fft.rfft(envelope - envelope.mean(), size))
    rates = bpm / 60 * (1 + np.linspace(-0.03, 0.03, 6001))
    bins = np.arange(len(spectrum))
    comb = sum(np.interp(h * rates * size / fps, bins, spectrum) for h in (1, 2, 3, 4))
    period = fps / float(rates[int(np.argmax(comb))])
    # Then the period under which the beats of the whole file add up best.
    beats = len(envelope) / period
    phases = np.arange(0, period, 1.0)
    trials = period + np.arange(-6, 6.01, 0.25) / beats
    sums = [float(_fold(envelope, t, phases).max()) for t in trials]
    return float(trials[int(np.argmax(sums))])


def find_phase(envelope, period):
    """The first beat, in frames: where the beats of the period add up to the most."""
    phases = np.arange(0, period, 1.0)
    profile = _fold(envelope, period, phases)
    best = int(np.argmax(profile))
    fine = phases[best] + np.arange(-1, 1.01, 0.125)
    fine = fine[(fine >= 0) & (fine < period)]
    return float(fine[int(np.argmax(_fold(envelope, period, fine)))])


def follow(envelope, period, phase):
    """How many frames each beat of the grid is late by, and the share of the song
    that keeps to the grid.

    Windows of beats are folded as the whole file was, and the path through their
    offsets that changes least is taken, so a tempo that wanders is followed for
    as long as it wanders slowly. The share counts windows within a few frames of
    the grid, each by how strong its onsets are.
    """
    count = int((len(envelope) - 1 - phase) // period) + 1
    size = int(round(period))
    half = size // 2
    at = phase + period * np.arange(count)[:, None] + np.arange(-half, size - half)[None, :]
    near = np.interp(at, np.arange(len(envelope)), envelope, left=0, right=0)
    sums = np.cumsum(np.vstack([np.zeros(size), near]), axis=0)
    starts = np.arange(0, max(1, count - WINDOW + 1), 2)
    profiles = sums[np.minimum(starts + WINDOW, count)] - sums[starts]
    # Against the file's strong windows, so a passage without a beat says little
    # and takes its offset from the passages around it.
    strong = max(float(np.percentile(profiles.max(axis=1), 90)), 1e-30)
    profiles = np.minimum(profiles / strong, 1.0)
    # Viterbi over the windows: an offset costs what its window lacks there, and a
    # change of offset costs by its size, around the circle of one beat.
    step = np.abs(np.arange(size)[:, None] - np.arange(size)[None, :])
    step = np.minimum(step, size - step)
    move = 0.002 * step.astype(float) ** 2
    cost, back = 1 - profiles[0], []
    for profile in profiles[1:]:
        through = cost[:, None] + move
        back.append(np.argmin(through, axis=0))
        cost = through.min(axis=0) + 1 - profile
    path = [int(np.argmin(cost))]
    for previous in reversed(back):
        path.append(int(previous[path[-1]]))
    offsets = np.array(path[::-1], dtype=float) - half
    # Unwrapped, so an offset can grow past half a beat either way.
    turns = np.round(np.diff(offsets, prepend=offsets[0]) / size)
    offsets -= size * np.cumsum(turns)
    offsets -= np.median(offsets)
    centers, beats = starts + WINDOW / 2, np.arange(count)
    late = np.interp(beats, centers, offsets)
    # Before the first window's middle and after the last one's, the offset
    # keeps changing as it was.
    n = min(4, len(centers) - 1)
    if n:
        head = (offsets[n] - offsets[0]) / (centers[n] - centers[0])
        tail = (offsets[-1] - offsets[-1 - n]) / (centers[-1] - centers[-1 - n])
        late = np.where(beats < centers[0], offsets[0] + head * (beats - centers[0]), late)
        late = np.where(beats > centers[-1], offsets[-1] + tail * (beats - centers[-1]), late)
    weight = profiles.max(axis=1)
    on_grid = np.abs(offsets) <= STEADY_FRAMES
    return late, float(weight[on_grid].sum() / max(weight.sum(), 1e-30))


def transients(mono, sr, times, search=SEARCH):
    """For each time, the sharpest rise in level near it: (seconds from the time, rise).

    The level is the energy of the signal's first difference over two milliseconds,
    which weights the click of an attack over the body under it. The rise is the
    log of how much it grew across one such window, and the transient is where the
    growing begins.
    """
    width = max(2, round(RISE * sr))
    reach = round(search * sr)
    edge = np.diff(mono.astype(np.float64), prepend=mono[:1])
    energy = np.concatenate([[0.0], np.cumsum(edge**2)])
    centers = np.round(np.asarray(times) * sr).astype(int)
    span = np.arange(-reach, reach + 1)
    ends = np.clip(centers[:, None] + span[None, :] + width, 0, len(edge))
    mids = np.clip(ends - width, 0, len(edge))
    starts = np.clip(mids - width, 0, len(edge))
    after, before = energy[ends] - energy[mids], energy[mids] - energy[starts]
    floor = 1e-4 * np.maximum(after.max(axis=1, keepdims=True), 1e-30)
    rise = np.log((after + floor) / (before + floor))
    # A mild preference for what is near the grid, so a grace note is not taken.
    best = np.argmax(rise - 0.5 * (span[None, :] / reach) ** 2, axis=1)
    rows = np.arange(len(centers))
    found = (centers + span[best]) / sr - np.asarray(times)
    return found, rise[rows, best]


def _line(index, offsets, weights):
    """A robust weighted line through offsets by beat number: (at zero, per beat, kept)."""
    keep = np.ones(len(index), dtype=bool)
    fit = np.zeros(2)
    for _ in range(8):
        if keep.sum() < 4:
            break
        fit = np.polyfit(index[keep], offsets[keep], 1, w=np.sqrt(weights[keep]))
        left = offsets - np.polyval(fit, index)
        spread = 1.4826 * float(np.median(np.abs(left[keep] - np.median(left[keep]))))
        kept = np.abs(left) <= max(3 * spread, 0.0005)
        if (kept == keep).all():
            break
        keep = kept
    return float(fit[1]), float(fit[0]), keep


def _fit(mono, sr, start, step, late, searches):
    """The grid fitted to the transients near its beats.

    Once for each width in `searches`: widely first, to find how far the
    envelope's beats sit from the transients, then closely. `late` is what each
    beat is already known to be late by. Gives the grid's start and step, how many
    transients it kept and their spread, how far runs of those stray from it, which
    beats have a sharp transient and which of those are on the grid, and how late
    each beat is against it, taken from its own transient where it has one that
    agrees with its neighbors and from the beats around it otherwise.
    """
    index = np.arange(len(late), dtype=float)
    fit = {"start": start, "step": step, "used": 0, "spread": None}
    fit["late"], fit["trend"] = late, np.zeros(len(late))
    for search in searches:
        grid = fit["start"] + fit["step"] * index + late
        offsets, rises = transients(mono, sr, grid, search)
        sharp = rises >= np.log(2)
        if sharp.sum() < 8:
            break
        at_zero, per_beat, kept = _line(index[sharp], offsets[sharp], rises[sharp])
        fit["start"], fit["step"] = fit["start"] + at_zero, fit["step"] + per_beat
        left = offsets[sharp] - at_zero - per_beat * index[sharp]
        fit["used"] = int(kept.sum())
        fit["spread"] = float(np.sqrt(np.mean(left[kept] ** 2)))
        fit["trend"] = np.interp(index, index[sharp][kept], _smooth(left[kept], 17))
        total = late[sharp] + left
        near = np.abs(total - _smooth(total, 5)) <= 0.010
        fit["late"] = _extend(index, index[sharp][near], total[near])
    grid = fit["start"] + fit["step"] * index + late
    offsets, rises = transients(mono, sr, grid, SEARCH)
    fit["sharp"] = rises >= np.log(2)
    fit["on_grid"] = fit["sharp"] & (np.abs(offsets) <= AGREE_MS / 1000)
    return fit


def _extend(at, known_at, known):
    """Values between the known ones, and past them along the line of the nearest four."""
    out = np.interp(at, known_at, known)
    if len(known) >= 4:
        for side, edge in ((slice(None, 4), at < known_at[0]), (slice(-4, None), at > known_at[-1])):
            out[edge] = np.polyval(np.polyfit(known_at[side], known[side], 1), at[edge])
    return out


def _smooth(values, width):
    half = width // 2
    padded = np.pad(values, half, mode="edge")
    return np.array([np.median(padded[i : i + width]) for i in range(len(values))])


def _per_beat(frames, fps, times):
    """The mean of a per-frame feature over each beat: one row for each time but the last."""
    sums = np.vstack([np.zeros(frames.shape[1]), np.cumsum(frames, axis=0)])
    at = np.clip(np.round(np.asarray(times) * fps).astype(int), 0, len(frames))
    lengths = np.maximum(np.diff(at), 1)[:, None]
    return (sums[at[1:]] - sums[at[:-1]]) / lengths


def _change(rows, span, distance):
    """For each beat, how unlike the `span` beats from it are to the `span` before it."""
    sums = np.vstack([np.zeros(rows.shape[1]), np.cumsum(rows, axis=0)])
    out = np.zeros(len(rows) + 1)
    k = np.arange(span, len(rows) - span + 1)
    if len(k):
        after = (sums[k + span] - sums[k]) / span
        before = (sums[k] - sums[k - span]) / span
        out[k] = distance(after, before)
    return out


def _level_distance(a, b):
    return np.sqrt(np.mean((a - b) ** 2, axis=1))


def _cosine_distance(a, b):
    norm = np.linalg.norm(a, axis=1) * np.linalg.norm(b, axis=1)
    return 1 - np.sum(a * b, axis=1) / np.maximum(norm, 1e-30)


def _standard(x):
    spread = float(x.std())
    return (x - x.mean()) / spread if spread > 0 else np.zeros_like(x)


def track(mono, sr, envelope, bpm=None):
    """Every beat of the file: their times, the times the envelope has them at, and
    what was found of the tempo."""
    fps = sr / HOP
    options = tempo_options(envelope, fps, bpm)

    def grid(tempo):
        """One grid for the whole file at a tempo, fitted to its transients."""
        period = find_period(envelope, fps, tempo)
        phase = find_phase(envelope, period)
        count = int((len(envelope) - 1 - phase) // period) + 1
        searches = (SEARCH, SEARCH / 2)
        fit = _fit(mono, sr, phase / fps, period / fps, np.zeros(count), searches)
        return period, phase, fit, float(fit["on_grid"].sum() / max(fit["sharp"].sum(), 1))

    for tempo in sorted(b for b in options if b <= PREFERRED_BPM[1])[::-1]:
        period, phase, fit, on_grid = grid(tempo)
        if on_grid >= ON_GRID_SHARE:
            break
    else:
        # No tempo has its transients on one grid: the likeliest by periodicity.
        tempo = options[0]
        period, phase, fit, on_grid = grid(tempo)
    index = np.arange(len(fit["late"]), dtype=float)
    late, share = follow(envelope, period, phase)
    steady = share >= STEADY_SHARE
    if steady:
        late = np.zeros(len(index))
        drift = float(np.abs(fit["trend"]).max())
        # A fit that takes the grid away from the envelope's over the song has
        # caught something other than its beats.
        if abs(fit["step"] - period / fps) * len(index) > 0.015:
            fit.update(start=phase / fps, step=period / fps, used=0, spread=None)
        times = fit["start"] + fit["step"] * index
    else:
        late = late / fps
        fit = _fit(mono, sr, phase / fps, period / fps, late, (SEARCH, SEARCH))
        # The grid is the line through the beats, so the tempo is their mean one.
        slope, level = np.polyfit(index, fit["late"], 1)
        fit["start"], fit["step"] = fit["start"] + level, fit["step"] + slope
        wander = fit["late"] - level - slope * index
        drift = float(np.ptp(wander)) / 2
        times = fit["start"] + fit["step"] * index + wander
    local = 60 / _smooth(np.diff(times), 2 * WINDOW + 1)
    found = {
        "bpm": round(60 / fit["step"], 4),
        "steady": steady,
        "drift_ms": round(drift * 1000, 2),
        "bpm_range": None
        if steady
        else [round(float(local.min()), 2), round(float(local.max()), 2)],
        "ambiguous_with": [
            round(60 / fit["step"] * b / tempo, 2) for b in options if b != tempo
        ],
        "transients_fitted": fit["used"],
        "transients_on_grid": round(on_grid, 3),
        "fit_spread_ms": None
        if fit["spread"] is None
        else round(fit["spread"] * 1000, 3),
    }
    return times, (phase + period * index) / fps + late, found


def downbeat_odds(cues, count):
    """For each of the four places a bar could start, the odds that it does.

    Each cue is something that tends to happen on a downbeat. A place's score is
    the sum, over the cues, of the mean of the cue's standard values on its beats;
    such a sum's chance spread shrinks with the number of beats, which the odds
    allow for.
    """
    place = np.arange(count) % 4
    scores = np.array(
        [sum(float(_standard(c)[place == p].mean()) for c in cues) for p in range(4)]
    )
    odds = np.exp((scores - scores.max()) * np.sqrt(count / 4) / 2)
    return odds / odds.sum()


def measure(path: Path, bpm=None, downbeat=None) -> dict:
    """The beat map of a file. `bpm` settles half or double time; `downbeat` is a
    time in seconds whose nearest beat is taken as a downbeat."""
    mono, sr = read(path)
    duration = len(mono) / sr
    peak = float(np.abs(mono).max())
    if peak <= 10 ** (-120 / 20):
        raise ValueError(f"Silent audio: {path}")
    sounding = np.flatnonzero(np.abs(mono) >= peak * 10 ** (SILENCE_DB / 20))
    first_sound, last_sound = sounding[0] / sr, (sounding[-1] + 1) / sr
    fps = sr / HOP
    power, tops = band_power(mono, sr)
    flux, low = onset_envelopes(power, tops)
    envelope = _unit(flux) + _unit(low)
    if len(envelope) < 8 * fps or envelope.max() <= 0:
        raise ValueError(f"Too short for a beat map: {path}")
    times, coarse, tempo = track(mono, sr, envelope, bpm)
    inside = (times >= first_sound - 0.02) & (times < min(last_sound, duration))
    times, coarse = times[inside], coarse[inside]
    if len(times) < 8:
        raise ValueError(f"Too few beats for a beat map: {path}")

    # Each beat's strength: the envelope's peak near it, against the strongest.
    frames = np.round(coarse * fps).astype(int)
    reach = np.clip(frames[:, None] + np.arange(-2, 3)[None, :], 0, len(envelope) - 1)
    strength = envelope[reach].max(axis=1)
    strength = np.minimum(strength / max(np.percentile(strength, 95), 1e-30), 1.0)

    # What changes on each beat: the level in bands, and the pitch classes.
    ends = np.append(times, min(times[-1] + 60 / tempo["bpm"], duration))
    floor = max(float(power.max()), 1e-30) * 1e-8
    by_beat = _per_beat(power, fps, ends)
    levels = 10 * np.log10(by_beat + floor)
    notes = _per_beat(chroma(mono, sr), sr / CHROMA_HOP, ends)
    odds = downbeat_odds(
        [
            low[reach].max(axis=1),  # a kick or a bass note
            _change(notes, 2, _cosine_distance)[:-1],  # a change of harmony
            _change(notes, 4, _cosine_distance)[:-1],
            _change(levels, 4, _level_distance)[:-1],  # a change of sound
            _change(levels, PHRASE, _level_distance)[:-1],
        ],
        len(times),
    )
    chosen = int(np.argmax(odds))
    if downbeat is not None:
        chosen = int(np.argmin(np.abs(times - downbeat))) % 4
    candidates = [
        {
            "first_downbeat_seconds": round(float(times[p]), 6),
            "confidence": round(float(odds[p]), 3),
            "chosen": p == chosen,
        }
        for p in sorted(range(4), key=lambda p: -odds[p])
    ]

    # Phrases: downbeats where what follows differs most from what came before,
    # over four bars, two and one. A change that all three see is on that bar.
    # The bands are wide here, so a bass line that moves is not a change of parts.
    wide = np.add.reduceat(by_beat, np.arange(0, by_beat.shape[1], 4), axis=1)
    parts = 10 * np.log10(wide + floor)
    downbeats = np.arange(chosen, len(times), 4)
    change = np.zeros(len(times))
    for span in (PHRASE, 8, 4):
        differs = _change(parts, span, _level_distance)[:-1]
        change += differs / max(float(differs[downbeats].max()), 1e-30)
    change = np.round(change / max(float(change[downbeats].max()), 1e-30), 3)
    phrases = []
    for i, beat in enumerate(downbeats):
        around = downbeats[max(0, i - 2) : i + 3]
        if i == 0 or (change[beat] >= 0.25 and change[beat] == change[around].max()):
            phrases.append(int(beat))

    beats = [
        {
            "seconds": round(float(t), 6),
            "bar": int((i - chosen) // 4 + 1),
            "beat": int((i - chosen) % 4 + 1),
            "strength": round(float(s), 3),
            **({"change": float(change[i])} if (i - chosen) % 4 == 0 else {}),
            **({"phrase_start": True} if i in phrases else {}),
        }
        for i, (t, s) in enumerate(zip(times, strength))
    ]
    return {
        "path": str(path),
        "analyzer": ANALYZER,
        "duration": duration,
        "sample_rate": sr,
        "tempo": tempo,
        "first_beat_seconds": beats[0]["seconds"],
        "first_downbeat_seconds": beats[chosen]["seconds"],
        "downbeat": {
            "confidence": round(float(odds[chosen]), 3),
            "set_by": "request" if downbeat is not None else "measurement",
            "candidates": candidates,
        },
        "phrases": [
            {k: beats[i][k] for k in ("seconds", "bar", "change")} for i in phrases
        ],
        "beat_count": len(beats),
        "beats": beats,
        "assumes": "4/4; bars are counted from the first downbeat, and beats before it are bar 0",
        "source": "measured from decoded audio (mono sum); estimates, not ground truth",
    }


def seconds(text) -> float:
    """A time given as seconds or as minutes:seconds, such as 41, 1:20 or 1:20.5."""
    minutes, colon, rest = str(text).strip().rpartition(":")
    try:
        value = float(rest) + (60 * int(minutes) if colon else 0)
    except ValueError:
        raise ValueError(f"Not a time: {text}; give seconds or m:ss") from None
    if value < 0 or (colon and float(rest) >= 60):
        raise ValueError(f"Not a time: {text}; give seconds or m:ss")
    return value


def near(beats, at: float, window: float) -> dict:
    """The beats within `window` seconds of a time, and the nearest beat and downbeat."""

    def row(beat):
        return {**beat, "offset_seconds": round(beat["seconds"] - at, 6)}

    def nearest(rows):
        return row(min(rows, key=lambda b: abs(b["seconds"] - at))) if rows else None

    downbeats = [b for b in beats if b["beat"] == 1]
    return {
        "seconds": at,
        "window_seconds": window,
        "nearest_beat": nearest(beats),
        "nearest_downbeat": nearest(downbeats),
        "nearest_phrase_start": nearest([b for b in downbeats if b.get("phrase_start")]),
        "beats": [row(b) for b in beats if abs(b["seconds"] - at) <= window],
    }


def click(path: Path, beats, output: Path, start: float, length: float) -> dict:
    """Part of a song with a click on each beat and a higher, louder one on each
    downbeat, for a person to hear whether the map is right."""
    info = sf.info(path)
    sr = info.samplerate
    first = min(max(0, round(start * sr)), max(0, info.frames - 1))
    x, _ = sf.read(path, start=first, frames=round(length * sr), always_2d=True)
    if not len(x):
        raise ValueError("Empty audio")
    peak = float(np.abs(x).max())
    x *= 0.5 / peak if peak else 1
    t = np.arange(round(0.012 * sr)) / sr
    placed = []
    for beat in beats:
        at = round(beat["seconds"] * sr) - first
        if not 0 <= at < len(x):
            continue
        down = beat["beat"] == 1
        tick = (0.45 if down else 0.3) * np.sin(2 * np.pi * (2000 if down else 1000) * t)
        tick = (tick * np.exp(-t / 0.003))[: len(x) - at]
        x[at : at + len(tick)] += tick[:, None]
        placed.append(beat)
    output.parent.mkdir(parents=True, exist_ok=True)
    sf.write(output, x, sr, subtype="PCM_24")
    return {
        "audio": str(output.resolve()),
        "from_seconds": first / sr,
        "duration": len(x) / sr,
        "beats": len(placed),
        "downbeats": sum(b["beat"] == 1 for b in placed),
    }
