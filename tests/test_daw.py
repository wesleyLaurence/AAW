import json
import subprocess
import sys
import numpy as np
import pytest
import soundfile as sf
from agent_daw.model import Project, beat, frame, save, load, project_hash
from agent_daw.engine import render, schedule, Sampler, Voice
from agent_daw.library import scan, search, import_asset


@pytest.fixture
def song(tmp_path):
    sr = 48000
    t = np.arange(sr) / sr
    x = np.sin(2 * np.pi * 220 * t) * 0.4
    sf.write(tmp_path / "tone.wav", x, sr, subtype="FLOAT")
    data = {
        "session": {"tempo": 120, "length_beats": 4, "master_gain_db": -6},
        "samples": {"tone": {"path": "tone.wav", "root_note": "A3"}},
        "patterns": {
            "main": {
                "length_beats": 4,
                "events": [{"at": 0, "pad": "a", "note": "A3", "duration": 1}],
            }
        },
        "tracks": [
            {
                "id": "bass",
                "pads": {
                    "a": {
                        "sample": "tone",
                        "mode": "gate",
                        "release_ms": 10,
                        "attack_ms": 1,
                    }
                },
                "clips": [{"pattern": "main"}],
            }
        ],
    }
    p = Project.model_validate(data)
    save(p, tmp_path / "song.yaml")
    return tmp_path / "song.yaml", data


def test_exact_timing_no_drift():
    assert frame("1/3", 120, 48000) == 8000
    assert frame(80, 160, 48000) == 1440000
    assert frame("1000000/3", 143, 48000) == round(1000000 / 3 * 60 / 143 * 48000)
    assert beat("0.25") == beat("1/4")


@pytest.mark.parametrize(
    "change",
    [
        lambda d: d["session"].update(tempo=0),
        lambda d: d["session"].update(tempo=float("nan")),
        lambda d: d["session"].update(effects=["compressor"]),
        lambda d: d["tracks"][0]["pads"]["a"].update(sample="missing"),
        lambda d: d["tracks"][0]["clips"][0].update(at=4),
        lambda d: d["patterns"]["main"]["events"][0].update(at=-1),
        lambda d: d["patterns"]["main"]["events"][0].update(duration=None),
        lambda d: d["patterns"]["main"]["events"][0].update(note="H2"),
        lambda d: d["patterns"]["main"].update(steps={"a": "x..."}),
    ],
)
def test_validation(song, change):
    path, d = song
    change(d)
    with pytest.raises(ValueError):
        Project.model_validate(d)


def test_pitch_and_gate(song):
    path, d = song
    d["patterns"]["main"]["events"][0]["note"] = "A4"
    p = Project.model_validate(d)
    tr = schedule(p)[0]
    voice = Sampler(p, path.parent).voice(p.tracks[0].pads["a"], tr)
    x = voice.process(48000)
    # Octave up halves the original one-second sample.
    assert len(x) == 24000
    spec = abs(np.fft.rfft(x[1000:20000, 0]))
    peak = np.fft.rfftfreq(19000, 1 / 48000)[np.argmax(spec)]
    assert abs(peak - 440) < 3
    assert abs(x[-1]).max() == 0


def test_choke_and_swing(song):
    path, d = song
    d["tracks"][0]["pads"]["a"].update(mode="one_shot", choke_group="hat")
    d["patterns"]["main"] = {
        "length_beats": 4,
        "grid": "1/2",
        "swing": 0.6,
        "steps": {"a": "xx......"},
    }
    p = Project.model_validate(d)
    tr = schedule(p)
    assert tr[1].start == 14400
    assert tr[0].cutoff == 14400


def test_block_size_invariance_and_stems(song, tmp_path):
    path, d = song
    d["patterns"]["main"]["events"] += [
        {"at": "1/3", "pad": "a", "note": "C4", "duration": "1/3"},
        {"at": "11/4", "pad": "a", "duration": "1/2"},
    ]
    d["tracks"].append(
        {
            "id": "second",
            "gain_db": -10,
            "pan": 0.4,
            "pads": d["tracks"][0]["pads"],
            "clips": [{"pattern": "main"}],
        }
    )
    save(Project.model_validate(d), path)
    a = render(path, tmp_path / "a", 127)
    b = render(path, tmp_path / "b", 4096)
    assert a["audio_sha256"] == b["audio_sha256"]
    x, sr = sf.read(tmp_path / "a/mix.wav", always_2d=True)
    stems = sum(
        sf.read(f, always_2d=True)[0] for f in (tmp_path / "a/stems").glob("*.wav")
    )
    assert np.max(abs(x - stems)) < 2e-7
    assert len(x) == 96000 and sr == 48000
    assert a["mix"]["over_range_samples"] == 0


def test_mute_solo(song, tmp_path):
    path, d = song
    d["tracks"][0]["mute"] = True
    d["tracks"][0]["solo"] = True
    save(Project.model_validate(d), path)
    report = render(path, tmp_path / "silent")
    assert report["mix"]["peak_dbfs"] == -240


def test_clipping_refuses_publication(song, tmp_path):
    path, d = song
    d["session"]["master_gain_db"] = 24
    save(Project.model_validate(d), path)
    with pytest.raises(ValueError, match="Unsafe PCM"):
        render(path, tmp_path / "bad")
    assert not (tmp_path / "bad/mix.wav").exists()


def test_import_portability_and_tamper(song, tmp_path):
    path, d = song
    projectdir = tmp_path / "portable"
    projectdir.mkdir()
    asset = import_asset(tmp_path / "tone.wav", projectdir, "A3")
    d["samples"]["tone"] = asset.model_dump()
    save(Project.model_validate(d), projectdir / "song.yaml")
    (tmp_path / "tone.wav").unlink()
    assert load(projectdir / "song.yaml")
    (projectdir / asset.path).write_bytes(b"changed")
    with pytest.raises(ValueError, match="content changed"):
        load(projectdir / "song.yaml")


def test_index_and_literal_search(tmp_path):
    root = tmp_path / "pack"
    root.mkdir()
    sf.write(root / "kick_clean.wav", np.zeros(100), 44100)
    db = tmp_path / "index.sqlite"
    assert scan(root, db)["indexed"] == 1
    assert scan(root, db)["unchanged"] == 1
    assert len(search(db, "kick", category="kick")) == 1
    assert search(db, "no_result") == []
    (root / "kick_clean.wav").unlink()
    scan(root, db)
    assert search(db) == []


def test_format_and_revision_edit(song, tmp_path):
    path, d = song
    p = load(path)
    original = path.read_text()
    save(p, path)
    assert path.read_text() == original
    patch = tmp_path / "patch.json"
    patch.write_text(json.dumps({"session": {"tempo": 160}}))
    cmd = [
        sys.executable,
        "-m",
        "agent_daw.cli",
        "apply",
        str(path),
        str(patch),
        "--expect",
        project_hash(p),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    assert result.returncode == 0, result.stderr
    result = subprocess.run(cmd, capture_output=True, text=True)
    assert result.returncode == 1 and "Stale" in result.stderr
    assert load(path).session.tempo == 160


def test_region_is_full_render_slice(song, tmp_path):
    path, d = song
    d["sections"] = [{"id": "tail", "at": "1/2", "length_beats": 1}]
    save(Project.model_validate(d), path)
    render(path, tmp_path / "full")
    preview = render(path, tmp_path / "region", section="tail")
    full, sr = sf.read(tmp_path / "full/mix.wav")
    region, _ = sf.read(tmp_path / "region/mix.wav")
    assert np.array_equal(region, full[12000:36000])
    assert preview["target"]["source_frames"] == (12000, 36000)
    # Section preview must not replace the full-mix pointer.
    assert json.loads((path.parent / "renders/latest.json").read_text())[
        "directory"
    ] == str((tmp_path / "full").resolve())


def test_track_preview_and_missing_target(song, tmp_path):
    path, d = song
    d["tracks"].append(
        {
            "id": "other",
            "gain_db": -10,
            "pads": d["tracks"][0]["pads"],
            "clips": [{"pattern": "main"}],
        }
    )
    save(Project.model_validate(d), path)
    r = render(path, tmp_path / "track", track_id="bass")
    assert list(r["tracks"]) == ["bass"]
    with pytest.raises(ValueError, match="Unknown track"):
        render(path, track_id="absent")


def test_voice_release_continuity():
    v = Voice(np.ones((1000, 2)), 0, 100, 300)
    a = v.process(227)
    b = v.process(1000)
    x = np.concatenate([a, b])
    assert len(x) == 400
    assert x[-1, 0] == 0
    assert np.max(abs(np.diff(x[:, 0]))) <= 0.010001


def test_resampling_44100_to_48000(song, tmp_path):
    path, d = song
    sf.write(
        path.parent / "tone.wav",
        np.sin(2 * np.pi * 220 * np.arange(44100) / 44100) * 0.4,
        44100,
        subtype="FLOAT",
    )
    p = Project.model_validate(d)
    s = Sampler(p, path.parent)
    audio = s.prepare(p.tracks[0].pads["a"], p.patterns["main"].events[0])
    assert len(audio) == 48000
    n = 48000
    freq = np.argmax(abs(np.fft.rfft(audio[:, 0]))) * 48000 / n
    assert abs(freq - 220) < 1


def test_output_refuses_different_render(song, tmp_path):
    path, d = song
    render(path, tmp_path / "shared")
    original = (tmp_path / "shared/mix.wav").read_bytes()
    d["session"]["tempo"] = 130
    save(Project.model_validate(d), path)
    with pytest.raises(ValueError, match="different render"):
        render(path, tmp_path / "shared")
    assert (tmp_path / "shared/mix.wav").read_bytes() == original


def test_tiny_session_rejected_before_export(tmp_path):
    path = tmp_path / "song.yaml"
    save(Project.model_validate({"session": {"length_beats": "1/1000000000"}}), path)
    with pytest.raises(ValueError, match="at least one audio frame"):
        render(path)


def test_malformed_yaml_has_json_error(tmp_path):
    path = tmp_path / "song.yaml"
    path.write_text("session: [oops")
    result = subprocess.run(
        [sys.executable, "-m", "agent_daw.cli", "check", str(path)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 1
    assert "error" in json.loads(result.stderr)
