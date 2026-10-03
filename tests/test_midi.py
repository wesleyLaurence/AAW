"""MIDI tracks through `daw` as an agent drives it: a phrase made by commands
alone, a sampler attached and rendered, `check` naming silent notes, the
Python tools reading such a song, and a labeled batch through a running host."""

import json
import os
import subprocess
import tempfile
import time

import numpy as np
import pytest
import soundfile as sf

from agent_daw.model import load
from helpers import cli, daw, stem, tone

SR = 48000


@pytest.fixture
def registry(monkeypatch):
    # Unix socket paths are short; pytest's tmp_path is long on macOS.
    with tempfile.TemporaryDirectory(prefix="aaw", dir="/tmp") as d:
        monkeypatch.setenv("AAW_HOST_DIR", d)
        yield d


def phrase(tmp_path):
    """A song with an instrumentless MIDI track holding a chord and a melody."""
    song = tmp_path / "song"
    daw("init", song, "--tempo", 120, "--bars", 8)
    path = song / "song.yaml"
    daw("track", "add", path, "keys", "--type", "midi")
    made = daw("clip", "add", path, "keys", "--at", 4, "--length-beats", 4)
    assert made["path"] == "tracks.keys.clips.clip1"
    notes = [
        {"pitch": "C4", "duration": 1, "velocity": 96},
        {"pitch": 64, "duration": 1, "velocity": 80},
        {"pitch": 67, "duration": 1},
        {"pitch": 62, "at": 1.975, "duration": 0.5},
        {"pitch": 64, "at": 2.025, "duration": "1/3"},
        {"pitch": 69, "at": 3, "duration": 2},
    ]
    added = daw("note", "add", path, made["path"], "--notes", json.dumps(notes))
    assert added["paths"][-1] == "tracks.keys.clips.clip1.notes.n6"
    return path


def test_a_phrase_made_by_commands_is_read_back_by_python_and_by_daw(tmp_path):
    path = phrase(tmp_path)
    assert path.read_text().startswith("schema_version: 2\n")
    song = load(path)
    keys = song["tracks"][0]
    assert keys["type"] == "midi" and keys["instrument"] is None
    assert [n["pitch"] for n in keys["clips"][0]["notes"]] == [60, 64, 67, 62, 64, 69]
    listed = daw("note", "list", path, "keys", "--from", 5)
    notes = listed["clips"][0]["notes"]
    assert [(n["name"], n["song_at"]) for n in notes] == [("D4", 5.975), ("E4", 6.025), ("A4", 7)]
    inspected = daw("inspect", path)["tracks"][0]
    assert (inspected["type"], inspected["instrument"], inspected["events"]) == ("midi", None, 0)
    assert inspected["clips"][0]["notes"] == 6
    described = daw("describe", "midi")
    assert "MidiTrack" in json.dumps(described["schema"]["track"]) and described["semantics"]["notes"]


def test_a_sampler_plays_the_notes_and_check_names_the_ones_it_cannot(tmp_path):
    path = phrase(tmp_path)
    (path.parent / "samples").mkdir(exist_ok=True)
    sf.write(path.parent / "samples" / "a3.wav", tone(220.0, seconds=2.0), SR)
    daw("set", path, "samples.a3", json.dumps({"path": "samples/a3.wav", "root_note": "A3"}))
    sampler = {"sampler": {"pads": {"p": {"sample": "a3", "mode": "gate", "release_ms": 40}}, "map": [{"notes": [60, 67], "pad": "p", "pitched": True}]}}
    attached = daw("instrument", "set", path, "keys", json.dumps(sampler))
    assert attached["label"] == "Attach a sampler to keys"
    before = load(path)["tracks"][0]["clips"]
    out = tmp_path / "render"
    report = daw("render", path, "--output", out)
    assert report["tracks"]["keys"]["events"] == 5, "note 69 is not mapped"
    x = stem(out, "keys")
    beat = SR // 2
    assert np.abs(x[4 * beat : 5 * beat]).max() > 0.05
    assert np.abs(x[: 4 * beat]).max() == 0
    code, checked = cli("check", path)
    assert code == 0, checked
    assert "keys.clip1: the sampler maps no pad to notes 69 (A4), which are silent" in checked["warnings"]
    # The Python timeline reads a MIDI track's sounds and clips.
    code, timeline = cli("timeline", path)
    assert code == 0, timeline
    assert timeline["tracks"]["keys"]["hits"] == 5
    daw("instrument", "remove", path, "keys")
    assert load(path)["tracks"][0]["clips"] == before


def test_a_labeled_batch_through_a_host_is_one_undo_step(tmp_path, registry):
    path = phrase(tmp_path)
    host = subprocess.Popen([os.environ["AAW_DAW"], "host", str(path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.time() + 30
        while not daw("status", path).get("host"):
            assert host.poll() is None and time.time() < deadline, host.stderr.read()
            time.sleep(0.05)
        before = load(path)["tracks"][0]["clips"]
        sha = daw("status", path)["project_sha256"]
        batch = tmp_path / "vary.json"
        batch.write_text(json.dumps([
            {"op": "clip.duplicate", "clip": "tracks.keys.clips.clip1", "id": "answer"},
            {"op": "note.transpose", "notes": ["tracks.keys.clips.answer"], "by": 12},
            {"op": "note.set", "note": "tracks.keys.clips.answer.notes.n6", "velocity": 40},
        ]))
        done = daw("--expect", sha, "batch", path, batch, "--label", "Answer the phrase an octave up")
        assert done["label"] == "Answer the phrase an octave up"
        assert done["handles"][0].startswith("@")
        clips = load(path)["tracks"][0]["clips"]
        assert clips[0] == before[0] and clips[1]["id"] == "answer"
        assert clips[1]["notes"][0]["pitch"] == 72 and clips[1]["notes"][5]["velocity"] == 40
        with pytest.raises(ValueError, match="Stale project revision"):
            daw("--expect", sha, "batch", path, batch)
        daw("undo", path)
        assert load(path)["tracks"][0]["clips"] == before
    finally:
        daw("close", path)
        host.wait(timeout=30)
