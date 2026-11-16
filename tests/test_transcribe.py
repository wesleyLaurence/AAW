"""`daw transcribe`: the notes a stand-in model hears, against a generated song
whose beats are known, joined, made one line, placed on the beat map and
written as a MIDI file, note text and a note clip."""

import json
import shlex
import shutil
import sys
from pathlib import Path

import pytest

from agent_daw import separation, transcription
from helpers import beat_song, cli, daw

STAND_IN = Path(__file__).with_name("stand_in_transcriber.py")
BPM = 120.0
FIRST = 0.137
BEAT = 60 / BPM


@pytest.fixture
def model(monkeypatch, tmp_path):
    monkeypatch.setenv("AAW_TRANSCRIBER", shlex.join([sys.executable, str(STAND_IN)]))
    monkeypatch.setenv("AAW_LIBRARY", str(tmp_path / "library.sqlite"))
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))


@pytest.fixture(scope="module")
def made(tmp_path_factory):
    """A song of 8 bars at 120 BPM whose first downbeat is 0.137 s in."""
    folder = tmp_path_factory.mktemp("song")
    beat_song(folder / "song.wav", bpm=BPM, bars=8, first=FIRST)
    return folder / "song.wav"


def part(made, folder):
    """A copy of the song, with its beat map, to hand the stand-in notes for."""
    path = folder / "part.wav"
    shutil.copy(made, path)
    return path


def at(beat, sixteenth=0):
    """The time of a beat counted from bar 1, and a sixteenth after it."""
    return FIRST + (beat + sixteenth / 4) * BEAT


def hears(path, notes):
    Path(f"{path}.notes.json").write_text(json.dumps({"notes": notes}))


def note(start, end, pitch, confidence=0.6, bend=(0, 0)):
    return [start, end, pitch, confidence, *bend]


def test_each_note_is_placed_on_the_beat_map(made, tmp_path, model):
    path = part(made, tmp_path)
    # Bar 1 beat 1, the "and" of bar 1 beat 2 a little late, and bar 2 beat 3.
    hears(path, [note(at(0), at(1), 42), note(at(1, 2) + 0.012, at(2), 49, 0.45, (-33, 0)), note(at(6), at(7), 40)])
    code, out = cli("transcribe", path)
    assert code == 0, out
    assert out["model"]["name"] == "basic-pitch" and out["tempo_bpm"] == pytest.approx(BPM, abs=0.05)
    assert out["count"] == 3 and out["sure"] == 2 and (out["lowest"], out["highest"]) == ("E2", "C#3")
    first, second, third = out["notes"]
    assert (first["pitch"], first["midi"], first["sixteenth"]) == ("F#2", 42, "1.1.1")
    assert abs(first["off_grid_ms"]) <= 2
    assert first["beats"] == pytest.approx(0, abs=0.01) and first["length_beats"] == pytest.approx(1, abs=0.01)
    assert second["sixteenth"] == "1.2.3" and second["off_grid_ms"] == pytest.approx(12, abs=2)
    assert second["beats"] == pytest.approx(1.5 + 0.024, abs=0.01) and second["bend_cents"] == [-33, 0]
    assert "bend_cents" not in first
    assert third["sixteenth"] == "2.3.1"
    assert all(1 <= n["velocity"] <= 127 for n in out["notes"])
    assert out["left_out"] == {"joined": 0, "under_min_confidence": 0}


def test_one_pitch_split_where_it_wavers_is_one_note(made, tmp_path, model):
    path = part(made, tmp_path)
    hears(path, [note(at(0), at(1), 42, 0.5), note(at(1) + 0.01, at(2), 42, 0.7), note(at(2) + 0.2, at(3), 42)])
    code, out = cli("transcribe", path)
    assert code == 0, out
    assert out["left_out"]["joined"] == 1
    held, again = out["notes"]
    assert held["length_beats"] == pytest.approx(2, abs=0.01) and held["confidence"] == pytest.approx(0.6, abs=0.01)
    assert again["beats"] == pytest.approx(2.4, abs=0.01)


def test_a_line_keeps_one_note_at_a_time(made, tmp_path, model):
    path = part(made, tmp_path)
    hears(
        path,
        [
            # An octave heard with the note, nearly as sure: kept beside it as a doubt.
            note(at(0), at(1), 30, 0.6),
            note(at(0) + 0.02, at(1), 42, 0.5),
            # A semitone ghost while a note sounds, less sure: dropped.
            note(at(1), at(2), 37, 0.6),
            note(at(1, 2), at(1, 3), 38, 0.4),
            # Another note while one sounds ends it.
            note(at(2), at(3, 2), 40, 0.6),
            note(at(3), at(4), 35, 0.5),
        ],
    )
    code, out = cli("transcribe", path, "--line")
    assert code == 0, out
    assert [n["pitch"] for n in out["notes"]] == ["F#1", "C#2", "E2", "B1"]
    assert out["notes"][0]["octave_doubt"] == "F#2"
    assert out["notes"][2]["length_beats"] == pytest.approx(1, abs=0.01)
    assert out["left_out"]["not_the_line"] == 2
    code, chords = cli("transcribe", path)
    assert chords["count"] == 6 and chords["line"] is False


def test_range_and_confidence_leave_notes_out(made, tmp_path, model):
    path = part(made, tmp_path)
    hears(path, [note(at(0), at(1), 30, 0.6), note(at(1), at(2), 60, 0.6), note(at(2), at(3), 40, 0.3)])
    code, out = cli("transcribe", path, "--range", "E1-G3", "--min-confidence", "0.4")
    assert code == 0, out
    assert [n["pitch"] for n in out["notes"]] == ["F#1"]
    assert out["left_out"] == {"joined": 0, "outside_range": 1, "under_min_confidence": 1}
    code, out = cli("transcribe", path, "--near", str(at(2)), "--window", "0.3")
    assert [n["pitch"] for n in out["notes"]] == ["C4", "E2"] and out["near"]["of"] == 3


def test_the_notes_as_text_and_a_midi_file_on_bar_one(made, tmp_path, model):
    path = part(made, tmp_path)
    # A pickup a beat before bar 1, then bar 1's downbeat and its "and".
    hears(path, [note(at(-1), at(-0.5), 45), note(at(0), at(0.5), 42), note(at(0.5) + 0.01, at(1), 49)])
    out_file = tmp_path / "part.mid"
    code, out = cli("transcribe", path, "--text", "--midi", out_file, "--quantize", "1/4")
    assert code == 0, out
    # The text starts a bar early, at bar 0, for the pickup, on the grid asked for.
    assert out["text"]["from_bar"] == 0 and out["text"]["grid"] == "1/4"
    assert out["text"]["text"].split("|")[0].split()[12].startswith("a2")
    assert out["text"]["text"].split("|")[1].split()[:3][0].startswith("f#2")
    assert out["midi"] == {"path": str(out_file), "bar_one_beat": 4.0}
    song = tmp_path / "check"
    daw("init", song)
    daw("midi", "import", song, out_file)
    notes = daw("note", "list", song, "tracks.part")["clips"][0]["notes"]
    assert [(n["name"], n["at"]) for n in notes] == [("A2", 3), ("F#2", 4), ("C#3", 4.5)]


def test_a_clip_of_the_notes_goes_where_the_audio_clip_plays_them(made, tmp_path, model):
    folder = tmp_path / "project"
    daw("init", folder)
    daw("set", folder, "session.tempo", BPM)
    stem = folder / "stem.wav"
    shutil.copy(made, stem)
    code, imported = cli("samples", "import", stem, "--project", folder / "song.yaml", "--id", "stem")
    assert code == 0, imported
    placed = folder / imported["path"]
    hears(placed, [note(at(0), at(1), 42), note(at(4), at(5), 44)])
    daw("track", "add", folder, "bass")
    daw("audio", "add", folder, "bass", "stem", "--at", 8)
    code, out = cli("transcribe", "tracks.bass.audio.0", "--project", folder, "--line")
    assert code == 0, out
    # The file's first downbeat, 0.137 s in, plays at beat 8 + 0.274.
    assert out["clip"]["bar_one_at"] == pytest.approx(8 + FIRST / BEAT, abs=0.001)
    assert out["clip"]["at"] == pytest.approx(out["clip"]["bar_one_at"], abs=0.001)
    notes = daw("note", "list", folder, out["clip"]["ref"])["clips"][0]["notes"]
    assert [n["name"] for n in notes] == ["F#2", "G#2"]
    assert notes[0]["song_at"] == pytest.approx(8 + FIRST / BEAT, abs=0.01)
    assert notes[1]["song_at"] - notes[0]["song_at"] == pytest.approx(4, abs=0.01)
    assert out["clip"]["ref"].startswith("tracks.stem-notes.clips.")
    # Transposed by repitching, the clip plays the notes two semitones up, sooner.
    daw("set", folder, "tracks.bass.audio.0.transpose", 2)
    code, out = cli("transcribe", "tracks.bass.audio.0", "--project", folder, "--track", "stem-notes")
    assert code == 0, out
    assert out["transposed_by"] == 2 and out["notes"][0]["pitch"] == "G#2"
    assert out["clip"]["bar_one_at"] == pytest.approx(8 + FIRST / BEAT / 2 ** (2 / 12), abs=0.001)
    # A sample with --at: bar 1 lands on that beat.
    code, out = cli("transcribe", "stem", "--project", folder, "--at", 16, "--track", "stem-notes")
    assert code == 0, out
    assert out["clip"]["at"] == 16 and out["clip"]["bar_one_at"] == 16


def test_a_note_before_the_song_starts_is_left_out(made, tmp_path, model):
    folder = tmp_path / "project"
    daw("init", folder)
    path = part(made, tmp_path)
    hears(path, [note(at(-1), at(-0.5), 45), note(at(0), at(1), 42)])
    code, out = cli("transcribe", path, "--project", folder, "--at", 0.5)
    assert code == 0, out
    assert out["before_the_song"] == 1 and out["clip"]["at"] == 0 and out["clip"]["notes"] == 1
    code, out = cli("transcribe", path, "--project", folder, "--at", 4)
    assert "before_the_song" not in out and out["clip"]["at"] == 0 and out["clip"]["notes"] == 2


def test_a_separated_part_is_placed_on_its_source_songs_beat_map(made, tmp_path, model):
    folder = tmp_path / "project"
    (folder / "samples").mkdir(parents=True)
    (folder / "separated").mkdir()
    source = folder / "samples" / "song.wav"
    shutil.copy(made, source)
    # The part is silence but for one tone: its own beat map would be nothing.
    import numpy as np
    import soundfile as sf

    x, sr = sf.read(made)
    quiet = np.zeros_like(x)
    quiet[int(at(0) * sr) : int(at(1) * sr)] = 0.3
    stem = folder / "separated" / "0123456789ab-htdemucs-bass.wav"
    sf.write(stem, quiet, sr)
    (folder / "separated" / "0123456789ab-htdemucs.separated.json").write_text(
        json.dumps({"source": {"path": "samples/song.wav"}, "parts": {"bass": {"path": "separated/" + stem.name}}})
    )
    hears(stem, [note(at(0), at(1), 42)])
    code, out = cli("transcribe", stem)
    assert code == 0, out
    assert out["beat_map_of"] == str(source.resolve()) and out["notes"][0]["sixteenth"] == "1.1.1"
    assert transcription.separated_from(source) is None


def test_without_a_beat_map_notes_are_in_seconds(tmp_path, model):
    import numpy as np
    import soundfile as sf

    path = tmp_path / "short.wav"
    sf.write(path, 0.3 * np.sin(np.arange(22050) / 44100 * 2 * np.pi * 110), 44100)
    hears(path, [note(0.0, 0.4, 45)])
    code, out = cli("transcribe", path)
    assert code == 0, out
    assert out["no_beat_map"] and out["notes"][0]["seconds"] == 0 and "bar" not in out["notes"][0]
    code, out = cli("transcribe", path, "--text")
    assert code == 1 and "beat map" in out["error"]


def test_a_failing_model_and_a_missing_one_say_why(made, tmp_path, model, monkeypatch):
    path = part(made, tmp_path)
    Path(f"{path}.notes.json").write_text(json.dumps({"fail": "out of memory"}))
    code, out = cli("transcribe", path)
    assert code == 1 and "out of memory" in out["error"]
    monkeypatch.delenv("AAW_TRANSCRIBER")
    code, out = cli("transcribe", path)
    assert code == 1 and "daw separate setup" in out["error"]


def test_arguments_are_checked(made, tmp_path, model):
    path = part(made, tmp_path)
    for args, words in [
        (["--track", "x"], "--track goes with --project"),
        (["--at", "4"], "--at goes with --project"),
        (["--range", "E1"], "range must look like"),
        (["--range", "G3-E1"], "low note is above"),
        (["--min-confidence", "2"], "min-confidence"),
        (["--midi", tmp_path / "x.txt"], "--midi is a .mid file"),
    ]:
        hears(path, [note(at(0), at(1), 42)])
        code, out = cli("transcribe", path, *args)
        assert code == 1 and words in out["error"], (args, out)


def test_the_listing_names_basic_pitch(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    listed = separation.listing()
    entry = next(m for m in listed["analysis"] if m["name"] == "basic-pitch")
    assert entry["license"].startswith("Apache-2.0") and entry["package"] == "basic-pitch==0.3.0"
    assert entry["here"] is False and entry["get"] == "daw separate setup" and entry["mb"] == 0
    site = tmp_path / "data/separation/env/lib/python3.12/site-packages/basic_pitch"
    site.mkdir(parents=True)
    assert next(m for m in separation.listing()["analysis"] if m["name"] == "basic-pitch")["here"] is True


def test_setup_installs_basic_pitch_alone(tmp_path, monkeypatch):
    calls = []
    monkeypatch.setattr(separation, "run_quietly", lambda command: calls.append([str(c) for c in command]))
    separation.install_without_dependencies(Path("uv"))
    assert calls == [["uv", "pip", "install", "--python", str(separation.env_python()), "--no-deps", "basic-pitch==0.3.0"]]
    also = separation.manifest()["runner"]["also"]
    assert {"mido==1.3.3", "mir-eval==0.8.2", "pretty-midi==0.2.11.post0", "resampy==0.4.3"} <= set(also)
