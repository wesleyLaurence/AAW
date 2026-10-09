"""Translation checks: what a mix loses in mono and on a small speaker, on audio whose answers are known."""

import json
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import translation
from agent_daw.model import digest, save, validate
from agent_daw.perception import BANDS, compare, listen, measure
from helpers import SR, cli, daw, render

SECONDS = 16
BEATS = SECONDS * 2  # at 120


def noise(seed=1, amp=0.1, seconds=SECONDS):
    return amp * np.random.default_rng(seed).standard_normal(seconds * SR)


def flipped(x, below=200):
    """The same audio with its polarity turned over under a frequency."""
    found = np.fft.rfft(x)
    found[np.fft.rfftfreq(len(x), 1 / SR) < below] *= -1
    return np.fft.irfft(found, len(x))


def sine(hz, amp):
    x = amp * np.sin(2 * np.pi * hz * np.arange(SECONDS * SR) / SR)
    return np.column_stack([x, x])


def heard(directory, name, x):
    """`translation` of a file that is not a render."""
    directory.mkdir(exist_ok=True)
    sf.write(directory / f"{name}.wav", x, SR, subtype="DOUBLE")
    return listen(directory / f"{name}.wav", images=False)


def song(directory, parts, tracks=None, **more):
    """A song that plays each part's audio once on a track of its name, and its render."""
    directory.mkdir(parents=True, exist_ok=True)
    rows = []
    for name, x in parts.items():
        sf.write(directory / f"{name}.wav", x, SR, subtype="DOUBLE")
        rows.append(
            {
                "id": name,
                "pads": {"x": {"sample": name, "attack_ms": 0, "release_ms": 0}},
                "clips": [{"pattern": "once"}],
                **(tracks or {}).get(name, {}),
            }
        )
    document = {
        "session": {"tempo": 120, "length_beats": BEATS, "master_gain_db": 0, "end_fade_ms": 0},
        "samples": {name: {"path": f"{name}.wav"} for name in parts},
        "patterns": {"once": {"length_beats": BEATS, "events": [{"at": 0, "pad": "x", "velocity": 127}]}},
        "tracks": rows,
        **more,
    }
    save(validate(document), directory / "song.yaml")
    return Path(render(directory / "song.yaml")["directory"])


def test_what_a_file_loses_in_mono_by_band(tmp_path):
    a = noise(1)
    # Channels that are the same lose nothing, in any band.
    same = heard(tmp_path, "same", np.column_stack([a, a]))["translation"]
    assert same["mono"]["loss_db"] == 0 and set(same["mono"]["band_loss_db"]) == set(BANDS)
    assert all(v == 0 for v in same["mono"]["band_loss_db"].values())
    assert same["observations"] == [] and same["observations_omitted"] == 0
    # Channels with nothing in common lose 3 dB in every band, given a minute
    # of them: 40 Hz of noise strays a quarter of a decibel over sixteen seconds.
    report = heard(tmp_path, "apart", np.column_stack([noise(1, seconds=60), noise(2, seconds=60)]))
    apart = report["translation"]["mono"]
    assert all(v == pytest.approx(-3, abs=0.2) for v in apart["band_loss_db"].values())
    # Over all frequencies it is what side_energy_fraction already says.
    side = report["mix"]["side_energy_fraction"]
    assert apart["loss_db"] == pytest.approx(10 * np.log10(1 - side), abs=0.05)
    assert report["translation"]["observations"] == []
    # One channel's polarity turned over under 200 Hz: the two low bands go, and nothing above.
    report = heard(tmp_path, "flip", np.column_stack([a, flipped(a)]))
    flip = report["translation"]["mono"]["band_loss_db"]
    assert flip["sub"] < -40 and flip["low"] < -5
    assert all(flip[band] > -0.5 for band in ("low_mid", "mid", "high_mid", "high", "air"))
    # White noise has little of its power under 200 Hz, so the whole says nothing: the bands do.
    assert report["translation"]["mono"]["loss_db"] > -0.1
    assert report["translation"]["observations"] == [
        "the mix loses 60 dB or more in mono in sub, 20 Hz to 60 Hz",
        f"the mix loses {-flip['low']:.1f} dB in mono in low, 60 Hz to 250 Hz",
    ]
    assert "side_energy_fraction" in report["methods"]["translation"]
    # Opposite channels are gone, which is written as 60 dB and not as null.
    gone = heard(tmp_path, "gone", np.column_stack([a, -a]))["translation"]["mono"]
    assert gone["loss_db"] == -60 and set(gone["band_loss_db"].values()) == {-60}
    # A file of one channel has nothing to sum, and silence nothing to lose.
    one = heard(tmp_path, "one", a[:, None])["translation"]
    assert one["mono"] is None and one["small_speaker"]["loss_db"] is not None
    silent = heard(tmp_path, "silent", np.zeros((SR, 2)))["translation"]
    assert silent["mono"]["loss_db"] is None and set(silent["mono"]["band_loss_db"].values()) == {None}
    assert silent["small_speaker"]["loss_lu"] is None and silent["small_speaker"]["loss_db"] is None


def test_where_a_loss_is_said_in_words():
    def said(*side):
        return translation.where(np.array(side, dtype=float), BANDS)

    assert said(5, 4, 0, 0, 0, 1, 0) == ", most of it under 250 Hz"
    assert said(0, 0, 0, 0, 1, 5, 4) == ", most of it over 4 kHz"
    assert said(0, 0, 1, 8, 0, 0, 1) == ", most of it from 500 Hz to 2 kHz"
    # Spread over the spectrum, or nothing lost, it says nothing.
    assert said(1, 1, 1, 1, 1, 1, 1) == "" and said(0, 0, 0, 0, 0, 0, 0) == ""
    assert translation.lost(-5.24) == "5.2 dB" and translation.lost(-60) == "60 dB or more"


def test_a_utility_that_sums_the_low_end_takes_the_loss_away(tmp_path):
    a = noise(1)
    parts = {"pad": np.column_stack([a, flipped(a)]), "lead": sine(1000, 0.1)}
    halves = [
        {"id": "first", "at": 0, "length_beats": BEATS // 2},
        {"id": "second", "at": BEATS // 2, "length_beats": BEATS // 2},
    ]
    wide = song(tmp_path / "wide", parts, sections=halves)
    report = listen(wide, images=False)
    mono = report["translation"]["mono"]
    pad = mono["tracks"]["pad"]["band_loss_db"]
    assert pad["sub"] == -60 and pad["low"] < -5 and pad["mid"] == 0
    # A tone has one band, and the rest of them hold nothing to lose.
    assert mono["tracks"]["lead"] == {
        "loss_db": 0,
        "band_loss_db": {band: 0 if band == "mid" else None for band in BANDS},
    }
    # The mix's band is said with the stem that loses it there.
    assert report["translation"]["observations"] == [
        "the mix loses 60 dB or more in mono in sub, 20 Hz to 60 Hz: pad loses 60 dB or more there",
        f"the mix loses {-mono['band_loss_db']['low']:.1f} dB in mono in low, 60 Hz to 250 Hz: "
        f"pad loses {-pad['low']:.1f} dB there",
    ]
    # Each section of the mix has its own.
    assert set(mono["sections"]) == {"first", "second"}
    assert mono["sections"]["second"]["band_loss_db"]["low"] == pytest.approx(pad["low"], abs=0.5)

    # Summed under 200 Hz, nothing is left to cancel. The crossover's upper half
    # lets a little of what lies under it through, which is all `low` then loses.
    fixed = {"pad": {"effects": [{"type": "utility", "mono_below_hz": 200}]}}
    narrow = song(tmp_path / "narrow", parts, fixed, sections=halves)
    after = listen(narrow, images=False)["translation"]
    assert all(v is None or v > -1 for v in after["mono"]["tracks"]["pad"]["band_loss_db"].values())
    assert all(v is None or v > -1 for v in after["mono"]["band_loss_db"].values())
    assert after["observations"] == []

    # daw compare has each loss before and after.
    change = compare(wide, narrow, images=False)["translation"]["mono"]
    low = change["band_loss_db"]
    assert low["before"]["low"] == mono["band_loss_db"]["low"]
    assert low["after"]["low"] == after["mono"]["band_loss_db"]["low"]
    assert low["delta"]["low"] == pytest.approx(low["after"]["low"] - low["before"]["low"], abs=0.11)
    assert low["delta"]["low"] > 4 and low["delta"]["mid"] == 0
    assert list(change["tracks"]) == ["pad", "lead"]
    assert change["tracks"]["pad"]["band_loss_db"]["delta"]["low"] > 4
    assert change["tracks"]["lead"]["loss_db"] == {"before": 0, "after": 0, "delta": 0}
    assert set(change["sections"]) == {"first", "second"}
    assert set(change["sections"]["first"]["loss_db"]) == {"before", "after", "delta"}


def bass_and_lead(directory, effects=(), **more):
    parts = {"bass": sine(50, 0.3), "lead": sine(1000, 0.1)}
    tracks = {"bass": {"effects": list(effects), **more.pop("bass", {})}}
    return song(directory, parts, tracks, **more)


def test_a_sine_bass_is_gone_on_a_small_speaker_and_saturation_brings_it_back(tmp_path):
    halves = [
        {"id": "first", "at": 0, "length_beats": BEATS // 2},
        {"id": "second", "at": BEATS // 2, "length_beats": BEATS // 2},
    ]
    plain = bass_and_lead(tmp_path / "plain", sections=halves)
    report = listen(plain, images=False)
    small = report["translation"]["small_speaker"]
    assert small["band_hz"] == [200, 8000] and small["slope_db_per_octave"] == 24
    # Two octaves under a highpass of 24 dB an octave.
    assert small["tracks"]["bass"]["loss_db"] == pytest.approx(-48.2, abs=1)
    assert small["tracks"]["lead"]["loss_db"] == 0
    # The bass carried most of the mix, so the mix loses and the lead comes forward.
    assert small["loss_db"] == pytest.approx(-10, abs=0.3) and small["loss_lu"] < -3
    assert small["tracks"]["lead"]["share_change_db"] == pytest.approx(-small["loss_db"], abs=0.11)
    assert small["tracks"]["bass"]["share_change_db"] == pytest.approx(-38.2, abs=1)
    assert small["sections"]["first"]["loss_db"] == pytest.approx(small["loss_db"], abs=0.3)
    assert small["sections"]["first"]["loss_lu"] == pytest.approx(small["loss_lu"], abs=0.5)
    assert report["translation"]["observations"] == [
        f"bass loses {-small['tracks']['bass']['loss_db']:.1f} dB on a small speaker: its energy is under 200 Hz"
    ]
    # daw describe listen says what each field means.
    said = " ".join(daw("describe", "listen")["semantics"].values())
    fields = ("translation", "band_loss_db", "loss_lu", "share_change_db", "small_speaker", "--write-translation")
    assert all(field in said for field in fields)
    # One section's, with --section.
    code, section = cli("listen", plain, "--section", "second", "--no-images")
    assert code == 0, section
    assert section["translation"] == {
        "mono": report["translation"]["mono"]["sections"]["second"],
        "small_speaker": small["sections"]["second"],
    }

    # Driven into a soft clip, the bass has harmonics the speaker plays.
    drive = {"type": "saturation", "mode": "soft", "drive_db": 24, "output_db": -12}
    driven = bass_and_lead(tmp_path / "driven", [drive], sections=halves)
    after = listen(driven, images=False)["translation"]
    assert -20 < after["small_speaker"]["tracks"]["bass"]["loss_db"] < -10
    assert after["observations"] == []
    change = compare(plain, driven, images=False)["translation"]["small_speaker"]
    assert list(change["tracks"]) == ["bass", "lead"]
    bass = change["tracks"]["bass"]
    assert bass["loss_db"]["before"] == small["tracks"]["bass"]["loss_db"]
    assert bass["loss_db"]["delta"] > 25 and bass["share_change_db"]["delta"] > 25
    assert set(change["loss_lu"]) == {"before", "after", "delta"}
    assert set(change["sections"]["first"]["loss_lu"]) == {"before", "after", "delta"}


def test_a_group_gives_way_to_its_track_and_a_stem_nobody_hears_is_not_named(tmp_path):
    parts = {"bass": sine(50, 0.3), "lead": sine(1000, 0.1), "rumble": sine(40, 0.3)}
    folder = song(
        tmp_path,
        parts,
        {"bass": {"group": "lows"}, "rumble": {"gain_db": -50}},
        groups=[{"id": "lows"}],
    )
    report = listen(folder, images=False)["translation"]
    stems = report["small_speaker"]["tracks"]
    # The group loses what its track does, and the track is the one named.
    assert stems["lows"]["loss_db"] == pytest.approx(stems["bass"]["loss_db"], abs=0.2)
    # A tone at 40 Hz is past what a loss is written to.
    assert stems["rumble"]["loss_db"] < -50
    (line,) = report["observations"]
    assert line.startswith("bass loses ") and report["observations_omitted"] == 0


def test_the_mix_is_written_in_mono_and_through_the_speaker_for_a_person_to_hear(tmp_path):
    a = noise(3)
    folder = song(tmp_path, {"pad": np.column_stack([a, noise(4)]), "bass": sine(50, 0.3)})
    mix = digest(folder / "mix.wav")
    code, plain = cli("listen", folder, "--no-images")
    assert code == 0 and plain["translation_audio"] == {}
    assert not list(folder.rglob("mono.wav"))
    code, shown = cli("listen", folder, "--no-images", "--write-translation")
    assert code == 0, shown
    written = shown["translation_audio"]
    beside = Path(shown["report_path"]).parent
    assert Path(written["mono"]) == beside / "mono.wav"
    assert Path(written["small_speaker"]) == beside / "small-speaker.wav"
    assert json.loads(Path(shown["report_path"]).read_text())["translation_audio"] == written
    # The render is not changed, and the measurements are the ones without the files.
    assert digest(folder / "mix.wav") == mix and shown["translation"] == plain["translation"]
    assert sf.info(written["mono"]).subtype == sf.info(folder / "mix.wav").subtype

    # The mono file is the mid in both channels: it has no side left to lose.
    mono = listen(written["mono"], images=False)
    assert mono["mix"]["side_energy_fraction"] == 0
    assert mono["translation"]["mono"]["loss_db"] == 0
    assert mono["mix"]["rms_dbfs"] - shown["mix"]["rms_dbfs"] == pytest.approx(
        shown["translation"]["mono"]["loss_db"], abs=0.1
    )
    # The speaker's file has lost the bass, and its level is the loss the report gave.
    x, rate = sf.read(written["small_speaker"], always_2d=True)
    small = measure(x, rate)
    assert small["band_dbfs"]["sub"] < shown["mix"]["band_dbfs"]["sub"] - 40
    assert written["small_speaker_gain_db"] <= 0
    assert small["rms_dbfs"] - written["small_speaker_gain_db"] - shown["mix"]["rms_dbfs"] == pytest.approx(
        shown["translation"]["small_speaker"]["loss_db"], abs=0.2
    )
    assert small["integrated_lufs"] - written["small_speaker_gain_db"] - shown["mix"][
        "integrated_lufs"
    ] == pytest.approx(shown["translation"]["small_speaker"]["loss_lu"], abs=0.2)
    assert float(np.abs(x).max()) <= 1

    # The Rust daw passes the flag on, and it goes with the whole report only.
    assert daw("listen", folder, "--no-images", "--write-translation")["translation_audio"] == written
    code, error = cli("listen", folder, "--write-translation", "--overlap", "pad", "bass")
    assert code == 1 and "whole report" in error["error"]
