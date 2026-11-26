"""A beat map through tempo changes: generated songs whose beats are known, with a
step, a band's drift and a half-time switch, with and without a stand-in model."""

import json
import shlex
import sys
from pathlib import Path

import numpy as np
import pytest

from agent_daw import beats, hits, segments
from helpers import beat_song as song, cli

STAND_IN = Path(__file__).with_name("stand_in_downbeats.py")


def step(at, before, after):
    """Seconds late each beat is when the beat's length goes from `before` to
    `after` seconds at beat `at`."""
    return lambda k: np.where(k >= at, (k - at) * (after - before), 0.0)


def drift(period, seed, count=400):
    """A band's drift: the tempo wandering 2% over eight bars, and each beat off
    it by up to 10 ms."""
    jitter = np.random.default_rng(seed).uniform(-0.010, 0.010, count)
    wander = np.cumsum(np.asarray(period) * 0.01 * np.sin(2 * np.pi * np.arange(count) / 64))
    return wander + jitter


def found(report):
    return np.array([b["seconds"] for b in report["beats"]])


def segs(report):
    return report["tempo"]["segments"]


@pytest.fixture
def model(monkeypatch):
    monkeypatch.setenv("AAW_DOWNBEATS", shlex.join([sys.executable, str(STAND_IN)]))


def hears(path, beat_times, downbeat_times):
    Path(f"{path}.heard.json").write_text(
        json.dumps({"beats": [float(t) for t in beat_times], "downbeats": [float(t) for t in downbeat_times]})
    )


def test_a_step_is_on_its_bar_and_each_segment_has_its_tempo(tmp_path):
    truth = song(tmp_path / "song.wav", 120.0, 32, late=step(64, 0.5, 0.4))
    report = beats.measure(tmp_path / "song.wav")
    first, second = segs(report)
    assert (first["bar"], first["bpm"], first["steady"]) == (1, pytest.approx(120, abs=0.01), True)
    assert (second["bar"], second["bpm"], second["steady"]) == (17, pytest.approx(150, abs=0.01), True)
    assert second["seconds"] == pytest.approx(truth[64], abs=0.001) and second["set_by"] == "measurement"
    assert second["ambiguous_with"] == [75.0]
    # The whole song's tempo is the first segment's, and not steady with two.
    tempo = report["tempo"]
    assert tempo["bpm"] == first["bpm"] and not tempo["steady"]
    assert tempo["bpm_range"] == [pytest.approx(120, abs=0.01), pytest.approx(150, abs=0.01)]
    # Every beat, counted on across the step: bar 17 of the song is bar 17 of the map.
    times = found(report)
    assert len(times) == len(truth) and np.abs(times - truth).max() < 0.005
    assert [(b["bar"], b["beat"]) for b in report["beats"][63:66]] == [(16, 4), (17, 1), (17, 2)]
    assert report["odd_bars"] == []


def test_a_step_off_the_bar_line_is_placed_on_the_bar_line_nearest(tmp_path):
    # The tempo changes on beat 3 of bar 17.
    truth = song(tmp_path / "song.wav", 120.0, 32, late=step(66, 0.5, 0.4))
    report = beats.measure(tmp_path / "song.wav")
    second = segs(report)[1]
    assert second["bar"] == 17 and second["seconds"] == pytest.approx(truth[64], abs=0.001)
    assert second["beat_seconds"] == pytest.approx(truth[66], abs=0.001)
    assert np.abs(found(report) - truth).max() < 0.005


@pytest.mark.parametrize("bpm, seed", [(120.0, 1), (150.0, 5), (170.0, 3)])
def test_a_band_that_drifts_is_one_tempo_followed_beat_by_beat(tmp_path, bpm, seed):
    wander = drift(60 / bpm, seed)
    truth = song(tmp_path / "song.wav", bpm, 24, late=lambda k: wander[k])
    report = beats.measure(tmp_path / "song.wav")
    (only,) = segs(report)
    assert not only["steady"] and only["bpm"] == pytest.approx(bpm, rel=0.01)
    assert report["tempo"]["drift_ms"] > 20
    times = found(report)
    assert len(times) == len(truth) and np.abs(times - truth).max() < 0.005


def test_a_band_that_drifts_and_steps(tmp_path):
    period = np.where(np.arange(400) < 64, 0.5, 0.4)
    wander, at = drift(period, 5), step(64, 0.5, 0.4)
    truth = song(tmp_path / "song.wav", 120.0, 40, late=lambda k: at(k) + wander[k])
    report = beats.measure(tmp_path / "song.wav")
    first, second = segs(report)
    assert first["bpm"] == pytest.approx(120, rel=0.01) and second["bpm"] == pytest.approx(150, rel=0.01)
    assert second["bar"] == 17 and second["beats_from"] == "followed"
    times = found(report)
    assert len(times) == len(truth) and np.abs(times - truth).max() < 0.005


def test_a_half_time_switch_is_one_tempo_unless_asked(tmp_path):
    # 150, then its beats twice as long from bar 17: the same beats, half of them.
    truth = song(tmp_path / "song.wav", 150.0, 32, late=step(64, 0.4, 0.8))
    report = beats.measure(tmp_path / "song.wav")
    (only,) = segs(report)
    assert only["bpm"] == pytest.approx(150, abs=0.01) and only["steady"] and only["ambiguous_with"] == [75.0]
    # Asked for, it is two segments, the second at 75 with its neighbour's 150 listed.
    asked = beats.measure(tmp_path / "song.wav", bpm=[150, 75], segment_at=truth[64] + 0.05)
    first, second = segs(asked)
    assert second["bpm"] == pytest.approx(75, abs=0.01) and second["set_by"] == "request"
    assert second["bar"] == 17 and 150.0 in second["ambiguous_with"]
    assert len(found(asked)) == len(truth) and np.abs(found(asked) - truth).max() < 0.005
    # More tempos than segments is refused, and says how to give the steps.
    with pytest.raises(ValueError, match="--segment-at"):
        beats.measure(tmp_path / "song.wav", bpm=[150, 75])


def test_a_song_of_one_tempo_has_one_segment_and_its_map_as_before(tmp_path):
    truth = song(tmp_path / "song.wav", 120.0, 24)
    report = beats.measure(tmp_path / "song.wav")
    (only,) = segs(report)
    tempo = report["tempo"]
    assert {k: only[k] for k in ("bpm", "steady", "drift_ms", "bpm_range", "ambiguous_with")} == {
        k: tempo[k] for k in ("bpm", "steady", "drift_ms", "bpm_range", "ambiguous_with")
    }
    assert (only["bar"], only["seconds"], only["set_by"]) == (1, report["first_beat_seconds"], "measurement")
    assert np.abs(found(report) - truth).max() < 0.0001


def test_the_model_steps_where_its_beats_do(tmp_path, model):
    path = tmp_path / "song.wav"
    truth = song(path, 120.0, 32, late=step(64, 0.5, 0.4))
    rng = np.random.default_rng(3)
    hears(path, truth + rng.uniform(-0.015, 0.015, len(truth)), truth[::4])
    report = beats.measure(path)
    assert report["model"]["used"] and [s["bar"] for s in segs(report)] == [1, 17]
    assert [round(s["bpm"]) for s in segs(report)] == [120, 150]
    assert np.abs(found(report) - truth).max() < 0.005 and report["odd_bars"] == []


def test_a_model_count_at_half_time_is_doubled_and_the_switch_counted_nearest(tmp_path, model):
    # 140 trap the model hears at 70, switching to 170 at bar 25, which it hears
    # as it is: the second segment is counted nearest the first, not doubled.
    path = tmp_path / "trap.wav"
    truth = song(path, 140.0, 40, late=step(96, 60 / 140, 60 / 170))
    hears(path, [*truth[:96:2], *truth[96:]], [*truth[:96:8], *truth[96::4]])
    report = beats.measure(path)
    assert report["model"]["counted"] == "doubled"
    assert [(s["bar"], round(s["bpm"])) for s in segs(report)] == [(1, 140), (25, 170)]
    assert len(found(report)) == len(truth) and np.abs(found(report) - truth).max() < 0.005
    assert report["odd_bars"] == []


def test_a_stretch_the_model_hears_at_half_time_is_no_step(tmp_path, model):
    # A steady 150 song whose sparse middle the model hears at 75.
    path = tmp_path / "song.wav"
    truth = song(path, 150.0, 32)
    heard = [*truth[:48], *truth[48:96:2], *truth[96:]]
    hears(path, heard, truth[::4])
    report = beats.measure(path)
    assert len(segs(report)) == 1 and report["tempo"]["bpm"] == pytest.approx(150, abs=0.01)


def test_steps_in_a_run_of_beats():
    beats_at = np.concatenate([0.5 * np.arange(40), 19.5 + 0.4 * np.arange(1, 41)])
    assert segments.steps_in(beats_at) == [39]
    # Half and double time are the same tempo, and a short fill is drift.
    assert segments.steps_in(np.concatenate([0.4 * np.arange(40), 15.6 + 0.8 * np.arange(1, 30)])) == []
    fill = 0.5 * np.arange(60)
    fill[30:33] += [0.03, 0.06, 0.03]
    assert segments.steps_in(fill) == []
    # Evened out, a stretch heard at half time is the beats it skipped.
    half = np.concatenate([0.4 * np.arange(20), 7.6 + 0.8 * np.arange(1, 11)])
    assert np.allclose(np.diff(segments.evened(half)), 0.4)


def test_the_command_keeps_the_steps_and_tempos_asked_for(tmp_path):
    truth = song(tmp_path / "song.wav", 150.0, 32, late=step(64, 0.4, 0.8))
    args = ("samples", "--db", tmp_path / "index.sqlite", "beats", tmp_path / "song.wav")
    code, report = cli(*args, "--segment-at", f"{truth[64]:.3f}", "--bpm", "150,75")
    assert code == 0, report
    assert report["requested"] == {"bpm": [150.0, 75.0], "downbeat": None, "meter": None, "segment_at": round(truth[64], 3)}
    assert [s["bar"] for s in report["tempo"]["segments"]] == [1, 17]
    code, kept = cli(*args)
    assert code == 0 and kept["cached"] and len(kept["tempo"]["segments"]) == 2
    code, fresh = cli(*args, "--refresh")
    assert code == 0 and len(fresh["tempo"]["segments"]) == 1
    assert fresh["requested"] == {"bpm": None, "downbeat": None, "meter": None, "segment_at": None}
    for bad, reason in [(("--bpm", "150,x"), "bpm is a tempo"), (("--bpm", "150,500"), "bpm must be"), (("--segment-at", "soon"), "Not a time")]:
        code, result = cli(*args, *bad)
        assert code == 1 and reason in result["error"], result


def test_a_map_is_continued_past_its_ends_at_its_end_segments_tempos(tmp_path):
    truth = song(tmp_path / "song.wav", 120.0, 32, late=step(64, 0.5, 0.4))
    grid = hits.Grid(beats.measure(tmp_path / "song.wav"))
    last = len(truth) - 1
    assert grid.seconds(-2) == pytest.approx(truth[0] - 1.0, abs=0.002)
    assert grid.seconds(last + 2) == pytest.approx(truth[-1] + 0.8, abs=0.002)
    assert grid.index(truth[-1] + 0.4) == pytest.approx(last + 1, abs=0.01)
    assert beats.last_bpm({"tempo": {"bpm": 90.0}}) == 90.0
