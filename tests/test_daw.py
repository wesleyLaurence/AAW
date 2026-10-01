import copy
import hashlib
import json
import numpy as np
import pytest
import soundfile as sf
import yaml
from agent_daw.model import (
    beat,
    frame,
    save,
    load,
    project_hash,
    hash_matches,
    schedule,
    validate,
)
from agent_daw.library import scan, search, import_asset
from helpers import cli, render, stem


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
    save(data, tmp_path / "song.yaml")
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
        validate(d)


def test_pitch_and_gate(song, tmp_path):
    path, d = song
    d["patterns"]["main"]["events"][0]["note"] = "A4"
    save(d, path)
    render(path, tmp_path / "out")
    x = stem(tmp_path / "out", "bass")
    # Octave up halves the original one-second sample, which the gate then
    # releases on the same frame: one beat at 120 BPM.
    assert np.flatnonzero(x.any(axis=1))[-1] == 23998
    spec = abs(np.fft.rfft(x[1000:20000, 0]))
    peak = np.fft.rfftfreq(19000, 1 / 48000)[np.argmax(spec)]
    assert abs(peak - 440) < 3


def test_choke_and_swing(song):
    path, d = song
    d["tracks"][0]["pads"]["a"].update(mode="one_shot", choke_group="hat")
    d["patterns"]["main"] = {
        "length_beats": 4,
        "grid": "1/2",
        "swing": 0.6,
        "steps": {"a": "xx......"},
    }
    tr = schedule(d)
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
    save(d, path)
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
    save(d, path)
    report = render(path, tmp_path / "silent")
    assert report["mix"]["peak_dbfs"] == -240


def test_clipping_refuses_publication(song, tmp_path):
    path, d = song
    d["session"]["master_gain_db"] = 24
    save(d, path)
    with pytest.raises(ValueError, match="Unsafe PCM"):
        render(path, tmp_path / "bad")
    assert not (tmp_path / "bad/mix.wav").exists()


def test_import_portability_and_tamper(song, tmp_path):
    path, d = song
    projectdir = tmp_path / "portable"
    projectdir.mkdir()
    asset = import_asset(tmp_path / "tone.wav", projectdir, "A3")
    d["samples"]["tone"] = asset
    save(d, projectdir / "song.yaml")
    (tmp_path / "tone.wav").unlink()
    assert load(projectdir / "song.yaml")
    (projectdir / asset["path"]).write_bytes(b"changed")
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
    cmd = ["apply", path, patch, "--expect", project_hash(p)]
    code, result = cli(*cmd)
    assert code == 0, result
    code, result = cli(*cmd)
    assert code == 1 and "Stale" in result["error"]
    assert load(path)["session"]["tempo"] == 160


def test_project_hash_fingerprints_the_saved_form(song):
    # A field added later and left at its default is not saved, so it must not
    # change the fingerprint that renders and apply --expect rely on.
    path, _ = song
    saved = yaml.safe_load(path.read_text())
    del saved["schema_version"]
    expected = hashlib.sha256(json.dumps(saved, sort_keys=True).encode()).hexdigest()
    assert project_hash(load(path)) == expected


def projects_by_schema():
    """A project using every model of each schema that hashed the full dump, newest
    first: automation, sends and returns, effects, and the first schema."""
    pads = {"k": {"sample": "hit"}}
    first = {
        "session": {"tempo": 120, "length_beats": 8},
        "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}},
        "patterns": {
            "beat": {
                "length_beats": 4,
                "steps": {"k": "x..." * 4},
                "events": [{"at": "1/3", "pad": "k"}],
            }
        },
        "tracks": [
            {"id": "kick", "pads": pads, "clips": [{"pattern": "beat"}]},
            {"id": "bass", "pads": pads, "clips": [{"pattern": "beat", "at": 4}]},
        ],
        "sections": [{"id": "drop", "at": 4, "length_beats": 4}],
    }
    effects = copy.deepcopy(first)
    effects["tracks"][1]["effects"] = [
        {"type": "filter", "mode": "lowpass", "cutoff_hz": 800},
        {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 200, "gain_db": 3}]},
        {"type": "compressor", "threshold_db": -20, "sidechain": "kick"},
    ]
    effects["master"] = {"effects": [{"type": "limiter"}]}
    sends = copy.deepcopy(effects)
    sends["tracks"][1]["sends"] = [{"to": "space"}]
    sends["returns"] = [
        {
            "id": "space",
            "effects": [{"type": "delay", "time_beats": "1/2"}, {"type": "reverb"}],
        }
    ]
    automation = copy.deepcopy(sends)
    automation["tracks"][1]["effects"][0]["id"] = "tone"
    automation["tracks"][1]["automation"] = [
        {
            "param": "effects.tone.cutoff_hz",
            "points": [{"at": 0, "value": 800}, {"at": 8, "value": 4000}],
        }
    ]
    automation["returns"][0]["automation"] = [
        {"param": "gain_db", "points": [{"at": 4, "value": -6, "curve": "hold"}]}
    ]
    automation["master"]["automation"] = [
        {"param": "gain_db", "points": [{"at": 0, "value": -6}]}
    ]
    return [automation, sends, effects, first]


# project_hash of each projects_by_schema() entry under the engine of its schema
# (commits 5dd2e6c, 695ca77, 74c074a and f71aa1d).
EARLIER_FINGERPRINTS = [
    "c0c7d463215a7839767cef6302532441594a6f9432486e4061406bc51fbdc509",
    "2079b7e7da4f90bbbfcd9b93414f91f2ede9df70a7bfec57401b4c8d32f06a30",
    "0caffe96828674076f68c7060c86e5cc553cd6fab7d196c58bea5a6a7ae2b159",
    "1b90bf9dcc3079a74cd794764f03445e7aa76eb30ea55b62cad15efa479a26dc",
]


def test_fingerprints_of_earlier_engines_still_verify():
    # The forms are rebuilt from today's full dump, so a field added to any model
    # breaks them here until the Rust model lists it among its legacy fields.
    for data, sha in zip(projects_by_schema(), EARLIER_FINGERPRINTS, strict=True):
        assert project_hash(data) != sha and hash_matches(data, sha)


def test_region_is_full_render_slice(song, tmp_path):
    path, d = song
    d["sections"] = [{"id": "tail", "at": "1/2", "length_beats": 1}]
    save(d, path)
    render(path, tmp_path / "full")
    preview = render(path, tmp_path / "region", section="tail")
    full, sr = sf.read(tmp_path / "full/mix.wav")
    region, _ = sf.read(tmp_path / "region/mix.wav")
    assert np.array_equal(region, full[12000:36000])
    assert preview["target"]["source_frames"] == [12000, 36000]
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
    save(d, path)
    r = render(path, tmp_path / "track", track_id="bass")
    assert list(r["tracks"]) == ["bass"]
    with pytest.raises(ValueError, match="Unknown track"):
        render(path, track_id="absent")


def test_gate_release_is_a_continuous_ramp(song, tmp_path):
    # A constant sample gated after 300 frames with a 100-frame release: the
    # voice lasts 400 frames, falls by a hundredth of its level each frame and
    # ends on zero.
    path, d = song
    sf.write(path.parent / "tone.wav", np.full((1000, 2), 0.5), 48000, subtype="FLOAT")
    d["session"].update(master_gain_db=0, end_fade_ms=0)
    d["tracks"][0]["pads"]["a"].update(attack_ms=0, release_ms=100 / 48)
    d["patterns"]["main"]["events"] = [
        {"at": 0, "pad": "a", "duration": "1/80", "velocity": 127}
    ]
    save(d, path)
    render(path, tmp_path / "out", 227)
    x = stem(tmp_path / "out", "bass")[:, 0]
    assert np.flatnonzero(x)[-1] == 398 and x[299] == 0.5 and x[399] == 0
    assert np.max(abs(np.diff(x[:400]))) <= 0.0050001


def test_resampling_44100_to_48000(song, tmp_path):
    path, d = song
    sf.write(
        path.parent / "tone.wav",
        np.sin(2 * np.pi * 220 * np.arange(44100) / 44100) * 0.4,
        44100,
        subtype="FLOAT",
    )
    d["tracks"][0]["pads"]["a"]["mode"] = "one_shot"
    save(d, path)
    render(path, tmp_path / "out")
    audio = stem(tmp_path / "out", "bass")
    # One second at 44.1 kHz is 48000 frames of the session, at the same pitch.
    assert 47000 < np.flatnonzero(audio.any(axis=1))[-1] < 48000
    freq = np.argmax(abs(np.fft.rfft(audio[:48000, 0])))
    assert abs(freq - 220) < 1


def test_output_refuses_different_render(song, tmp_path):
    path, d = song
    render(path, tmp_path / "shared")
    original = (tmp_path / "shared/mix.wav").read_bytes()
    d["session"]["tempo"] = 130
    save(d, path)
    with pytest.raises(ValueError, match="different render"):
        render(path, tmp_path / "shared")
    assert (tmp_path / "shared/mix.wav").read_bytes() == original


def test_tiny_session_rejected_before_export(tmp_path):
    path = tmp_path / "song.yaml"
    save({"session": {"length_beats": "1/1000000000"}}, path)
    with pytest.raises(ValueError, match="at least one audio frame"):
        render(path)


def test_malformed_yaml_has_json_error(tmp_path):
    path = tmp_path / "song.yaml"
    path.write_text("session: [oops")
    for command in ("check", "inspect"):
        code, result = cli(command, path)
        assert code == 1 and "error" in result
