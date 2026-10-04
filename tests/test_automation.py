"""Automation lanes on channels, sends and effects, tested with generated audio.

An effect chain is run on its own by rendering a song that plays the test signal
once on a track with those effects and lanes; the track's stem is its output.
"""

import copy
import json
import numpy as np
import pytest
import soundfile as sf
from agent_daw.model import load, save, validate
from helpers import SR, cli, render, run_chain, stem, tone

TEMPO = 120  # one beat is 24000 frames


def rms_db(x):
    return 20 * np.log10(np.sqrt(np.mean(x**2)))


def lowpass(cutoff=1000, slope=24):
    return {
        "type": "filter",
        "mode": "lowpass",
        "cutoff_hz": cutoff,
        "slope_db_per_octave": slope,
    }


def eq(*bands):
    return {"type": "eq", "bands": list(bands)}


@pytest.mark.parametrize("slope", [12, 24, 36, 48])
def test_constant_filter_lane_matches_static_filter(tmp_path, slope):
    x = np.random.default_rng(1).normal(0, 0.3, (SR, 2))
    static, _ = run_chain(tmp_path / "static", [lowpass(700, slope)], x)
    lanes = {0: {"cutoff_hz": [(0, 700)]}}
    moved, _ = run_chain(tmp_path / "lane", [lowpass(5000, slope)], x, lanes)
    assert np.array_equal(moved, static)


def test_constant_eq_lane_matches_static_eq(tmp_path):
    bands = [
        {"shape": "bell", "freq_hz": 300, "gain_db": -6, "q": 2},
        {"shape": "high_shelf", "freq_hz": 6000, "gain_db": 4},
    ]
    x = np.random.default_rng(2).normal(0, 0.1, (SR, 2))
    static, _ = run_chain(tmp_path / "static", [eq(*bands)], x)
    moved = eq(dict(bands[0], gain_db=0), bands[1])
    lanes = {0: {"bands.0.gain_db": [(0, -6)]}}
    assert np.array_equal(run_chain(tmp_path / "lane", [moved], x, lanes)[0], static)


def test_constant_lanes_on_other_effects_match_static_values(tmp_path):
    specs = [
        {"type": "compressor", "threshold_db": -20, "makeup_db": 3},
        {
            "type": "delay",
            "time_beats": "1/4",
            "feedback_percent": 40,
            "mix_percent": 30,
        },
        {"type": "reverb", "decay_seconds": 0.5, "mix_percent": 25},
    ]
    moved = [
        {"type": "compressor", "threshold_db": 0},
        {"type": "delay", "time_beats": "1/4"},
        {"type": "reverb", "decay_seconds": 0.5},
    ]
    lanes = {
        0: {"threshold_db": [(0, -20)], "makeup_db": [(1, 3), (2, 3)]},
        1: {"feedback_percent": [(0, 40)], "mix_percent": [(0, 30)]},
        2: {"mix_percent": [(2, 25)]},
    }
    x = np.random.default_rng(4).normal(0, 0.3, (SR, 2))
    static, _ = run_chain(tmp_path / "static", specs, x)
    assert np.array_equal(run_chain(tmp_path / "lanes", moved, x, lanes)[0], static)


def test_filter_sweep_opens_over_time(tmp_path):
    # Lowpass sweeps 200 Hz -> 10 kHz over beats 0-4 on a 4 kHz tone.
    x = tone(4000, 2)
    lanes = {0: {"cutoff_hz": [(0, 200), (4, 10000)]}}
    y, _ = run_chain(tmp_path / "a", [lowpass()], x, lanes)
    quarters = [rms_db(y[i * SR // 2 : (i + 1) * SR // 2]) for i in range(4)]
    assert quarters == sorted(quarters)
    assert quarters[0] < -60 and quarters[-1] > rms_db(x) - 3


def test_eq_band_gain_ramp(tmp_path):
    band = eq({"shape": "bell", "freq_hz": 1000, "gain_db": 0})
    x = tone(1000, 2, 0.1)
    lanes = {0: {"bands.0.gain_db": [(0, -12), (4, 12)]}}
    y, _ = run_chain(tmp_path / "a", [band], x, lanes)
    start, end = rms_db(y[2000:6000]), rms_db(y[-6000:-2000])
    assert start == pytest.approx(rms_db(x) - 11.5, abs=1)
    assert end == pytest.approx(rms_db(x) + 11.5, abs=1)


@pytest.mark.parametrize("mode", ["lowpass", "highpass"])
@pytest.mark.parametrize("before, after", [(20000, 10), (10, 20000), (18000, 300)])
def test_filter_cutoff_jump_does_not_burst(tmp_path, mode, before, after):
    # Direct-form state carried into very different coefficients reaches 6 to 220
    # times the input peak here. The state-variable state stays below 3 times; the
    # worst case, a highpass dropping to 10 Hz, is a decaying low thump.
    x = np.random.default_rng(4).normal(0, 0.05, (SR, 2))
    f = dict(lowpass(1000, 48), mode=mode)
    jump = [(0, before), (1, before), (1, after)]
    y, _ = run_chain(tmp_path / "a", [f], x, {0: {"cutoff_hz": jump}})
    assert np.max(np.abs(y)) < 4 * np.max(np.abs(x))


def test_eq_frequency_and_q_jumps_do_not_burst(tmp_path):
    band = eq({"shape": "bell", "freq_hz": 20, "gain_db": -12})
    x = np.random.default_rng(5).normal(0, 0.05, (SR, 2))
    lanes = {
        0: {
            "bands.0.freq_hz": [(0, 20000), (1, 20000), (1, 20)],
            "bands.0.q": [(0, 18), (1, 0.1, "hold"), (1.5, 18)],
        }
    }
    y, _ = run_chain(tmp_path / "a", [band], x, lanes)
    assert np.max(np.abs(y)) < 4 * np.max(np.abs(x))


def test_filter_updates_align_to_the_timeline_after_latency(tmp_path):
    # The limiter delays the filter's input by 144 frames; coefficient periods
    # still start on timeline multiples of 64, so a step on beat 1 lands there.
    specs = [{"type": "limiter", "ceiling_db": -0.1, "lookahead_ms": 3}, lowpass(20000, 12)]
    x = tone(5000, 1, 0.2)
    step = {1: {"cutoff_hz": [(0, 20000, "hold"), (1, 100)]}}
    stepped, _ = run_chain(tmp_path / "stepped", specs, x, step)
    # The same automated path, stepping only after the tone ends.
    later = {1: {"cutoff_hz": [(0, 20000, "hold"), (2, 100)]}}
    steady, _ = run_chain(tmp_path / "steady", specs, x, later)
    assert np.flatnonzero(np.any(stepped != steady, axis=1))[0] == 24000


def all_automated_chain():
    specs = [
        lowpass(slope=24),
        eq({"shape": "bell", "freq_hz": 500, "gain_db": 0}),
        {"type": "limiter", "ceiling_db": -3, "lookahead_ms": 2},
        {"type": "compressor", "threshold_db": -18, "sidechain": "kick"},
        {
            "type": "delay",
            "time_beats": "3/4",
            "feedback_percent": 30,
            "ping_pong": True,
        },
        {"type": "reverb", "decay_seconds": 0.5, "mix_percent": 30},
    ]
    automation = {
        0: {"cutoff_hz": [(0, 300), (3, 12000)]},
        1: {
            "bands.0.freq_hz": [(0, 200), (4, 4000)],
            "bands.0.gain_db": [(0, -6), (2, 6, "hold"), (4, 0)],
        },
        3: {
            "threshold_db": [(0, -30), (4, -6)],
            "makeup_db": [(1, 0), (1, 3)],
        },
        4: {
            "feedback_percent": [(0, 0), (4, 80)],
            "mix_percent": [(0, 100), (4, 20)],
        },
        5: {"mix_percent": [(0, 0), (4, 100)]},
    }
    return specs, automation


def test_automated_chain_is_block_partition_invariant(tmp_path):
    specs, automation = all_automated_chain()
    rng = np.random.default_rng(3)
    x = rng.normal(0, 0.3, (3 * SR, 2))
    keys = {"kick": rng.normal(0, 0.5, (3 * SR, 2))}
    a, b, c = (
        run_chain(tmp_path / str(size), specs, x, automation, keys, block_size=size)[0]
        for size in (127, 4096, 6 * SR)
    )
    assert np.array_equal(a, b) and np.array_equal(b, c)


def test_automation_after_latency_lands_on_the_timeline(tmp_path):
    # The limiter delays everything after it; the chain compensates, so a makeup
    # jump at beat 1 must change the output exactly at frame 24000.
    specs = [
        {"type": "limiter", "ceiling_db": -0.1, "lookahead_ms": 5},
        {"type": "compressor", "threshold_db": 0, "knee_db": 0},
    ]
    x = np.full((SR, 2), 0.25)
    lanes = {1: {"makeup_db": [(1, 0), (1, -20)]}}
    y, _ = run_chain(tmp_path / "a", specs, x, lanes)
    assert np.allclose(y[:24000], 0.25) and np.allclose(y[24000:], 0.025)


def test_reverb_mix_after_latency_lands_on_the_timeline(tmp_path):
    # After the limiter's delay a jump from dry to wet on beat 1 still starts
    # at frame 24000.
    specs = [
        {"type": "limiter", "ceiling_db": -0.1, "lookahead_ms": 3},
        {"type": "reverb", "decay_seconds": 0.5},
    ]
    x = np.random.default_rng(5).normal(0, 0.03, (SR, 2))
    lanes = {1: {"mix_percent": [(1, 0), (1, 100)]}}
    y, _ = run_chain(tmp_path / "a", specs, x, lanes)
    assert np.flatnonzero(np.any(y != x.astype(np.float32), axis=1))[0] == 24000


def test_chain_report_lists_automated_fields(tmp_path):
    specs, automation = all_automated_chain()
    quiet = np.zeros((2 * SR, 2))
    _, report = run_chain(tmp_path / "all", specs, quiet, automation, {"kick": quiet})
    assert report[1]["automated"] == ["bands.0.freq_hz", "bands.0.gain_db"]
    assert "automated" not in report[2]
    # A constant lane renders as the static value but is still listed.
    lanes = {0: {"cutoff_hz": [(0, 700)]}}
    _, report = run_chain(tmp_path / "constant", [lowpass()], quiet, lanes)
    assert report[0]["automated"] == ["cutoff_hz"]


@pytest.fixture
def song(tmp_path):
    sf.write(tmp_path / "hum.wav", tone(220, 5, 0.25)[:, 0], SR, subtype="FLOAT")
    data = {
        "session": {"tempo": TEMPO, "length_beats": 8, "master_gain_db": 0},
        "samples": {"hum": {"path": "hum.wav"}},
        "patterns": {"hold": {"length_beats": 8, "steps": {"h": "x" + "." * 31}}},
        "tracks": [
            {
                "id": "pad",
                "pads": {"h": {"sample": "hum"}},
                "clips": [{"pattern": "hold"}],
                "effects": [
                    {
                        "type": "filter",
                        "id": "sweep",
                        "mode": "lowpass",
                        "cutoff_hz": 8000,
                    }
                ],
                "sends": [{"to": "space", "gain_db": -6}],
                "automation": [
                    {
                        "param": "gain_db",
                        "points": [{"at": 0, "value": -24}, {"at": 4, "value": 0}],
                    }
                ],
            }
        ],
        "returns": [
            {
                "id": "space",
                "effects": [{"type": "delay", "time_beats": "1/2", "id": "echo"}],
            }
        ],
        "sections": [{"id": "b", "at": 4, "length_beats": 4}],
    }
    path = tmp_path / "song.yaml"
    save(data, path)
    return path, data


def rerender(path, data, directory, **kw):
    save(data, path)
    return render(path, directory, **kw)


def test_track_gain_lane_ramps_the_stem(song, tmp_path):
    path, data = song
    report = render(path, tmp_path / "a")
    y = stem(tmp_path / "a", "pad")
    source = tone(220, 5, 0.25)
    level = np.sqrt(0.5) * 100 / 127  # centre-panned mono pad at step velocity
    for beat_, db in [(1, -18), (2, -12), (3, -6), (5, 0)]:
        i = beat_ * 24000
        window = slice(i - 1200, i + 1200)  # 11 whole cycles of 220 Hz
        ratio = rms_db(y[window]) - rms_db(source[window] * level)
        assert ratio == pytest.approx(db, abs=0.1)
    assert report["tracks"]["pad"]["automation"] == ["gain_db"]
    assert report["master_automation"] == []
    assert report["stems_sum_to_mix"]
    mix = sf.read(tmp_path / "a" / "mix.wav", always_2d=True)[0]
    assert np.max(np.abs(y + stem(tmp_path / "a", "space") - mix)) < 1e-6


@pytest.mark.parametrize("shape, levels", [(0.5, (-22.5, -18, -10.5)), (-0.5, (-13.5, -6, -1.5)), (1, (-23.9, -22.5, -16.4))])
def test_a_shape_bends_the_gain_ramp(song, tmp_path, shape, levels):
    # The lane rises 24 dB over four beats. Bent, it is late or early by the
    # beat: progress to the power two at 0.5, four at 1, and the mirror below zero.
    path, data = song
    data["tracks"][0]["automation"][0]["points"][0]["shape"] = shape
    rerender(path, data, tmp_path / "a")
    y = stem(tmp_path / "a", "pad")
    source = tone(220, 5, 0.25)
    level = np.sqrt(0.5) * 100 / 127
    for beat_, db in zip((1, 2, 3), levels):
        i = beat_ * 24000
        window = slice(i - 600, i + 600)
        assert rms_db(y[window]) - rms_db(source[window] * level) == pytest.approx(db, abs=0.25)
    # A straight shape is the lane as it was, to the sample.
    data["tracks"][0]["automation"][0]["points"][0]["shape"] = 0
    rerender(path, data, tmp_path / "straight")
    del data["tracks"][0]["automation"][0]["points"][0]["shape"]
    rerender(path, data, tmp_path / "plain")
    assert np.array_equal(stem(tmp_path / "straight", "pad"), stem(tmp_path / "plain", "pad"))


def test_constant_gain_lane_matches_static_gain(song, tmp_path):
    path, data = song
    data["tracks"][0]["automation"] = [
        {"param": "gain_db", "points": [{"at": 2, "value": -7}]}
    ]
    rerender(path, data, tmp_path / "lane")
    del data["tracks"][0]["automation"]
    data["tracks"][0]["gain_db"] = -7
    rerender(path, data, tmp_path / "static")
    lane, static = (
        json.loads((tmp_path / d / "report.json").read_text())
        for d in ["lane", "static"]
    )
    assert lane["audio_sha256"] == static["audio_sha256"]
    for name in ["pad", "space"]:
        tracks = lane["tracks"][name], static["tracks"][name]
        assert tracks[0]["audio_sha256"] == tracks[1]["audio_sha256"]


def test_send_lane_silences_the_return(song, tmp_path):
    path, data = song
    data["returns"][0]["effects"][0]["feedback_percent"] = 0
    data["tracks"][0]["automation"] = [
        {
            "param": "sends.space.gain_db",
            "points": [{"at": 0, "value": 0, "curve": "hold"}, {"at": 4, "value": -96}],
        }
    ]
    rerender(path, data, tmp_path / "a")
    wet = stem(tmp_path / "a", "space")
    # The single echo lands half a beat after the send closes at beat 4.
    assert rms_db(wet[3 * 24000 : 4 * 24000]) > -30
    assert rms_db(wet[5 * 24000 :]) < -100


def test_pan_and_return_lanes(song, tmp_path):
    path, data = song
    data["tracks"][0]["automation"] = [
        {
            "param": "pan",
            "points": [{"at": 0, "value": -1, "curve": "hold"}, {"at": 4, "value": 1}],
        }
    ]
    data["returns"][0]["automation"] = [
        {"param": "effects.echo.mix_percent", "points": [{"at": 0, "value": 0}]},
    ]
    report = rerender(path, data, tmp_path / "a")
    y = stem(tmp_path / "a", "pad")
    assert not y[:96000, 1].any() and not y[96000:, 0].any()
    assert y[:96000, 0].any() and y[96000:, 1].any()
    assert report["tracks"]["space"]["effects"][0]["automated"] == ["mix_percent"]
    assert report["tracks"]["space"]["automation"] == ["effects.echo.mix_percent"]


def test_master_gain_lane_scales_mix_and_stems(song, tmp_path):
    path, data = song
    rerender(path, data, tmp_path / "flat")
    data["master"] = {
        "automation": [
            {
                "param": "gain_db",
                "points": [{"at": 4, "value": 0}, {"at": 4, "value": -6}],
            }
        ]
    }
    report = rerender(path, data, tmp_path / "ducked")
    for name in ["pad", "space"]:
        flat, ducked = stem(tmp_path / "flat", name), stem(tmp_path / "ducked", name)
        assert np.allclose(ducked[:96000], flat[:96000])
        assert np.allclose(ducked[96000:], flat[96000:] * 10 ** (-6 / 20))
    assert report["master_automation"] == ["gain_db"]
    assert report["stems_sum_to_mix"]


def test_master_effect_lane_with_limiter(song, tmp_path):
    path, data = song
    data["master"] = {
        "effects": [
            {"type": "limiter", "ceiling_db": -1},
            {
                "type": "eq",
                "id": "tilt",
                "bands": [{"shape": "high_shelf", "freq_hz": 3000, "gain_db": 0}],
            },
        ],
        "automation": [
            {
                "param": "effects.tilt.bands.0.gain_db",
                "points": [{"at": 0, "value": -6}, {"at": 8, "value": 6}],
            }
        ],
    }
    report = rerender(path, data, tmp_path / "a")
    assert report["master_effects"][1]["automated"] == ["bands.0.gain_db"]


def test_section_preview_matches_full_render(song, tmp_path):
    path, data = song
    data["tracks"][0]["automation"].append(
        {
            "param": "effects.sweep.cutoff_hz",
            "points": [{"at": 0, "value": 100}, {"at": 8, "value": 4000}],
        }
    )
    rerender(path, data, tmp_path / "full")
    render(path, tmp_path / "part", section="b")
    full, part = stem(tmp_path / "full", "pad"), stem(tmp_path / "part", "pad")
    assert np.array_equal(full[96000:], part)


def test_render_with_automation_is_block_size_invariant(song, tmp_path):
    path, data = song
    data["tracks"][0]["automation"].append(
        {
            "param": "effects.0.cutoff_hz",
            "points": [{"at": 0, "value": 100}, {"at": 8, "value": 4000}],
        }
    )
    save(data, path)
    a = render(path, tmp_path / "a", 127)
    b = render(path, tmp_path / "b", 4096)
    assert a["audio_sha256"] == b["audio_sha256"]
    for name in ["pad", "space"]:
        assert a["tracks"][name]["audio_sha256"] == b["tracks"][name]["audio_sha256"]


def points(*pairs):
    return [{"at": a, "value": v} for a, v in pairs]


def automate(where, param, pts):
    def change(d):
        owner = d.setdefault("master", {}) if where == "master" else d[where][0]
        owner.setdefault("automation", []).append({"param": param, "points": pts})

    return change


@pytest.mark.parametrize(
    "change, message",
    [
        (automate("tracks", "volume", points((0, 0))), "unknown automation target"),
        (
            automate("tracks", "effects.sweep.mode", points((0, 0))),
            "cannot be automated",
        ),
        (automate("tracks", "effects.2.cutoff_hz", points((0, 100))), "no effect"),
        (automate("tracks", "effects.nope.cutoff_hz", points((0, 100))), "no effect"),
        (automate("tracks", "sends.hall.gain_db", points((0, 0))), "no send to hall"),
        (
            automate("tracks", "sends.space.pre_fader", points((0, 0))),
            "only a send's gain_db",
        ),
        (automate("tracks", "pan", points((0, 2))), r"outside -1 to 1"),
        (
            automate("tracks", "effects.sweep.cutoff_hz", points((0, 5))),
            "outside 10 to 20000",
        ),
        (automate("tracks", "pan", points((9, 0))), "after the session end"),
        (automate("tracks", "pan", points((2, 0), (1, 0))), "time order"),
        (automate("tracks", "pan", points((1, 0), (1, 1), (1, 0))), "at most two"),
        (automate("tracks", "gain_db", points((0, 0))), "more than one lane"),
        (
            automate("returns", "effects.echo.time_beats", points((0, 1))),
            "cannot be automated",
        ),
        (
            automate("returns", "sends.space.gain_db", points((0, 0))),
            "unknown automation",
        ),
        (automate("master", "pan", points((0, 0))), "unknown automation target"),
        (automate("master", "gain_db", points((0, 30))), "outside -96 to 24"),
        (automate("tracks", "gain_db", []), "at least 1"),
        (automate("master", "gain_db", [{"at": 0, "value": 0, "shape": 1.5}]), "less than or equal to 1"),
    ],
)
def test_automation_validation(song, change, message):
    path, data = song
    change(data)
    with pytest.raises(ValueError, match=message):
        validate(data)


def accepted(data):
    try:
        validate(data)
        return True
    except ValueError:
        return False


def test_lane_limits_match_the_static_field(song):
    # A lane's values have the limits of the field it replaces: at each limit of
    # the schema, and one step either side, a lane point and the static field
    # are accepted or refused together.
    _, data = song
    code, described = cli("describe", "automation")
    models = cli("describe", "project", "--schema")[1]["schema"]["$defs"]
    track = data["tracks"][0]
    del track["automation"]
    track["effects"] = [
        {"type": "filter", "mode": "lowpass", "cutoff_hz": 8000},
        {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 500, "gain_db": 0}]},
        {"type": "compressor", "threshold_db": -20},
        {"type": "delay", "time_beats": "1/2"},
        {"type": "reverb"},
    ]
    assert accepted(data)
    # (schema model, field, the object holding the static field, lane owner, param)
    cases = [
        ("Track", "gain_db", lambda d: d["tracks"][0], "tracks", "gain_db"),
        ("Track", "pan", lambda d: d["tracks"][0], "tracks", "pan"),
        ("Return", "gain_db", lambda d: d["returns"][0], "returns", "gain_db"),
        ("Return", "pan", lambda d: d["returns"][0], "returns", "pan"),
        ("Session", "master_gain_db", lambda d: d["session"], "master", "gain_db"),
        (
            "Send",
            "gain_db",
            lambda d: d["tracks"][0]["sends"][0],
            "tracks",
            "sends.space.gain_db",
        ),
    ]
    for index, effect in enumerate(track["effects"]):
        kind = effect["type"]
        for field in described["automatable"]["effects"][kind]:
            if kind == "eq":
                holder = lambda d, i=index: d["tracks"][0]["effects"][i]["bands"][0]
                cases.append(("EqBand", field, holder, "tracks", f"effects.{index}.bands.0.{field}"))
            else:
                holder = lambda d, i=index: d["tracks"][0]["effects"][i]
                cases.append((kind.title(), field, holder, "tracks", f"effects.{index}.{field}"))
    assert len(cases) == 15
    for model, field, holder, owner, param in cases:
        limits = models[model]["properties"][field]
        for edge in (limits["minimum"], limits["maximum"]):
            for value in (np.nextafter(edge, -np.inf), edge, np.nextafter(edge, np.inf)):
                static = copy.deepcopy(data)
                holder(static)[field] = float(value)
                lane = copy.deepcopy(data)
                automate(owner, param, points((0, float(value))))(lane)
                assert accepted(static) == accepted(lane), (param, value)
                assert accepted(lane) == (limits["minimum"] <= value <= limits["maximum"])


def test_exclusive_limits_are_named_in_errors(song):
    _, data = song
    automate("tracks", "pan", points((0, 2)))(data)
    with pytest.raises(ValueError, match="pan value 2.0 outside -1 to 1"):
        validate(data)


def test_lanes_by_id_and_index_collide(song):
    path, data = song
    automate("tracks", "effects.sweep.cutoff_hz", points((0, 100)))(data)
    automate("tracks", "effects.0.cutoff_hz", points((0, 200)))(data)
    with pytest.raises(ValueError, match="more than one lane"):
        validate(data)


def test_effect_ids_are_unique_per_chain(song):
    path, data = song
    data["tracks"][0]["effects"].append(
        {"type": "filter", "id": "sweep", "mode": "highpass", "cutoff_hz": 30}
    )
    with pytest.raises(ValueError, match="effect IDs must be unique"):
        validate(data)


def test_eq_band_addressing(song):
    path, data = song
    data["tracks"][0]["effects"].append(
        {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 500, "gain_db": 0}]}
    )
    automate("tracks", "effects.1.bands.0.q", points((0, 2)))(data)
    validate(data)
    data["tracks"][0]["automation"][-1]["param"] = "effects.1.bands.1.q"
    with pytest.raises(ValueError, match="eq has 1 bands"):
        validate(data)
    data["tracks"][0]["automation"][-1]["param"] = "effects.1.q"
    with pytest.raises(ValueError, match="effects.REF.bands.N.FIELD"):
        validate(data)


def ok(*args):
    code, result = cli(*args)
    assert code == 0, result
    return result


def test_automation_round_trip_and_cli(song):
    path, data = song
    data["returns"][0]["effects"][0]["bypass"] = True
    data["returns"][0]["automation"] = [
        {
            "param": "effects.echo.feedback_percent",
            "points": points((0, 10), ("15/2", 50)),
        }
    ]
    save(data, path)
    text = path.read_text()
    save(load(path), path)
    assert path.read_text() == text
    assert "- {at: 15/2, value: 50.0}" in text
    assert "- {type: filter, id: sweep, mode: lowpass, cutoff_hz: 8000.0}" in text
    inspected = ok("inspect", path)
    assert inspected["tracks"][0]["automation"] == ["gain_db"]
    assert inspected["returns"][0]["automation"] == ["effects.echo.feedback_percent"]
    assert inspected["master_automation"] == []
    checked = ok("check", path)
    assert checked["warnings"] == [
        "space: effects.echo.feedback_percent automates a bypassed effect"
    ]
    described = ok("describe", "automation", "--schema")
    assert described["automatable"]["effects"]["filter"] == {"cutoff_hz": "log"}
    assert "lane" in described["schema"]
    assert "curves" in described["semantics"]
