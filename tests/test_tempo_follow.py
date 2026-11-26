"""`daw tempo follow`: the grid laid on a generated song played live, with an
intro, a step and a band's drift, so its render's downbeats are on the bars and
a steady song stretched over it follows them; and `--keep-time`, which moves a
song built at one tempo onto the new grid without changing what it sounds
like."""

from fractions import Fraction

import numpy as np
import pytest
import soundfile as sf

from agent_daw import beats
from agent_daw.model import TempoMap, load, save, validate
from helpers import beat_song, daw, drum_track, render

LIVE_BARS = 32


def live(path):
    """A band at 120 that steps to 150 at bar 17, its tempo wandering 2% over
    eight bars and each beat off it by up to 10 ms, as the beat map's own
    tests drift, after a two-beat pickup that starts 1.1 s in."""
    count = LIVE_BARS * 4 + 2
    k = np.arange(count)
    period = np.where(k < 66, 0.5, 0.4)
    step = np.where(k >= 66, (k - 66) * (0.4 - 0.5), 0.0)
    wander = np.cumsum(period * 0.01 * np.sin(2 * np.pi * k / 64))
    late = step + wander + np.random.default_rng(5).uniform(-0.010, 0.010, count)
    return beat_song(path, 120.0, LIVE_BARS, first=1.1, pickup=2, late=lambda i: late[i])


def downbeats(path):
    report = beats.measure(path)
    return np.array([b["seconds"] for b in report["beats"] if b["beat"] == 1 and b["bar"] >= 1])


def song(tmp_path, tracks, tempo=120.0, samples=("live",)):
    project = {
        "session": {"tempo": tempo, "length_beats": 140, "end_fade_ms": 0},
        "samples": {name: {"path": f"{name}.wav"} for name in samples},
        "tracks": tracks,
    }
    path = tmp_path / "song.yaml"
    save(validate(project), path)
    return path


def test_a_song_played_live_plays_on_the_bars_and_a_steady_one_follows_it(tmp_path):
    truth = live(tmp_path / "live.wav")
    beat_song(tmp_path / "vocal.wav", 154.0, 8, first=0.3)
    path = song(tmp_path, [{"id": "live", "audio": [{"sample": "live", "at": 0}]}], samples=("live", "vocal"))
    reply = daw("tempo", "follow", path, "tracks.live.audio.0")
    # The first downbeat is 2.1 s in, 4.2 beats at 120: bar 3 is the first
    # its lead-in fits before.
    assert reply["bar"] == 3 and reply["by"] == "bar", reply
    assert reply["first_beat_seconds"] == pytest.approx(truth[2], abs=0.005)
    assert [s["bpm"] for s in reply["segments"]] == [pytest.approx(120, rel=0.01), pytest.approx(150, rel=0.01)]
    assert reply["segments"][1]["bar"] == 19
    assert reply["largest_off_ms"] < 1

    # The studio song from bar 21, at its own 154 against the band's 150.
    vocal = {"sample": "vocal", "at": 80, "source_start_seconds": 0.3, "source_bpm": 154, "stretch": "preserve_pitch"}
    data = load(path)
    data["tracks"].append({"id": "vocal", "audio": [vocal]})
    save(validate(data), path)

    render(path, tmp_path / "render")
    lane = TempoMap.of(load(path))
    bars = lambda first, n: np.array([lane.seconds(4 * (first - 1 + i)) for i in range(n)])
    heard = downbeats(tmp_path / "render" / "stems" / "live.wav")
    assert len(heard) == LIVE_BARS
    assert np.abs(heard - bars(3, LIVE_BARS)).max() < 0.005
    sung = downbeats(tmp_path / "render" / "stems" / "vocal.wav")
    assert len(sung) >= 7
    assert np.abs(sung[:7] - bars(21, 7)).max() < 0.010


def test_a_step_a_segment_and_one_a_beat(tmp_path):
    live(tmp_path / "live.wav")
    path = song(tmp_path, [{"id": "live", "audio": [{"sample": "live", "at": 0}]}])
    reply = daw("tempo", "follow", path, "tracks.live.audio.0", "--by", "segment", "--bar", "3")
    assert reply["bar"] == 3 and reply["by"] == "segment"
    # The tempo before the pickup, then each segment's from its first bar,
    # each the length of its bars in the file.
    assert [s["bar"] for s in reply["steps"]] == [1, 3, 19]
    assert reply["largest_off_ms"] < 1
    by_beat = daw("tempo", "follow", path, "tracks.live.audio.0", "--by", "beat")
    assert len(by_beat["steps"]) > LIVE_BARS
    assert by_beat["largest_off_ms"] < 1


def test_keep_time_moves_a_song_built_at_one_tempo_onto_the_grid(tmp_path):
    live(tmp_path / "live.wav")
    click = np.zeros((64, 2), dtype=np.float32)
    click[0] = 0.5
    sf.write(tmp_path / "click.wav", click, 48000, subtype="FLOAT")
    hits = drum_track("clicks", "c", "click", 64, [])
    hits["clips"] = [{"id": f"c{i}", "at": at, "length_beats": 1, "notes": [{"pitch": 36, "duration": "1/4"}]} for i, at in enumerate([3, 17.5, 40])]
    hits["instrument"]["sampler"]["pads"]["c"]["attack_ms"] = 0
    path = song(tmp_path, [{"id": "live", "audio": [{"sample": "live", "at": 2}]}, hits], samples=("live", "click"))
    data = load(path)
    data["markers"] = [{"id": "m1", "at": 33, "text": "here"}]
    save(validate(data), path)

    render(path, tmp_path / "before")
    reply = daw("tempo", "follow", path, "tracks.live.audio.0", "--keep-time")
    assert reply["moved"] >= 4, reply
    render(path, tmp_path / "after")

    def onsets(directory, name):
        x, _ = sf.read(directory / "stems" / f"{name}.wav", always_2d=True)
        loud = np.flatnonzero(np.abs(x[:, 0]) > 1e-4)
        return np.array([i for k, i in enumerate(loud) if k == 0 or i - loud[k - 1] > 2000])

    # The clicks sound where they did, within a frame of the rounding.
    before, after = onsets(tmp_path / "before", "clicks"), onsets(tmp_path / "after", "clicks")
    assert len(before) == len(after) == 3 and np.abs(before - after).max() <= 1
    # So does the live song, its start within a millisecond.
    a, _ = sf.read(tmp_path / "before" / "stems" / "live.wav")
    b, _ = sf.read(tmp_path / "after" / "stems" / "live.wav")
    # From 3 s to 5 s, searched 0.1 s either way, found within a millisecond.
    lag = np.argmax(np.correlate(b[144000 - 4800 : 240000 + 4800, 0], a[144000:240000, 0], "valid")) - 4800
    assert abs(lag) <= 48
    # And the grid is under it: its first downbeat on the bar the reply gives.
    lane = TempoMap.of(load(path))
    at = float(Fraction(str(load(path)["tracks"][0]["audio"][0]["at"])))
    first = lane.seconds(at) + reply["first_beat_seconds"]
    assert lane.seconds(4 * (reply["bar"] - 1)) == pytest.approx(first, abs=0.001)
    marker = load(path)["markers"][0]["at"]
    assert lane.seconds(float(Fraction(str(marker)))) == pytest.approx(33 * 60 / 120, abs=1e-4)
