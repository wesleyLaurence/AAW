"""Dynamics in detail: the loudness range and maxima, how hard hits land, and reduction by section, on audio whose answers are known."""

import json
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import aaw_py, dynamics
from agent_daw.model import save, validate
from agent_daw.perception import compare, integrated, listen
from helpers import SR, cli, daw, render

BEATS = 32  # sixteen seconds at 120
HALVES = [
    {"id": "first", "at": 0, "length_beats": BEATS // 2},
    {"id": "second", "at": BEATS // 2, "length_beats": BEATS // 2},
]


def tone(hz, seconds, amp):
    x = amp * np.sin(2 * np.pi * hz * np.arange(round(seconds * SR)) / SR)
    return np.column_stack([x, x])


def kick(amp=0.5):
    """A sweep that falls to 50 Hz and dies away in a quarter of a second."""
    t = np.arange(round(0.25 * SR)) / SR
    sweep = np.cumsum(50 + 120 * np.exp(-t / 0.02)) / SR
    x = amp * np.sin(2 * np.pi * sweep) * np.exp(-t / 0.08)
    return np.column_stack([x, x])


def hat(amp=0.2):
    """Twenty milliseconds of noise: an attack and nothing after it."""
    n = round(0.02 * SR)
    x = amp * np.random.default_rng(5).standard_normal(n) * np.exp(-np.arange(n) / (0.004 * SR))
    return np.column_stack([x, x]).clip(-0.9, 0.9)


def pads(*names):
    return {name: {"sample": name, "attack_ms": 0, "release_ms": 0} for name in names}


def every(pad, step, length=4, start=0):
    return [{"at": start + step * i, "pad": pad, "velocity": 127} for i in range(round((length - start) / step))]


def song(directory, samples, tracks, patterns, **more):
    """A song of these tracks at 120, and its render."""
    directory.mkdir(parents=True, exist_ok=True)
    for name, x in samples.items():
        sf.write(directory / f"{name}.wav", x, SR, subtype="DOUBLE")
    document = {
        "session": {"tempo": 120, "length_beats": BEATS, "master_gain_db": 0, "end_fade_ms": 0},
        "samples": {name: {"path": f"{name}.wav"} for name in samples},
        "patterns": patterns,
        "tracks": tracks,
        **more,
    }
    save(validate(document), directory / "song.yaml")
    return Path(render(directory / "song.yaml")["directory"])


FOUR = {"four": {"length_beats": 4, "events": every("kick", 1)}}


def kicks(effects=(), **more):
    return {
        "id": "kick",
        "pads": pads("kick"),
        "clips": [{"pattern": "four", "repeats": BEATS // 4}],
        "effects": list(effects),
        **more,
    }


def squeeze(attack_ms):
    return {"type": "compressor", "threshold_db": -30, "ratio": 8, "attack_ms": attack_ms, "release_ms": 80}


def test_the_range_and_the_loudest_seconds_of_a_tone_at_two_levels(tmp_path):
    # Twenty seconds at -20 LUFS, then twenty at -30.
    x = np.concatenate([tone(1000, 20, 0.1), tone(1000, 20, 0.1 * 10**-0.5)])
    sf.write(tmp_path / "steps.wav", x, SR, subtype="DOUBLE")
    report = listen(tmp_path / "steps.wav", images=False)
    mix = report["mix"]
    assert mix["loudness_range_lu"] == pytest.approx(10, abs=0.1)
    assert mix["max_short_term_lufs"] == pytest.approx(-20, abs=0.1)
    assert mix["max_momentary_lufs"] == pytest.approx(-20, abs=0.1)
    assert "EBU Tech 3342" in report["methods"]["dynamics"]
    # The loudest three seconds are those of the short-term loudness the timeline lists.
    listed = max(row["lufs"] for row in report["timeline"]["short_term_loudness"])
    assert mix["max_short_term_lufs"] == pytest.approx(listed, abs=0.1)
    # A file has no stems, so nothing hits and no device is reported.
    assert report["effects"] is None and report["tracks"] == {}

    # The meter's own gated loudness is the one pyloudnorm gives.
    (whole, quiet) = aaw_py.loudness(x, SR, [(0, len(x)), (20 * SR, 40 * SR)])
    assert whole["integrated_lufs"] == pytest.approx(integrated(x, SR), abs=0.1)
    assert quiet["integrated_lufs"] == pytest.approx(-30, abs=0.1)
    assert quiet["loudness_range_lu"] == pytest.approx(0, abs=0.01)
    # One channel is measured as one: 3 dB under the same sound on two.
    (one,) = aaw_py.loudness(np.ascontiguousarray(x[:, :1]), SR, [(0, len(x))])
    assert one["max_short_term_lufs"] == pytest.approx(whole["max_short_term_lufs"] - 3.01, abs=0.01)
    # Under three seconds there is no short-term loudness, and under 400 ms none at all.
    brief = dynamics.loudness(x, SR, [(0, 2 * SR), (0, SR // 4), (0, 0)])
    assert brief[0]["loudness_range_lu"] is None and brief[0]["max_short_term_lufs"] is None
    assert brief[0]["max_momentary_lufs"] == pytest.approx(-20, abs=0.1)
    assert set(brief[1].values()) == {None} and set(brief[2].values()) == {None}
    silent = dynamics.loudness(np.zeros((5 * SR, 2)), SR, [(0, 5 * SR)])
    assert set(silent[0].values()) == {None}
    with pytest.raises(ValueError, match="No frames"):
        aaw_py.loudness(x, SR, [(0, len(x) + 1)])
    with pytest.raises(ValueError, match="C-ordered"):
        aaw_py.loudness(x[::2], SR, [(0, 4)])

    # daw compare has the change: a gain moves the maxima and leaves the range.
    sf.write(tmp_path / "louder.wav", x * 2, SR, subtype="DOUBLE")
    change = compare(tmp_path / "steps.wav", tmp_path / "louder.wav", images=False)["mix"]
    assert change["actual_delta"]["max_short_term_lufs"] == pytest.approx(6.02, abs=0.02)
    assert change["actual_delta"]["loudness_range_lu"] == pytest.approx(0, abs=0.02)
    assert change["loudness_matched_delta"]["max_momentary_lufs"] == pytest.approx(0, abs=0.02)
    assert "loudness_range_lu" not in change["loudness_matched_delta"]


def test_a_compressor_s_attack_decides_how_hard_a_kick_hits(tmp_path):
    fast = song(tmp_path / "fast", {"kick": kick()}, [kicks([squeeze(0.1)])], FOUR, sections=HALVES)
    slow = song(tmp_path / "slow", {"kick": kick()}, [kicks([squeeze(30)])], FOUR, sections=HALVES)
    plain = song(tmp_path / "plain", {"kick": kick()}, [kicks()], FOUR, sections=HALVES)
    read = {name: listen(folder, images=False)["tracks"]["kick"]["hits"] for name, folder in (("fast", fast), ("slow", slow), ("plain", plain))}
    assert [hits["count"] for hits in read.values()] == [BEATS] * 3
    # A fast attack takes the front of the kick off, and a slow one lets it through
    # and takes the rest down.
    assert read["fast"]["punch_db"] < read["plain"]["punch_db"] - 2
    assert read["slow"]["punch_db"] > read["plain"]["punch_db"] + 2
    # Alone in the mix, the kick hits there as it does at its stem.
    assert read["plain"]["punch_in_mix_db"] == pytest.approx(read["plain"]["punch_db"], abs=0.11)
    assert "punch_in_group_db" not in read["plain"] and "pads" not in read["plain"]
    assert read["plain"]["sections"] == {
        half: {"count": BEATS // 2, "punch_db": read["plain"]["punch_db"], "punch_in_mix_db": read["plain"]["punch_in_mix_db"]}
        for half in ("first", "second")
    }
    # daw compare reports the difference.
    change = compare(fast, slow, images=False)["tracks"]["kick"]["hits"]
    assert change["punch_db"]["before"] == read["fast"]["punch_db"]
    assert change["punch_db"]["delta"] == pytest.approx(read["slow"]["punch_db"] - read["fast"]["punch_db"], abs=0.11)
    assert change["punch_db"]["delta"] > 4 and change["count"] == {"before": BEATS, "after": BEATS, "delta": 0}
    assert change["sections"]["second"]["punch_in_mix_db"]["delta"] > 4
    # One section's hits, with --section.
    code, section = cli("listen", plain, "--section", "second", "--no-images")
    assert code == 0, section
    assert section["tracks"]["kick"]["hits"] == read["plain"]["sections"]["second"]
    assert section["effects"] == {"master": [], "tracks": {}}
    assert set(dynamics.FIGURES) <= set(section["mix"])


def test_a_kick_under_a_held_tone_hits_at_its_stem_and_not_in_the_mix(tmp_path):
    samples = {"kick": kick(), "held": tone(300, BEATS / 2, 0.3)}
    held = {"id": "held", "pads": {}, "audio": [{"sample": "held", "at": 0}]}
    alone = song(tmp_path / "alone", samples, [kicks()], FOUR)
    under = song(tmp_path / "under", samples, [kicks(), held], FOUR)
    a = listen(alone, images=False)["tracks"]["kick"]["hits"]
    report = listen(under, images=False)
    b = report["tracks"]["kick"]["hits"]
    assert b["punch_db"] == a["punch_db"]
    assert b["punch_in_mix_db"] < a["punch_in_mix_db"] - 3
    # The song says nothing of what an audio clip holds, so the tone has no hits.
    assert report["tracks"]["held"]["hits"] is None
    assert a["sections"] == {}
    change = compare(alone, under, images=False)["tracks"]["kick"]["hits"]
    assert change["punch_db"]["delta"] == 0 and change["punch_in_mix_db"]["delta"] < -3
    # daw describe listen says what each field means.
    said = " ".join(daw("describe", "listen")["semantics"].values())
    fields = (
        "loudness_range_lu",
        "max_short_term_lufs",
        "max_momentary_lufs",
        "punch_db",
        "punch_in_mix_db",
        "punch_in_group_db",
        "fraction_over_1db_reduction",
        "keyed_by",
    )
    assert all(field in said for field in fields)


def test_a_kit_s_drums_are_read_apart_and_through_their_group(tmp_path):
    # A kick on one and three, hats on the eighths, and a second hat 5 ms
    # after the one on two: a flam, which is one hit.
    beat = {"length_beats": 4, "events": [*every("kick", 2), *every("hat", 0.5), {"at": 1.01, "pad": "hat"}]}
    kit = {"id": "kit", "pads": pads("kick", "hat"), "clips": [{"pattern": "beat", "repeats": BEATS // 4}], "group": "drums"}
    plate = {"id": "plate", "effects": [{"type": "reverb", "decay_seconds": 0.5}]}
    folder = song(
        tmp_path,
        {"kick": kick(), "hat": hat()},
        [kit | {"sends": [{"to": "plate", "gain_db": -12}]}],
        {"beat": beat},
        groups=[{"id": "drums", "effects": [squeeze(0.1)]}],
        returns=[plate],
    )
    report = listen(folder, images=False)
    hits = report["tracks"]["kit"]["hits"]
    assert hits["count"] == BEATS * 2
    assert {pad: one["count"] for pad, one in hits["pads"].items()} == {"hat": BEATS * 2, "kick": BEATS // 2}
    # A hat alone is all attack; the kick has a body, and its hits are read with the hat on them.
    assert hits["punch_db"] == 60 and 5 < hits["pads"]["kick"]["punch_db"] < 25
    # The group's compressor takes the front off, which the track's own stem does not show.
    assert hits["pads"]["kick"]["punch_in_group_db"] < hits["pads"]["kick"]["punch_db"] - 2
    assert set(hits["pads"]["kick"]) == {"count", "punch_db", "punch_in_group_db", "punch_in_mix_db"}
    # A group and a return schedule nothing themselves.
    assert report["tracks"]["drums"]["hits"] is None and report["tracks"]["plate"]["hits"] is None
    # The group's compressor is the one device, at its place in the song.
    (device,) = report["effects"]["tracks"]["drums"]
    assert device["path"] == "groups.drums.effects.0" and device["type"] == "compressor"
    assert device["max_gain_reduction_db"] > 6 and "sections" not in device
    assert report["effects"]["master"] == [] and set(report["effects"]["tracks"]) == {"drums"}


def test_a_synth_s_chords_are_hits_and_its_patch_s_limiter_is_under_its_track(tmp_path):
    chord = [{"pitch": pitch, "at": at, "duration": 1} for at in range(0, 8, 2) for pitch in ("A3", "C4", "E4")]
    lead = {
        "id": "lead",
        "type": "midi",
        "gain_db": -12,
        "instrument": {"synth": {"oscillators": {"a": {}}, "effects": [{"type": "limiter", "id": "hold", "ceiling_db": -20}]}},
        "clips": [{"id": "c", "length_beats": 8, "notes": chord}],
        "effects": [{"type": "filter", "mode": "lowpass", "cutoff_hz": 8000}],
    }
    folder = song(tmp_path, {}, [lead], {})
    report = listen(folder, images=False)
    assert report["tracks"]["lead"]["hits"]["count"] == 4
    (device,) = report["effects"]["tracks"]["lead"]
    assert device["path"] == "tracks.lead.instrument.synth.effects.hold" and device["id"] == "hold"
    assert device["max_gain_reduction_db"] > 3


def loud_in_the_second_half(directory, ceiling=-6.0, more=()):
    """A tone under the ceiling, and over it from beat 18 to 30."""
    quiet, loud = tone(220, 9, 0.25), tone(220, 6, 0.9)
    x = np.concatenate([quiet, loud, tone(220, 1, 0.25)])
    track = {"id": "line", "pads": pads("line"), "clips": [{"pattern": "once"}], "effects": list(more)}
    key = {"id": "kick", "pads": pads("kick"), "clips": [{"pattern": "four", "repeats": BEATS // 4}], "gain_db": -30}
    return song(
        directory,
        {"line": x, "kick": kick()},
        [track, key],
        {"once": {"length_beats": BEATS, "events": [{"at": 0, "pad": "line", "velocity": 127}]}} | FOUR,
        sections=HALVES,
        master={"effects": [{"type": "filter", "mode": "highpass", "cutoff_hz": 20}, {"type": "limiter", "id": "lim", "ceiling_db": ceiling}]},
    )


def test_a_limiter_reports_its_reduction_in_the_section_that_is_loud(tmp_path):
    folder = loud_in_the_second_half(tmp_path / "six")
    written = json.loads((folder / "report.json").read_text())
    highpass, limiter = written["master_effects"]
    assert "id" not in highpass and "sections" not in highpass
    assert limiter["id"] == "lim" and set(limiter["sections"]) == {"first", "second"}
    # 0.9 is 5.1 dB over a ceiling of -6, and the kick under it a little more,
    # for six of the second half's eight seconds.
    assert limiter["sections"]["first"] == {
        "max_gain_reduction_db": 0,
        "mean_gain_reduction_db": 0,
        "fraction_over_1db_reduction": 0,
    }
    second = limiter["sections"]["second"]
    assert 5.1 < second["max_gain_reduction_db"] < 6
    assert second["max_gain_reduction_db"] == limiter["max_gain_reduction_db"]
    assert second["fraction_over_1db_reduction"] == pytest.approx(0.76, abs=0.02)  # and the release
    assert second["mean_gain_reduction_db"] == pytest.approx(2 * limiter["mean_gain_reduction_db"], abs=0.01)

    # A section's render is cut from the song's, so it reports the song's reduction.
    preview = render(tmp_path / "six" / "song.yaml", tmp_path / "preview", section="first")
    assert preview["master_effects"][1]["sections"] == limiter["sections"]

    # daw listen holds the levels and what the devices did to them in one report.
    report = listen(folder, images=False)
    (device,) = report["effects"]["master"]
    assert device == {
        "path": "master.effects.lim",
        "type": "limiter",
        "id": "lim",
        **{key: pytest.approx(limiter[key], abs=0.006) for key in dynamics.REDUCTION},
        "sections": {
            half: {key: pytest.approx(limiter["sections"][half][key], abs=0.006) for key in dynamics.REDUCTION}
            for half in ("first", "second")
        },
    }
    assert report["effects"]["tracks"] == {}
    # The limited half is the louder one, and its section says so.
    halves = report["sections"]
    assert halves["second"]["audio"]["max_short_term_lufs"] > halves["first"]["audio"]["max_short_term_lufs"] + 4
    assert report["mix"]["max_short_term_lufs"] == halves["second"]["audio"]["max_short_term_lufs"]
    assert report["mix"]["loudness_range_lu"] > 4 and halves["first"]["audio"]["loudness_range_lu"] < 0.5
    code, section = cli("listen", folder, "--section", "second", "--no-images")
    assert code == 0, section
    assert section["effects"]["master"] == [{k: v for k, v in device.items() if k not in (*dynamics.REDUCTION, "sections")} | device["sections"]["second"]]

    # A ceiling 3 dB higher, a compressor keyed by the kick and a bypassed one before it.
    keyed = [squeeze(5) | {"bypass": True}, squeeze(5) | {"sidechain": "kick", "id": "duck"}]
    after = loud_in_the_second_half(tmp_path / "three", ceiling=-3.0, more=keyed)
    there = listen(after, images=False)["effects"]
    assert there["tracks"]["line"][0] == {"path": "tracks.line.effects.0", "type": "compressor", "bypass": True}
    duck = there["tracks"]["line"][1]
    assert duck["path"] == "tracks.line.effects.duck" and duck["keyed_by"] == "kick"
    assert duck["sections"]["first"]["max_gain_reduction_db"] > 3
    change = compare(folder, after, images=False)["effects"]
    (device,) = change["devices"]
    assert device["path"] == "master.effects.lim" and "path_before" not in device
    assert device["max_gain_reduction_db"]["before"] == report["effects"]["master"][0]["max_gain_reduction_db"]
    # The limiter has 3 dB less to do, and less again where the kick ducks the line.
    assert -5 < device["max_gain_reduction_db"]["delta"] < -3
    assert device["sections"]["second"]["fraction_over_1db_reduction"]["delta"] < 0
    assert device["sections"]["first"]["max_gain_reduction_db"] == {"before": 0, "after": 0, "delta": 0}
    assert change["added"] == ["tracks.line.effects.0", "tracks.line.effects.duck"] and change["removed"] == []
    back = compare(after, folder, images=False)["effects"]
    assert back["removed"] == change["added"] and back["added"] == []
