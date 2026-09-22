"""Delay, reverb, sends and returns, tested with generated audio."""

import json
import subprocess
import sys
import numpy as np
import pytest
import soundfile as sf
from scipy.signal import butter, sosfilt
from agent_daw.effects import Chain, reverb_ir
from agent_daw.engine import render
from agent_daw.model import Compressor, Delay, Project, Reverb, load, save
from agent_daw.perception import compare, listen

SR = 48000


def impulse(seconds):
    x = np.zeros((round(seconds * SR), 2))
    x[0] = 1
    return x


def noise(seconds, seed=1, amp=0.1):
    n = np.random.default_rng(seed).normal(0, amp, round(seconds * SR))
    return np.column_stack([n, n])


def level_db(x):
    return 20 * np.log10(np.sqrt(np.mean(x**2)))


def decay_time(x, low, high):
    """Schroeder backward-integrated T30 in a band, extrapolated to 60 dB."""
    band = sosfilt(butter(4, [low, high], "bandpass", fs=SR, output="sos"), x)
    energy = np.cumsum(band[::-1] ** 2)[::-1]
    curve = 10 * np.log10(energy / energy[0] + 1e-300)
    return (np.argmax(curve < -35) - np.argmax(curve < -5)) / SR * 2


def delay(**kw):
    return Delay(type="delay", **{"time_beats": "1/2", **kw})


def reverb(**kw):
    return Reverb(type="reverb", **kw)


def test_delay_echoes_land_on_tempo_synced_frames():
    # A third of a beat at 120 BPM is exactly 8000 frames.
    y = Chain([delay(time_beats="1/3", feedback_percent=50)], SR, 120).run(impulse(1))
    assert list(np.flatnonzero(y[:30000, 0])) == [8000, 16000, 24000]
    assert list(y[[8000, 16000, 24000], 0]) == [1, 0.5, 0.25]
    assert np.array_equal(y[:, 0], y[:, 1])


def test_ping_pong_starts_left_and_alternates():
    spec = delay(feedback_percent=50, ping_pong=True)
    y = Chain([spec], SR, 120).run(impulse(2))
    assert list(y[[12000, 24000, 36000], 0]) == [1, 0, 0.25]
    assert list(y[[12000, 24000, 36000], 1]) == [0, 0.5, 0]


def test_delay_highcut_darkens_each_repeat():
    spec = delay(feedback_percent=80, highcut_hz=3000)
    y = Chain([spec], SR, 120).run(impulse(2))[:, 0]
    bright = []
    for k in (1, 2, 3):
        echo = y[k * 12000 : k * 12000 + 4000]
        power = abs(np.fft.rfft(echo)) ** 2
        freqs = np.fft.rfftfreq(len(echo), 1 / SR)
        bright.append(power[freqs > 6000].sum() / power.sum())
    assert bright[0] > bright[1] > bright[2]


def test_reverb_decay_damping_and_predelay():
    spec = reverb(decay_seconds=2, predelay_ms=20, damping_hz=3000)
    y = Chain([spec], SR).run(impulse(4))
    mid = decay_time(y[:, 0], 700, 1400)
    assert mid == pytest.approx(2, rel=0.1)
    # Above damping_hz the decay time falls as 1/f: about 0.6 s at 10 kHz.
    assert decay_time(y[:, 0], 8000, 12000) < 0.5 * mid
    assert np.max(abs(y[:960])) < 1e-12  # 20 ms at 48 kHz
    assert np.max(abs(y[960:1500])) > 1e-3
    assert np.sum(y**2, axis=0) == pytest.approx([1, 1], rel=1e-6)


def test_reverb_matches_direct_convolution_after_latency():
    spec = reverb(decay_seconds=0.4, predelay_ms=5)
    chain = Chain([spec], SR)
    x = noise(1)
    y = chain.run(x)
    ir = reverb_ir(spec, SR)
    expected = np.convolve(x[:, 0], ir[:, 1])[: len(x)]
    assert np.allclose(y[:, 1], expected, atol=1e-12)
    assert chain.report() == [{"type": "reverb", "latency_frames": 4096}]


def test_reverb_is_seeded_and_width_controls_correlation():
    x = noise(2)
    a = Chain([reverb()], SR).run(x)
    assert np.array_equal(a, Chain([reverb()], SR).run(x))
    assert not np.allclose(a, Chain([reverb(seed=1)], SR).run(x))
    assert abs(np.corrcoef(a[:, 0], a[:, 1])[0, 1]) < 0.1
    mono = Chain([reverb(width_percent=0)], SR).run(x)
    assert np.array_equal(mono[:, 0], mono[:, 1])


def test_reverb_level_is_energy_normalized():
    x = noise(6, amp=0.1)
    y = Chain([reverb(decay_seconds=1)], SR).run(x)
    assert level_db(y[3 * SR :]) == pytest.approx(level_db(x[3 * SR :]), abs=0.5)


def test_reverb_lowcut_removes_low_end():
    t = np.arange(3 * SR) / SR
    x = np.column_stack([0.3 * np.sin(2 * np.pi * 60 * t)] * 2)
    full = Chain([reverb(lowcut_hz=20)], SR).run(x)
    cut = Chain([reverb(lowcut_hz=400)], SR).run(x)
    assert level_db(cut[SR:]) < level_db(full[SR:]) - 15


def test_zero_mix_is_identity():
    x = noise(1)
    assert np.array_equal(Chain([delay(mix_percent=0)], SR, 120).run(x), x)
    assert np.array_equal(Chain([reverb(mix_percent=0)], SR).run(x), x)


def test_new_devices_are_block_partition_invariant():
    specs = [
        delay(time_beats="3/4", ping_pong=True, lowcut_hz=200, highcut_hz=5000),
        reverb(decay_seconds=0.5, predelay_ms=12),
        Compressor(type="compressor", threshold_db=-24, sidechain="kick"),
        reverb(decay_seconds=3, mix_percent=30, seed=7),
    ]
    rng = np.random.default_rng(3)
    x = rng.normal(0, 0.3, (3 * SR, 2))
    keys = {"kick": rng.normal(0, 0.5, (3 * SR, 2))}
    a = Chain(specs, SR, 128).run(x, keys, 127)
    b = Chain(specs, SR, 128).run(x, keys, 4096)
    c = Chain(specs, SR, 128).run(x, keys, 6 * SR)
    assert np.array_equal(a, b) and np.array_equal(b, c)


@pytest.fixture
def mix(tmp_path):
    t = np.arange(SR // 4) / SR
    kick = 0.9 * np.sin(2 * np.pi * 55 * t) * np.exp(-t * 18)
    click = np.zeros(SR // 50)
    click[:480] = 0.5 * np.sin(2 * np.pi * 1000 * np.arange(480) / SR)
    sf.write(tmp_path / "kick.wav", kick, SR, subtype="FLOAT")
    sf.write(tmp_path / "click.wav", click, SR, subtype="FLOAT")
    data = {
        "session": {"tempo": 120, "length_beats": 8, "master_gain_db": -6},
        "samples": {"kick": {"path": "kick.wav"}, "click": {"path": "click.wav"}},
        "patterns": {
            "four": {"length_beats": 4, "steps": {"k": "x...x...x...x..."}},
            "hit": {"length_beats": 4, "steps": {"s": "............x..."}},
        },
        "tracks": [
            {
                "id": "snare",
                "pads": {"s": {"sample": "click"}},
                "clips": [{"pattern": "hit"}],
                "sends": [
                    {"to": "plate", "gain_db": -6},
                    {"to": "echo", "gain_db": -12},
                ],
            },
            {
                "id": "kick",
                "pads": {"k": {"sample": "kick"}},
                "clips": [{"pattern": "four", "repeats": 2}],
            },
        ],
        "returns": [
            {"id": "plate", "effects": [{"type": "reverb", "decay_seconds": 1}]},
            {
                "id": "echo",
                "effects": [
                    {
                        "type": "delay",
                        "time_beats": "3/4",
                        "feedback_percent": 40,
                        "highcut_hz": 5000,
                        "ping_pong": True,
                    }
                ],
            },
        ],
        "sections": [{"id": "end", "at": 4, "length_beats": 4}],
    }
    path = tmp_path / "song.yaml"
    save(Project.model_validate(data), path)
    return path, data


def stem(directory, name):
    return sf.read(directory / "stems" / f"{name}.wav", always_2d=True)[0]


def rerender(path, data, directory, **kw):
    save(Project.model_validate(data), path)
    render(path, directory, **kw)
    return directory


def test_returns_have_stems_that_sum_to_mix(mix, tmp_path):
    path, data = mix
    report = render(path, tmp_path / "full")
    assert list(report["tracks"]) == ["snare", "kick", "plate", "echo"]
    assert report["tracks"]["plate"]["kind"] == "return"
    assert report["tracks"]["plate"]["senders"] == ["snare"]
    assert report["tracks"]["plate"]["effects"][0]["type"] == "reverb"
    assert report["tracks"]["snare"]["kind"] == "track"
    assert report["tracks"]["snare"]["events"] == 1
    assert report["stems_sum_to_mix"] is True
    out = tmp_path / "full"
    total = sum(stem(out, n) for n in ["snare", "kick", "plate", "echo"])
    mixed = sf.read(out / "mix.wav", always_2d=True)[0]
    assert np.max(abs(mixed - total)) < 2e-7
    # The snare stem is dry; its reverb and echoes live on the return stems.
    after = round(1.5 * SR) + SR // 10
    assert not stem(out, "snare")[after:].any()
    assert level_db(stem(out, "plate")[after : after + SR // 2]) > -60
    echoes = stem(out, "echo")
    first = round(1.5 * SR) + 18000  # 3/4 beat after the hit
    assert abs(echoes[first : first + 480, 0]).max() > 0.01
    assert abs(echoes[first : first + 480, 1]).max() == 0


def test_post_fader_send_follows_gain_and_pre_fader_does_not(mix, tmp_path):
    path, data = mix
    base = stem(rerender(path, data, tmp_path / "base"), "plate")
    data["tracks"][0]["gain_db"] = -6
    quieter = stem(rerender(path, data, tmp_path / "quieter"), "plate")
    assert level_db(quieter) - level_db(base) == pytest.approx(-6, abs=1e-6)
    data["tracks"][0].update(pan=0.5)
    data["tracks"][0]["sends"][0]["pre_fader"] = True
    pre = stem(rerender(path, data, tmp_path / "pre"), "plate")
    assert np.array_equal(pre, base)


def test_mute_and_solo_silence_sends_but_never_returns(mix, tmp_path):
    path, data = mix
    data["tracks"][0]["mute"] = True
    assert not stem(rerender(path, data, tmp_path / "muted"), "plate").any()
    data["tracks"][0]["mute"] = False
    data["tracks"][1]["solo"] = True
    assert not stem(rerender(path, data, tmp_path / "kick-solo"), "plate").any()
    data["tracks"][1]["solo"] = False
    data["tracks"][0]["solo"] = True
    out = rerender(path, data, tmp_path / "snare-solo")
    assert stem(out, "plate").any() and not stem(out, "kick").any()
    data["returns"][0]["mute"] = True
    assert not stem(rerender(path, data, tmp_path / "return-muted"), "plate").any()


def test_return_preview_is_wet_stem_and_track_preview_is_dry(mix, tmp_path):
    path, data = mix
    render(path, tmp_path / "full")
    preview = render(path, tmp_path / "plate", track_id="plate")
    assert list(preview["tracks"]) == ["plate"]
    assert np.array_equal(
        stem(tmp_path / "plate", "plate"), stem(tmp_path / "full", "plate")
    )
    dry = render(path, tmp_path / "snare", track_id="snare")
    assert list(dry["tracks"]) == ["snare"]
    assert np.array_equal(
        stem(tmp_path / "snare", "snare"), stem(tmp_path / "full", "snare")
    )
    with pytest.raises(ValueError, match="Unknown track or return"):
        render(path, track_id="absent")


def test_section_preview_keeps_tails_from_earlier_notes(mix, tmp_path):
    path, data = mix
    render(path, tmp_path / "full")
    render(path, tmp_path / "end", section="end")
    full = sf.read(tmp_path / "full/mix.wav", always_2d=True)[0]
    region = sf.read(tmp_path / "end/mix.wav", always_2d=True)[0]
    assert np.array_equal(region, full[2 * SR : 4 * SR])
    # Half a second after the hit, a 1 s reverb tail is quiet but present.
    assert level_db(stem(tmp_path / "end", "plate")[: SR // 4]) > -90


def test_return_compressor_ducks_under_sidechain(mix, tmp_path):
    path, data = mix
    sf.write(path.parent / "pad.wav", noise(4, amp=0.2)[:, 0], SR, subtype="FLOAT")
    data["samples"]["pad"] = {"path": "pad.wav"}
    data["patterns"]["hold"] = {"length_beats": 8, "steps": {"p": "x" + "." * 31}}
    data["tracks"].append(
        {
            "id": "pad",
            "pads": {"p": {"sample": "pad"}},
            "clips": [{"pattern": "hold"}],
            "sends": [{"to": "plate"}],
        }
    )
    data["returns"][0]["effects"].append(
        {
            "type": "compressor",
            "threshold_db": -40,
            "ratio": 10,
            "attack_ms": 1,
            "release_ms": 100,
            "sidechain": "kick",
        }
    )
    save(Project.model_validate(data), path)
    report = render(path, tmp_path / "duck")
    plate = stem(tmp_path / "duck", "plate")
    beat = SR // 2
    after = level_db(plate[5 * beat + 1000 : 5 * beat + 3000])
    before = level_db(plate[6 * beat - 3000 : 6 * beat - 1000])
    assert after < before - 6
    assert report["tracks"]["plate"]["effects"][1]["max_gain_reduction_db"] > 6


def test_render_with_returns_is_block_size_invariant(mix, tmp_path):
    path, data = mix
    a = render(path, tmp_path / "a", 127)
    b = render(path, tmp_path / "b", 4096)
    assert a["audio_sha256"] == b["audio_sha256"]
    assert a["tracks"]["echo"]["audio_sha256"] == b["tracks"]["echo"]["audio_sha256"]


def test_float_stems_have_no_timestamped_peak_chunk(mix, tmp_path):
    # libsndfile's PEAK chunk holds a write time, which made equal audio hash unequally.
    path, data = mix
    render(path, tmp_path / "a")
    assert b"PEAK" not in (tmp_path / "a" / "stems" / "echo.wav").read_bytes()


def slow_long_delay(d):
    d["session"]["tempo"] = 60
    d["returns"][1]["effects"][0]["time_beats"] = 12  # 12 s at 60 BPM


@pytest.mark.parametrize(
    "change, message",
    [
        (lambda d: d["tracks"][0]["sends"][0].update(to="hall"), "unknown return"),
        (lambda d: d["tracks"][0]["sends"][1].update(to="plate"), "one send"),
        (lambda d: d["returns"][0].update(id="kick"), "distinct from track"),
        (
            lambda d: d["tracks"][0].update(
                effects=[
                    {"type": "compressor", "threshold_db": -20, "sidechain": "plate"}
                ]
            ),
            "not return plate",
        ),
        (
            lambda d: d["returns"][0]["effects"].append(
                {"type": "compressor", "threshold_db": -20, "sidechain": "nope"}
            ),
            "unknown sidechain",
        ),
        (lambda d: d["returns"][0].update(sends=[]), "sends"),
        (
            lambda d: d["returns"][1]["effects"][0].update(time_beats="1/10000"),
            "1 ms to 10 s",
        ),
        (slow_long_delay, "1 ms to 10 s"),
        (lambda d: d["returns"][1]["effects"][0].update(time_beats=0), "time_beats"),
        (
            lambda d: d["returns"][1]["effects"][0].update(lowcut_hz=6000),
            "below highcut",
        ),
        (
            lambda d: d["returns"][0]["effects"][0].update(decay_seconds=30),
            "decay_seconds",
        ),
        (
            lambda d: d["returns"][0]["effects"][0].update(width_percent=150),
            "width_percent",
        ),
    ],
)
def test_routing_validation(mix, change, message):
    path, data = mix
    change(data)
    with pytest.raises(ValueError, match=message):
        Project.model_validate(data)


def test_routing_round_trip_and_cli(mix, tmp_path):
    path, data = mix
    text = path.read_text()
    save(load(path), path)
    assert path.read_text() == text
    assert "- {to: plate, gain_db: -6.0}" in text
    assert "time_beats: 3/4" in text
    out = subprocess.run(
        [sys.executable, "-m", "agent_daw.cli", "inspect", str(path)],
        capture_output=True,
        text=True,
    )
    assert out.returncode == 0, out.stderr
    inspected = json.loads(out.stdout)
    assert inspected["returns"][0] == {
        "id": "plate",
        "gain_db": 0,
        "mute": False,
        "effects": ["reverb"],
        "sidechain": [],
        "senders": ["snare"],
        "automation": [],
    }
    snare = inspected["tracks"][0]
    assert snare["sends"][0] == {"to": "plate", "gain_db": -6, "pre_fader": False}
    out = subprocess.run(
        [sys.executable, "-m", "agent_daw.cli", "describe", "effects"],
        capture_output=True,
        text=True,
    )
    described = json.loads(out.stdout)
    assert {"delay", "reverb"} <= set(described["schema"])
    assert set(described["routing"]) == {"send", "return"}
    assert "returns" in described["semantics"]


def test_listen_measures_return_stems(mix, tmp_path):
    path, data = mix
    render(path, tmp_path / "full")
    report = listen(tmp_path / "full", images=False)
    assert report["tracks"]["plate"]["kind"] == "return"
    assert report["tracks"]["snare"]["kind"] == "track"
    assert report["tracks"]["plate"]["audio"]["silent"] is False
    assert "plate" not in report["musical_context"]["tracks"]


def test_compare_diffs_returns_present_in_both_renders(mix, tmp_path):
    path, data = mix
    before = rerender(path, data, tmp_path / "before")
    data["tracks"][0]["sends"][0]["gain_db"] = -12
    after = rerender(path, data, tmp_path / "after")
    report = compare(before, after, images=False)
    assert report["tracks"]["plate"]["actual_delta"]["rms_dbfs"] == pytest.approx(
        -6, abs=1e-3
    )
    assert report["tracks"]["snare"]["actual_delta"]["rms_dbfs"] == 0
    assert set(report["musical_context_delta"]) == {"snare", "kick"}
