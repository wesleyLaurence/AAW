"""Learning `daw` quickly: short topics, a first song, help that lists what a
command takes, and errors that say what to do next."""

import json
import os
import shlex
import subprocess

import numpy as np
import soundfile as sf

from agent_daw.model import save
from helpers import SR, cli, daw

TOPICS = ["start", "project", "sampler", "synth", "midi", "effects", "automation", "edit", "check", "beats", "joins", "export"]


def run(*args, cwd=None):
    return subprocess.run([os.environ["AAW_DAW"], *map(str, args)], capture_output=True, text=True, cwd=cwd)


def test_describe_is_short_and_keeps_the_schema_for_the_asking():
    assert list(daw("describe")["topics"]) == TOPICS
    for topic in TOPICS:
        short, full = daw("describe", topic), daw("describe", topic, "--schema")
        # Everything the semantics said is still said.
        assert short["semantics"] == full["semantics"], topic
        assert "schema" not in short
    midi = run("describe", "midi").stdout
    assert len(midi.encode()) < 8000
    fields = json.loads(midi)["fields"]
    assert fields["tracks[].clips[].notes[].velocity"] == "integer 1..127, default 100"
    assert fields["tracks[].clips[].length_beats"] == "beats, required"
    assert fields["tracks[].instrument.synth"] == "Synth (daw describe synth) or null, default null"
    assert daw("describe", "project")["fields"]["session.tempo"] == "number 20..400, default 144"
    # The Synth's fields are listed by their path in the patch, as synth set takes them.
    synth = daw("describe", "synth")
    assert synth["fields"]["filter.cutoff_hz"] == "number 10..20000, default 20000"
    assert synth["modulation"]["targets"]["filter.cutoff_hz"] == "octaves"
    assert daw("describe", "effects")["fields"]["limiter.ceiling_db"] == "number -24..-0.1, default -1"


def test_a_closed_pipe_ends_quietly():
    # More than a pipe holds, so the write meets the closed end.
    process = subprocess.Popen(
        [os.environ["AAW_DAW"], "describe", "midi", "--schema"], stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    process.stdout.read(1)
    process.stdout.close()
    assert process.wait() == 141
    assert process.stderr.read() == b""


def test_help_lists_what_a_command_takes():
    effect = run("effect", "add", "--help").stdout
    for kind in ["filter", "eq", "compressor", "limiter", "delay", "reverb", "chorus", "saturation"]:
        assert f"\n{kind}\n" in effect
    assert "--ceiling-db" in effect and "number -24..-0.1, default -1" in effect
    assert "--slope-db-per-octave      12|24|36|48, default 12" in effect
    assert "session.tempo              number 20..400, default 144" in run("set", "--help").stdout
    lanes = run("lane", "set", "--help").stdout
    assert "filter: cutoff_hz" in lanes and "sends.RETURN.gain_db" in lanes
    assert "--source-start-seconds" in run("audio", "add", "--help").stdout
    assert "--type" in run("track", "add", "--help").stdout and "--length-beats" in run("clip", "add", "--help").stdout


def test_errors_say_what_to_do(tmp_path):
    song = daw("init", tmp_path / "song", "--bars", 4)["project"]
    daw("track", "add", song, "drums")

    def error(*args):
        code, result = cli(*args)
        assert code == 1, result
        return result["error"]

    refused = error("set", song, "bpm", 90)
    assert refused.startswith("`bpm` is not a field; did you mean `session.tempo`?\n")
    # The model's own message follows.
    assert "Extra inputs are not permitted" in refused
    assert "did you mean `session.tempo`?" in error("set", song, "session.bpm", 90)
    assert "did you mean `tracks.drums.gain_db`?" in error("set", song, "tracks.drums.volume", -3)
    assert "did you mean `tracks.drums.gain_db`?" in error("set", song, "tracks.drums.gian_db", -3)
    nothing_near = error("set", song, "tracks.drums.zzz", 1)
    assert "`tracks.drums.zzz` is not a field; tracks.drums has id, gain_db" in nothing_near
    assert error("effect", "add", song, "master", "limiter").startswith("`limiter` is an effect type; use --type limiter")
    # A union's tag is not part of the path a command takes.
    tagged = error("effect", "add", song, "master", "--type", "limiter", "--ceiling", -1)
    assert "`master.effects.0.ceiling` is not a field; did you mean `master.effects.0.ceiling_db`?" in tagged
    assert "--type midi" in error("track", "add", song, "keys", "midi")


def test_a_clipped_render_names_the_loud_stems_and_the_fix(tmp_path):
    t = np.arange(SR) / SR
    sf.write(tmp_path / "tone.wav", 0.9 * np.sin(2 * np.pi * 110 * t), SR)
    song = {
        "session": {"length_beats": 4, "master_gain_db": 12},
        "samples": {"tone": {"path": "tone.wav"}},
        "patterns": {"p": {"length_beats": 4, "steps": {"t": "x..."}, "grid": 1}},
        "tracks": [
            {"id": "loud", "pads": {"t": {"sample": "tone"}}, "clips": [{"pattern": "p"}]},
            {"id": "quiet", "gain_db": -30, "pads": {"t": {"sample": "tone"}}, "clips": [{"pattern": "p"}]},
        ],
    }
    path = tmp_path / "song.yaml"
    save(song, path)
    code, result = cli("render", path)
    assert code == 1
    message = result["error"]
    assert message.startswith("Unsafe PCM export: the mix peaks at +")
    assert "loudest stems loud +" in message and ", quiet -" in message
    assert "lower `tracks.loud.gain_db` or `session.master_gain_db`" in message
    assert message.endswith("daw effect add PROJECT master --type limiter")


def test_inspect_lists_clips_in_the_order_they_play(tmp_path):
    song = daw("init", tmp_path / "song", "--bars", 4)["project"]
    daw("track", "add", song, "drums")
    daw("pattern", "add", song, "p", "--length-beats", 4)
    daw("clip", "add", song, "drums", "p", "--at", 8)
    daw("clip", "add", song, "drums", "p", "--at", 0)
    daw("synth", "add", song, "keys")
    daw("clip", "add", song, "keys", "--id", "later", "--at", 4, "--length-beats", 4)
    daw("clip", "add", song, "keys", "--id", "first", "--length-beats", 4)
    tracks = daw("inspect", song)["tracks"]
    # Each keeps the reference of its place in the song.
    assert [(c["ref"], c["at"]) for c in tracks[0]["clips"]] == [("tracks.drums.clips.1", 0), ("tracks.drums.clips.0", 8)]
    assert [c["id"] for c in tracks[1]["clips"]] == ["first", "later"]


def test_duplicates_lay_out_in_a_row_and_a_stack_is_mapped_and_checked(tmp_path):
    # The agent's song of October 4, 2026: a clip duplicated three times.
    song = daw("init", tmp_path / "song", "--bars", 8)["project"]
    sf.write(tmp_path / "song" / "kick.wav", np.zeros(SR // 10), SR)
    daw("set", song, "samples.kick", '{"path": "kick.wav"}')
    daw("track", "add", song, "drums")
    daw("pad", "add", song, "drums", "k", "--sample", "kick")
    daw("pattern", "add", song, "p", "--length-beats", 4)
    daw("pattern", "steps", song, "p", "k", "x...x...x...x...")
    daw("clip", "add", song, "drums", "p", "--repeats", 2)
    for _ in range(3):
        daw("clip", "duplicate", song, "tracks.drums.clips.0")
    out = run("map", song).stdout
    # Printed JSON keeps the grid's columns, a line of map each.
    assert '    "drums  AAAAAAAA",' in out.splitlines()
    mapped = json.loads(out)
    assert mapped["map"][0] == "bar    1   5"
    assert mapped["clips"]["A"].split() == [f"tracks.drums.clips.{i}" for i in range(4)]
    assert "pattern p, 4 beats, 4 hits; drums bars 1 ×2, 3 ×2, 5 ×2, 7 ×2" in mapped["map"][3]
    assert "daw map PROJECT" in daw("describe", "project")["semantics"]["map"]
    assert cli("check", song)[1]["warnings"] == []
    # A copy put on a beat where the same clip starts is stacked, and said so.
    daw("clip", "duplicate", song, "tracks.drums.clips.0", "--at", 8)
    assert '    "drums  AA##AAAA",' in run("map", song).stdout.splitlines()
    warnings = cli("check", song)[1]["warnings"]
    assert [w["code"] for w in warnings] == ["clips-stacked"]
    assert warnings[0]["at"] == 8 and len(warnings[0]["paths"]) == 2
    assert "clips-stacked" in daw("describe", "check")["codes"]


def test_the_first_song_recipe_runs_as_written(tmp_path, monkeypatch):
    library = tmp_path / "library"
    library.mkdir()
    t = np.arange(SR // 4) / SR
    sf.write(library / "kick.wav", 0.8 * np.sin(2 * np.pi * (50 + 120 * np.exp(-30 * t)) * t) * np.exp(-8 * t), SR)
    monkeypatch.setenv("AAW_LIBRARY", str(tmp_path / "library.sqlite"))
    assert daw("samples", "scan", library)["indexed"] == 1

    start = daw("describe", "start")
    work = tmp_path / "work"
    work.mkdir()
    batch = start["batch"]
    (work / batch["file"]).write_text(json.dumps(batch["content"]))
    found = {}
    for step in start["steps"]:
        args = shlex.split(step["run"])
        assert args[0] == "daw", step
        # The placeholders are the paths a search finds.
        args = [found.get(a, a) for a in args[1:]]
        out = run(*args, cwd=work)
        assert out.returncode == 0, (step["run"], out.stderr)
        result = json.loads(out.stdout)
        if args[:2] == ["samples", "search"]:
            found[f"{args[2].upper()}_PATH"] = result[0]["path"]
    assert result["kind"] == "listen"
    tracks = [t["id"] for t in daw("inspect", work / "song")["tracks"]]
    assert tracks == ["drums", "bass", "keys"]
