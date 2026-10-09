"""Translation checks: the mix as it arrives somewhere else.

Summed to mono, and through the band a small speaker plays: what the mix, each
band and each stem loses, in decibels. A person presses a mono button and plays
the song on a phone; both are arithmetic on audio the render already wrote.
These are measurements: a sub that a phone cannot play may be meant.
"""

from __future__ import annotations

import numpy as np
import soundfile as sf
from numpy.lib.stride_tricks import sliding_window_view
from scipy import fft
from scipy.signal import butter, get_window, sosfilt, sosfreqz

# The small speaker: a highpass and a lowpass, each SLOPE_DB an octave. It
# stands in for a phone's speaker and is not a model of one; the corners are
# round numbers, not measured.
SPEAKER_HZ = (200, 8000)
SLOPE_DB = 24
WINDOW = 8192  # frames of the spectrum, as band_dbfs has it
# A loss is never written past this: the sound is gone. And a band this far
# under a sound's loudest holds nothing to lose.
FLOOR_DB = 60.0
SILENT = 1e-12  # power: -120 dBFS
# What an observation is made of. First guesses, heard by nobody.
MONO_DB = 3.0  # more than uncorrelated channels lose
SPEAKER_DB = 20.0
HEARD_DB = 40.0  # a stem this far under the mix is not named
MOST = 2 / 3  # of what is lost, for "most of it under 250 Hz"
LISTED = 5  # stems named for each of the two


def speaker(rate):
    """The small speaker's filter, as second-order sections."""
    order = SLOPE_DB // 6
    return np.vstack(
        [
            butter(order, SPEAKER_HZ[0], "highpass", fs=rate, output="sos"),
            butter(order, SPEAKER_HZ[1], "lowpass", fs=rate, output="sos"),
        ]
    )


def loss(kept, whole) -> float | None:
    """What is kept of a power, in decibels under it; None of nothing."""
    if whole <= 0:
        return None
    return max(float(10 * np.log10(kept / whole)), -FLOOR_DB) if kept > 0 else -FLOOR_DB


def tenth(value):
    return None if value is None else round(value, 1) + 0.0


def change(before, after):
    return None if before is None or after is None else tenth(after - before)


def spectra(x, rate, block=64):
    """The power in each bin of a spectrum of the channels, averaged, and of
    their mid: Hann windows of WINDOW frames, half overlapped, each with its
    mean taken out, as Welch's method has it. The mid's spectrum is the mean of
    the channels' before the power is taken, so no third transform is made. The
    scale is not a level's: what is read from these is one power against another."""
    n = min(len(x), WINDOW)
    shape = get_window("hann", n).astype(np.float32)
    starts = np.arange(0, len(x) - n + 1, max(1, n // 2))
    channels, mid = np.zeros(n // 2 + 1), np.zeros(n // 2 + 1)
    for at in range(0, len(starts), block):
        rows = starts[at : at + block]
        summed = 0.0
        for channel in range(x.shape[1]):
            pieces = sliding_window_view(x[:, channel], n)[rows].astype(np.float32)
            found = fft.rfft((pieces - pieces.mean(axis=1, keepdims=True)) * shape, axis=1, workers=-1)
            channels += (found.real**2 + found.imag**2).sum(axis=0, dtype=np.float64)
            summed = summed + found
        mid += (summed.real**2 + summed.imag**2).sum(axis=0, dtype=np.float64)
    count = x.shape[1]
    return fft.rfftfreq(n, 1 / rate), channels / count, mid / count**2


def reading(x, rate, bands, sos) -> dict:
    """What one stretch of audio loses: in mono over all frequencies and in
    each band, and on the small speaker, with the power lost in each band and
    either side of the speaker's, for saying where."""
    power = float(np.mean(x * x))
    out = {
        "power": power,
        "mono": None,
        "bands": {name: None for name in bands},
        "side": np.zeros(len(bands)),
        "speaker": None,
        "outside": (0.0, 0.0),
    }
    if power <= SILENT:
        return out
    stereo = x.shape[1] == 2
    mid = x.mean(axis=1)
    freq, channels, middle = spectra(x, rate)
    response = np.abs(sosfreqz(sos, worN=freq, fs=rate)[1]) ** 2
    out["speaker"] = loss(float(np.sum(channels * response)), float(np.sum(channels)))
    gone = channels * (1 - response)
    out["outside"] = (
        float(gone[freq < SPEAKER_HZ[0]].sum()),
        float(gone[freq > SPEAKER_HZ[1]].sum()),
    )
    if not stereo:
        return out
    out["mono"] = loss(float(np.mean(mid * mid)), power)
    inside = [(freq >= lo) & (freq < hi) for lo, hi in bands.values()]
    whole = np.array([channels[at].sum() for at in inside])
    kept = np.array([middle[at].sum() for at in inside])
    out["side"] = np.maximum(whole - kept, 0)
    floor = whole.max() * 10 ** (-FLOOR_DB / 10)
    out["bands"] = {
        name: loss(float(k), float(w)) if w > floor else None
        for name, w, k in zip(bands, whole, kept)
    }
    return out


def hz(value) -> str:
    return f"{value:g} Hz" if value < 1000 else f"{value / 1000:g} kHz"


def lost(db) -> str:
    """A loss in words, as the report's number has it."""
    return f"{FLOOR_DB:g} dB or more" if tenth(db) <= -FLOOR_DB else f"{-tenth(db):.1f} dB"


def where(side, bands) -> str:
    """The shortest run of bands that holds MOST of what a sound loses in mono,
    in words; nothing when it takes over half of them."""
    total = float(side.sum())
    edges = list(bands.values())
    if total <= 0:
        return ""
    for length in range(1, len(edges) // 2 + 2):
        runs = [(float(side[i : i + length].sum()), i) for i in range(len(edges) - length + 1)]
        most, first = max(runs)
        if most >= MOST * total:
            low, high = edges[first][0], edges[first + length - 1][1]
            if first == 0:
                return f", most of it under {hz(high)}"
            if first + length == len(edges):
                return f", most of it over {hz(low)}"
            return f", most of it from {hz(low)} to {hz(high)}"
    return ""


class Checks:
    """What a mix, its sections and its stems lose in mono and on a small
    speaker. `loudness` is perception's integrated loudness of some audio."""

    def __init__(self, x, rate, bands, loudness):
        self.rate, self.bands, self.loudness = rate, bands, loudness
        self.sos = speaker(rate)
        self.speaker = sosfilt(self.sos, x, axis=0)
        self.stereo = x.shape[1] == 2
        self.mix = reading(x, rate, bands, self.sos)
        self.sections: dict[str, tuple] = {}
        self.stems: dict[str, tuple] = {}

    def section(self, name, first, last, x, lufs):
        """A section of the mix, with its loudness as it is."""
        after = self.loudness(self.speaker[first:last], self.rate)
        self.sections[name] = (reading(x, self.rate, self.bands, self.sos), change(lufs, after))

    def add(self, name, stem, entry):
        """A stem, with its entry in the render's report."""
        self.stems[name] = (reading(stem, self.rate, self.bands, self.sos), entry)

    def mono(self, read) -> dict:
        return {
            "loss_db": tenth(read["mono"]),
            "band_loss_db": {name: tenth(value) for name, value in read["bands"].items()},
        }

    def share(self, read) -> float | None:
        """How a stem's share of the mix moves on the small speaker."""
        return change(self.mix["speaker"], read["speaker"])

    def summary(self, lufs) -> dict:
        """`translation` of a listen report; `lufs` is the mix's loudness as it is."""
        mono = None
        if self.stereo:
            mono = {
                **self.mono(self.mix),
                "sections": {name: self.mono(read) for name, (read, _) in self.sections.items()},
                "tracks": {name: self.mono(read) for name, (read, _) in self.stems.items()},
            }
        small = {
            "band_hz": list(SPEAKER_HZ),
            "slope_db_per_octave": SLOPE_DB,
            "loss_lu": change(lufs, self.loudness(self.speaker, self.rate)),
            "loss_db": tenth(self.mix["speaker"]),
            "sections": {
                name: {"loss_lu": lu, "loss_db": tenth(read["speaker"])}
                for name, (read, lu) in self.sections.items()
            },
            "tracks": {
                name: {"loss_db": tenth(read["speaker"]), "share_change_db": self.share(read)}
                for name, (read, _) in self.stems.items()
            },
        }
        said, omitted = self.observations()
        return {
            "mono": mono,
            "small_speaker": small,
            "observations": said,
            "observations_omitted": omitted,
        }

    def named(self, past) -> list[str]:
        """The stems an observation is made of, the largest loss first: those
        heard in the mix whose loss, as `past` gives it, is past its line. A
        group is left out where one of its tracks is named, which says more."""
        floor = self.mix["power"] * 10 ** (-HEARD_DB / 10)
        found = {
            name: past(read)
            for name, (read, _) in self.stems.items()
            if read["power"] > floor and past(read) is not None
        }
        for name, (_, entry) in self.stems.items():
            if name in found and any(track in found for track in entry.get("tracks", [])):
                del found[name]
        return sorted(found, key=lambda name: found[name])

    def observations(self) -> tuple[list[str], int]:
        """The largest losses in words, and how many stems were left out."""
        out, omitted = [], 0
        if self.stereo:
            bands = list(self.bands)
            for value, at in sorted(
                (value, at)
                for at, value in enumerate(self.mix["bands"].values())
                if value is not None and tenth(value) < -MONO_DB
            ):
                name = bands[at]
                low, high = self.bands[name]
                line = f"the mix loses {lost(value)} in mono in {name}, {hz(low)} to {hz(high)}"
                # The stem that loses the most power there, if it loses past the line itself.
                most = [
                    (read["side"][at], stem)
                    for stem, (read, entry) in self.stems.items()
                    if not entry.get("tracks")
                    and read["bands"][name] is not None
                    and tenth(read["bands"][name]) < -MONO_DB
                ]
                if most:
                    stem = max(most)[1]
                    line += f": {stem} loses {lost(self.stems[stem][0]['bands'][name])} there"
                out.append(line)
            wide = self.named(
                lambda r: r["mono"] if r["mono"] is not None and tenth(r["mono"]) < -MONO_DB else None
            )
            omitted += max(0, len(wide) - LISTED)
            for name in wide[:LISTED]:
                read = self.stems[name][0]
                out.append(f"{name} loses {lost(read['mono'])} in mono{where(read['side'], self.bands)}")
        thin = self.named(
            lambda r: r["speaker"] if r["speaker"] is not None and tenth(r["speaker"]) < -SPEAKER_DB else None
        )
        omitted += max(0, len(thin) - LISTED)
        for name in thin[:LISTED]:
            read = self.stems[name][0]
            below, above = read["outside"]
            if below >= 9 * above:
                why = f"its energy is under {hz(SPEAKER_HZ[0])}"
            elif above >= 9 * below:
                why = f"its energy is over {hz(SPEAKER_HZ[1])}"
            else:
                why = f"its energy is outside {hz(SPEAKER_HZ[0])} to {hz(SPEAKER_HZ[1])}"
            out.append(f"{name} loses {lost(read['speaker'])} on a small speaker: {why}")
        return out, omitted

    def write(self, x, folder, subtype) -> dict:
        """`mono.wav` and `small-speaker.wav`, for a person to hear what was
        measured: the mid in both channels, and the mix through the speaker's
        filter, lowered where its peaks would pass full scale."""
        subtype = subtype if sf.check_format("WAV", subtype) else "PCM_24"
        mid = x.mean(axis=1)
        sf.write(folder / "mono.wav", np.column_stack([mid, mid]), self.rate, subtype=subtype)
        peak = float(np.max(np.abs(self.speaker)))
        gain = min(1.0, 1 / peak) if peak > 0 else 1.0
        sf.write(folder / "small-speaker.wav", self.speaker * gain, self.rate, subtype=subtype)
        return {
            "mono": str((folder / "mono.wav").resolve()),
            "small_speaker": str((folder / "small-speaker.wav").resolve()),
            "small_speaker_gain_db": round(float(20 * np.log10(gain)), 2) + 0.0,
        }


def both(before, after) -> dict:
    return {"before": before, "after": after, "delta": change(before, after)}


def each(before, after) -> dict:
    """Each band's loss before and after."""
    return {
        "before": before,
        "after": after,
        "delta": {name: change(before[name], after[name]) for name in before},
    }


def largest_first(rows, key) -> dict:
    return dict(sorted(rows.items(), key=lambda row: -abs(row[1][key]["delta"] or 0)))


def between(a, b, sections) -> dict:
    """`translation` in `daw compare`: each loss before and after, of the mix,
    the sections where the two renders line up and the stems both have, the
    stem whose loss changed most first."""
    mono = None
    if a["mono"] and b["mono"]:
        ma, mb = a["mono"], b["mono"]
        mono = {
            "loss_db": both(ma["loss_db"], mb["loss_db"]),
            "band_loss_db": each(ma["band_loss_db"], mb["band_loss_db"]),
            "sections": {
                s: {"loss_db": both(ma["sections"][s]["loss_db"], mb["sections"][s]["loss_db"])}
                for s in sections
            },
            "tracks": largest_first(
                {
                    name: {
                        "loss_db": both(was["loss_db"], mb["tracks"][name]["loss_db"]),
                        "band_loss_db": each(was["band_loss_db"], mb["tracks"][name]["band_loss_db"]),
                    }
                    for name, was in ma["tracks"].items()
                    if name in mb["tracks"]
                },
                "loss_db",
            ),
        }
    sa, sb = a["small_speaker"], b["small_speaker"]
    return {
        "mono": mono,
        "small_speaker": {
            "band_hz": sb["band_hz"],
            "loss_lu": both(sa["loss_lu"], sb["loss_lu"]),
            "loss_db": both(sa["loss_db"], sb["loss_db"]),
            "sections": {
                s: {"loss_lu": both(sa["sections"][s]["loss_lu"], sb["sections"][s]["loss_lu"])}
                for s in sections
            },
            "tracks": largest_first(
                {
                    name: {key: both(was[key], sb["tracks"][name][key]) for key in ("loss_db", "share_change_db")}
                    for name, was in sa["tracks"].items()
                    if name in sb["tracks"]
                },
                "loss_db",
            ),
        },
    }


METHOD = (
    "mono is the mid, (L+R)/2, against the mean power of the two channels: 0 dB for channels "
    "that are the same, -3 for channels with nothing in common, and further as they near "
    "opposite polarity; over all frequencies it is 10*log10(1 - side_energy_fraction), and by "
    f"band it is read from Welch spectra of the mid and of the channels, {WINDOW} frames. "
    f"small_speaker is a highpass at {SPEAKER_HZ[0]} Hz and a lowpass at {SPEAKER_HZ[1]} Hz, "
    f"Butterworth, {SLOPE_DB} dB an octave each, which stands in for a phone's speaker and is "
    "not a model of one: loss_lu is the filtered mix's integrated loudness against the mix's, "
    "loss_db the power kept, and share_change_db a stem's loss against the mix's. A loss is "
    f"never written past -{FLOOR_DB:g} dB, which means gone, and a band more than "
    f"{FLOOR_DB:g} dB under a sound's loudest is null; mono is null for a file of one channel. "
    f"observations name a band of the mix or a stem that loses more than {MONO_DB:g} dB in "
    f"mono and a stem that loses more than {SPEAKER_DB:g} dB on the small speaker, among the "
    f"stems within {HEARD_DB:g} dB of the mix; measured, not judged"
)
