"""MIDI tracks through `daw` as an agent drives it: a phrase made by commands
alone, a sampler attached and rendered, `check` naming silent notes, the
Python tools reading such a song, and a labeled batch through a running host."""

import json
import os
import subprocess
import tempfile
from fractions import Fraction
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
    daw("init", song, "--tempo", 120)
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
    described = daw("describe", "midi", "--schema")
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
    messages = [w["message"] for w in checked["warnings"]]
    assert "keys.clip1: the sampler maps no pad to notes 69 (A4), which are silent" in messages
    # The Python timeline reads a MIDI track's sounds and clips.
    code, timeline = cli("timeline", path)
    assert code == 0, timeline
    assert timeline["tracks"]["keys"]["hits"] == 5
    daw("instrument", "remove", path, "keys")
    assert load(path)["tracks"][0]["clips"] == before


def test_a_synth_plays_the_phrase_is_checked_and_auditioned(tmp_path):
    path = phrase(tmp_path)
    assert daw("synth", "add", path, "keys")["label"] == "Attach a Synth to keys"
    daw("synth", "set", path, "keys", "voices", 2, "filter.cutoff_hz", 600, "macros.tone", 30,
        "oscillators.sub", json.dumps({"wave": "sine", "octave": -1, "filter": False}))
    daw("synth", "mod", path, "keys", "velocity", "filter.cutoff_hz", 2)
    shown = daw("synth", "show", path, "keys")
    assert shown["oscillators"]["sub"]["wave"] == "sine" and shown["modulation"][0]["amount"] == 2
    song = load(path)
    assert song["tracks"][0]["instrument"]["synth"]["voices"] == 2
    assert song["tracks"][0]["instrument"]["sampler"] is None
    out = tmp_path / "render"
    report = daw("render", path, "--output", out)
    assert report["tracks"]["keys"]["events"] == 6
    x = stem(out, "keys")
    beat = SR // 2
    assert np.abs(x[4 * beat : 5 * beat]).max() > 0.05
    assert np.abs(x[: 4 * beat]).max() == 0
    # A second render is the same bytes.
    again = daw("render", path, "--output", tmp_path / "again")
    assert again["audio_sha256"] == report["audio_sha256"]
    code, checked = cli("check", path)
    assert code == 0, checked
    messages = [w["message"] for w in checked["warnings"]]
    assert "keys: macro tone moves nothing; the matrix has no entry with source macros.tone" in messages
    assert any(m.startswith("keys: 3 notes sound at once and the synth has 2 voices") for m in messages)
    heard = daw("synth", "audition", path, "keys", "--notes", "C2,G2", "--length-beats", 1)
    assert heard["notes"] == ["C2", "G2"] and heard["seconds"] > 1.0
    assert heard["peak_dbfs"] < 3 and heard["loudness_lufs"] < 0
    assert 20 < heard["spectral_centroid_hz"] < 5000
    assert sf.info(heard["path"]).frames == heard["frames"]
    # --play sends the notes to the running host as note.preview; with none it says so.
    with pytest.raises(Exception, match="No host is running"):
        daw("synth", "audition", path, "keys", "--notes", "C2", "--play")
    # Lanes reach the synth, and the instrument comes off with them.
    daw("lane", "point", "add", path, "tracks.keys", "instrument.filter.cutoff_hz", "--at", 0, "--value", 300)
    assert load(path)["tracks"][0]["automation"][0]["param"] == "instrument.filter.cutoff_hz"
    daw("instrument", "remove", path, "keys")
    assert load(path)["tracks"][0]["automation"] == []


def test_patches_are_listed_loaded_saved_and_heard_the_same_in_another_song(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_WORKSPACE", str(tmp_path / "ws"))
    listed = daw("patch", "list")
    names = [p["name"] for p in listed["patches"]]
    assert names[:3] == ["Init", "Sub Bass", "Reese"] and "Riser" in names
    assert all(p["factory"] for p in listed["patches"])
    assert listed["directory"] == str(tmp_path / "ws" / "library" / "patches")
    assert [p["name"] for p in daw("patch", "list", "drum")["patches"]] == ["Kick", "Hat"]
    shown = daw("patch", "show", "Soft Pad")
    assert shown["synth"]["envelopes"]["amp"]["attack_ms"] == 400 and "pad" in shown["tags"]
    # A factory patch on the phrase, then changed and saved as the person's own.
    path = phrase(tmp_path)
    added = daw("synth", "add", path, "keys", "--patch", "pluck")
    assert added["label"] == "Attach a Synth with Pluck to keys" and added["factory"] is True
    assert load(path)["tracks"][0]["instrument"]["synth"]["patch"] == "Pluck"
    daw("synth", "set", path, "keys", "filter.cutoff_hz", 450)
    saved = daw("patch", "save", path, "keys", "Dull Pluck", "--description", "The pluck, closed", "--tags", "pluck,dark")
    assert saved["label"] == "Save patch Dull Pluck from keys"
    file = tmp_path / "ws" / "library" / "patches" / "dull-pluck.yaml"
    assert saved["file"] == str(file) and file.is_file()
    assert load(path)["tracks"][0]["instrument"]["synth"]["patch"] == "Dull Pluck"
    mine = [p for p in daw("patch", "list")["patches"] if not p["factory"]]
    assert [(p["name"], p["tags"], p["saved_by"]) for p in mine] == [("Dull Pluck", ["pluck", "dark"], "agent")]
    # Loaded into another song, it is the same sound.
    other = phrase(tmp_path / "other")
    loaded = daw("patch", "load", other, "keys", "Dull Pluck")
    assert loaded["label"] == "Load patch Dull Pluck into keys" and loaded["factory"] is False
    assert load(other)["tracks"][0]["instrument"]["synth"] == load(path)["tracks"][0]["instrument"]["synth"]
    a = daw("synth", "audition", path, "keys", "--notes", "C3", "--output", tmp_path / "a.wav")
    b = daw("synth", "audition", other, "keys", "--notes", "C3", "--output", tmp_path / "b.wav")
    assert (tmp_path / "a.wav").read_bytes() == (tmp_path / "b.wav").read_bytes()
    assert a["peak_dbfs"] < 0 and a["peak_dbfs"] == b["peak_dbfs"]
    # The notes stayed.
    assert load(other)["tracks"][0]["clips"] == load(path)["tracks"][0]["clips"]
    # A second save of the name wants --replace.
    code, err = cli("patch", "save", path, "keys", "Dull Pluck")
    assert code != 0 and "--replace" in err["error"]


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


def test_clips_are_joined_into_one_that_plays_what_they_played(tmp_path):
    path = phrase(tmp_path)
    clip = "tracks.keys.clips.clip1"
    daw("clip", "duplicate", path, clip, "--at", 12, "--id", "answer")
    daw("clip", "loop", path, clip, 2)
    joined = daw("clip", "join", path, "tracks.keys.clips.answer", clip)
    assert joined["label"] == "Join 2 clips into clip1 at 4"
    assert joined["path"] == clip
    clips = load(path)["tracks"][0]["clips"]
    assert len(clips) == 1
    assert (clips[0]["at"], clips[0]["length_beats"], clips[0].get("loop_beats")) == (4, 12, None)
    # The loop's two repetitions of the first two beats, then the copy's six
    # notes eight beats on; the A at beat 3 was past the loop's end and silent.
    starts = sorted(Fraction(str(n.get("at", 0))) for n in clips[0]["notes"])
    assert starts == sorted(
        [Fraction(0), Fraction(0), Fraction(0), Fraction("1.975"), Fraction(2), Fraction(2), Fraction(2), Fraction("3.975")]
        + [Fraction(8), Fraction(8), Fraction(8), Fraction("9.975"), Fraction("10.025"), Fraction(11)]
    )
    with pytest.raises(ValueError, match="Name two or more clips"):
        daw("clip", "join", path, clip, clip)


def test_a_clip_loops_its_first_beats_and_the_notes_past_the_loop_are_named(tmp_path):
    path = phrase(tmp_path)
    clip = "tracks.keys.clips.clip1"
    looped = daw("clip", "loop", path, clip, 2)
    assert looped["label"] == "Loop clip clip1 at 4 every 2 beats"
    daw("clip", "resize", path, clip, 12)
    song = load(path)["tracks"][0]["clips"][0]
    assert (song["loop_beats"], song["length_beats"]) == (2, 12)
    # The notes at and after beat 2 of the clip are kept and do not play.
    listed = daw("note", "list", path, clip)["clips"][0]["notes"]
    assert [n["id"] for n in listed if n.get("outside")] == ["n5", "n6"]
    warnings = [w for w in daw("check", path)["warnings"] if w["code"] == "notes-outside-clip"]
    assert [w["message"] for w in warnings] == ["keys.clip1: notes n5, n6 start at or after the loop's end and do not play"]
    # The map shows one clip of three bars, and describe says what loops do.
    assert "AAA" in daw("map", path, "--track", "keys")["map"][1]
    assert "loop_beats" in daw("describe", "midi")["semantics"]["loops"]
    assert "loop_beats" in daw("describe", "edit")["semantics"]["loops"]
    off = daw("clip", "loop", path, clip, "off")
    assert off["label"] == "Loop clip clip1 at 4 off"
    assert "loop_beats" not in load(path)["tracks"][0]["clips"][0] or load(path)["tracks"][0]["clips"][0]["loop_beats"] is None
    # An audio clip loops by the same field, and Python's timeline sees its repetitions.
    from agent_daw.timeline import regions

    sf.write(path.parent / "loop.wav", tone(110.0, seconds=4.0), SR)
    daw("set", path, "samples.loop", json.dumps({"path": "loop.wav"}))
    daw("track", "add", path, "drums")
    daw("audio", "add", path, "drums", "loop", "--at", 0, "--source-end-seconds", 2)
    daw("clip", "loop", path, "tracks.drums.audio.0", 2, "--length", 7)
    found = regions(load(path), path.parent)["drums"]
    assert [(r.start, round(r.length, 3)) for r in found] == [(0.0, 1.008), (1.0, 1.008), (2.0, 1.008), (3.0, 0.508)]


def test_racks_are_saved_from_a_chain_listed_and_added_to_a_chain_in_another_song(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_WORKSPACE", str(tmp_path / "ws"))
    listed = daw("rack", "list")
    assert listed == {"directory": str(tmp_path / "ws" / "library" / "racks"), "racks": []}
    path = phrase(tmp_path)
    daw("effect", "add", path, "tracks.keys", "--type", "utility", "--id", "trim", "--gain-db", -3)
    daw("effect", "add", path, "tracks.keys", "--type", "eq", "--bands", '[{"shape": "highpass", "freq_hz": 80}]')
    daw("effect", "add", path, "tracks.keys", "--type", "limiter", "--ceiling-db", -1)
    saved = daw("rack", "save", path, "tracks.keys", "Keys Chain", "--description", "Trim, a highpass and a lid", "--tags", "keys,bus")
    assert saved["label"] == "Save rack Keys Chain from tracks.keys" and saved["changed"] is False
    file = tmp_path / "ws" / "library" / "racks" / "keys-chain.yaml"
    assert saved["file"] == str(file) and file.is_file()
    assert saved["effects"] == ["utility", "eq", "limiter"]
    racks = daw("rack", "list")["racks"]
    assert [(r["name"], r["slug"], r["kinds"], r["tags"], r["saved_by"]) for r in racks] == [("Keys Chain", "keys-chain", ["utility", "eq", "limiter"], ["keys", "bus"], "agent")]
    assert [r["name"] for r in daw("rack", "list", "limiter")["racks"]] == ["Keys Chain"]
    assert daw("rack", "list", "reverb")["racks"] == []
    shown = daw("rack", "show", "keys-chain")
    assert shown["effects"][0] == {"type": "utility", "id": "trim", "gain_db": -3} and shown["description"] == "Trim, a highpass and a lid"
    with pytest.raises(ValueError, match="--replace"):
        daw("rack", "save", path, "tracks.keys", "Keys Chain")
    # Laid into another song's master with --flat, in one undo step, with the same effects.
    other = tmp_path / "other"
    daw("init", other, "--tempo", 100)
    added = daw("rack", "load", other, "master", "Keys Chain", "--flat")
    assert added["label"] == "Add rack Keys Chain to master" and added["effects"] == ["utility", "eq", "limiter"]
    assert added["device"] is False
    assert [e["type"] for e in load(other / "song.yaml")["master"]["effects"]] == ["utility", "eq", "limiter"]
    assert load(other / "song.yaml")["master"]["effects"][0]["id"] == "trim"
    # Laid in again where the id is taken, the effect is numbered.
    again = daw("rack", "load", other, "master", "Keys Chain", "--index", 0, "--flat")
    assert again["also"] == ["master: effect trim of the rack is trim-2 here, since the chain has trim"]
    assert [e.get("id") for e in load(other / "song.yaml")["master"]["effects"]] == ["trim-2", None, None, "trim", None, None]
    # Without --flat it is one rack device named for the file, its effects
    # inside with their own ids, addressed through it.
    device = daw("rack", "load", other, "master", "Keys Chain")
    assert device["device"] is True and device["path"] == "master.effects.keys-chain"
    rack = load(other / "song.yaml")["master"]["effects"][6]
    assert (rack["type"], rack["id"], rack["name"]) == ("rack", "keys-chain", "Keys Chain")
    assert [e["type"] for e in rack["effects"]] == ["utility", "eq", "limiter"] and rack["effects"][0]["id"] == "trim"
    assert daw("get", other, "master.effects.keys-chain.effects.trim.gain_db") == -3
    daw("set", other, "master.effects.keys-chain.effects.trim.gain_db", -6)
    assert load(other / "song.yaml")["master"]["effects"][6]["effects"][0]["gain_db"] == -6
    # Wrapped and unwrapped from the terminal, and a rack in a rack refused.
    wrapped = daw("rack", "wrap", other, "master.effects.0", "master.effects.1", "--name", "Front")
    assert wrapped["label"] == "Wrap 2 effects of master in a rack" and wrapped["path"] == "master.effects.0"
    assert [e["type"] for e in load(other / "song.yaml")["master"]["effects"]] == ["rack", "limiter", "utility", "eq", "limiter", "rack"]
    with pytest.raises(ValueError, match="holds no rack"):
        daw("effect", "add", other, "master.effects.0", "--type", "rack")
    daw("rack", "unwrap", other, "master.effects.0")
    assert [e["type"] for e in load(other / "song.yaml")["master"]["effects"]][:3] == ["utility", "eq", "limiter"]
    for topic in ("rack", "racks", "instrument_racks"):
        assert topic in daw("describe", "effects")["semantics"]


def test_midi_effects_change_the_notes_the_instrument_plays_and_keep_the_clips(tmp_path):
    """An arpeggiator, a chord, a scale and a velocity before the sampler: the
    schedule plays what they make, through `daw effect` on `midi_effects`."""
    song = tmp_path / "song"
    daw("init", song, "--tempo", 120)
    path = song / "song.yaml"
    daw("track", "add", path, "keys", "--type", "midi")
    daw("track", "add", path, "drums")
    (song / "samples").mkdir(exist_ok=True)
    sf.write(song / "samples" / "a3.wav", tone(220.0, seconds=0.05), SR)
    daw("set", path, "samples.a3", json.dumps({"path": "samples/a3.wav", "root_note": "A3"}))
    sampler = {"sampler": {"pads": {"p": {"sample": "a3"}}, "map": [{"notes": [36, 96], "pad": "p", "pitched": True}]}}
    daw("instrument", "set", path, "keys", json.dumps(sampler))
    clip = daw("clip", "add", path, "keys", "--at", 4, "--length-beats", 4)["path"]
    chord = [{"pitch": "C4", "duration": 2}, {"pitch": "E4", "duration": 2}, {"pitch": "G4", "duration": 2}]
    daw("note", "add", path, clip, "--notes", json.dumps(chord))
    before = load(path)["tracks"][0]["clips"]
    assert daw("render", path, "--output", tmp_path / "plain")["tracks"]["keys"]["events"] == 3
    # A MIDI effect goes on a MIDI track and nowhere else; an audio effect
    # on the same track keeps going under effects.
    with pytest.raises(ValueError, match="arpeggiator is a MIDI effect: it goes on a MIDI track"):
        daw("effect", "add", path, "tracks.drums", "--type", "arpeggiator")
    added = daw("effect", "add", path, "tracks.keys", "--type", "arpeggiator", "--id", "arp", "--rate", "1/8", "--gate-percent", 50)
    assert added["label"] == "Add arpeggiator to tracks.keys"
    daw("effect", "add", path, "tracks.keys", "--type", "limiter")
    keys = load(path)["tracks"][0]
    assert [e["type"] for e in keys["midi_effects"]] == ["arpeggiator"] and keys["midi_effects"][0]["rate"] == "1/8"
    assert [e["type"] for e in keys["effects"]] == ["limiter"]
    assert keys["clips"] == before
    out = tmp_path / "arp"
    report = daw("render", path, "--output", out)
    # Eighths from beat 4 for two beats: four notes, each a quarter beat
    # long, with silence between them.
    assert report["tracks"]["keys"]["events"] == 4
    x = np.abs(stem(out, "keys")).max(axis=1)
    beat = SR // 2
    for step in range(4):
        start = 4 * beat + step * beat // 2
        assert x[start : start + beat // 4].max() > 0.05, step
        assert x[start + beat // 4 + 200 : start + beat // 2].max() == 0, step
    # The fields are set by their path, the device is addressed by its id,
    # and bypassing it plays the chord as written.
    daw("set", path, "tracks.keys.midi_effects.arp.rate", "1/16")
    assert daw("render", path, "--output", tmp_path / "sixteenths")["tracks"]["keys"]["events"] == 8
    with pytest.raises(ValueError, match="rate"):
        daw("set", path, "tracks.keys.midi_effects.arp.rate", "1/5")
    daw("effect", "bypass", path, "tracks.keys.midi_effects.arp")
    assert daw("render", path, "--output", tmp_path / "bypassed")["tracks"]["keys"]["events"] == 3
    daw("effect", "bypass", path, "tracks.keys.midi_effects.0", "off")
    # A chord before the arpeggiator doubles the run; moved after it, each
    # step is a pair; a scale snaps the fifth up to the major sixth of D
    # and a velocity scales what is left.
    daw("effect", "add", path, "tracks.keys", "--type", "chord", "--index", 0, "--shift-1", 12)
    keys = load(path)["tracks"][0]
    assert [e["type"] for e in keys["midi_effects"]] == ["chord", "arpeggiator"]
    assert daw("render", path, "--output", tmp_path / "chord-first")["tracks"]["keys"]["events"] == 8
    daw("effect", "move", path, "tracks.keys.midi_effects.0", 1)
    assert [e["type"] for e in load(path)["tracks"][0]["midi_effects"]] == ["arpeggiator", "chord"]
    assert daw("render", path, "--output", tmp_path / "chord-after")["tracks"]["keys"]["events"] == 16
    with pytest.raises(ValueError, match="stays before its track's instrument"):
        daw("effect", "move", path, "tracks.keys.midi_effects.0", 0, "--to", "tracks.keys")
    daw("effect", "remove", path, "tracks.keys.midi_effects.arp")
    daw("effect", "remove", path, "tracks.keys.midi_effects.0")
    daw("effect", "add", path, "tracks.keys", "--type", "scale", "--root", "D", "--scale", "major")
    daw("effect", "add", path, "tracks.keys", "--type", "velocity", "--scale-percent", 50, "--min", 40)
    scale, velocity = load(path)["tracks"][0]["midi_effects"]
    assert (scale["type"], scale["root"], scale["scale"], scale["transpose"]) == ("scale", "D", "major", 0)
    assert (velocity["type"], velocity["scale_percent"], velocity["min"], velocity["max"]) == ("velocity", 50, 40, 127)
    assert "midi_effects: [{type: scale, root: D}, {type: velocity, scale_percent: 50.0, min: 40}]" in path.read_text().replace("\n   ", " ")
    assert load(path)["tracks"][0]["clips"] == before
    report = daw("render", path, "--output", tmp_path / "scaled")
    assert report["tracks"]["keys"]["events"] == 3
    with pytest.raises(ValueError, match="min is at most its max"):
        daw("effect", "add", path, "tracks.keys", "--type", "velocity", "--min", 100, "--max", 50)
    # What the agent reads says so.
    described = daw("describe", "midi")
    assert "arpeggiator" in described["semantics"]["midi_effects"]
    assert described["fields"]["arpeggiator.rate"].startswith("1/1|1/2|1/4|1/4T|1/8|")
    assert described["fields"]["tracks[].midi_effects"] == "list of MIDI effect: arpeggiator, chord, scale or velocity (daw describe midi), at most 16"
