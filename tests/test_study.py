"""`daw samples study`: every measure run on a generated song and the parts
`daw separate` would have made of it, with stand-ins for the note model and
Whisper, kept in one JSON with each measure's state and confidence, a table of
the bars and a markdown summary; a song without parts, a measure that fails and
what the command refuses."""

import json
import shlex
import shutil
import sys
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import sound, study
from helpers import chord_song, cli

NOTES_STAND_IN = Path(__file__).with_name("stand_in_transcriber.py")
WORDS_STAND_IN = Path(__file__).with_name("stand_in_words.py")
RATE = 44100
BPM = 120.0
BEAT = 60 / BPM
LOOP = [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]]
NAME = "0123456789ab_song.wav"


def drum_part(seconds):
    """A kick on beats 1 and 3, a snare on 2 and 4 and a hat on every eighth."""
    g = np.random.default_rng(3)
    x = np.zeros(int(seconds * RATE))
    t = np.arange(int(0.15 * RATE)) / RATE
    kick = 0.9 * np.sin(2 * np.pi * (50 + 100 * np.exp(-t * 40)) * t) * np.exp(-t * 20)
    snare = 0.5 * g.standard_normal(len(t)) * np.exp(-t * 30)
    hat = 0.2 * np.diff(g.standard_normal(len(t) + 1)) * np.exp(-t * 80)
    for i, at in enumerate(np.arange(0, seconds - 1, BEAT / 2)):
        start = int(at * RATE)
        end = min(start + len(t), len(x))
        x[start:end] += hat[: end - start]
        if i % 2 == 0:
            x[start:end] += (kick if (i // 2) % 2 == 0 else snare)[: end - start]
    return np.column_stack([x, x])


def voice_part(seconds, sung):
    """A tone where `sung` lists (from, to) seconds and silence elsewhere."""
    t = np.arange(int(seconds * RATE)) / RATE
    x = np.zeros(len(t))
    for a, b in sung:
        inside = (t >= a) & (t < b)
        x[inside] = 0.3 * np.sin(2 * np.pi * 330 * t[inside])
    return np.column_stack([x, x])


def notes_for(path, notes):
    """What the stand-in note model hears in a part."""
    Path(f"{path}.notes.json").write_text(json.dumps({"notes": notes}))


@pytest.fixture
def stand_ins(monkeypatch, tmp_path):
    return set_stand_ins(monkeypatch, tmp_path)


def make_project(folder: Path, parts=True) -> Path:
    """A project of a 32-bar song at 120 BPM, and with `parts` the drums, bass,
    other and vocals daw separate would have made of it, by their recipe."""
    (folder / "samples").mkdir(parents=True)
    (folder / "song.yaml").write_text("session:\n  tempo: 120\n")
    song = chord_song(None, LOOP * 8, bpm=BPM, drums=False)
    seconds = len(song) / RATE + 1
    pad = np.zeros(int(seconds * RATE))
    pad[: len(song)] = song
    other = np.column_stack([pad, pad]) * 0.5
    drums = drum_part(seconds)
    vocals = voice_part(seconds, [(16 * BEAT * k + 0.2, 16 * BEAT * k + 7.5) for k in range(4)])
    mix = other + drums + vocals
    sf.write(folder / "samples" / NAME, 0.8 * mix / np.abs(mix).max(), RATE, subtype="PCM_16")
    if not parts:
        return folder / "samples" / NAME
    (folder / "separated").mkdir()
    record = {
        "source": {"path": f"samples/{NAME}", "sha256": "0"},
        "split": "stems",
        "model": {"name": "stand-in"},
        "residual_db": -40.0,
        "parts": {},
        "made": "2026-11-18T00:00:00+00:00",
    }
    for name, x in {"drums": drums, "bass": other, "other": other, "vocals": vocals}.items():
        path = f"separated/0123456789ab-m-{name}.wav"
        sf.write(folder / path, x.astype(np.float32), RATE, subtype="FLOAT")
        record["parts"][name] = {"path": path, "sha256": "0"}
    (folder / "separated" / "0123456789ab-m.separated.json").write_text(json.dumps(record))
    return folder / "samples" / NAME


def spoken(start, text, each=0.25, p=0.95):
    words = [[start + i * each, start + (i + 1) * each - 0.02, f" {w}", p] for i, w in enumerate(text.split())]
    return {"text": text, "words": words}


def set_stand_ins(mp, folder):
    script = folder / "heard.json"
    mp.setenv("AAW_TRANSCRIBER", shlex.join([sys.executable, str(NOTES_STAND_IN)]))
    mp.setenv("AAW_WORDS", shlex.join([sys.executable, str(WORDS_STAND_IN)]))
    mp.setenv("AAW_WORDS_SCRIPT", str(script))
    mp.setenv("AAW_LIBRARY", str(folder / "library.sqlite"))
    mp.setenv("AAW_DATA_DIR", str(folder / "data"))
    return script


@pytest.fixture(scope="module")
def studied(tmp_path_factory):
    tmp_path = tmp_path_factory.mktemp("study")
    with pytest.MonkeyPatch.context() as mp:
        stand_ins = set_stand_ins(mp, tmp_path)
        return study_of(tmp_path, stand_ins)


def study_of(tmp_path, stand_ins):
    source = make_project(tmp_path / "project")
    parts = source.parent.parent / "separated"
    notes_for(parts / "0123456789ab-m-bass.wav", [[1.0, 1.4, 45, 0.9, 0, 0], [3.0, 3.4, 41, 0.3, 0, 0]])
    notes_for(parts / "0123456789ab-m-other.wav", [[1.0, 2.9, 57, 0.8, 0, 0], [1.0, 2.9, 60, 0.7, 0, 0]])
    notes_for(parts / "0123456789ab-m-vocals.wav", [[0.5, 1.0, 64, 0.6, 0, 0]])
    stand_ins.write_text(
        json.dumps({"whisper-large-v3-turbo": [spoken(0.3 + 8 * k, "nothing can hold us down tonight") for k in range(4)]})
    )
    code, out = cli("samples", "study", source)
    assert code == 0, out
    return source, out, json.loads(Path(out["study"]).read_text())


def test_every_measure_is_run_and_kept_in_one_file(studied):
    source, out, whole = studied
    folder = source.parent.parent / "study" / "song"
    assert Path(out["folder"]) == folder.resolve() and out["song"] == "song"
    assert out["measured"] == list(study.MEASURES), out
    assert whole["kind"] == "study" and whole["schema_version"] == study.SCHEMA
    assert whole["song"]["sha256"] and whole["song"]["sample_rate"] == RATE
    assert whole["parts"]["model"] == "stand-in" and whole["parts"]["residual_db"] == -40.0
    for name, entry in whole["measures"].items():
        assert entry["state"] == "measured", (name, entry)
        assert set(entry["confidence"]) == {"value", "of"} and entry["confidence"]["of"], name
    m = whole["measures"]
    assert m["grid"]["tempo"]["bpm"] == pytest.approx(BPM, abs=1) and m["grid"]["bar_count"] >= 30
    # The notes' confidence is the share of sure notes over every part read.
    assert set(m["notes"]["parts"]) == {"bass", "other", "vocals"}
    assert m["notes"]["confidence"]["value"] == pytest.approx(4 / 5, abs=0.001)
    assert m["notes"]["parts"]["bass"]["line"] is True and m["notes"]["parts"]["other"]["line"] is False
    assert m["words"]["hook"] == "nothing can hold us down tonight" and m["words"]["confidence"]["value"] == 1.0
    assert m["tuning"]["read_from"] == "other" and abs(m["tuning"]["cents"]) < 5
    assert m["drums"]["count"] > 0 and m["drums"]["grooves"]
    assert set(m["sound"]["sound"]) == {"mix", "drums", "bass", "other", "vocals"}
    assert m["master"]["mix"]["integrated_lufs"] < 0 and m["master"]["sections"]
    assert m["master"]["sections"][0]["label"] == m["form"]["sections"][0]["label"]
    for written in ("study.json", "study.md", "form.png", "sound.png", "overview.png", "spectrum.png",
                    "lyrics.txt", "bass.mid", "other.mid", "vocals.mid", "drums.mid"):
        assert (folder / written).is_file(), written
        assert written in out["files"]


def test_the_bars_hold_every_measure_side_by_side(studied):
    _, _, whole = studied
    bars = whole["bars"]
    assert len(bars) == whole["measures"]["grid"]["bar_count"] and bars[0]["bar"] == 1
    assert all("section" in b and b["chords"] for b in bars)
    assert any("groove" in b for b in bars)
    assert any(b.get("words", "").startswith("nothing can hold us") for b in bars)
    # Each bass note's name in the bar it plays in.
    by_bar = {b["bar"]: b for b in bars}
    placed = [n for n in whole["measures"]["notes"]["parts"]["bass"]["notes"] if n["bar"] >= 1]
    assert placed
    for n in placed:
        assert n["pitch"] in by_bar[n["bar"]]["bass"]


def test_the_summary_says_what_each_measure_found(studied):
    _, out, whole = studied
    text = Path(out["summary"]).read_text()
    assert text.startswith("# Study of song")
    for heading in ("## At a glance", "## Measures", "## Sections", "## Harmony", "## Words", "## Notes",
                    "## Drums", "## Sound", "## Master", "## Bars", "## Not measured"):
        assert heading in text, heading
    assert f"{whole['measures']['grid']['tempo']['bpm']:g} BPM" in text
    assert "nothing can hold us down tonight" in text
    assert out["at_a_glance"]["form"] == whole["measures"]["form"]["form"]


def test_a_song_without_parts_says_how_to_get_them(tmp_path, stand_ins):
    source = make_project(tmp_path / "project", parts=False)
    code, out = cli("samples", "study", source, "--out", tmp_path / "elsewhere")
    assert code == 0, out
    assert Path(out["folder"]) == (tmp_path / "elsewhere").resolve()
    assert set(out["measured"]) == {"grid", "harmony", "tuning", "form", "sound", "master"}
    for name in ("words", "notes", "drums"):
        assert "daw separate" in out["not_measured"][name], name
    whole = json.loads(Path(out["study"]).read_text())
    assert whole["parts"] is None and "daw separate" in whole["no_parts"]
    assert whole["measures"]["tuning"]["read_from"] == "mix"
    assert "- words: No voice part" in Path(out["summary"]).read_text()


def test_a_measure_that_fails_leaves_the_rest_to_run(tmp_path, stand_ins, monkeypatch):
    source = make_project(tmp_path / "project", parts=False)

    def broken(*args, **kwargs):
        raise RuntimeError("a fault in the measure")

    monkeypatch.setattr(sound, "measure", broken)
    out = study.run(source, tmp_path / "library.sqlite", skip=["words"])
    whole = json.loads(Path(out["study"]).read_text())
    assert whole["measures"]["sound"]["state"] == "failed"
    assert whole["measures"]["sound"]["error"] == "RuntimeError: a fault in the measure"
    assert out["failed"] == {"sound": "RuntimeError: a fault in the measure"}
    assert whole["measures"]["words"]["state"] == "skipped" and out["skipped"] == {"words": "skipped"}
    assert whole["measures"]["master"]["state"] == "measured"


def test_a_study_run_again_leaves_nothing_stale(tmp_path, stand_ins):
    source = make_project(tmp_path / "project", parts=False)
    folder = source.parent.parent / "study" / "song"
    folder.mkdir(parents=True)
    (folder / "bass.mid").write_bytes(b"old")
    (folder / "mine.txt").write_text("the person's own")
    code, out = cli("samples", "study", source, "--skip", "sound", "--skip", "words")
    assert code == 0, out
    assert not (folder / "bass.mid").exists() and not (folder / "sound.png").exists()
    assert (folder / "mine.txt").read_text() == "the person's own"


@pytest.mark.parametrize(
    "args, said",
    [
        (["--skip", "grid"], "grid cannot be skipped"),
        (["--skip", "tempo"], "No measure tempo"),
    ],
)
def test_what_the_command_refuses(tmp_path, stand_ins, args, said):
    source = make_project(tmp_path / "project", parts=False)
    code, out = cli("samples", "study", source, *args)
    assert code != 0 and said in out["error"], out


def test_a_file_outside_a_project_takes_out(tmp_path, stand_ins):
    source = make_project(tmp_path / "project", parts=False)
    loose = tmp_path / "loose.wav"
    shutil.copy(source, loose)
    code, out = cli("samples", "study", loose)
    assert code != 0 and "give --out DIR" in out["error"], out


@pytest.mark.parametrize(
    "key, number",
    [("C major", "8B"), ("A minor", "8A"), ("G major", "9B"), ("E minor", "9A"), ("F# minor", "11A"),
     ("Bb major", "6B"), ("B major", "1B"), ("G# minor", "1A"), (None, None)],
)
def test_the_camelot_number(key, number):
    assert study.camelot(key) == number
