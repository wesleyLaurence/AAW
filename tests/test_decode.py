"""Importing compressed audio: decoded once into the project, and refused when unreadable.

The files are generated: a tone with a click, encoded here by whichever of the
system's encoders is present. Tests that need a decoder are skipped without one.
"""

import shutil
import subprocess

import numpy as np
import pytest
import soundfile as sf

from agent_daw.library import DECODERS, decode
from agent_daw.model import digest, load, save
from helpers import cli, daw, render

RATE = 44100
FRAMES = RATE * 2 + 137
CLICK = 1000
# One AAC or MP3 frame and its overlap: what a decoder may leave after the audio.
PADDING = 2112


def original(path):
    """A stereo tone with a click near its start, as 16-bit WAV."""
    x = 0.4 * np.sin(2 * np.pi * 220 * np.arange(FRAMES) / RATE)
    x[CLICK : CLICK + 40] += 0.4
    sf.write(path, np.column_stack([x, 0.5 * x]), RATE, subtype="PCM_16")
    return path


def compressed(directory, kind):
    """The original encoded as `kind`, or a skip when nothing here encodes it."""
    wav, out = original(directory / "original.wav"), directory / f"My Song.{kind}"
    if kind == "mp3":
        x, rate = sf.read(wav)
        sf.write(out, x, rate, format="MP3")
        return out
    for command in (
        ["afconvert", "-f", "m4af", "-d", "aac", "-b", "256000", wav, out],
        ["ffmpeg", "-v", "error", "-i", wav, "-c:a", "aac", "-b:a", "256k", out],
    ):
        if shutil.which(command[0]):
            subprocess.run(list(map(str, command)), check=True)
            return out
    pytest.skip("No AAC encoder here")


def lag(decoded, source):
    """How many frames late the decoded audio is against what `source` was encoded from."""
    x = sf.read(source.parent / "original.wav", always_2d=True)[0][:, 0]
    y = sf.read(decoded, always_2d=True)[0][:, 0]
    window = slice(CLICK - 200, CLICK + 4000)
    scores = {
        k: float(np.dot(x[window], y[window.start + k : window.stop + k]))
        for k in range(-200, 201)
    }
    return max(scores, key=scores.get)


def project(directory):
    path = directory / "song" / "song.yaml"
    path.parent.mkdir()
    save({"session": {"tempo": 120, "length_beats": 4}}, path)
    return path


def bring(source, path, *more):
    """`daw samples import` of a file into the song at `path`."""
    db = path.parent.parent / "index.sqlite"
    return cli("samples", "--db", db, "import", source, "--project", path, *more)


@pytest.fixture
def decoder():
    if not any(shutil.which(name) for name in DECODERS):
        pytest.skip("No decoder here")


@pytest.mark.parametrize("kind", ["m4a", "mp3"])
def test_a_compressed_file_is_decoded_into_the_project(tmp_path, decoder, kind):
    source, path = compressed(tmp_path, kind), project(tmp_path)
    code, result = bring(source, path, "--id", "song")
    assert code == 0, result
    # The copy is WAV, named by the original's hash; the song has both hashes.
    copy = path.parent / result["path"]
    assert result["path"] == f"samples/{digest(source)[:12]}_My Song.wav"
    assert result["sha256"] == digest(copy) != digest(source) == result["source_sha256"]
    entry = load(path)["samples"]["song"]
    assert entry == {k: result[k] for k in ("path", "sha256", "source", "source_sha256", "root_note")}
    assert entry["source"] == str(source.resolve()) and source.is_file()
    # The audio starts where the original's does, and nothing is clipped to fit.
    info = sf.info(copy)
    assert (info.subtype, info.samplerate, info.channels) == ("FLOAT", RATE, 2)
    assert lag(copy, source) == 0 and abs(info.frames - FRAMES) <= PADDING
    assert result["decoded"] == {"sample_rate": RATE, "channels": 2, "duration": info.duration}
    # The same file again is the copy already made.
    made = copy.stat().st_mtime_ns
    code, again = bring(source, path, "--copy-only")
    assert code == 0 and again == {**entry, "decoded": result["decoded"]}
    assert copy.stat().st_mtime_ns == made
    assert sorted(p.name for p in copy.parent.iterdir()) == [copy.name]
    # The engine plays it.
    song = load(path)
    song["patterns"] = {"once": {"length_beats": 4, "events": [{"at": 0, "pad": "x"}]}}
    song["tracks"] = [{"id": "song", "pads": {"x": {"sample": "song"}}, "clips": [{"pattern": "once"}]}]
    save(song, path)
    assert daw("check", path)["warnings"] == []
    assert render(path, tmp_path / "out")["mix"]["rms_dbfs"] > -30


@pytest.mark.parametrize("name", list(DECODERS))
@pytest.mark.parametrize("kind", ["m4a", "mp3"])
def test_each_decoder_leaves_out_the_padding_before_the_audio(tmp_path, name, kind):
    if not shutil.which(name):
        pytest.skip(f"No {name} here")
    source = compressed(tmp_path, kind)
    assert decode(source, tmp_path / "decoded.wav", decoder=name) == name
    info = sf.info(tmp_path / "decoded.wav")
    assert info.subtype == "FLOAT" and lag(tmp_path / "decoded.wav", source) == 0
    assert 0 <= info.frames - FRAMES <= PADDING if name == "ffmpeg" else info.frames == FRAMES


def test_a_pitched_compressed_sample_takes_its_root_from_the_decoded_copy(tmp_path, decoder):
    source, path = compressed(tmp_path, "mp3"), project(tmp_path)
    code, result = bring(source, path, "--id", "tone", "--root-note", "auto")
    assert code == 0, result
    assert result["root_note"] == "A3" == load(path)["samples"]["tone"]["root_note"]


def test_import_refuses_what_the_engine_cannot_read(tmp_path):
    path = project(tmp_path)
    before = path.read_text()
    (tmp_path / "notes.wav").write_text("not audio")
    (tmp_path / "broken.m4a").write_bytes(b"not audio either" * 64)
    sf.write(tmp_path / "surround.wav", np.zeros((RATE, 6)), RATE)
    for name, reason in [
        ("notes.wav", "Cannot read notes.wav as audio"),
        ("broken.m4a", "Could not decode broken.m4a"),
        ("surround.wav", "surround.wav has 6 channels"),
    ]:
        if name.endswith(".m4a") and not any(shutil.which(d) for d in DECODERS):
            continue
        code, result = bring(tmp_path / name, path, "--id", "x")
        assert code == 1 and reason in result["error"], result
    # Nothing was left in the project, and the song is as it was.
    samples = path.parent / "samples"
    assert not samples.exists() or not list(samples.iterdir())
    assert path.read_text() == before


def test_check_warns_of_a_sample_the_engine_cannot_read(tmp_path):
    # A song an earlier import left with a compressed file as its sample.
    path = project(tmp_path)
    (path.parent / "song.m4a").write_bytes(b"compressed" * 64)
    sf.write(path.parent / "hit.wav", np.zeros(RATE), RATE)
    save(
        {
            "session": {"length_beats": 4},
            "samples": {"song": {"path": "song.m4a"}, "hit": {"path": "hit.wav"}},
        },
        path,
    )
    code, checked = cli("check", path)
    assert code == 0, checked
    assert len(checked["warnings"]) == 1
    assert checked["warnings"][0].startswith("song: Cannot read song.m4a as audio")
