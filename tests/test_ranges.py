"""Ranges of beats across the tracks through `daw`: a verse repeated, beats
inserted and deleted, and what `describe edit` and `--help` say of them."""

import json
import os
import subprocess

from helpers import daw


def run(*args):
    return subprocess.run([os.environ["AAW_DAW"], *map(str, args)], capture_output=True, text=True)


def test_ranges_edit_the_song_across_its_tracks(tmp_path):
    project = tmp_path / "song"
    daw("init", project, "--tempo", 120, "--bars", 8)
    daw("synth", "add", project, "bass", "--patch", "Sub Bass")
    daw(
        "clip", "add", project, "bass", "--id", "line", "--at", 0, "--length-beats", 16,
        "--notes", json.dumps([{"pitch": "C2", "duration": 6}, {"pitch": "G1", "at": 8, "duration": 4}]),
    )
    daw("section", "add", project, "verse", 0, 16)
    daw("lane", "point", "add", project, "tracks.bass", "gain_db", "--at", 0, "--value", -12)
    daw("lane", "point", "add", project, "tracks.bass", "gain_db", "--at", 16, "--value", 0)
    before = (project / "song.yaml").read_text()

    # Four beats opened at beat 4 cut the clip there and move the rest later.
    reply = daw("range", "insert", project, 4, 4)
    assert reply["label"] == "Insert 4 beats at 4"
    assert reply["split"] == ["tracks.bass.clips.line"]
    assert reply["length_beats"] == 36
    clips = daw("get", project, "tracks.bass.clips")
    assert [(c["id"], c.get("at", 0), c["length_beats"]) for c in clips] == [("line", 0, 4), ("clip1", 8, 12)]
    assert daw("get", project, "sections.verse")["length_beats"] == 20
    # Deleted again, the song is as it was, byte for byte.
    reply = daw("range", "delete", project, 4, 4)
    assert reply["label"] == "Delete beats 4–8"
    assert (project / "song.yaml").read_text() == before

    # The verse again after itself: its notes, lane and label copied.
    reply = daw("section", "duplicate", project, "verse")
    assert reply["label"] == "Duplicate section verse at 16 as verse-2"
    assert reply["sections"] == ["verse-2"]
    assert daw("get", project, "session.length_beats") == 48
    copy = daw("get", project, "tracks.bass.clips.clip1")
    assert (copy["at"], copy["length_beats"]) == (16, 16)
    assert [n["pitch"] for n in copy["notes"]] == [36, 31]
    points = [(p["at"], p["value"]) for p in daw("get", project, "tracks.bass.automation.0.points")]
    assert points == [(0, -12.0), (16, 0.0), (16, -12.0), (32, 0.0)]

    # A refused range leaves nothing behind.
    failed = run("range", "clear", project, 48, 4)
    assert failed.returncode == 1
    assert json.loads(failed.stderr)["error"] == "Beats 48–52 are past the song's end at 48"
    assert daw("get", project, "session.length_beats") == 48


def test_describe_and_help_say_what_ranges_do():
    semantics = daw("describe", "edit")["semantics"]
    assert "daw range copy SONG START LENGTH --to AT" in semantics["ranges"]
    assert "cut only between repeats" in semantics["edges"]
    assert "daw section duplicate" in semantics["sections"]
    help_text = run("range", "--help").stdout
    for verb in ["copy", "insert", "delete", "clear"]:
        assert f"\n  {verb} " in help_text, help_text
    assert "--track" in run("range", "copy", "--help").stdout
    assert "--with-content" in run("section", "move", "--help").stdout
