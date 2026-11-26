"""Beat and downbeat map of a whole song, measured from its audio.

A song made to a click has one tempo, so the map is first a steady grid: a tempo
and a first beat, found from the onset envelope of the whole file and then fitted
to the transients near its beats, which places it to a fraction of a millisecond.
How far the song's beats stray from that grid is measured, and a song that leaves
it is followed beat by beat. Which beat of the bar is the downbeat is scored from
what tends to happen there and reported with its alternatives, since it is the
least certain part. A bar is four beats unless a meter is given. How late its
off-beat notes fall, its swing, is measured against the beats (swing.py).

Every value is an estimate from decoded audio; nothing here listens.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

import numpy as np
import soundfile as sf

# The map, its swing (swing.py, which measures on this module's envelope) and its
# downbeats (downbeats.py, and the model's runner).
ANALYZER = hashlib.sha256(
    b"".join(
        Path(__file__).with_name(name).read_bytes()
        for name in ("beats.py", "segments.py", "swing.py", "downbeats.py", "downbeat_runner.py")
    )
).hexdigest()[:12]
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
# A beat followed one by one takes its own transient when it is this near the
# median of its neighbours': a band pushes and pulls a beat by some milliseconds.
NEIGHBOURS = 0.025
WINDOW = 8  # beats in a window when following a tempo that drifts
PHRASE_BARS = 4  # bars either side of a downbeat compared for a phrase change


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
        near = np.abs(total - _smooth(total, 5)) <= NEIGHBOURS
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


def downbeat_odds(cues, count, per_bar=4):
    """For each of the `per_bar` places a bar could start, the odds that it does.

    Each cue is something that tends to happen on a downbeat. A place's score is
    the sum, over the cues, of the mean of the cue's standard values on its beats;
    such a sum's chance spread shrinks with the number of beats, which the odds
    allow for.
    """
    place = np.arange(count) % per_bar
    scores = np.array(
        [sum(float(_standard(c)[place == p].mean()) for c in cues) for p in range(per_bar)]
    )
    odds = np.exp((scores - scores.max()) * np.sqrt(count / per_bar) / 2)
    return odds / odds.sum()


def bar_of(meter) -> int:
    """How many of the map's beats make a bar, from a time signature such as 3/4:
    its beats. The map's beat is the pulse it finds, so a 6/8 song whose pulse is
    found as the dotted quarter is counted as 2/4."""
    above, slash, below = str(meter).partition("/")
    if not slash or not above.strip().isdigit() or not below.strip().isdigit():
        raise ValueError(f"meter is a time signature such as 3/4 or 6/8, not {meter}")
    beats = int(above)
    if not 1 <= beats <= 32:
        raise ValueError("meter has 1 to 32 beats a bar")
    return beats


def requested_times(downbeat) -> list[float]:
    """The --downbeat times asked for: none, one or several."""
    if downbeat is None:
        return []
    if isinstance(downbeat, (int, float)):
        return [float(downbeat)]
    return sorted(float(t) for t in downbeat)


def bpm_values(bpm) -> list[float]:
    """The --bpm asked for: none, one, or one for each segment."""
    if bpm is None:
        return []
    if isinstance(bpm, (int, float)):
        return [float(bpm)]
    return [float(b) for b in bpm]


def measure(path: Path, bpm=None, downbeat=None, meter=None, segment_at=None) -> dict:
    """The beat map of a file. `bpm` settles half or double time, a tempo or one
    for each segment in turn; `downbeat` is a time in seconds, or a list of them,
    whose nearest beat is a downbeat; `meter` is the song's time signature, whose
    beats make a bar; `segment_at` is a time, or a list of them, whose nearest
    beat starts a new tempo.

    Where the downbeat model is installed it leads (downbeats.py), and a bar is
    the length it hears unless `meter` says; without it a bar is 4/4 unless
    `meter` says, and one downbeat is scored for the whole song from what tends to
    happen there. Where the tempo steps, each segment is measured on its own
    (segments.py)."""
    from . import downbeats, segments

    asked_bar = bar_of(meter) if meter else None
    asked = requested_times(downbeat)
    hints = bpm_values(bpm)
    hint = hints[0] if hints else None
    asked_steps = requested_times(segment_at)
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

    # The model's frames, where it is installed and runs.
    found, model = None, {"name": downbeats.NAME, "used": False}
    why = downbeats.unavailable()
    if why is None:
        try:
            found = downbeats.frames(path)
            plan = downbeats.plan(found, _unit(flux), fps, hint)
            if len(plan["beats"]) < 8:
                found, why = None, "The downbeat model heard too few beats"
        except ValueError as exc:
            found, why = None, str(exc)
    split = None  # the segments, where the tempo steps
    if found is None:
        model["note"] = why
        times, coarse, tempo = track(mono, sr, envelope, hint)
        # One grid can fit the stronger of two tempos and call the song steady, so
        # the envelope is read for steps whatever the grid says. A song that keeps
        # to no grid, or steps, has its beats followed one by one, each moved to
        # its attack, as the model's are.
        cuts = segments.rough_steps(envelope, fps)
        if cuts or asked_steps or len(hints) > 1 or not tempo["steady"]:
            follow_beats = None
            if cuts or not tempo["steady"]:
                follow_beats = downbeats.refined(
                    mono, sr, segments.followed(mono, sr, envelope, cuts, 60 / tempo["bpm"])
                )
            first = times if follow_beats is None else follow_beats
            asked_at = segments.forced_at(first, asked_steps)
            starts = segments.joined(segments.steps_in(first), asked_at, len(first))
            if starts or follow_beats is not None:
                split = {
                    "first": first,
                    "starts": [0, *starts],
                    "hints": [hints[k] if k < len(hints) else None for k in range(len(starts) + 1)],
                    "forced": [False, *(s in asked_at for s in starts)],
                    "followed": follow_beats,
                }
    else:
        seg = downbeats.segmented(found, _unit(flux), fps, hints, asked_steps)
        if seg is not None:
            split = {
                "first": seg["beats"],
                "starts": seg["starts"],
                "hints": seg["bpm"],
                "forced": seg["forced"],
                "model": seg,
            }
        else:
            times, coarse, tempo = track(mono, sr, envelope, plan["bpm"] or hint)
            agreement = downbeats.shared(plan["beats"], times) if plan["factor"] else None
            model.update(
                used=True,
                heard_bpm=round(plan["heard_bpm"], 2),
                counted={1.0: "as heard", 2.0: "doubled", 0.5: "halved", None: "at --bpm"}[plan["factor"]],
                beats_from="onsets",
                agreement=None if agreement is None else round(agreement, 3),
                device=found["device"],
                seconds=found["seconds"],
            )
            if agreement is not None and agreement < downbeats.SAME_SHARE:
                times, tempo = downbeats.tempo_of(downbeats.refined(mono, sr, plan["beats"]), mono, sr)
                coarse = times.copy()
                model["beats_from"] = "model"
            # Half and double the tempo, and the onsets' own first choice where it is another.
            chosen = tempo["bpm"]
            others = [b for b in (chosen / 2, chosen * 2) if MIN_BPM <= b <= MAX_BPM]
            own = tempo_options(envelope, fps)[0]
            if all(abs(np.log(own / b)) > 0.04 for b in [chosen, *others]):
                others.append(own)
            tempo["ambiguous_with"] = sorted(round(float(b), 2) for b in others)
    if split is None and len(hints) > 1:
        raise ValueError(
            f"--bpm gives {len(hints)} tempos and the map has one tempo; give --segment-at for each step"
        )
    if split is not None:
        spans = segments.bounds(split["first"], split["starts"], duration)
        seg = split.get("model")
        if seg is None:
            pieces = segments.measured(mono, sr, envelope, spans, split["hints"], split["followed"], label="followed")
        else:
            pieces = segments.measured(mono, sr, envelope, spans, split["hints"], seg["beats"], seg["counted"])
        pieces, spans, split = segments.merged(mono, sr, envelope, pieces, spans, split)
        seg = split.get("model")
        times, coarse, seg_starts = segments.joined_beats(pieces)
        tempo = segments.summary(pieces)
        if seg is not None:
            sources = {f["beats_from"] for _, _, f in pieces}
            shares = [f["agreement"] for _, _, f in pieces if f.get("agreement") is not None]
            model.update(
                used=True,
                heard_bpm=round(seg["heard_bpm"][0], 2),
                counted=seg["how"][0],
                beats_from=sources.pop() if len(sources) == 1 else "both",
                agreement=round(min(shares), 3) if shares else None,
                device=found["device"],
                seconds=found["seconds"],
            )
        tempo["ambiguous_with"] = segments.ambiguous(
            tempo["bpm"], [f["bpm"] for _, _, f in pieces[1:2]], tempo["ambiguous_with"]
        )
        # Where each segment starts, by its first beat's time, since beats
        # before the first sound and after the last are dropped below.
        seg_times = [float(times[s]) if s < len(times) else duration for s in seg_starts]
    inside = (times >= first_sound - 0.02) & (times < min(last_sound, duration))
    times, coarse = times[inside], coarse[inside]
    if len(times) < max(8, 2 * (asked_bar or 4)):
        raise ValueError(f"Too few beats for a beat map: {path}")
    forced = sorted({int(np.argmin(np.abs(times - t))) for t in asked})
    if split is not None:
        seg_at = [int(np.searchsorted(times, t - 1e-9)) for t in seg_times]

    # Each beat's strength: the envelope's peak near it, against the strongest.
    frames = np.round(coarse * fps).astype(int)
    reach = np.clip(frames[:, None] + np.arange(-2, 3)[None, :], 0, len(envelope) - 1)
    strength = envelope[reach].max(axis=1)
    strength = np.minimum(strength / max(np.percentile(strength, 95), 1e-30), 1.0)

    # What changes on each beat: the level in bands, and the pitch classes.
    end_bpm = tempo["bpm"] if split is None else pieces[-1][2]["bpm"]
    ends = np.append(times, min(times[-1] + 60 / end_bpm, duration))
    floor = max(float(power.max()), 1e-30) * 1e-8
    by_beat = _per_beat(power, fps, ends)

    if found is not None:
        # The bars: where the model hears downbeats, in bars of the length it hears.
        # In a song in segments each is counted its own way, and its bars are
        # found as a song's are, from where it starts.
        how = {"as heard": 1.0, "doubled": 2.0, "halved": 0.5, "at --bpm": None}
        if split is None:
            counts = [(0, len(times), plan["factor"])]
        else:
            ends_at = [*seg_at[1:], len(times)]
            counts = [(lo, hi, how[h]) for lo, hi, h in zip(seg_at, ends_at, split["model"]["how"]) if hi > lo]
        factor = counts[0][2]
        if asked_bar:
            per_bar = asked_bar
        elif factor == 2.0:
            per_bar = plan["bar"]
        elif factor == 0.5:
            per_bar = max(1, plan["bar"] // 2)
        elif factor is None:
            per_bar = 4
        else:
            per_bar = plan["bar"]
        odds_on = downbeats.likelihood(found["downbeat"], found["fps"], times)
        # The model's bars up to the first downbeat asked for; from each one asked
        # for, bars of the map's length to the next.
        until = forced[0] if forced else len(times)
        heard_starts, model_starts = [], []
        for lo, hi, factor in counts:
            hi = min(hi, until)
            if hi <= lo:
                continue
            length = per_bar * (2 if factor == 2.0 else 1)
            if hi == until and forced:
                found_starts = downbeats.bars(odds_on[lo : hi + 1], length, [hi - lo]) + lo
                found_starts = found_starts[found_starts < hi]
            else:
                found_starts = downbeats.bars(odds_on[lo:hi], length) + lo
            heard_starts.extend(found_starts)
            if factor == 2.0 and len(found_starts):
                model_starts.extend(downbeats.halves(found_starts - lo, hi - lo, per_bar) + lo)
            else:
                model_starts.extend(found_starts)
        heard_starts = np.array(heard_starts, dtype=int)
        model_starts = np.array(model_starts, dtype=int)
        asked_starts = downbeats.anchored(len(times), forced, per_bar, back=False) if forced else []
        starts = np.array([*model_starts, *asked_starts], dtype=int)
        # A bar a doubled count cut in two is as sure as the bar it came from.
        sure = np.zeros(0)
        if len(heard_starts):
            source = np.clip(np.searchsorted(heard_starts, model_starts, side="right") - 1, 0, None)
            sure = odds_on[heard_starts[source]]
        downbeat_report = {
            "confidence": round(float(np.median(sure)), 3) if len(sure) else None,
            "set_by": "request" if asked else "model",
            "uncertain_bars": [int(b) for b in np.flatnonzero(sure < 0.5) + 1],
        }
        meter_set_by = "request" if asked_bar else "model"
    else:
        per_bar = asked_bar or 4
        levels = 10 * np.log10(by_beat + floor)
        notes = _per_beat(chroma(mono, sr), sr / CHROMA_HOP, ends)
        odds = downbeat_odds(
            [
                low[reach].max(axis=1),  # a kick or a bass note
                _change(notes, 2, _cosine_distance)[:-1],  # a change of harmony
                _change(notes, 4, _cosine_distance)[:-1],
                _change(levels, 4, _level_distance)[:-1],  # a change of sound
                _change(levels, PHRASE_BARS * per_bar, _level_distance)[:-1],
            ],
            len(times),
            per_bar,
        )
        chosen = int(np.argmax(odds)) if not forced else forced[0] % per_bar
        starts = downbeats.anchored(len(times), forced or [chosen], per_bar)
        downbeat_report = {
            "confidence": round(float(odds[chosen]), 3),
            "set_by": "request" if asked else "measurement",
            "candidates": [
                {
                    "first_downbeat_seconds": round(float(times[p]), 6),
                    "confidence": round(float(odds[p]), 3),
                    "chosen": p == chosen,
                }
                for p in sorted(range(per_bar), key=lambda p: -odds[p])
            ],
        }
        meter_set_by = "request" if asked_bar else "assumed"

    # Phrases: downbeats where what follows differs most from what came before,
    # over four bars, two and one. A change that all three see is on that bar.
    # The bands are wide here, so a bass line that moves is not a change of parts.
    wide = np.add.reduceat(by_beat, np.arange(0, by_beat.shape[1], 4), axis=1)
    parts = 10 * np.log10(wide + floor)
    change = np.zeros(len(times))
    for span in (PHRASE_BARS * per_bar, 2 * per_bar, per_bar):
        differs = _change(parts, span, _level_distance)[:-1]
        change += differs / max(float(differs[starts].max()), 1e-30)
    change = np.round(change / max(float(change[starts].max()), 1e-30), 3)
    phrases = []
    for i, beat in enumerate(starts):
        around = starts[max(0, i - 2) : i + 3]
        if i == 0 or (change[beat] >= 0.25 and change[beat] == change[around].max()):
            phrases.append(int(beat))

    from . import swing

    groove = swing.measure(mono, sr, ends)
    first = set(int(s) for s in starts)
    beats = [
        {
            "seconds": round(float(t), 6),
            "bar": bar,
            "beat": beat,
            "index": index,
            "strength": round(float(s), 3),
            **({"change": float(change[i])} if i in first else {}),
            **({"phrase_start": True} if i in phrases else {}),
        }
        for i, (t, s, (bar, beat, index)) in enumerate(
            zip(times, strength, downbeats.labels(len(times), starts, per_bar))
        )
    ]
    if split is None:
        first_segment = {
            "bar": beats[0]["bar"],
            "seconds": beats[0]["seconds"],
            "beat_seconds": beats[0]["seconds"],
            "bpm": tempo["bpm"],
            "steady": tempo["steady"],
            "drift_ms": tempo["drift_ms"],
            "bpm_range": tempo["bpm_range"],
            "ambiguous_with": tempo["ambiguous_with"],
            "beats_from": model.get("beats_from", "onsets"),
            "set_by": "measurement",
        }
        tempo["segments"] = [first_segment]
    else:
        at = [min(i, len(times) - 1) for i in seg_at]
        set_by = ["measurement", *("request" if f else "measurement" for f in split["forced"][1:])]
        tempo["segments"] = segments.report(times, at, pieces, beats, per_bar, set_by)
    return {
        "path": str(path),
        "analyzer": ANALYZER,
        "duration": duration,
        "sample_rate": sr,
        "tempo": tempo,
        "meter": meter or f"{per_bar}/4",
        "meter_set_by": meter_set_by,
        "beats_per_bar": per_bar,
        "odd_bars": downbeats.odd(starts, times, per_bar),
        "first_beat_seconds": beats[0]["seconds"],
        "first_downbeat_seconds": beats[int(starts[0])]["seconds"],
        "downbeat": downbeat_report,
        "model": model,
        "phrases": [
            {k: beats[i][k] for k in ("seconds", "bar", "change")} for i in phrases
        ],
        "swing": groove,
        "beat_count": len(beats),
        "beats": beats,
        "assumes": (
            f"{per_bar} beats a bar but where odd_bars lists another; bars are counted from the "
            "first downbeat, and beats before it are bar 0"
        ),
        "source": "measured from decoded audio (mono sum); estimates, not ground truth",
    }


def last_bpm(report) -> float:
    """The tempo a map ends at: its last segment's, which is its one tempo in a
    map made before segments."""
    found = report["tempo"].get("segments") or [report["tempo"]]
    return float(found[-1]["bpm"])


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
