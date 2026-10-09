"""Markers through `daw`: the person's notes at beats, read with what plays
there, and what a running host adds: the playhead's beat and the news an
agent's next edit carries."""

import json
import os
import subprocess
import tempfile
import time

import pytest

from agent_daw.model import load
from helpers import cli, daw


def run(*args):
    return subprocess.run([os.environ["AAW_DAW"], *map(str, args)], capture_output=True, text=True)


@pytest.fixture
def registry(monkeypatch):
    # Unix socket paths are short; pytest's tmp_path is long on macOS.
    with tempfile.TemporaryDirectory(prefix="aaw", dir="/tmp") as d:
        monkeypatch.setenv("AAW_HOST_DIR", d)
        yield d


def song(tmp_path):
    project = tmp_path / "song"
    daw("init", project, "--tempo", 120, "--bars", 16)
    daw("synth", "add", project, "bass", "--patch", "Sub Bass")
    daw(
        "clip", "add", project, "bass", "--id", "line", "--at", 32, "--length-beats", 8,
        "--notes", json.dumps([{"pitch": "C2", "duration": 6}]),
    )
    daw("section", "add", project, "drop", 32, 16)
    return project


def test_markers_are_left_read_and_removed_from_a_terminal(tmp_path):
    project = song(tmp_path)
    assert daw("marker", "list", project) == {"markers": []}
    reply = daw("marker", "add", project, 34.5, "--text", "too busy")
    assert (reply["label"], reply["marker"], reply["path"]) == ("Add marker m1 in bar 9: too busy", "m1", "markers.m1")
    daw("marker", "add", project, 8)
    daw("marker", "add", project, "1/3", "--id", "top", "--text", "love this")

    # In time order, each with its bar, its section and what plays there.
    markers = daw("marker", "list", project)["markers"]
    assert [(m["id"], m["at"], m["bar"], m["beat"], m["section"], m["text"]) for m in markers] == [
        ("top", "1/3", 1, 1.33, None, "love this"),
        ("m2", 8, 3, 1.0, None, ""),
        ("m1", 34.5, 9, 3.5, "drop", "too busy"),
    ]
    assert markers[0]["playing"] == []
    assert markers[2]["playing"] == [{"track": "bass", "clip": "tracks.bass.clips.line", "what": "notes line, 8 beats, 1 note C2"}]

    # Python reads them with the song, and the map draws them.
    assert [m["id"] for m in load(project / "song.yaml")["markers"]] == ["top", "m2", "m1"]
    text = daw("map", project)["map"]
    row = next(line for line in text if line.startswith("markers"))
    assert row.split(None, 1)[1] == "! !     !"
    assert "!  m1 at bar 9, beat 3.5: too busy" in text
    # check has nothing to say of a marker, and a render has none in it.
    code, checked = cli("check", project)
    assert code == 0 and checked["valid"], checked

    assert daw("marker", "text", project, "m2", "louder here")["label"] == "Marker m2: louder here"
    assert daw("marker", "move", project, "m2", 12)["label"] == "Move marker m2 to bar 4: louder here"
    # Time opened before a marker moves it with what it marks.
    daw("range", "insert", project, 0, 4)
    assert daw("get", project, "markers.m1.at") == 38.5
    assert daw("marker", "remove", project, "m1", "top")["removed"] == ["top", "m1"]
    assert daw("marker", "remove", project, "--all")["label"] == "Remove marker m2"
    assert "markers" not in (project / "song.yaml").read_text()

    for args, said in [
        (("marker", "add", project), "needs a beat where no host is playing"),
        (("marker", "add", project, 500), "past the session's end"),
        (("marker", "move", project, "m7", 4), "m7 is not a marker; the song has none"),
        (("marker", "remove", project), "Name the markers to remove"),
    ]:
        failed = run(*args)
        assert failed.returncode == 1 and said in json.loads(failed.stderr)["error"], failed.stderr


def test_describe_and_help_say_what_markers_are():
    said = daw("describe", "project")
    note = said["semantics"]["markers"]
    for phrase in ["daw marker list", "M in the app", "a moment after what was heard", "The markers are the person's", "daw marker remove"]:
        assert phrase in note, phrase
    assert "markers[].at" in json.dumps(said["fields"])
    assert "markers[].text" in json.dumps(said["fields"])
    help_text = run("marker", "--help").stdout
    for verb in ["list", "add", "move", "text", "remove"]:
        assert f"\n  {verb}" in help_text, help_text
    assert "where a running host is" in run("marker", "add", "--help").stdout


def test_a_host_gives_a_marker_its_beat_and_tells_the_agent(tmp_path, registry):
    project = song(tmp_path)
    binary = os.environ["AAW_DAW"]
    host = subprocess.Popen([binary, "host", str(project)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.time() + 30
        while not daw("status", project).get("host"):
            assert host.poll() is None, host.stderr.read()
            assert time.time() < deadline, "the host did not start"
            time.sleep(0.05)
        daw("locate", project, 34)
        # The person's marker, as the app sends it: where the song is.
        reply = daw("--origin", "user", "marker", "add", project, "--text", "too busy")
        assert reply["label"] == "Add marker m1 in bar 9: too busy"
        assert daw("get", project, "markers.m1.at") == 34
        assert "new_markers" not in reply
        change = daw("changes", project, "--since", 0)["changes"][-1]
        assert (change["origin"], change["label"]) == ("user", "Add marker m1 in bar 9: too busy")
        # The agent's next edit says so, once.
        reply = daw("set", project, "tracks.bass.gain_db", -3)
        assert "m1 in bar 9, beat 3: too busy" in reply["new_markers"]
        assert "new_markers" not in daw("set", project, "tracks.bass.gain_db", -4)
        # It answers the marker and removes it in one step, which one undo takes back.
        batch = tmp_path / "answer.json"
        batch.write_text(json.dumps([
            {"op": "note.remove", "notes": ["tracks.bass.clips.line.notes.n1"]},
            {"op": "marker.remove", "markers": ["m1"]},
        ]))
        daw("batch", project, batch, "--label", "Thin the bass in bar 9 for marker m1")
        assert daw("marker", "list", project) == {"markers": []}
        daw("undo", project)
        assert [m["id"] for m in daw("marker", "list", project)["markers"]] == ["m1"]
        assert len(daw("get", project, "tracks.bass.clips.line.notes")) == 1
    finally:
        daw("close", project)
        host.wait(timeout=10)
