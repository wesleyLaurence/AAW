"""A song whose tempo steps, as the Python tools read it: one tempo map from
the Rust core, the timeline and a render through it, and `daw tempo`."""

from fractions import Fraction

import numpy as np
import soundfile as sf

from agent_daw import timeline
from agent_daw.model import TempoMap, frame, save, validate
from helpers import SR, daw, drum_track, render

LANE = {"automation": [{"param": "tempo", "points": [
    {"at": 0, "value": 120.0, "curve": "hold"},
    {"at": 64, "value": 150.0, "curve": "hold"},
]}]}


def song(tmp_path, master=None):
    click = np.zeros((64, 2), dtype=np.float32)
    click[0] = 0.5
    sf.write(tmp_path / "click.wav", click, SR, subtype="FLOAT")
    project = {
        "session": {"tempo": 120, "sample_rate": SR, "length_beats": 96, "end_fade_ms": 0},
        "samples": {"click": {"path": "click.wav"}},
        "tracks": [drum_track("drums", "c", "click", 96, list(range(96)))],
    }
    project["tracks"][0]["instrument"]["sampler"]["pads"]["c"]["attack_ms"] = 0
    if master:
        project["master"] = master
    path = tmp_path / "song.yaml"
    save(validate(project), path)
    return path


def test_the_map_is_the_cores_and_one_tempo_is_today_s_arithmetic():
    one = TempoMap.of(validate({"session": {"tempo": 143.987}}))
    assert one.tempo == 143.987
    for b in ["0", "1/3", "17", "1023/7"]:
        assert one.frame(b, SR) == frame(b, 143.987, SR)
        x = float(Fraction(b))
        assert one.seconds(x) == x * 60 / 143.987
        assert one.beat(x) == x * 143.987 / 60
    stepped = TempoMap.of(validate({"session": {"tempo": 120}, "master": LANE}))
    assert stepped.tempo is None and stepped.opening == 120.0
    assert [(float(b), bpm, s) for b, bpm, s in stepped.steps] == [(0.0, 120.0, 0.0), (64.0, 150.0, 32.0)]
    assert stepped.frame(64, SR) == 32 * SR
    assert stepped.frame(65, SR) == 32 * SR + 19200
    assert stepped.seconds(66) == 32.8
    assert abs(stepped.beat(32.8) - 66) < 1e-12
    assert stepped.tempo_at(63.9) == 120.0 and stepped.tempo_at(64) == 150.0


def test_a_render_plays_each_hit_on_the_frame_of_its_beat(tmp_path):
    path = song(tmp_path, LANE)
    render(path, tmp_path / "render")
    mix, _ = sf.read(tmp_path / "render" / "mix.wav", always_2d=True)
    loud = np.flatnonzero(np.abs(mix[:, 0]) > 1e-6)
    onsets = [int(i) for k, i in enumerate(loud) if k == 0 or i - loud[k - 1] > 200]
    stepped = TempoMap.of(validate({"session": {"tempo": 120}, "master": LANE}))
    assert onsets == [stepped.frame(b, SR) for b in range(96)]
    assert len(mix) == stepped.frame(96, SR)


def test_the_timeline_reads_seconds_through_the_steps(tmp_path):
    path = song(tmp_path, LANE)
    report = timeline.report(path, seconds=["33"], beats=["70"])
    assert report["tempo_steps"] == [{"at": 0.0, "bpm": 120.0, "seconds": 0.0}, {"at": 64.0, "bpm": 150.0, "seconds": 32.0}]
    assert report["seconds"][0]["beats"] == 66.5
    assert report["beats"][0]["seconds"] == 34.4
    assert report["beats"][0]["bar"] == 18


def test_daw_tempo_sets_lists_and_clears_the_steps(tmp_path):
    path = song(tmp_path)
    daw("tempo", "set", path, "--at", "17", "--bpm", "150")
    listed = daw("tempo", "list", path)
    assert [(s["bar"], s["bpm"]) for s in listed["steps"]] == [(1, 120.0), (17, 150.0)]
    assert listed["steps"][1]["seconds"] == 32.0
    assert path.read_text().startswith("schema_version: 3\n")
    # A step off a bar line is placed, and daw check says so.
    daw("tempo", "set", path, "--at", "19:3", "--bpm", "140")
    codes = [w["code"] for w in daw("check", path)["warnings"]]
    assert "tempo-step-off-bar" in codes
    daw("tempo", "remove", path, "--at", "19:3")
    daw("tempo", "set", path, "--at", "1", "--bpm", "100")
    assert daw("get", path, "session.tempo") == 100.0
    daw("tempo", "clear", path)
    assert not daw("tempo", "list", path)["changes"]
    assert path.read_text().startswith("schema_version: 2\n")
