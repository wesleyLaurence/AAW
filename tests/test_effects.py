"""Filter, eq, compressor and limiter, tested with generated audio.

An effect chain is run on its own by rendering a song that plays the test signal
once on a track with those effects; the track's stem is the chain's output.
"""

import numpy as np
import pytest
import soundfile as sf
from agent_daw.model import load, save, validate
from helpers import SR, cli, level_db, render, run_chain, stem, tone


def highpass(cutoff, slope=12, **kw):
    return {
        "type": "filter",
        "mode": "highpass",
        "cutoff_hz": cutoff,
        "slope_db_per_octave": slope,
        **kw,
    }


def lowpass(cutoff, slope=12, **kw):
    return {**highpass(cutoff, slope, **kw), "mode": "lowpass"}


@pytest.mark.parametrize("slope", [12, 24, 48])
def test_highpass_attenuates_below_cutoff(tmp_path, slope):
    spec = highpass(400, slope)
    low, high = tone(100), tone(4000)
    out_low, _ = run_chain(tmp_path / "low", [spec], low)
    out_high, _ = run_chain(tmp_path / "high", [spec], high)
    loss_low = level_db(out_low[SR // 2 :]) - level_db(low[SR // 2 :])
    loss_high = level_db(out_high[SR // 2 :]) - level_db(high[SR // 2 :])
    # Butterworth: two octaves below cutoff is attenuated about slope * 2 dB.
    assert loss_low < -2 * slope + 1
    assert abs(loss_high) < 0.1


def test_lowpass_and_eq_bands(tmp_path):
    def gain(name, spec, freq):
        out, _ = run_chain(tmp_path / name, [spec], tone(freq))
        return level_db(out[SR // 2 :]) - level_db(tone(freq))

    assert gain("lp", lowpass(1000, 24), 8000) < -60
    bell = {
        "type": "eq",
        "bands": [{"shape": "bell", "freq_hz": 1000, "gain_db": 6, "q": 1}],
    }
    assert abs(gain("bell", bell, 1000) - 6) < 0.05
    assert abs(gain("far", bell, 60)) < 0.2
    shelves = {
        "type": "eq",
        "bands": [
            {"shape": "low_shelf", "freq_hz": 200, "gain_db": -9},
            {"shape": "high_shelf", "freq_hz": 5000, "gain_db": 4},
        ],
    }
    assert abs(gain("low", shelves, 40) + 9) < 0.3
    assert abs(gain("high", shelves, 15000) - 4) < 0.3


def test_compressor_static_curve_and_report(tmp_path):
    spec = {
        "type": "compressor",
        "threshold_db": -20,
        "ratio": 4,
        "knee_db": 0,
        "attack_ms": 1,
        "release_ms": 200,
    }
    y, report = run_chain(tmp_path / "a", [spec], tone(1000, 2, amp=0.5))
    peak = 20 * np.log10(np.max(abs(y[SR:])))
    # -6.02 dBFS in, 14 dB over the threshold at 4:1 leaves 3.5 dB over.
    assert abs(peak - (-20 + (20 * np.log10(0.5) + 20) / 4)) < 0.3
    assert report[0]["type"] == "compressor"
    assert abs(report[0]["max_gain_reduction_db"] - 10.5) < 0.3
    assert report[0]["fraction_over_1db_reduction"] > 0.95


def test_compressor_below_threshold_is_transparent(tmp_path):
    spec = {"type": "compressor", "threshold_db": -6, "knee_db": 0}
    x = tone(220, amp=0.25)
    y, _ = run_chain(tmp_path / "a", [spec], x)
    assert np.array_equal(y, x.astype(np.float32))


def test_limiter_ceiling_latency_and_transparency(tmp_path):
    spec = {"type": "limiter", "ceiling_db": -1}
    x = tone(100, amp=2.0)
    x[SR // 2 : SR // 2 + 10] *= 3  # a sharp transient over a loud tone
    y, report = run_chain(tmp_path / "loud", [spec], x)
    assert len(y) == len(x)
    assert 20 * np.log10(np.max(abs(y))) <= -1 + 1e-6
    # The 3 ms look-ahead is 144 frames; 6.02 dBFS is held 7.02 dB down.
    assert report[0]["latency_frames"] == 144
    assert report[0]["max_gain_reduction_db"] == pytest.approx(7.02, abs=0.01)
    quiet = tone(100, amp=0.3)
    # Below the ceiling, the look-ahead delay is compensated exactly.
    y, _ = run_chain(tmp_path / "quiet", [spec], quiet)
    assert np.array_equal(y, quiet.astype(np.float32))


def test_chain_block_partition_invariance(tmp_path):
    specs = [
        highpass(80, 24),
        {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 300, "gain_db": -4}]},
        {"type": "limiter", "ceiling_db": -3, "lookahead_ms": 2},
        {"type": "compressor", "threshold_db": -18, "sidechain": "kick"},
        {"type": "limiter", "ceiling_db": -6},
    ]
    rng = np.random.default_rng(3)
    x = rng.normal(0, 0.5, (SR, 2))
    keys = {"kick": rng.normal(0, 0.5, (SR, 2))}
    a, b, c = (
        run_chain(tmp_path / str(size), specs, x, keys=keys, block_size=size)[0]
        for size in (127, 4096, SR * 2)
    )
    assert np.array_equal(a, b) and np.array_equal(b, c)


def test_bypass_is_identity(tmp_path):
    x = tone(300)
    y, report = run_chain(tmp_path / "a", [lowpass(100, bypass=True)], x)
    assert np.array_equal(y, x.astype(np.float32))
    assert report == [{"type": "filter", "bypass": True}]


@pytest.fixture
def beat(tmp_path):
    t = np.arange(SR // 4) / SR
    kick = 0.9 * np.sin(2 * np.pi * 55 * t) * np.exp(-t * 18)
    sf.write(tmp_path / "kick.wav", kick, SR, subtype="FLOAT")
    sf.write(tmp_path / "bass.wav", tone(110, 4, 0.4)[:, 0], SR, subtype="FLOAT")
    data = {
        "session": {"tempo": 120, "length_beats": 8, "master_gain_db": 0},
        "samples": {"kick": {"path": "kick.wav"}, "bass": {"path": "bass.wav"}},
        "patterns": {
            "four": {"length_beats": 4, "steps": {"k": "x...x...x...x..."}},
            "hold": {"length_beats": 4, "steps": {"b": "x..............."}},
        },
        "tracks": [
            {
                "id": "bass",
                "pads": {"b": {"sample": "bass"}},
                "clips": [{"pattern": "hold", "repeats": 2}],
                "effects": [
                    {"type": "filter", "mode": "highpass", "cutoff_hz": 40},
                    {
                        "type": "compressor",
                        "threshold_db": -30,
                        "ratio": 8,
                        "attack_ms": 1,
                        "release_ms": 150,
                        "sidechain": "kick",
                    },
                ],
            },
            {
                "id": "kick",
                "pads": {"k": {"sample": "kick"}},
                "clips": [{"pattern": "four", "repeats": 2}],
            },
        ],
    }
    path = tmp_path / "song.yaml"
    save(data, path)
    return path, data


def test_sidechain_ducks_bass_under_kick(beat, tmp_path):
    path, data = beat
    report = render(path, tmp_path / "duck")
    bass = stem(tmp_path / "duck", "bass")
    beat_frames = SR // 2
    # Just after each kick versus just before the next one, over the held note.
    after = level_db(bass[beat_frames + 1000 : beat_frames + 3000])
    before = level_db(bass[2 * beat_frames - 3000 : 2 * beat_frames - 1000])
    assert after < before - 6
    effects = report["tracks"]["bass"]["effects"]
    assert effects[0] == {"type": "filter", "latency_frames": 0}
    assert effects[1]["max_gain_reduction_db"] > 6
    assert report["stems_sum_to_mix"] is True
    assert list(report["tracks"]) == [
        "bass",
        "kick",
    ]  # document order, not render order
    mix = sf.read(tmp_path / "duck/mix.wav", always_2d=True)[0]
    assert (
        np.max(
            abs(mix - stem(tmp_path / "duck", "bass") - stem(tmp_path / "duck", "kick"))
        )
        < 2e-7
    )


def test_sidechain_key_is_pre_fader_and_previews_match(beat, tmp_path):
    path, data = beat
    render(path, tmp_path / "full")
    data["tracks"][1].update(mute=True, gain_db=-40)
    save(data, path)
    render(path, tmp_path / "muted")
    assert np.array_equal(
        stem(tmp_path / "full", "bass"), stem(tmp_path / "muted", "bass")
    )
    preview = render(path, tmp_path / "preview", track_id="bass")
    assert list(preview["tracks"]) == ["bass"]
    assert np.array_equal(
        stem(tmp_path / "preview", "bass"), stem(tmp_path / "full", "bass")
    )


def test_master_limiter_allows_hot_mix(beat, tmp_path):
    path, data = beat
    data["session"]["master_gain_db"] = 12
    save(data, path)
    with pytest.raises(ValueError, match="master limiter"):
        render(path, tmp_path / "hot")
    data["master"] = {"effects": [{"type": "limiter", "ceiling_db": -1}]}
    save(data, path)
    a = render(path, tmp_path / "limited", 127)
    b = render(path, tmp_path / "limited-b", 4096)
    assert a["audio_sha256"] == b["audio_sha256"]
    assert a["mix"]["peak_dbfs"] <= -1 + 1e-4
    assert a["stems_sum_to_mix"] is False
    assert a["master_effects"][0]["max_gain_reduction_db"] > 3
    # Stems remain the pre-master bus and are not limited.
    total = stem(tmp_path / "limited", "bass") + stem(tmp_path / "limited", "kick")
    assert np.max(abs(total)) > 1


@pytest.mark.parametrize(
    "change, message",
    [
        (
            lambda d: d["tracks"][0]["effects"][1].update(sidechain="nope"),
            "unknown sidechain",
        ),
        (lambda d: d["tracks"][0]["effects"][1].update(sidechain="bass"), "itself"),
        (
            lambda d: d["tracks"][1].update(
                effects=[
                    {"type": "compressor", "threshold_db": -10, "sidechain": "bass"}
                ]
            ),
            "cycle",
        ),
        (
            lambda d: d.update(
                master={
                    "effects": [
                        {"type": "compressor", "threshold_db": -10, "sidechain": "kick"}
                    ]
                }
            ),
            "Master compressor",
        ),
        (lambda d: d["tracks"][0]["effects"][0].update(cutoff_hz=30000), "cutoff_hz"),
        (lambda d: d["tracks"][0]["effects"][0].update(type="flanger"), "type"),
        (
            lambda d: d["tracks"][0]["effects"][0].update(slope_db_per_octave=18),
            "slope",
        ),
        (
            lambda d: d.update(
                master={"effects": [{"type": "limiter", "ceiling_db": 0}]}
            ),
            "ceiling",
        ),
    ],
)
def test_effect_validation(beat, change, message):
    path, data = beat
    change(data)
    with pytest.raises(ValueError, match=message):
        validate(data)


def test_effects_round_trip_and_cli(beat, tmp_path):
    path, data = beat
    data["master"] = {"effects": [{"type": "limiter"}]}
    save(data, path)
    text = path.read_text()
    save(load(path), path)
    assert path.read_text() == text
    assert "type: limiter" in text
    code, described = cli("describe", "effects")
    assert code == 0, described
    assert set(described["schema"]) >= {"filter", "eq", "compressor", "limiter"}
    code, inspected = cli("inspect", path)
    bass = next(t for t in inspected["tracks"] if t["id"] == "bass")
    assert bass["effects"] == ["filter", "compressor"]
    assert bass["sidechain"] == ["kick"]
    assert inspected["master_effects"] == ["limiter"]
