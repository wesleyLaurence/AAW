"""The Rust session host and its commands, against the Python model and CLI.

A `daw host` process is started for a generated song and driven the way an agent
drives it: set, undo, changes, an external edit and close. No audio device is
opened; `daw play` is exercised by hand. Skipped without a Rust toolchain.
"""

import json
import os
import random
import subprocess
import tempfile
import time

import pytest
import yaml

from agent_daw.cli import execute, parser
from agent_daw.model import Project, load, project_hash, save
from test_rust_engine_parity import make_song
from test_rust_model_parity import rust_daw  # noqa: F401  (session fixture)


@pytest.fixture
def registry(monkeypatch):
    # Unix socket paths are short; pytest's tmp_path is long on macOS.
    with tempfile.TemporaryDirectory(prefix="aaw", dir="/tmp") as d:
        monkeypatch.setenv("AAW_HOST_DIR", d)
        yield d


def daw(binary, *args, check=True):
    out = subprocess.run([str(binary), *map(str, args)], capture_output=True, text=True)
    if check and out.returncode:
        raise AssertionError(out.stderr)
    return json.loads(out.stdout) if out.returncode == 0 else json.loads(out.stderr)


def song_with_routing(rng, directory):
    """A generated song plus a return, a send, effects and a lane."""
    path = make_song(rng, directory)
    song = yaml.safe_load(path.read_text())
    song["returns"] = [{"id": "plate", "effects": [{"type": "reverb", "mix_percent": 30}]}]
    first = song["tracks"][0]
    first["sends"] = [{"to": "plate", "gain_db": -9}]
    first["effects"] = [{"type": "filter", "mode": "lowpass", "cutoff_hz": 900, "id": "tone"}]
    first["automation"] = [{"param": "effects.tone.cutoff_hz", "points": [{"at": 0, "value": 300}, {"at": 4, "value": 3000}]}]
    song["master"] = {"effects": [{"type": "limiter"}]}
    path.write_text(yaml.safe_dump(song))
    return path


def python(argv):
    return json.loads(json.dumps(execute(parser().parse_args([str(a) for a in argv]))))


def test_inspect_matches_python(rust_daw, tmp_path, registry):
    rng = random.Random(21)
    for i in range(6):
        directory = tmp_path / f"song{i}"
        directory.mkdir()
        path = song_with_routing(rng, directory) if i % 2 else make_song(rng, directory)
        rust = daw(rust_daw, "inspect", path)
        for track in rust["tracks"]:
            clips = track.pop("clips")
            assert [c["ref"] for c in clips] == [f"tracks.{track['id']}.clips.{n}" for n in range(len(clips))]
        assert rust == python(["inspect", path])


def python_edit(path, change):
    """The song after `change` edits its full dump, as Python would save it."""
    data = load(path).model_dump(mode="json")
    change(data)
    copy = path.with_name("expected.yaml")
    save(Project.model_validate(data), copy)
    return copy.read_text()


def test_commands_write_what_python_would(rust_daw, tmp_path, registry):
    path = song_with_routing(random.Random(3), tmp_path)
    daw(rust_daw, "fmt", path)
    track = load(path).tracks[0].id
    pattern = next(iter(load(path).patterns))
    cases = [
        (["set", path, f"tracks.{track}.gain_db", "-4.5"], lambda d: d["tracks"][0].update(gain_db=-4.5)),
        (
            ["clip", "add", path, track, pattern, "--at", "1/3"],
            lambda d: d["tracks"][0]["clips"].append({"pattern": pattern, "at": "1/3"}),
        ),
        (
            ["effect", "add", path, "returns.plate", "--type", "delay", "--time-beats", "3/4", "--index", "0"],
            lambda d: d["returns"][0]["effects"].insert(0, {"type": "delay", "time_beats": "3/4"}),
        ),
        (
            ["section", "add", path, "tail", "4", "2"],
            lambda d: d["sections"].append({"id": "tail", "at": 4, "length_beats": 2}),
        ),
        (["toggle", path, f"tracks.{track}.mute"], lambda d: d["tracks"][0].update(mute=not d["tracks"][0]["mute"])),
        (
            ["send", "set", path, track, "plate", "--pre-fader"],
            lambda d: d["tracks"][0]["sends"][0].update(pre_fader=True),
        ),
        (["return", "add", path, "room"], lambda d: d["returns"].append({"id": "room"})),
        (["return", "move", path, "room", "0"], lambda d: d["returns"].insert(0, d["returns"].pop())),
    ]
    for argv, change in cases:
        expected = python_edit(path, change)
        result = daw(rust_daw, *argv)
        assert result["project_sha256"] == project_hash(load(path)), argv
        assert path.read_text() == expected, argv


def wait_for_host(binary, path, process):
    deadline = time.time() + 30
    while time.time() < deadline:
        status = daw(binary, "status", path)
        if status.get("host"):
            return status
        assert process.poll() is None, process.stderr.read()
        time.sleep(0.05)
    raise AssertionError("the host did not start")


def test_a_host_takes_commands_from_other_processes(rust_daw, tmp_path, registry):
    path = song_with_routing(random.Random(8), tmp_path)
    daw(rust_daw, "fmt", path)
    original = path.read_text()
    track = load(path).tracks[0].id
    host = subprocess.Popen([str(rust_daw), "host", str(path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        status = wait_for_host(rust_daw, path, host)
        assert status["revision"] == 0 and status["playing"] is False

        # The exit test's edit, without the audio: the file updates at once.
        expected = python_edit(path, lambda d: d["tracks"][0].update(gain_db=-6.0))
        result = daw(rust_daw, "set", path, f"tracks.{track}.gain_db", "-6")
        assert result["revision"] == 1
        assert path.read_text() == expected
        # Python reads the host's song from the file.
        assert python(["inspect", path])["project_sha256"] == result["project_sha256"]

        undone = daw(rust_daw, "undo", path)
        assert undone["revision"] == 2
        assert path.read_text() == original

        # Handles address clips while the host runs.
        clip = daw(rust_daw, "inspect", path)["tracks"][0]["clips"][0]["ref"]
        assert clip.startswith("@")
        moved = daw(rust_daw, "--origin", "user", "clip", "move", path, clip, "--at", "0")
        assert moved["revision"] in (2, 3)

        # A Python writer is an external edit, picked up as one undoable step.
        sha = daw(rust_daw, "status", path)["project_sha256"]
        patch = tmp_path / "patch.json"
        patch.write_text(json.dumps({"session": {"title": "External"}}))
        python(["apply", path, patch, "--expect", sha])
        deadline = time.time() + 5
        while daw(rust_daw, "status", path)["project_sha256"] == sha:
            assert time.time() < deadline, "the external edit was not loaded"
            time.sleep(0.05)

        changes = daw(rust_daw, "changes", path, "--since", "0")["changes"]
        rows = [(c["op"], c["origin"]) for c in changes]
        assert rows[:2] == [("set", "agent"), ("undo", "agent")]
        assert rows[-1] == ("reload", "external")
        before = yaml.safe_load(original)["tracks"][0].get("gain_db", 0.0)
        assert changes[0]["label"] == f"Set tracks.{track}.gain_db: {json.dumps(float(before))} → -6"

        # A second host for the same song is refused.
        second = subprocess.run([str(rust_daw), "host", str(path)], capture_output=True, text=True, timeout=30)
        assert second.returncode == 1 and "already open" in second.stderr

        closed = daw(rust_daw, "close", path)
        assert closed["closed"] is True
        out, err = host.communicate(timeout=30)
        assert host.returncode == 0, err
        summary = json.loads(out)
        assert summary["revision"] == closed["revision"]
        # Every change was printed to stderr as it landed.
        feed = [json.loads(line) for line in err.splitlines()]
        assert [x["change"]["revision"] for x in feed if "change" in x] == list(range(1, summary["revision"] + 1))
        # Commands run headless again.
        assert daw(rust_daw, "status", path)["host"] is False
        assert "undo history exists only while a host runs" in daw(rust_daw, "undo", path, check=False)["error"]
    finally:
        if host.poll() is None:
            host.kill()
