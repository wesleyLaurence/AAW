"""Exporting a render as a named deliverable: formats, the level policy and its record."""

import json
import shutil
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw.export import ENCODERS
from agent_daw.library import decode
from agent_daw.model import load, save
from agent_daw.perception import measure
from helpers import SR, cli, daw, render


@pytest.fixture
def song(tmp_path):
    """Four seconds of a loud tone under a quieter beat, peaking near -6 dBFS."""
    folder = tmp_path / "song"
    folder.mkdir()
    t = np.arange(4 * SR) / SR
    sf.write(folder / "tone.wav", 0.9 * np.sin(2 * np.pi * 220 * t), SR, subtype="PCM_24")
    hit = np.random.default_rng(0).standard_normal(SR // 10) * np.exp(-np.arange(SR // 10) / 500)
    sf.write(folder / "hit.wav", 0.5 * hit / np.abs(hit).max(), SR, subtype="PCM_24")
    save(
        {
            "session": {"tempo": 120, "length_beats": 8, "master_gain_db": 0, "end_fade_ms": 0},
            "samples": {"tone": {"path": "tone.wav"}, "hit": {"path": "hit.wav"}},
            "patterns": {
                "once": {"length_beats": 8, "events": [{"at": 0, "pad": "x"}]},
                "beat": {"length_beats": 8, "grid": 1, "steps": {"h": "xxxxxxxx"}},
            },
            "tracks": [
                {"id": "tone", "gain_db": -9, "pads": {"x": {"sample": "tone"}}, "clips": [{"pattern": "once"}]},
                {"id": "beat", "gain_db": -6, "pads": {"h": {"sample": "hit"}}, "clips": [{"pattern": "beat"}]},
            ],
        },
        folder / "song.yaml",
    )
    return folder / "song.yaml"


def level(path):
    x, rate = sf.read(path, always_2d=True)
    return measure(x, rate)


def test_a_wav_export_is_the_render_under_its_name(song):
    to = song.parent / "exports" / "My Edit v1.wav"
    code, made = cli("export", song, "--to", to)
    assert code == 0, made
    # There was no render, so one was made; the file is its mix, sample for sample.
    folder = song.parent / "renders" / made["render"]["render_id"][:12]
    assert made["render"]["rendered_now"] and made["render"]["directory"] == str(folder.resolve())
    mix, exported = sf.read(folder / "mix.wav"), sf.read(to)
    assert exported[1] == SR and np.array_equal(mix[0], exported[0])
    assert sf.info(to).subtype == "PCM_24"
    assert made["file"] == str(to.resolve()) and made["format"] == {"container": "wav", "bits": 24}
    assert made["duration_seconds"] == 4 and made["warnings"] == []
    assert made["level"]["policy"] == "as rendered" and made["level"]["gain_db"] == 0
    assert made["level"]["peak_dbfs"] == pytest.approx(level(to)["peak_dbfs"])
    # Beside it, what it is and which render it came from, without local paths.
    record = json.loads((to.parent / "My Edit v1.wav.json").read_text())
    report = json.loads((folder / "report.json").read_text())
    assert record["file"] == "My Edit v1.wav" and record["sha256"] == made["sha256"]
    assert record["render"] == {
        k: report[k] for k in ("render_id", "project_sha256", "audio_sha256", "engine_sha256")
    }
    assert str(song.parent) not in json.dumps(record)

    # A file with that name is kept unless it is to be replaced; the render is reused.
    code, again = cli("export", song, "--to", to)
    assert code == 1 and "exists" in again["error"]
    code, again = cli("export", song, "--to", to, "--replace")
    assert code == 0 and not again["render"]["rendered_now"] and again["sha256"] == made["sha256"]
    # A song changed since is rendered again.
    daw("set", song, "session.master_gain_db", -6)
    code, quieter = cli("export", song, "--to", to, "--replace")
    assert quieter["render"]["rendered_now"]
    assert quieter["render"]["render_id"] != made["render"]["render_id"]
    assert quieter["level"]["peak_dbfs"] == pytest.approx(made["level"]["peak_dbfs"] - 6, abs=0.01)


def test_sixteen_bits_are_dithered_the_same_each_time(song):
    exports = song.parent / "exports"
    code, a = cli("export", song, "--to", exports / "a.wav", "--bits", 16)
    code, b = cli("export", song, "--to", exports / "b.wav", "--bits", 16)
    assert code == 0 and a["sha256"] == b["sha256"]
    assert a["format"] == {"container": "wav", "bits": 16, "dither": "triangular, one step"}
    assert sf.info(exports / "a.wav").subtype == "PCM_16"
    mix = sf.read(f"{a['render']['directory']}/mix.wav")[0]
    assert np.abs(sf.read(exports / "a.wav")[0] - mix).max() <= 1.5 / 32768


def test_the_level_policy_is_one_gain_and_says_what_it_did(song):
    exports = song.parent / "exports"
    code, plain = cli("export", song, "--to", exports / "plain.wav")
    rendered = {k: plain["level"][k] for k in ("integrated_lufs", "peak_dbfs", "estimated_true_peak_dbtp")}

    code, peak = cli("export", song, "--to", exports / "peak.wav", "--peak", -3)
    assert code == 0 and peak["level"]["policy"] == "peak"
    assert peak["level"]["target"] == {"true_peak_dbtp": -3}
    assert peak["level"]["rendered"] == rendered and peak["level"]["held_back_db"] == 0
    assert peak["level"]["gain_db"] == pytest.approx(-3 - rendered["estimated_true_peak_dbtp"], abs=0.001)
    assert level(exports / "peak.wav")["estimated_true_peak_dbtp"] == pytest.approx(-3, abs=0.01)

    code, gain = cli("export", song, "--to", exports / "gain.wav", "--gain", -2.5)
    assert gain["level"]["policy"] == "gain" and gain["level"]["gain_db"] == -2.5
    assert level(exports / "gain.wav")["peak_dbfs"] == pytest.approx(rendered["peak_dbfs"] - 2.5, abs=0.01)

    code, loud = cli("export", song, "--to", exports / "lufs.wav", "--lufs", rendered["integrated_lufs"] + 1)
    assert loud["level"]["policy"] == "loudness" and loud["level"]["gain_db"] == pytest.approx(1, abs=0.001)
    assert level(exports / "lufs.wav")["integrated_lufs"] == pytest.approx(rendered["integrated_lufs"] + 1, abs=0.05)

    # The tone sample is far louder than the mix it is turned down in. Matching it
    # is held under the ceiling, and the export says by how much.
    code, match = cli("export", song, "--to", exports / "match.wav", "--match", "tone", "--ceiling", -1)
    held = match["level"]
    assert code == 0 and held["policy"] == "match" and held["target"]["sample"] == "tone"
    assert held["target"]["integrated_lufs"] > rendered["integrated_lufs"] + 6
    assert held["gain_db"] == pytest.approx(-1 - rendered["peak_dbfs"], abs=0.001)
    assert held["held_back_db"] > 1 and "ceiling" in match["warnings"][0]
    assert level(exports / "match.wav")["peak_dbfs"] == pytest.approx(-1, abs=0.01)

    for bad, reason in [(("--match", "none"), "Unknown sample"), (("--ceiling", 1), "ceiling must be")]:
        code, result = cli("export", song, "--to", exports / "bad.wav", *bad)
        assert code == 1 and reason in result["error"]
    assert not (exports / "bad.wav").exists()


@pytest.mark.parametrize("kind, rate", [(".m4a", 256), (".mp3", 320)])
def test_a_compressed_export_is_encoded_by_what_is_installed(song, kind, rate):
    if not any(shutil.which(name) for name in ENCODERS[kind]):
        pytest.skip(f"No {kind} encoder here")
    to = song.parent / "exports" / f"edit{kind}"
    code, made = cli("export", song, "--to", to, "--peak", -1)
    assert code == 0, made
    codec = "aac" if kind == ".m4a" else "mp3"
    assert {k: made["format"][k] for k in ("container", "codec", "bitrate_kbps")} == {
        "container": kind[1:], "codec": codec, "bitrate_kbps": rate,
    }  # fmt: skip
    assert made["format"]["encoder"] in ENCODERS[kind] and made["warnings"] == []
    # It decodes to the length of the render, and to its tone.
    decode(to, to.with_suffix(".wav"))
    x, got = sf.read(to.with_suffix(".wav"), always_2d=True)
    assert got == SR and abs(len(x) / SR - 4) < 0.06
    assert measure(x, got)["peak_dbfs"] == pytest.approx(made["level"]["peak_dbfs"], abs=1.5)
    code, small = cli("export", song, "--to", to, "--replace", "--bitrate", 96)
    assert code == 0 and small["format"]["bitrate_kbps"] == 96
    assert to.stat().st_size < 0.6 * (rate / 8 * 1000 * 4)
    # As rendered with the peak near full scale, which a decoder may clip.
    daw("set", song, "session.master_gain_db", -0.3 - made["level"]["rendered"]["estimated_true_peak_dbtp"])
    code, hot = cli("export", song, "--to", to, "--replace")
    assert code == 0 and "True peak" in hot["warnings"][0]


def test_only_a_whole_mix_in_a_known_format_is_exported(song, tmp_path):
    preview = render(song, tmp_path / "preview", track_id="beat")
    assert preview["target"]["track"] == "beat"
    code, result = cli("export", song, "--to", tmp_path / "x.wav", "--render", tmp_path / "preview")
    assert code == 1 and "whole mix" in result["error"]
    code, result = cli("export", song, "--to", tmp_path / "x.ogg")
    assert code == 1 and ".wav, .aiff, .flac, .m4a or .mp3" in result["error"]
    code, result = cli("export", song, "--to", tmp_path / "x.m4a", "--bitrate", 999)
    assert code == 1 and "bitrate must be" in result["error"]
    # A render named outright is exported as it is, whatever the song is now.
    full = render(song, tmp_path / "full")
    daw("set", song, "session.master_gain_db", -12)
    code, made = cli("export", song, "--to", tmp_path / "x.wav", "--render", tmp_path / "full")
    assert code == 0 and made["render"]["render_id"] == full["render_id"]
    assert not made["render"]["rendered_now"]
    assert load(song)["session"]["master_gain_db"] == -12
    # The Rust binary passes the command on.
    assert daw("export", song, "--to", tmp_path / "y.wav", "--render", tmp_path / "full")["sha256"] == made["sha256"]


def test_a_range_is_cut_from_the_full_render(song):
    # Two bars of 4/4 at 120: a bar is two seconds.
    code, made = cli("export", song, "--to", "bar 2.wav", "--bars", "2")
    assert code == 0, made
    to = song.parent / "exports" / "bar 2.wav"
    assert made["file"] == str(to.resolve())
    mix = sf.read(f"{made['render']['directory']}/mix.wav")[0]
    assert np.array_equal(sf.read(to)[0], mix[2 * SR : 4 * SR])
    assert made["duration_seconds"] == 2
    assert made["range"] == {
        "whole_song": False, "start_seconds": 2, "end_seconds": 4,
        "start_beat": 4, "end_beat": 8, "start": "2:1", "end": "3:1",
    }
    assert json.loads((to.parent / "bar 2.wav.json").read_text())["range"] == made["range"]
    # The level is measured over the range, not the song.
    assert made["level"]["peak_dbfs"] == pytest.approx(measure(mix[2 * SR : 4 * SR], SR)["peak_dbfs"])

    # BAR:BEAT and beats name the same places; --end is where the file stops.
    code, places = cli("export", song, "--to", "a.wav", "--start", "1:3", "--end", "2:1")
    code, beats = cli("export", song, "--to", "b.wav", "--start", 2, "--end", 4)
    assert places["sha256"] == beats["sha256"] and places["duration_seconds"] == 1
    assert not places["render"]["rendered_now"]
    code, until = cli("export", song, "--to", "c.wav", "--end", "1:2.5")
    assert until["range"]["start_beat"] == 0 and until["range"]["end_beat"] == 1.5

    # A section is a range too.
    data = load(song)
    data["sections"] = [{"id": "drop", "at": 4, "length_beats": 2}]
    save(data, song)
    code, drop = cli("export", song, "--to", "drop.wav", "--section", "drop")
    assert code == 0 and drop["range"]["start"] == "2:1" and drop["range"]["end"] == "2:3"

    for bad, reason in [
        (("--end", "3:2"), "past the song's end at 3:1"),
        (("--start", 4, "--end", 4), "ends after it starts"),
        (("--start", "0:1"), "not in a bar"),
        (("--start", "1:5"), "not in a bar"),
        (("--bars", "3-2"), "first to last"),
        (("--bars", "1", "--section", "drop"), "one of them"),
        (("--section", "verse"), "Unknown or ambiguous section"),
    ]:
        code, result = cli("export", song, "--to", "bad.wav", *bad)
        assert code == 1 and reason in result["error"], (bad, result)


def test_what_is_muted_is_not_exported(song):
    code, both = cli("export", song, "--to", "both.wav", "--bars", "2")
    daw("set", song, "tracks.tone.mute", True)
    code, beat = cli("export", song, "--to", "beat.wav", "--bars", "2")
    assert code == 0 and beat["render"]["rendered_now"]
    # The tone is the loud part; without it only the beat is left.
    assert beat["level"]["integrated_lufs"] < both["level"]["integrated_lufs"] - 6
    daw("set", song, "tracks.beat.mute", True)
    code, result = cli("export", song, "--to", "none.wav", "--bars", "2")
    assert code == 1 and "Nothing is heard from 2:1 to 3:1" in result["error"]


def test_the_name_place_rate_and_depth_are_chosen(song, tmp_path):
    # No name: the folder's, as a 24-bit WAV in exports.
    code, made = cli("export", song)
    assert code == 0 and made["file"] == str((song.parent / "exports" / "song.wav").resolve())
    data = load(song)
    data["session"]["title"] = "My Song"
    save(data, song)
    code, titled = cli("export", song)
    assert titled["file"].endswith("exports/My Song.wav")
    # A path with a folder goes where it says.
    code, there = cli("export", song, "--to", tmp_path / "there.wav")
    assert there["file"] == str((tmp_path / "there.wav").resolve())

    code, low = cli("export", song, "--to", "low.wav", "--sample-rate", 44100, "--bits", 16)
    assert code == 0 and low["sample_rate"] == 44100 and low["resampled_from"] == SR
    info = sf.info(song.parent / "exports" / "low.wav")
    assert info.samplerate == 44100 and info.subtype == "PCM_16" and info.frames == 4 * 44100
    assert low["duration_seconds"] == 4
    assert low["level"]["integrated_lufs"] == pytest.approx(made["level"]["integrated_lufs"], abs=0.1)

    for name, bits, subtype in [("f.aiff", 32, "FLOAT"), ("f.aiff", 16, "PCM_16"), ("f.flac", 24, "PCM_24"), ("f.wav", 32, "FLOAT")]:
        code, made = cli("export", song, "--to", name, "--bits", bits, "--replace", "--bars", "1")
        assert code == 0, made
        info = sf.info(song.parent / "exports" / name)
        assert info.subtype == subtype and info.duration == 2
        assert made["format"]["container"] == Path(name).suffix[1:]

    for bad, reason in [
        (("--to", "x.flac", "--bits", 32), ".flac file is 16 or 24 bits"),
        (("--to", "x.mp3", "--bits", 16), "--bits is for .wav"),
        (("--to", "x.mp3", "--sample-rate", 96000), "at most 48000 Hz"),
        (("--to", "x.wav", "--sample-rate", 100), "sample rate must be"),
    ]:
        code, result = cli("export", song, *bad)
        assert code == 1 and reason in result["error"], (bad, result)
