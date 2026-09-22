import json
import subprocess
import sys

import numpy as np
import pytest
import soundfile as sf

from agent_daw.analysis import analyze, compare_root, measure_pitch
from agent_daw.library import analyze as analyze_indexed, analyze_all, scan, search
from agent_daw.model import Project, save

SR = 48000


def harmonic(hz, seconds=2.0, weights=(1, 0.6, 0.4, 0.3, 0.2, 0.1), decay=1.5):
    t = np.arange(round(SR * seconds)) / SR
    x = sum(w * np.sin(2 * np.pi * hz * (k + 1) * t) for k, w in enumerate(weights))
    return 0.2 * x * np.exp(-decay * t)


def drop_808(hz=41.2, seconds=1.5):
    t = np.arange(round(SR * seconds)) / SR
    phase = 2 * np.pi * np.cumsum(hz + 0.6 * hz * np.exp(-t / 0.03)) / SR
    return 0.8 * (np.sin(phase) + 0.3 * np.sin(2 * phase)) * np.exp(-2 * t)


def drum_loop(bpm, bars):
    n = round(bars * 4 * 60 / bpm * SR)
    x = np.zeros(n)
    rng = np.random.default_rng(0)
    eighth = 30 / bpm
    for i in range(bars * 8):
        at = round(i * eighth * SR)
        if i % 2 == 0:
            kick = drop_808(55, 0.25)[: n - at]
            x[at : at + len(kick)] += 0.5 * kick
        m = round(0.03 * SR)
        hat = (rng.standard_normal(m) * np.exp(-np.arange(m) / (0.005 * SR)) * 0.2)[
            : n - at
        ]
        x[at : at + len(hat)] += hat
    return x


def write(path, x):
    sf.write(path, x, SR, subtype="FLOAT")
    return path


@pytest.mark.parametrize(
    "signal, note, cents",
    [
        (harmonic(110), "A2", 0),
        (harmonic(55, weights=(0.3, 1, 0.8, 0.5, 0.3)), "A1", 0),  # weak fundamental
        (drop_808(), "E1", 0),  # pitch drop at the attack
        (harmonic(440 * 2 ** (0.4 / 12)), "A4", 40),
        (np.stack([harmonic(220), 0.5 * harmonic(220)], axis=1), "A3", 0),
    ],
)
def test_pitch_octave_and_cents(tmp_path, signal, note, cents):
    report = analyze(write(tmp_path / "s.wav", signal))
    pitch = report["pitch"]
    assert pitch["pitched"] and pitch["note"] == note
    assert abs(pitch["cents"] - cents) <= 3
    assert report["rhythm"]["kind"] == "one_shot"


def test_noise_and_chord_are_not_reported_as_pitched(tmp_path):
    rng = np.random.default_rng(1)
    noise = rng.standard_normal(SR) * np.exp(-np.arange(SR) / SR * 5) * 0.3
    assert not analyze(write(tmp_path / "n.wav", noise))["pitch"]["pitched"]
    chord = harmonic(261.63) + harmonic(329.63) + harmonic(392.0)
    pitch = measure_pitch(chord, SR)
    assert not pitch["pitched"] and pitch["fundamental_support_db"] < -30


@pytest.mark.parametrize(
    "bpm, bars, rival", [(128, 2, 64), (90, 1, 180), (174, 4, None)]
)
def test_loop_tempo_from_length_and_grid(tmp_path, bpm, bars, rival):
    rhythm = analyze(write(tmp_path / "loop.wav", drum_loop(bpm, bars)))["rhythm"]
    assert rhythm["kind"] == "loop"
    assert rhythm["tempo"]["bpm"] == pytest.approx(bpm, abs=0.01)
    assert rhythm["tempo"]["bars"] == bars
    assert (rival in rhythm["tempo"]["ambiguous_with"]) if rival else True


def test_loop_not_cut_to_whole_bars(tmp_path):
    x = np.concatenate([drum_loop(97, 8), np.zeros(4321)])
    rhythm = analyze(write(tmp_path / "loop.wav", x))["rhythm"]
    assert rhythm["kind"] == "loop" and not rhythm["tempo"]["fits_whole_bars"]
    assert rhythm["tempo"]["bpm"] == pytest.approx(97, abs=0.2)


def test_leading_silence_and_filename_hint(tmp_path):
    x = np.concatenate([np.zeros(SR // 10), harmonic(110, 1)])
    report = analyze(write(tmp_path / "s.wav", x))
    assert report["level"]["first_sound_seconds"] == pytest.approx(0.1, abs=0.002)
    loop = write(tmp_path / "loop_128.wav", drum_loop(128, 2))
    assert analyze(loop, 128)["rhythm"]["filename_bpm_hint"]["agrees"]
    hint = analyze(loop, 100)["rhythm"]["filename_bpm_hint"]
    assert not hint["agrees"] and not hint["fits_whole_bars"]


def test_silent_file(tmp_path):
    report = analyze(write(tmp_path / "s.wav", np.zeros(SR)))
    assert report["level"]["silent"] and not report["pitch"]["pitched"]


def test_compare_root_statuses():
    pitch = measure_pitch(harmonic(110), SR)
    assert compare_root("A2", pitch)["status"] == "ok"
    assert compare_root("A3", pitch)["status"] == "octave_mismatch"
    assert compare_root("C3", pitch)["status"] == "note_mismatch"
    detuned = compare_root("A4", measure_pitch(harmonic(440 * 2 ** (0.4 / 12)), SR))
    assert detuned["status"] == "detuned"
    assert detuned["suggested_pad_transpose"] == pytest.approx(-0.4, abs=0.03)


def test_index_cache_and_measured_search(tmp_path):
    pack = tmp_path / "pack"
    pack.mkdir()
    write(pack / "bass_a.wav", harmonic(110, weights=(1, 0.5)))
    write(pack / "bass_e.wav", drop_808())
    write(pack / "loop_128.wav", drum_loop(128, 2))
    db = tmp_path / "index.sqlite"
    scan(pack, db)
    assert all(r["measured"] is None for r in search(db))
    assert search(db, pitched=True) == []
    summary = analyze_all(db)
    assert summary["analyzed"] == 3 and not summary["errors"]
    assert analyze_all(db)["unchanged"] == 3
    assert analyze_indexed(db, pack / "bass_a.wav")["cached"]
    low = search(db, note_range=(28, 35))  # E1–B1
    assert [r["name"] for r in low] == ["bass_e.wav"]
    assert low[0]["measured"]["note"] == "E1"
    assert {r["name"] for r in search(db, pitched=True)} == {"bass_a.wav", "bass_e.wav"}
    loops = search(db, measured_kind="loop", measured_bpm=128)
    assert [r["name"] for r in loops] == ["loop_128.wav"]
    write(
        pack / "bass_a.wav", harmonic(220, weights=(1, 0.5))
    )  # edit invalidates cache
    scan(pack, db)
    row = next(r for r in search(db) if r["name"] == "bass_a.wav")
    assert row["measured"] is None
    assert analyze_all(db)["analyzed"] == 1
    (pack / "bass_e.wav").unlink()
    scan(pack, db)
    assert len(search(db, pitched=True)) == 1


def run(*args):
    proc = subprocess.run(
        [sys.executable, "-m", "agent_daw.cli", *map(str, args)],
        capture_output=True,
        text=True,
    )
    return proc.returncode, json.loads(proc.stdout or proc.stderr)


def test_cli_auto_root_and_check_warnings(tmp_path):
    source = write(tmp_path / "bass.wav", harmonic(110))
    noise = write(
        tmp_path / "noise.wav", np.random.default_rng(2).standard_normal(SR) * 0.1
    )
    project = tmp_path / "song" / "song.yaml"
    project.parent.mkdir()
    save(Project.model_validate({"session": {"tempo": 120}}), project)
    db = tmp_path / "index.sqlite"
    code, result = run(
        "samples",
        "--db",
        db,
        "import",
        source,
        "--project",
        project,
        "--id",
        "bass",
        "--root-note",
        "auto",
    )
    assert code == 0 and result["root_note"] == "A2"
    assert result["measured_pitch"]["note"] == "A2"
    code, result = run(
        "samples",
        "--db",
        db,
        "import",
        noise,
        "--project",
        project,
        "--id",
        "noise",
        "--root-note",
        "auto",
    )
    assert code == 1 and "No reliable pitch" in result["error"]
    code, result = run("check", project)
    assert code == 0 and result["root_notes"]["bass"]["status"] == "ok"
    assert result["warnings"] == []
    text = project.read_text().replace("root_note: A2", "root_note: A3")
    project.write_text(text)
    code, result = run("check", project)
    assert result["root_notes"]["bass"]["status"] == "octave_mismatch"
    assert "octave_mismatch" in result["warnings"][0]
    code, result = run("samples", "--db", db, "analyze", source)
    assert code == 0 and result["pitch"]["note"] == "A2" and not result["indexed"]
    code, result = run("samples", "--db", db, "search", "--note-range", "C2")
    assert code == 1 and "note-range" in result["error"]
