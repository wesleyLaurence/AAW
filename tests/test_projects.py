"""Projects from the command line: a folder as PROJECT, `daw projects`, and a
project saved under another name with `daw move` and `daw copy`.

A `daw host` process stands in for the app. No audio device is opened.
"""

import json
import random
import subprocess
import tempfile
import time

import pytest

from agent_daw.model import load
from helpers import cli
from song_fixtures import make_song


@pytest.fixture
def folders(monkeypatch, tmp_path):
    """A registry of hosts in a short directory, and a data folder of the test's own."""
    with tempfile.TemporaryDirectory(prefix="aaw", dir="/tmp") as d:
        monkeypatch.setenv("AAW_HOST_DIR", d)
        monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
        yield tmp_path / "data"


def run(binary, *args):
    """A command's exit status, its JSON result or error, and what else it printed to stderr."""
    out = subprocess.run([str(binary), *map(str, args)], capture_output=True, text=True)
    lines = [json.loads(line) for line in out.stderr.splitlines() if line.strip()]
    if out.returncode:
        return out.returncode, lines[-1], lines[:-1]
    return 0, json.loads(out.stdout), lines


def daw(binary, *args):
    code, result, _ = run(binary, *args)
    assert code == 0, result
    return result


def host(binary, path):
    process = subprocess.Popen([str(binary), "host", str(path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    deadline = time.time() + 30
    while not daw(binary, "status", path).get("host"):
        assert process.poll() is None, process.stderr.read()
        assert time.time() < deadline, "the host did not start"
        time.sleep(0.05)
    return process


def song_in(folder, seed):
    folder.mkdir()
    return make_song(random.Random(seed), folder).resolve()


def test_a_folder_names_its_project(rust_daw, tmp_path, folders):
    folder = tmp_path / "beat"
    path = song_in(folder, 4)
    inspected = daw(rust_daw, "inspect", path)
    assert daw(rust_daw, "inspect", folder) == inspected
    # Through the Python entry point, and for a command that Python runs.
    assert cli("inspect", folder) == (0, inspected)
    assert cli("check", folder) == cli("check", path)
    assert cli("timeline", folder) == cli("timeline", path)
    # An edit, a read by path and a render.
    track = load(path)["tracks"][0]["id"]
    daw(rust_daw, "set", folder, f"tracks.{track}.gain_db", "-5")
    assert daw(rust_daw, "get", folder, f"tracks.{track}.gain_db") == -5
    assert load(path)["tracks"][0]["gain_db"] == -5
    assert daw(rust_daw, "schedule", folder) == daw(rust_daw, "schedule", path)
    rendered = daw(rust_daw, "render", folder, "--output", tmp_path / "out")
    assert rendered["project_sha256"] == daw(rust_daw, "inspect", path)["project_sha256"]
    # A folder without a song says which file is missing.
    code, error, _ = run(rust_daw, "inspect", tmp_path)
    assert code == 1 and "song.yaml" in error["error"]


def test_projects_lists_what_is_open_and_what_the_app_knows(rust_daw, tmp_path, folders):
    assert daw(rust_daw, "projects") == {"projects": []}
    one = song_in(tmp_path / "one", 1)
    two = song_in(tmp_path / "two", 2)
    daw(rust_daw, "set", two, "session.title", "Second")
    hosts = [host(rust_daw, one), host(rust_daw, two)]
    try:
        daw(rust_daw, "set", one, "session.tempo", "97")
        rows = daw(rust_daw, "projects")["projects"]
        assert [(r["project"], r["title"], r["revision"]) for r in rows] == [
            (str(one.parent), "Untitled", 1),
            (str(two.parent), "Second", 0),
        ]
        assert all(r["open"] and not r["front"] and not r["untitled"] and not r["playing"] for r in rows)
        assert {r["pid"] for r in rows} == {h.pid for h in hosts}
        # The path listed is one every command takes.
        assert daw(rust_daw, "status", rows[1]["project"])["title"] == "Second"

        # With --all, the projects in the app's index that no host has open.
        folders.mkdir()
        index = [
            {"path": str(one), "title": "Untitled", "untitled": False, "opened": 50.0},
            {"path": str(tmp_path / "gone" / "song.yaml"), "title": "Gone", "untitled": False, "opened": 20.0},
            {"path": str(tmp_path / "three" / "song.yaml"), "title": "Third", "untitled": True, "opened": 30.0},
        ]
        song_in(tmp_path / "three", 3)
        (folders / "projects.json").write_text(json.dumps({"projects": index}))
        assert daw(rust_daw, "projects") == {"projects": rows}
        assert daw(rust_daw, "projects", "--all")["projects"] == rows + [
            {"project": str(tmp_path / "three"), "title": "Third", "open": False, "untitled": True, "missing": False, "opened": 30.0},
            {"project": str(tmp_path / "gone"), "title": "Gone", "open": False, "untitled": False, "missing": True, "opened": 20.0},
        ]
    finally:
        for h in hosts:
            h.kill()


def test_a_moved_project_answers_at_its_old_path(rust_daw, tmp_path, folders):
    old = song_in(tmp_path / "a", 5)
    track = load(old)["tracks"][0]["id"]
    new = tmp_path.resolve() / "My Beat" / "song.yaml"
    process = host(rust_daw, old)
    try:
        daw(rust_daw, "set", old, f"tracks.{track}.gain_db", "-6")
        code, moved, said = run(rust_daw, "move", tmp_path / "a", tmp_path / "My Beat")
        assert code == 0 and said == []
        assert (moved["label"], moved["project"], moved["from"], moved["revision"]) == ("Saved as My Beat", str(new), str(old), 2)
        assert not old.parent.exists() and new.is_file()
        song = load(new)
        assert song["session"]["title"] == "My Beat" and song["tracks"][0]["gain_db"] == -6

        # A command for the old path, by its file or its folder, lands in the
        # project and says where that is now.
        for asked in [old, tmp_path / "a"]:
            code, result, said = run(rust_daw, "set", asked, f"tracks.{track}.pan", "0.25")
            assert code == 0 and result["project"] == str(new)
            assert said == [{"notice": f"{old.parent} is now at {new.parent}", "project": str(new)}]
        assert load(new)["tracks"][0]["pan"] == 0.25
        # A command for the new path says nothing more.
        code, result, said = run(rust_daw, "status", new.parent)
        assert code == 0 and said == [] and result["revision"] == 3
        # Undo is the edits made before and after the move, never the move.
        assert daw(rust_daw, "undo", new)["label"].startswith(f"Undo Set tracks.{track}.pan")
        assert daw(rust_daw, "undo", old)["label"].startswith(f"Undo Set tracks.{track}.gain_db")
        assert load(new)["session"]["title"] == "My Beat" and load(new)["tracks"][0]["gain_db"] != -6
        # A command that reads the files itself follows the project too.
        code, scheduled, said = run(rust_daw, "schedule", old)
        assert code == 0 and scheduled == daw(rust_daw, "schedule", new) and said[0]["project"] == str(new)

        listed = daw(rust_daw, "projects")["projects"]
        assert [(r["project"], r["title"]) for r in listed] == [(str(new.parent), "My Beat")]
        ops = [c["op"] for c in daw(rust_daw, "changes", new)["changes"]]
        assert ops == ["set", "project.move", "set", "undo", "undo"]

        # A name that is taken is refused, and nothing moves.
        code, error, _ = run(rust_daw, "move", new, tmp_path)
        assert code == 1 and "already there" in error["error"] and new.is_file()

        daw(rust_daw, "close", old)
        assert process.wait(timeout=30) == 0
        # With the host gone the old path names nothing.
        code, error, _ = run(rust_daw, "status", old)
        assert code == 1 and "No such file" in error["error"]
        assert daw(rust_daw, "projects") == {"projects": []}
    finally:
        if process.poll() is None:
            process.kill()


def test_a_copy_carries_the_host_on_and_leaves_the_original(rust_daw, tmp_path, folders):
    old = song_in(tmp_path / "a", 6)
    track = load(old)["tracks"][0]["id"]
    new = tmp_path.resolve() / "b" / "song.yaml"
    process = host(rust_daw, old)
    try:
        daw(rust_daw, "set", old, f"tracks.{track}.gain_db", "-6")
        before = old.read_text()
        copied = daw(rust_daw, "copy", old, new.parent)
        assert (copied["label"], copied["project"]) == ("Saved as b", str(new))
        assert old.read_text() == before
        assert sorted(p.name for p in new.parent.iterdir()) == sorted(p.name for p in old.parent.iterdir() if p.name != ".daw.lock")
        assert load(new)["session"]["title"] == "b"

        # A command for the original's path follows the host to the copy.
        code, result, said = run(rust_daw, "set", old, "session.tempo", "91")
        assert code == 0 and result["project"] == str(new) and said[0]["project"] == str(new)
        assert load(new)["session"]["tempo"] == 91 and old.read_text() == before
        # The original cannot be hosted while the copy answers for it.
        second = subprocess.run([str(rust_daw), "host", str(old)], capture_output=True, text=True, timeout=30)
        assert second.returncode == 1 and "was saved as" in second.stderr

        daw(rust_daw, "close", new)
        assert process.wait(timeout=30) == 0
        # With the copy closed, the path is the original's again.
        status = daw(rust_daw, "status", old)
        assert status["host"] is False and status["title"] == "Untitled"
    finally:
        if process.poll() is None:
            process.kill()


def test_move_and_copy_without_a_host(rust_daw, tmp_path, folders):
    made = daw(rust_daw, "init", tmp_path / "one", "--tempo", 100, "--bars", 8)
    daw(rust_daw, "track", "add", tmp_path / "one", "drums")
    moved = daw(rust_daw, "move", tmp_path / "one", tmp_path / "two")
    two = tmp_path.resolve() / "two" / "song.yaml"
    assert (moved["label"], moved["project"], moved["from"]) == ("Saved as two", str(two), made["project"])
    assert not (tmp_path / "one").exists()
    copied = daw(rust_daw, "copy", two, tmp_path / "deep" / "three")
    three = tmp_path / "deep" / "three" / "song.yaml"
    assert copied["project"] == str(three.resolve())
    assert [load(p)["session"]["title"] for p in (two, three)] == ["two", "three"]
    assert load(three)["tracks"][0]["id"] == "drums" and load(three)["session"]["tempo"] == 100
    code, error, _ = run(rust_daw, "copy", two, three.parent)
    assert code == 1 and "already there" in error["error"]
    code, error, _ = run(rust_daw, "move", tmp_path / "one", tmp_path / "four")
    assert code == 1 and "No such file" in error["error"]
