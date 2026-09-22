"""Controlled audio fixtures test the meaning of reports, not just their shape."""

import json
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw.engine import render
from agent_daw.model import Project, save
from agent_daw.perception import analyze, compare, listen, measure

RATE = 48000


def tone(frequency=1000, seconds=4, amplitude=0.1):
    return amplitude * np.sin(
        2 * np.pi * frequency * np.arange(round(seconds * RATE)) / RATE
    )


def write(path, data):
    sf.write(path, data, RATE, subtype="FLOAT")
    return path


def test_known_loudness_and_stereo():
    x = tone(amplitude=0.1)
    mono = measure(x[:, None], RATE)
    # A full-scale 1 kHz sine is approximately -3.05 LUFS in mono.
    assert mono["integrated_lufs"] == pytest.approx(-23.05, abs=0.1)
    assert mono["rms_dbfs"] == pytest.approx(-23.0103, abs=0.001)
    assert mono["crest_db"] == pytest.approx(3.0103, abs=0.001)
    assert mono["band_fraction"]["mid"] > 0.999
    stereo = measure(np.column_stack([x, x]), RATE)
    assert stereo["integrated_lufs"] - mono["integrated_lufs"] == pytest.approx(
        3.0103, abs=0.001
    )
    assert stereo["stereo_correlation"] == pytest.approx(1)
    assert stereo["side_energy_fraction"] == pytest.approx(0)
    inverted = measure(np.column_stack([x, -x]), RATE)
    assert inverted["stereo_correlation"] == pytest.approx(-1)
    assert inverted["side_energy_fraction"] == pytest.approx(1)
    assert inverted["band_dbfs"] == pytest.approx(stereo["band_dbfs"])


def test_gain_comparison_and_timeline(tmp_path):
    x = tone()
    before = write(tmp_path / "before.wav", x)
    after = write(tmp_path / "after.wav", x * 10 ** (6 / 20))
    report = compare(before, after, images=False)
    assert report["matching"]["after_gain_db"] == pytest.approx(-6, abs=1e-5)
    assert report["mix"]["actual_delta"]["rms_dbfs"] == pytest.approx(6, abs=1e-5)
    assert report["mix"]["loudness_matched_delta"]["rms_dbfs"] == pytest.approx(
        0, abs=1e-5
    )
    assert all(
        v["matched_rms_delta_db"] == pytest.approx(0, abs=1e-5)
        for v in report["timeline"]
    )
    assert before.exists() and after.exists()
    data, _ = analyze(before)
    assert data["musical_context"] is None
    assert data["timeline"]["short_term_loudness"][0]["lufs"] == pytest.approx(
        -23.05, abs=0.1
    )


def test_frequency_change_and_silence(tmp_path):
    before = write(tmp_path / "low.wav", tone(100))
    after = write(tmp_path / "high.wav", tone(5000))
    report = compare(before, after, images=False)
    bands = report["mix"]["actual_delta"]["band_fraction"]
    assert bands["low"] < -0.99
    assert bands["high"] > 0.99
    silence = write(tmp_path / "silence.wav", np.zeros((RATE, 2)))
    report = listen(silence, images=False)
    assert report["mix"]["silent"]
    assert report["mix"]["integrated_lufs"] is None
    assert report["mix"]["rms_dbfs"] is None
    assert report["mix"]["stereo_correlation"] is None
    assert not report["timeline"]["short_term_loudness"]
    report = compare(silence, before, images=False)
    assert report["matching"]["after_gain_db"] is None
    assert report["mix"]["loudness_matched_delta"]["rms_dbfs"] is None
    assert report["timeline_aligned"] is False
    assert report["timeline"] == []
    json.dumps(report, allow_nan=False)


@pytest.fixture
def rendered(tmp_path):
    write(tmp_path / "source.wav", tone(100, seconds=4))
    p = Project.model_validate(
        {
            "session": {"tempo": 120, "length_beats": 8, "end_fade_ms": 0},
            "samples": {"tone": {"path": "source.wav"}},
            "patterns": {
                "hit": {"length_beats": 4, "events": [{"at": 0, "pad": "tone"}]}
            },
            "tracks": [
                {
                    "id": "bass",
                    "pads": {"tone": {"sample": "tone"}},
                    "clips": [{"pattern": "hit", "repeats": 2}],
                }
            ],
            "sections": [
                {"id": "intro", "at": 0, "length_beats": 4},
                {"id": "drop", "at": 4, "length_beats": 4},
            ],
        }
    )
    project = tmp_path / "song.yaml"
    save(p, project)
    full = render(project)
    preview = render(project, section="drop")
    return project, p, Path(full["directory"]), Path(preview["directory"])


def test_snapshot_sections_preview_and_hashes(rendered):
    path, p, full, preview = rendered
    # Analysis must work after original assets disappear and the project changes.
    (path.parent / "source.wav").unlink()
    p.session.tempo = 160
    save(p, path)
    a, _ = analyze(full)
    b, _ = analyze(preview)
    assert a["source"]["tempo"] == 120
    assert b["source"]["start_seconds"] == 2
    assert set(b["sections"]) == {"drop"}
    assert b["sections"]["drop"]["audio"]["rms_dbfs"] == pytest.approx(
        a["sections"]["drop"]["audio"]["rms_dbfs"]
    )
    assert b["timeline"]["energy"][0]["at_beat"] == 4
    regions = a["musical_context"]["tracks"]["bass"]["regions"]
    assert regions[0]["trigger_count"] == 2
    assert regions[0]["pattern_occurrences"] == {"hit": 2}
    assert b["musical_context"]["tracks"]["bass"]["regions"][0]["trigger_count"] == 1
    assert a["tracks"]["bass"]["hash_verified_against_render"]
    stem = full / "stems" / "bass.wav"
    x, sr = sf.read(stem)
    sf.write(stem, x * 0.5, sr, subtype="FLOAT")
    with pytest.raises(ValueError, match="Stem hash mismatch"):
        analyze(full)


def test_render_resolution_cli_and_images(rendered):
    path, _, full, _ = rendered
    pointer = path.parent / "renders" / "latest.json"
    data = listen(pointer)
    assert Path(data["images"][0]).read_bytes().startswith(b"\x89PNG")
    for source in (full, full / "report.json", full / "mix.wav"):
        other, _ = analyze(source)
        assert other["mix"] == data["mix"]
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "agent_daw.cli",
            "compare",
            str(pointer),
            str(full),
            "--no-images",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    output = json.loads(result.stdout)
    assert output["mix"]["actual_delta"]["rms_dbfs"] == 0
    compared = compare(pointer, full)
    assert Path(compared["images"][0]).is_file()


def test_invalid_audio_and_short_measurements(tmp_path):
    short = write(tmp_path / "short.wav", tone(seconds=0.01))
    report = listen(short)
    assert report["mix"]["integrated_lufs"] is None
    invalid = write(tmp_path / "invalid.wav", np.zeros((100, 3)))
    with pytest.raises(ValueError, match="mono/stereo"):
        analyze(invalid)
    nonfinite = write(tmp_path / "nan.wav", np.full(100, np.nan))
    with pytest.raises(ValueError, match="finite"):
        analyze(nonfinite)
    result = subprocess.run(
        [sys.executable, "-m", "agent_daw.cli", "listen", str(invalid)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 1
    assert "error" in json.loads(result.stderr)


def test_mix_and_snapshot_tamper_rejected(rendered):
    _, _, full, preview = rendered
    mix = full / "mix.wav"
    write(mix, tone())
    with pytest.raises(ValueError, match="mix hash mismatch"):
        analyze(full)
    snapshot = preview / "song.snapshot.yaml"
    snapshot.write_text(snapshot.read_text().replace("tempo: 120", "tempo: 121"))
    with pytest.raises(ValueError, match="snapshot hash mismatch"):
        analyze(preview)


def test_localized_changes_and_musical_deltas(rendered):
    path, p, full, _ = rendered
    p.patterns["busier"] = p.patterns["hit"].model_copy(deep=True)
    from agent_daw.model import Clip, Event

    p.patterns["busier"].events.append(Event(at=2, pad="tone"))
    p.tracks[0].clips = [Clip(pattern="hit", at=0), Clip(pattern="busier", at=4)]
    p.tracks[0].gain_db = -3
    save(p, path)
    after = render(path)
    result = compare(full, Path(after["directory"]), images=False)
    assert result["sections"]["intro"]["actual_delta"]["rms_dbfs"] == pytest.approx(
        -3, abs=1e-4
    )
    assert result["sections"]["drop"]["actual_delta"]["rms_dbfs"] > -3
    regions = result["musical_context_delta"]["bass"]
    assert regions[1]["trigger_count_delta"] == 0
    assert regions[2]["trigger_count_delta"] == 1
    assert regions[2]["pattern_occurrences_after"] == {"busier": 1}
    assert result["tracks"]["bass"]["sections"]["intro"]["actual_delta"][
        "rms_dbfs"
    ] == pytest.approx(-3, abs=1e-5)


def test_changed_section_bounds_and_track_ids(rendered):
    path, p, full, _ = rendered
    p.sections[1].at = 5
    p.sections[1].length_beats = 3
    p.tracks[0].id = "new_bass"
    save(p, path)
    after = render(path)
    report = compare(full, Path(after["directory"]), images=False)
    assert set(report["sections"]) == {"intro"}
    assert report["unmatched_sections"] == {"before": ["drop"], "after": ["drop"]}
    assert report["unmatched_tracks"] == {"before": ["bass"], "after": ["new_bass"]}


def test_one_frame_and_old_manifest(rendered, tmp_path):
    short = write(tmp_path / "one.wav", np.zeros(1))
    report = listen(short)
    assert Path(report["images"][0]).is_file()
    _, _, full, _ = rendered
    manifest_path = full / "report.json"
    manifest = json.loads(manifest_path.read_text())
    del manifest["tracks"]["bass"]["audio_sha256"]
    manifest_path.write_text(json.dumps(manifest))
    report, _ = analyze(full)
    assert not report["tracks"]["bass"]["hash_verified_against_render"]


def test_44100_clipping_and_one_sided_stereo(tmp_path):
    sr = 44100
    tone441 = 1.1 * np.cos(2 * np.pi * 1000 * np.arange(sr) / sr)
    path = tmp_path / "441.wav"
    sf.write(path, np.column_stack([tone441, np.zeros(sr)]), sr, subtype="FLOAT")
    report, _ = analyze(path)
    assert report["source"]["sample_rate"] == sr
    assert report["mix"]["over_range_samples"] > 0
    assert report["mix"]["stereo_correlation"] is None
    assert report["mix"]["side_energy_fraction"] == pytest.approx(0.5)
    assert report["mix"]["peak_dbfs"] > 0
