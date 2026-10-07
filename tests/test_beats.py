"""The beat map of a whole song, against generated songs whose grid is known."""

import json
import shutil
import subprocess

import numpy as np
import pytest
import soundfile as sf

from agent_daw import beats
from agent_daw.library import DECODERS, scan
from helpers import beat_song as song, cli

RATE = 44100


def found(report):
    return np.array([b["seconds"] for b in report["beats"]])


@pytest.mark.parametrize(
    "bpm, bars, first, pickup, other",
    [(120.0, 24, 0.137, 0, 60.0), (93.7, 32, 0.412, 0, 187.4), (174.0, 48, 0.05, 3, 87.0)],
)
def test_a_steady_song_has_its_grid_to_the_millisecond(tmp_path, bpm, bars, first, pickup, other):
    truth = song(tmp_path / "song.wav", bpm, bars, first, pickup, seed=round(bpm))
    report = beats.measure(tmp_path / "song.wav")
    tempo = report["tempo"]
    assert tempo["bpm"] == pytest.approx(bpm, abs=0.002) and tempo["steady"]
    assert tempo["drift_ms"] < 0.5 and tempo["ambiguous_with"] == [other]
    # Every beat, none missing and none added, each within a tenth of a millisecond.
    times = found(report)
    assert len(times) == len(truth) == report["beat_count"]
    assert np.abs(times - truth).max() < 0.0001
    # Bars are counted from the first downbeat; what comes before it is bar 0.
    assert report["first_beat_seconds"] == pytest.approx(truth[0], abs=0.0001)
    assert report["first_downbeat_seconds"] == pytest.approx(truth[pickup], abs=0.0001)
    numbers = [(b["bar"], b["beat"]) for b in report["beats"]]
    assert numbers[pickup : pickup + 5] == [(1, 1), (1, 2), (1, 3), (1, 4), (2, 1)]
    assert all(bar == 0 for bar, _ in numbers[:pickup])
    # The downbeat is clear here, and the other three are given as its rivals.
    downbeat = report["downbeat"]
    assert downbeat["confidence"] > 0.9 and downbeat["set_by"] == "measurement"
    assert [c["chosen"] for c in downbeat["candidates"]] == [True, False, False, False]
    assert sum(c["confidence"] for c in downbeat["candidates"]) == pytest.approx(1, abs=0.01)
    # The layers that come in at bars 9 and 17 start phrases.
    assert {1, 9, 17} <= {p["bar"] for p in report["phrases"]}
    assert all(0 <= b["strength"] <= 1 for b in report["beats"])


def test_a_tempo_that_wanders_is_followed(tmp_path):
    # A quarter of a second early and late over the song: under one percent of tempo.
    truth = song(
        tmp_path / "song.wav", 110.0, 40, 0.2, late=lambda k: 0.12 * np.sin(2 * np.pi * k / 160)
    )
    report = beats.measure(tmp_path / "song.wav")
    tempo, times = report["tempo"], found(report)
    assert not tempo["steady"] and tempo["drift_ms"] > 100
    assert tempo["bpm_range"][0] < 110 < tempo["bpm_range"][1]
    assert len(times) == len(truth) and np.abs(times - truth).max() < 0.002
    assert report["first_downbeat_seconds"] == pytest.approx(truth[0], abs=0.002)


def test_a_request_settles_the_tempo_and_the_downbeat(tmp_path):
    truth = song(tmp_path / "song.wav")
    # At half the tempo every other beat is one.
    half = beats.measure(tmp_path / "song.wav", bpm=60)
    assert half["tempo"]["bpm"] == pytest.approx(60, abs=0.002)
    assert np.abs(found(half) - truth[::2]).max() < 0.0005
    # A downbeat is the beat nearest the time given.
    moved = beats.measure(tmp_path / "song.wav", downbeat=truth[6] + 0.1)
    assert moved["downbeat"]["set_by"] == "request"
    assert moved["first_downbeat_seconds"] == pytest.approx(truth[2], abs=0.0001)
    assert [(b["bar"], b["beat"]) for b in moved["beats"][:3]] == [(0, 3), (0, 4), (1, 1)]
    chosen = [c for c in moved["downbeat"]["candidates"] if c["chosen"]]
    assert len(chosen) == 1 and chosen[0]["confidence"] < 0.1


def test_what_cannot_have_a_beat_map_is_refused(tmp_path):
    sf.write(tmp_path / "silent.wav", np.zeros(RATE * 10), RATE)
    with pytest.raises(ValueError, match="Silent audio"):
        beats.measure(tmp_path / "silent.wav")
    sf.write(tmp_path / "short.wav", np.random.default_rng(0).standard_normal(RATE) * 0.1, RATE)
    with pytest.raises(ValueError, match="Too short"):
        beats.measure(tmp_path / "short.wav")


@pytest.mark.parametrize("text, value", [("41", 41), ("0:41", 41), ("1:20.5", 80.5), ("12:03", 723)])
def test_times_are_seconds_or_minutes_and_seconds(text, value):
    assert beats.seconds(text) == value


@pytest.mark.parametrize("text", ["", "abc", "1:75", "-3", "1:2:3x"])
def test_other_times_are_refused(text):
    with pytest.raises(ValueError, match="Not a time"):
        beats.seconds(text)


def test_the_command_keeps_the_map_beside_the_song(tmp_path):
    truth = song(tmp_path / "song.wav")
    args = ("samples", "--db", tmp_path / "index.sqlite", "beats", tmp_path / "song.wav")
    code, report = cli(*args)
    assert code == 0, report
    # A summary: every beat is in the map beside the file, or listed on asking.
    assert "beats" not in report and report["beat_count"] == len(truth)
    assert not report["cached"] and report["map"] == str(tmp_path.resolve() / "song.beats.json")
    saved = json.loads((tmp_path / "song.beats.json").read_text())
    assert "path" not in saved and len(saved["beats"]) == len(truth)
    code, again = cli(*args, "--all")
    assert code == 0 and again["cached"] and again["beats"] == saved["beats"]
    assert {k: again[k] for k in report if k != "cached"} == {k: report[k] for k in report if k != "cached"}

    # The beats around a time, each with how far it is from it.
    code, around = cli(*args, "--near", "0:09", "--window", 1.2)
    near = around["near"]
    assert code == 0 and [(b["bar"], b["beat"]) for b in near["beats"]] == [(5, 1), (5, 2), (5, 3), (5, 4), (6, 1)]
    assert near["nearest_beat"]["offset_seconds"] == pytest.approx(truth[18] - 9, abs=0.0001)
    assert (near["nearest_downbeat"]["bar"], near["nearest_phrase_start"]["bar"]) == (5, 9)
    assert "change" in near["nearest_downbeat"] and near["nearest_phrase_start"]["phrase_start"]

    # A correction is kept with the map, until it is measured again.
    code, moved = cli(*args, "--downbeat", "0:01.1")
    assert code == 0 and not moved["cached"]
    assert moved["first_downbeat_seconds"] == pytest.approx(truth[2], abs=0.0001)
    code, kept = cli(*args)
    assert kept["cached"] and kept["requested"] == {"bpm": None, "downbeat": 1.1, "meter": None}
    assert kept["first_downbeat_seconds"] == moved["first_downbeat_seconds"]
    code, slower = cli(*args, "--bpm", 60)
    assert slower["requested"] == {"bpm": 60.0, "downbeat": 1.1, "meter": None}
    assert slower["tempo"]["bpm"] == pytest.approx(60, abs=0.002)
    code, fresh = cli(*args, "--refresh")
    assert fresh["requested"] == {"bpm": None, "downbeat": None, "meter": None}
    assert {k: fresh[k] for k in report} == report

    # A changed file is measured again.
    song(tmp_path / "song.wav", bpm=100.0, bars=12)
    code, changed = cli(*args)
    assert not changed["cached"] and changed["tempo"]["bpm"] == pytest.approx(100, abs=0.002)

    for bad, reason in [(("--bpm", 20), "bpm must be"), (("--near", "soon"), "Not a time")]:
        code, result = cli(*args, *bad)
        assert code == 1 and reason in result["error"]


def test_nothing_is_written_beside_a_file_in_the_library(tmp_path):
    library = tmp_path / "library" / "pack"
    library.mkdir(parents=True)
    song(library / "loop.wav", bars=8)
    db = tmp_path / "index.sqlite"
    scan(tmp_path / "library", db)
    code, report = cli("samples", "--db", db, "beats", library / "loop.wav")
    assert code == 0 and report["map"] is None and not report["cached"]
    assert report["tempo"]["bpm"] == pytest.approx(120, abs=0.002)
    assert sorted(p.name for p in library.iterdir()) == ["loop.wav"]


def test_the_click_audition_marks_each_beat(tmp_path):
    truth = song(tmp_path / "song.wav")
    args = ("samples", "--db", tmp_path / "index.sqlite", "beats", tmp_path / "song.wav")
    code, report = cli(*args, "--click", tmp_path / "click.wav", "--near", "0:20", "--seconds", 8)
    assert code == 0, report
    click = report["click"]
    assert (click["from_seconds"], click["duration"]) == (16, 8)
    assert (click["beats"], click["downbeats"]) == (16, 4)
    # The song is there at a lower level, and each click starts on its beat.
    y, rate = sf.read(tmp_path / "click.wav")
    x, _ = sf.read(tmp_path / "song.wav", start=16 * RATE, frames=8 * RATE)
    added = y - x * 0.5 / np.abs(x).max()
    assert rate == RATE and len(y) == 8 * RATE
    starts = np.round((truth[(truth >= 16) & (truth < 24)] - 16) * RATE).astype(int)
    for start in starts:
        assert np.abs(added[start - 200 : start - 2]).max() < 1e-3
        assert np.abs(added[start : start + 200]).max() > 0.2
    # Without a time it starts just before the first downbeat.
    code, report = cli(*args, "--click", tmp_path / "start.wav", "--seconds", 5)
    assert report["click"]["from_seconds"] == 0 and report["click"]["beats"] == 10


def test_a_decoded_song_keeps_its_grid(tmp_path):
    truth = song(tmp_path / "song.wav")
    if shutil.which("afconvert"):
        command = ["afconvert", "-f", "m4af", "-d", "aac", "-b", "192000", "song.wav", "song.m4a"]
    elif shutil.which("ffmpeg"):
        command = ["ffmpeg", "-v", "error", "-i", "song.wav", "-c:a", "aac", "-b:a", "192k", "song.m4a"]
    else:
        pytest.skip("No AAC encoder here")
    assert any(shutil.which(name) for name in DECODERS)
    subprocess.run(command, cwd=tmp_path, check=True)
    project = tmp_path / "project" / "song.yaml"
    code, made = cli("init", project.parent, "--tempo", 120, "--bars", 8)
    db = tmp_path / "index.sqlite"
    code, asset = cli("samples", "--db", db, "import", tmp_path / "song.m4a", "--project", project, "--id", "song")
    assert code == 0, asset
    code, report = cli("samples", "--db", db, "beats", project.parent / asset["path"], "--all")
    assert code == 0, report
    assert report["tempo"]["bpm"] == pytest.approx(120, abs=0.005) and report["tempo"]["steady"]
    times = np.array([b["seconds"] for b in report["beats"]])
    assert len(times) == len(truth) and np.abs(times - truth).max() < 0.001
    # The map is in the project, beside the decoded copy.
    assert report["map"] == str((project.parent / asset["path"]).resolve().with_suffix("")) + ".beats.json"


def test_a_meter_counts_the_bars_and_is_kept_with_the_map(tmp_path):
    # The song's accents are in four, so the meter is a counting of the map's
    # beats, not a measurement: three a bar from the chosen downbeat.
    truth = song(tmp_path / "song.wav", bars=12)
    report = beats.measure(tmp_path / "song.wav", downbeat=truth[0], meter="3/4")
    assert (report["meter"], report["beats_per_bar"]) == ("3/4", 3)
    assert [(b["bar"], b["beat"]) for b in report["beats"][:7]] == [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2), (2, 3), (3, 1)]
    assert len(report["downbeat"]["candidates"]) == 3
    assert report["assumes"].startswith("3 beats a bar")
    with pytest.raises(ValueError):
        beats.measure(tmp_path / "song.wav", meter="waltz")
    # Through the command the meter is a correction, kept until measured again.
    args = ("samples", "--db", tmp_path / "index.sqlite", "beats", tmp_path / "song.wav")
    code, report = cli(*args, "--meter", "3/4", "--all")
    assert code == 0, report
    assert report["beats_per_bar"] == 3 and report["requested"]["meter"] == "3/4"
    code, again = cli(*args)
    assert code == 0 and again["cached"] and again["beats_per_bar"] == 3
    code, fresh = cli(*args, "--refresh")
    assert code == 0 and fresh["beats_per_bar"] == 4 and fresh["meter"] == "4/4"
    code, err = cli(*args, "--meter", "x")
    assert code != 0
