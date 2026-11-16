"""The beat map led by the downbeat model, against generated songs whose beats are
known and a stand-in model that hears what each test says it does."""

import json
import os
import shlex
import sys
from pathlib import Path

import numpy as np
import pytest

from agent_daw import beats, downbeats, hits, reference, separation
from helpers import beat_song as song, cli

STAND_IN = Path(__file__).with_name("stand_in_downbeats.py")


@pytest.fixture
def model(monkeypatch):
    monkeypatch.setenv("AAW_DOWNBEATS", shlex.join([sys.executable, str(STAND_IN)]))


def hears(path, beat_times, downbeat_times):
    Path(f"{path}.heard.json").write_text(
        json.dumps({"beats": [float(t) for t in beat_times], "downbeats": [float(t) for t in downbeat_times]})
    )


def starts(report):
    return [b["seconds"] for b in report["beats"] if b["beat"] == 1]


def test_the_model_places_the_bars_and_the_onsets_place_the_beats(tmp_path, model):
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    # The model's beats are a frame off; the onsets' are kept.
    hears(path, truth + 0.012, truth[::4] + 0.012)
    report = beats.measure(path)
    assert report["model"]["used"] and report["model"]["counted"] == "as heard"
    assert report["model"]["beats_from"] == "onsets" and report["model"]["agreement"] >= 0.95
    assert report["model"]["heard_bpm"] == pytest.approx(120, abs=0.5)
    found = np.array([b["seconds"] for b in report["beats"]])
    assert len(found) == len(truth) and np.abs(found - truth).max() < 0.002
    assert report["downbeat"]["set_by"] == "model" and report["downbeat"]["confidence"] > 0.9
    assert report["downbeat"]["uncertain_bars"] == [] and "candidates" not in report["downbeat"]
    assert report["odd_bars"] == [] and (report["meter"], report["meter_set_by"]) == ("4/4", "model")
    assert np.allclose(starts(report), truth[::4], atol=0.002)
    assert [b["index"] for b in report["beats"]] == list(range(len(truth)))


def test_a_bar_of_three_is_said_and_the_bars_after_it_counted_from_it(tmp_path, model):
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    # Bar 12 has three beats: the downbeats after it come a beat early.
    downs = [*truth[0:45:4], *truth[47::4]]
    hears(path, truth, downs)
    report = beats.measure(path)
    assert report["odd_bars"] == [{"bar": 12, "seconds": pytest.approx(truth[44], abs=0.002), "beats": 3}]
    rows = report["beats"]
    assert [(rows[i]["bar"], rows[i]["beat"]) for i in (44, 46, 47, 48)] == [(12, 1), (12, 3), (13, 1), (13, 2)]
    assert rows[47]["index"] == 47 and report["beats_per_bar"] == 4
    assert np.allclose(starts(report), downs, atol=0.002)


def test_a_stray_downbeat_is_passed_over_and_a_change_that_holds_is_followed(tmp_path, model):
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    hears(path, truth, sorted([*truth[::4], truth[22]]))
    assert beats.measure(path)["odd_bars"] == []
    # Two downbeats a bar from bar 3 to 6, as a model can in a riff intro:
    # the bars keep to the beats it heard most.
    hears(path, truth, sorted([*truth[::4], *truth[9:24:4]]))
    report = beats.measure(path)
    assert report["odd_bars"] == [] and np.allclose(starts(report), truth[::4], atol=0.002)


def test_a_half_time_count_is_doubled_where_the_beats_between_hold_attacks(tmp_path, model):
    path = tmp_path / "trap.wav"
    truth = song(path, bpm=140.0, bars=24)
    # The model counts it at 70, a bar of four of its beats, eight of the song's.
    hears(path, truth[::2], truth[::8])
    report = beats.measure(path)
    assert report["model"]["counted"] == "doubled" and report["model"]["heard_bpm"] == pytest.approx(70, abs=0.5)
    assert report["tempo"]["bpm"] == pytest.approx(140, abs=0.01) and 70 in [round(b) for b in report["tempo"]["ambiguous_with"]]
    assert report["beats_per_bar"] == 4 and report["odd_bars"] == []
    assert np.allclose(starts(report), truth[::4], atol=0.002)
    # Asked for, it is counted as the model heard it.
    slow = beats.measure(path, bpm=70)
    assert slow["model"]["counted"] == "as heard" and slow["tempo"]["bpm"] == pytest.approx(70, abs=0.01)
    assert np.allclose(starts(slow), truth[::8], atol=0.002)


def test_a_slow_count_whose_beats_between_are_quiet_is_kept(tmp_path, model):
    path = tmp_path / "slow.wav"
    truth = song(path, bpm=82.0, bars=24)
    hears(path, truth, truth[::4])
    report = beats.measure(path)
    assert report["model"]["counted"] == "as heard" and report["tempo"]["bpm"] == pytest.approx(82, abs=0.01)


def test_where_the_onsets_disagree_the_model_beats_are_moved_to_their_attacks(tmp_path, model):
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    rng = np.random.default_rng(3)
    heard = truth + rng.uniform(-0.02, 0.02, len(truth))
    moved = downbeats.refined(*beats.read(path), heard)
    assert np.abs(moved - truth).max() < 0.002
    # Beats a 4:3 count apart share few beats.
    assert downbeats.shared(truth, truth) == 1.0
    assert downbeats.shared(truth, truth[0] + np.arange(len(truth)) * 0.5 * 4 / 3) < 0.5


def test_downbeats_asked_for_hold_until_the_next(tmp_path, model):
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    hears(path, truth, truth[::4])
    # From beat 29 the bars run from there, the model's bars before.
    report = beats.measure(path, downbeat=[truth[29] + 0.05])
    assert report["downbeat"]["set_by"] == "request"
    assert report["odd_bars"] == [{"bar": 8, "seconds": pytest.approx(truth[28], abs=0.002), "beats": 1}]
    assert np.allclose(starts(report), [*truth[0:29:4], *truth[29::4]], atol=0.002)
    # Two are kept with the map, and one as a time.
    args = ("samples", "--db", tmp_path / "index.sqlite", "beats", path)
    code, kept = cli(*args, "--downbeat", "0:14.637", "--downbeat", "30.137")
    assert code == 0, kept
    assert kept["requested"]["downbeat"] == [14.637, 30.137]
    code, one = cli(*args, "--downbeat", "0:14.637")
    assert code == 0 and one["requested"]["downbeat"] == 14.637


def test_without_the_model_the_map_says_why_and_scores_one_downbeat(tmp_path, monkeypatch):
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    report = beats.measure(path)
    assert report["model"] == {"name": "beat-this-final0", "used": False, "note": "AAW_DOWNBEATS is off"}
    assert report["downbeat"]["set_by"] == "measurement" and report["downbeat"]["candidates"]
    assert report["meter_set_by"] == "assumed"
    # A model that fails is the same, with its words.
    monkeypatch.setenv("AAW_DOWNBEATS", shlex.join([sys.executable, str(STAND_IN)]))
    Path(f"{path}.heard.json").write_text(json.dumps({"fail": "out of memory"}))
    failed = beats.measure(path)
    assert not failed["model"]["used"] and "out of memory" in failed["model"]["note"]
    assert np.allclose(starts(failed), starts(report))
    # Two downbeats asked for without the model hold the same way.
    asked = beats.measure(path, downbeat=[truth[0], truth[29]])
    assert asked["odd_bars"] == [{"bar": 8, "seconds": pytest.approx(truth[28], abs=0.002), "beats": 1}]


def test_hits_and_a_reference_count_a_bar_of_three(tmp_path, model):
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    hears(path, truth, [*truth[0:45:4], *truth[47::4]])
    report = beats.measure(path)
    grid = hits.Grid(report)
    placed, _ = grid.place(truth[51] + 0.001)
    assert (placed["bar"], placed["sixteenth"]) == (14, "14.1.1")
    assert grid.bar_beat(-2) == (0, 3.0) and grid.bar_beat(len(truth) + 3)[0] == 26
    rows, per_bar = report["beats"], report["beats_per_bar"]
    assert reference.bar_at(rows, per_bar, 47.0) == 13 and reference.bar_at(rows, per_bar, 46.5) == 12
    assert reference.beat_seconds(rows, per_bar, 47) == pytest.approx(truth[47], abs=0.002)
    # A map made before beats had an index counts as it did.
    old = [{k: v for k, v in r.items() if k != "index"} for r in rows[:40]]
    assert reference.index_of(old[5], 4) == 5 and reference.bar_at(old, 4, 9.0) == 3


def test_the_bars_path_and_the_halves():
    p = np.full(20, 0.05)
    p[[0, 4, 8, 12, 16]] = 0.95
    assert list(downbeats.bars(p, 4)) == [0, 4, 8, 12, 16]
    assert list(downbeats.bars(p, 4, forced=[2])) == [2, 4, 8, 12, 16] or 2 in downbeats.bars(p, 4, forced=[2])
    assert list(downbeats.halves(np.array([3, 11]), 20, 4)) == [3, 7, 11, 15, 19]
    assert list(downbeats.halves(np.array([5, 13]), 20, 4)) == [1, 5, 9, 13, 17]
    assert list(downbeats.anchored(12, [2, 7], 4)) == [2, 6, 7, 11]
    assert list(downbeats.anchored(12, [2, 7], 4, back=False)) == [2, 6, 7, 11]
    assert list(downbeats.anchored(12, [6], 4)) == [2, 6, 10]
    assert downbeats.labels(6, [2, 5], 4) == [(0, 3, -2), (0, 4, -1), (1, 1, 0), (1, 2, 1), (1, 3, 2), (2, 1, 3)]
    times = np.array([1.0, 1.5, 2.0, 3.5, 4.0, 4.5])
    assert np.allclose(downbeats.filled(times), [1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5])
    logits = np.full(100, -6.0)
    logits[[10, 11]] = 3.0
    logits[50] = 4.0
    assert np.allclose(downbeats.peaks(logits, 50), [0.21, 1.0])


def test_setup_adds_what_an_older_environment_lacks(tmp_path, monkeypatch):
    monkeypatch.delenv("AAW_SEPARATOR", raising=False)
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path))
    python = separation.env_python()
    python.parent.mkdir(parents=True)
    python.write_text("")
    probes = iter([None, {"beat_this": "1.1.0", "torch": "2.14.1"}])
    ran = []
    monkeypatch.setattr(separation, "probe", lambda: next(probes))
    monkeypatch.setattr(separation, "find_uv", lambda: Path("/bin/uv"))
    monkeypatch.setattr(separation, "run_quietly", lambda command: ran.append([str(c) for c in command]))
    status = separation.install_runtime()
    assert status["added_now"] and status["versions"]["beat_this"] == "1.1.0"
    assert ran == [["/bin/uv", "pip", "install", "--python", str(python), "audioread", "beat-this==1.1.0", "torchaudio==2.11.1"]]
    assert json.loads(separation.runtime_file().read_text())["versions"]["beat_this"] == "1.1.0"
    # Once it imports them, nothing is installed.
    monkeypatch.setattr(separation, "probe", lambda: {"beat_this": "1.1.0"})
    assert not separation.install_runtime().get("added_now") and len(ran) == 1


def test_the_model_is_listed_and_unavailable_says_what_to_run(tmp_path, monkeypatch):
    monkeypatch.delenv("AAW_DOWNBEATS")
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path))
    (spec,) = separation.manifest()["analysis"]
    assert spec["name"] == downbeats.NAME and spec["license"].startswith("MIT")
    (file,) = spec["files"]
    assert file["url"].startswith("https://cloud.cp.jku.at/") and file["mirrors"][0].startswith("https://huggingface.co/")
    assert file["bytes"] == 81058141 and len(file["sha256"]) == 64
    listed = separation.listing()["analysis"]
    assert [(m["name"], m["mb"], m["here"]) for m in listed] == [("beat-this-final0", 81, False)]
    assert listed[0]["get"] == "daw separate setup --models beat-this-final0"
    assert "daw separate setup" in downbeats.unavailable()
    python = separation.env_python()
    python.parent.mkdir(parents=True)
    python.write_text("")
    assert "lacks Beat This!" in downbeats.unavailable()
    (python.parent.parent / "lib" / "python3.12" / "site-packages" / "beat_this").mkdir(parents=True)
    assert "--models beat-this-final0" in downbeats.unavailable()


def test_a_mirror_is_tried_when_the_authors_address_fails(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path))
    tried = []

    def download(url, target, expected=None):
        tried.append(url)
        if "cloud" in url:
            raise ValueError(f"Could not download {url}: HTTP Error 500")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(b"x")

    monkeypatch.setattr(separation, "download", download)
    (spec,) = separation.manifest()["analysis"]
    separation.fetch(spec["files"][0])
    assert tried == [spec["files"][0]["url"], spec["files"][0]["mirrors"][0]]


@pytest.mark.skipif(not os.environ.get("AAW_DOWNBEATS_REAL"), reason="AAW_DOWNBEATS_REAL runs the installed model")
def test_the_installed_model_hears_a_generated_song(tmp_path, monkeypatch):
    monkeypatch.delenv("AAW_DOWNBEATS")
    if downbeats.unavailable():
        pytest.skip(downbeats.unavailable())
    path = tmp_path / "song.wav"
    truth = song(path, bpm=120.0, bars=24)
    report = beats.measure(path)
    assert report["model"]["used"], report["model"]
    assert report["tempo"]["bpm"] == pytest.approx(120, abs=0.05)
    assert np.allclose(starts(report), truth[::4], atol=0.002)
