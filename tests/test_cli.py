"""One `daw`: the Rust binary and the Python entry point pass commands to each other."""

import numpy as np
import pytest
import soundfile as sf

from agent_daw.model import load, save
from helpers import cli, daw


def test_init_and_describe_through_either_entry_point(tmp_path):
    code, made = cli("init", tmp_path / "new", "--tempo", 120)
    assert code == 0, made
    path = tmp_path / "new" / "song.yaml"
    notes = path.with_name("SONG.md")
    assert made == {"notes": str(notes.resolve()), "project": str(path.resolve())}
    assert path.read_text() == "schema_version: 1\nsession:\n  tempo: 120.0\n  length_beats: 4\n"
    # The song's notes start as four headings, and the project's AGENTS.md imports them.
    assert notes.read_text() == "## Vision\n\n## Instructions\n\n## Direction\n\n## Decisions\n"
    assert path.with_name("AGENTS.md").read_text().splitlines()[-1] == "@SONG.md"
    assert sorted(p.name for p in path.parent.iterdir()) == ["AGENTS.md", "SONG.md", "song.yaml"]
    code, again = cli("init", tmp_path / "new")
    assert code == 1 and "already exists" in again["error"]
    # Notes that are already in the folder are kept.
    (tmp_path / "kept").mkdir()
    (tmp_path / "kept" / "SONG.md").write_text("## Vision\n\nA waltz.\n")
    daw("init", tmp_path / "kept")
    assert (tmp_path / "kept" / "SONG.md").read_text() == "## Vision\n\nA waltz.\n"
    assert (tmp_path / "kept" / "AGENTS.md").exists()
    notes = daw("describe", "project")["semantics"]["notes"]
    assert all(word in notes for word in ["SONG.md", "Vision", "Instructions", "Direction", "Decisions", "no history"])
    assert "SONG.md" in daw("describe", "start")["steps"][0]["does"]
    assert daw("init", tmp_path / "other")["project"] == str((tmp_path / "other" / "song.yaml").resolve())
    assert load(tmp_path / "other" / "song.yaml")["session"]["length_beats"] == 4, "an empty song is one bar"

    for topic in ["start", "project", "sampler", "synth", "midi", "effects", "automation", "edit", "beats", "joins", "export"]:
        described = daw("describe", topic)
        assert cli("describe", topic) == (0, described)
        assert described["semantics"]
    schema = daw("describe", "--schema")["schema"]
    assert schema == daw("describe", "project", "--schema")["schema"]
    assert schema["$defs"]["Track"]["properties"]["gain_db"]["maximum"] == 24
    assert daw("describe", "sampler", "--schema")["schema"]["pad"]["required"] == ["sample"]
    synth = daw("describe", "synth", "--schema")
    assert synth["schema"]["synth"]["required"] == ["oscillators"]
    assert [f["name"] for f in synth["fields"]["filter"]][:3] == ["enabled", "mode", "slope_db_per_octave"]
    assert synth["modulation"]["targets"]["filter.cutoff_hz"] == "octaves"
    assert synth["automatable"]["fields"]["filter"]["cutoff_hz"] == "log"
    assert "recipes" in synth["semantics"]


def test_python_commands_run_from_the_rust_binary(tmp_path):
    sf.write(tmp_path / "hit.wav", np.zeros(4800), 48000)
    song = {
        "session": {"length_beats": 4},
        "samples": {"hit": {"path": "hit.wav"}},
        "patterns": {"p": {"length_beats": 4, "steps": {"h": "x..."}, "grid": 1}},
        "tracks": [{"id": "drums", "pads": {"h": {"sample": "hit"}}, "clips": [{"pattern": "p"}]}],
    }
    path = tmp_path / "song.yaml"
    save(song, path)
    checked = daw("check", path)
    assert cli("check", path) == (0, checked)
    # check is inspect with what Python measures.
    inspected = daw("inspect", path)
    assert {k: checked[k] for k in inspected} == inspected
    assert checked["root_notes"] == {} and checked["warnings"] == []
    assert checked["tracks"][0]["events"] == 1

    db = tmp_path / "library.sqlite"
    assert daw("samples", "--db", db, "scan", tmp_path)["indexed"] == 1
    assert len(daw("samples", "--db", db, "search", "hit")) == 1
    listened = daw("listen", tmp_path / "hit.wav", "--no-images")
    assert listened["mix"]["silent"] is True
    # A Python command's own error comes back with its exit status.
    with pytest.raises(ValueError, match="limit must be"):
        daw("samples", "--db", db, "search", "--limit", 0)


def test_a_missing_engine_is_reported(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_DAW", str(tmp_path / "absent"))
    code, result = cli("inspect", tmp_path / "song.yaml")
    assert code == 1 and result["command"] == "inspect" and "absent" in result["error"]
    code, result = cli("check", tmp_path / "song.yaml")
    assert code == 1 and "error" in result
