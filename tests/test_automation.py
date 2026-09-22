"""Automation lanes on channels, sends and effects, tested with generated audio."""

import json
import subprocess
import sys
import numpy as np
import pytest
import soundfile as sf
from agent_daw.automation import Envelope
from agent_daw.effects import Chain
from agent_daw.engine import render
from agent_daw.model import (
    Compressor,
    Delay,
    Eq,
    Filter,
    Lane,
    Limiter,
    Project,
    Reverb,
    load,
    save,
)

SR = 48000
TEMPO = 120  # one beat is 24000 frames


def lane(param, *points):
    return Lane(
        param=param,
        points=[dict(zip(("at", "value", "curve"), p)) for p in points],
    )


def envelope(domain, *points):
    return Envelope(lane("x", *points), domain, TEMPO, SR)


def tone(freq, seconds, amp=0.5):
    x = amp * np.sin(2 * np.pi * freq * np.arange(round(seconds * SR)) / SR)
    return np.column_stack([x, x])


def rms_db(x):
    return 20 * np.log10(np.sqrt(np.mean(x**2)))


def test_envelope_holds_outside_points_and_interpolates_between():
    env = envelope("linear", (1, -12), (2, 0))
    v = env.at([-5, 0, 24000, 36000, 48000, 10**6])
    assert list(v) == [-12, -12, -12, -6, 0, 0]


def test_envelope_hold_curve_and_jump():
    env = envelope("linear", (0, 1, "hold"), (1, 2), (1, 5), (2, 7))
    v = env.at([0, 23999, 24000, 36000, 48000])
    assert list(v) == [1, 1, 5, 6, 7]


def test_log_domain_moves_in_equal_ratios():
    env = envelope("log", (0, 100), (2, 10000))
    assert env.at([24000])[0] == pytest.approx(1000)
    assert env.at([0, 48000]) == pytest.approx([100, 10000])


def test_single_point_lane_is_constant():
    assert list(envelope("linear", (3, 0.25)).at([0, 10**7])) == [0.25, 0.25]


def run(specs, x, automation, block_size=4096, keys=None):
    envs = {
        i: {name: envelope(domain, *points) for name, (domain, points) in a.items()}
        for i, a in automation.items()
    }
    return Chain(specs, SR, TEMPO, envs).run(x, keys, block_size)


def lowpass(cutoff=1000, slope=24):
    return Filter(
        type="filter", mode="lowpass", cutoff_hz=cutoff, slope_db_per_octave=slope
    )


@pytest.mark.parametrize("slope", [12, 24, 36, 48])
def test_constant_filter_lane_matches_static_filter(slope):
    x = np.random.default_rng(1).normal(0, 0.3, (SR, 2))
    static = Chain([lowpass(700, slope)], SR).run(x)
    automated = run([lowpass(5000, slope)], x, {0: {"cutoff_hz": ("log", [(0, 700)])}})
    assert np.max(np.abs(automated - static)) < 1e-9


def test_constant_eq_lane_matches_static_eq():
    bands = [
        {"shape": "bell", "freq_hz": 300, "gain_db": -6, "q": 2},
        {"shape": "high_shelf", "freq_hz": 6000, "gain_db": 4},
    ]
    x = np.random.default_rng(2).normal(0, 0.3, (SR, 2))
    static = Chain([Eq(type="eq", bands=bands)], SR).run(x)
    moved = [dict(bands[0], gain_db=0), bands[1]]
    automated = run(
        [Eq(type="eq", bands=moved)],
        x,
        {0: {"bands.0.gain_db": ("linear", [(0, -6)])}},
    )
    assert np.max(np.abs(automated - static)) < 1e-9


def test_filter_sweep_opens_over_time():
    # Lowpass sweeps 200 Hz -> 10 kHz over beats 0-4 on a 4 kHz tone.
    x = tone(4000, 2)
    y = run([lowpass()], x, {0: {"cutoff_hz": ("log", [(0, 200), (4, 10000)])}})
    quarters = [rms_db(y[i * SR // 2 : (i + 1) * SR // 2]) for i in range(4)]
    assert quarters == sorted(quarters)
    assert quarters[0] < -60 and quarters[-1] > rms_db(x) - 3


def test_eq_band_gain_ramp():
    eq = Eq(type="eq", bands=[{"shape": "bell", "freq_hz": 1000, "gain_db": 0}])
    x = tone(1000, 2)
    y = run([eq], x, {0: {"bands.0.gain_db": ("linear", [(0, -12), (4, 12)])}})
    start, end = rms_db(y[2000:6000]), rms_db(y[-6000:-2000])
    assert start == pytest.approx(rms_db(x) - 11.5, abs=1)
    assert end == pytest.approx(rms_db(x) + 11.5, abs=1)


def all_automated_chain():
    specs = [
        lowpass(slope=24),
        Eq(type="eq", bands=[{"shape": "bell", "freq_hz": 500, "gain_db": 0}]),
        Limiter(type="limiter", ceiling_db=-3, lookahead_ms=2),
        Compressor(type="compressor", threshold_db=-18, sidechain="kick"),
        Delay(type="delay", time_beats="3/4", feedback_percent=30, ping_pong=True),
        Reverb(type="reverb", decay_seconds=0.5, mix_percent=30),
    ]
    automation = {
        0: {"cutoff_hz": ("log", [(0, 300), (3, 12000)])},
        1: {
            "bands.0.freq_hz": ("log", [(0, 200), (4, 4000)]),
            "bands.0.gain_db": ("linear", [(0, -6), (2, 6, "hold"), (4, 0)]),
        },
        3: {
            "threshold_db": ("linear", [(0, -30), (4, -6)]),
            "makeup_db": ("linear", [(1, 0), (1, 3)]),
        },
        4: {
            "feedback_percent": ("linear", [(0, 0), (4, 80)]),
            "mix_percent": ("linear", [(0, 100), (4, 20)]),
        },
        5: {"mix_percent": ("linear", [(0, 0), (4, 100)])},
    }
    return specs, automation


def test_automated_chain_is_block_partition_invariant():
    specs, automation = all_automated_chain()
    rng = np.random.default_rng(3)
    x = rng.normal(0, 0.3, (3 * SR, 2))
    keys = {"kick": rng.normal(0, 0.5, (3 * SR, 2))}
    a = run(specs, x, automation, 127, keys)
    b = run(specs, x, automation, 4096, keys)
    c = run(specs, x, automation, 6 * SR, keys)
    assert np.array_equal(a, b) and np.array_equal(b, c)


def test_automation_after_latency_lands_on_the_timeline():
    # The limiter delays everything after it; the chain compensates, so a makeup
    # jump at beat 1 must change the output exactly at frame 24000.
    specs = [
        Limiter(type="limiter", ceiling_db=-0.1, lookahead_ms=5),
        Compressor(type="compressor", threshold_db=0, knee_db=0),
    ]
    x = np.full((SR, 2), 0.25)
    y = run(specs, x, {1: {"makeup_db": ("linear", [(1, 0), (1, -20)])}})
    assert np.allclose(y[:24000], 0.25) and np.allclose(y[24000:], 0.025)


def test_chain_report_lists_automated_fields():
    specs, automation = all_automated_chain()
    chain = Chain(
        specs,
        SR,
        TEMPO,
        {
            i: {k: envelope(d, *p) for k, (d, p) in a.items()}
            for i, a in automation.items()
        },
    )
    chain.run(np.zeros((1000, 2)), {"kick": np.zeros((1000, 2))})
    report = chain.report()
    assert report[1]["automated"] == ["bands.0.freq_hz", "bands.0.gain_db"]
    assert "automated" not in report[2]


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
    save(Project.model_validate(data), path)
    return path, data


def stem(directory, name):
    return sf.read(directory / "stems" / f"{name}.wav", always_2d=True)[0]


def rerender(path, data, directory, **kw):
    save(Project.model_validate(data), path)
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


def test_constant_gain_lane_matches_static_gain(song, tmp_path):
    path, data = song
    data["tracks"][0]["automation"] = [
        {"param": "gain_db", "points": [{"at": 2, "value": -7}]}
    ]
    rerender(path, data, tmp_path / "lane")
    del data["tracks"][0]["automation"]
    data["tracks"][0]["gain_db"] = -7
    rerender(path, data, tmp_path / "static")
    for name in ["pad", "space"]:
        diff = stem(tmp_path / "lane", name) - stem(tmp_path / "static", name)
        assert np.max(np.abs(diff)) < 1e-7


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
    save(Project.model_validate(data), path)
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
    ],
)
def test_automation_validation(song, change, message):
    path, data = song
    change(data)
    with pytest.raises(ValueError, match=message):
        Project.model_validate(data)


def test_lanes_by_id_and_index_collide(song):
    path, data = song
    automate("tracks", "effects.sweep.cutoff_hz", points((0, 100)))(data)
    automate("tracks", "effects.0.cutoff_hz", points((0, 200)))(data)
    with pytest.raises(ValueError, match="more than one lane"):
        Project.model_validate(data)


def test_effect_ids_are_unique_per_chain(song):
    path, data = song
    data["tracks"][0]["effects"].append(
        {"type": "filter", "id": "sweep", "mode": "highpass", "cutoff_hz": 30}
    )
    with pytest.raises(ValueError, match="effect IDs must be unique"):
        Project.model_validate(data)


def test_eq_band_addressing(song):
    path, data = song
    data["tracks"][0]["effects"].append(
        {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 500, "gain_db": 0}]}
    )
    automate("tracks", "effects.1.bands.0.q", points((0, 2)))(data)
    Project.model_validate(data)
    data["tracks"][0]["automation"][-1]["param"] = "effects.1.bands.1.q"
    with pytest.raises(ValueError, match="eq has 1 bands"):
        Project.model_validate(data)
    data["tracks"][0]["automation"][-1]["param"] = "effects.1.q"
    with pytest.raises(ValueError, match="effects.REF.bands.N.FIELD"):
        Project.model_validate(data)


def cli(*args):
    out = subprocess.run(
        [sys.executable, "-m", "agent_daw.cli", *map(str, args)],
        capture_output=True,
        text=True,
    )
    assert out.returncode == 0, out.stderr
    return json.loads(out.stdout)


def test_automation_round_trip_and_cli(song):
    path, data = song
    data["returns"][0]["effects"][0]["bypass"] = True
    data["returns"][0]["automation"] = [
        {
            "param": "effects.echo.feedback_percent",
            "points": points((0, 10), ("15/2", 50)),
        }
    ]
    save(Project.model_validate(data), path)
    text = path.read_text()
    save(load(path), path)
    assert path.read_text() == text
    assert "- {at: 15/2, value: 50.0}" in text
    assert "- {type: filter, id: sweep, mode: lowpass, cutoff_hz: 8000.0}" in text
    inspected = cli("inspect", path)
    assert inspected["tracks"][0]["automation"] == ["gain_db"]
    assert inspected["returns"][0]["automation"] == ["effects.echo.feedback_percent"]
    assert inspected["master_automation"] == []
    checked = cli("check", path)
    assert checked["warnings"] == [
        "space: effects.echo.feedback_percent automates a bypassed effect"
    ]
    described = cli("describe", "automation")
    assert described["automatable"]["effects"]["filter"] == {"cutoff_hz": "log"}
    assert "lane" in described["schema"]
    assert "curves" in described["semantics"]
