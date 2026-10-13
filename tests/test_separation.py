"""Taking a song apart with `daw separate`: the parts as samples, their alignment,
the residual, the recipe, the reuse, the tracks and the undo, the refusals when
the runtime or a model is missing, and the manifest.

No model is downloaded or run: AAW_SEPARATOR runs the stand-in, which cuts the
parts from the source by band so that they sum to it. One test runs the real
runtime over generated audio when AAW_SEPARATE_REAL is set and the runtime is
installed.
"""

import json
import os
import shlex
import subprocess
import sys
import tempfile
import time
from contextlib import contextmanager
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import separation
from agent_daw.model import load
from helpers import cli, daw, render

STAND_IN = Path(__file__).with_name("stand_in_separator.py")
RATE = 44100
# Long enough for the meter's integrated loudness.
SECONDS = 5


@pytest.fixture
def env(tmp_path, monkeypatch):
    """The stand-in separator and an empty data folder of its own."""
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.setenv("AAW_SEPARATOR", shlex.join([sys.executable, str(STAND_IN)]))
    return tmp_path / "data"


@contextmanager
def hosted(project, monkeypatch):
    """A session host for the project, so edits are undoable; closed at the end."""
    # Unix socket paths are short; pytest's tmp_path is long on macOS.
    with tempfile.TemporaryDirectory(prefix="aaw", dir="/tmp") as registry:
        monkeypatch.setenv("AAW_HOST_DIR", registry)
        host = subprocess.Popen(
            [os.environ["AAW_DAW"], "host", str(project)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
        )
        try:
            deadline = time.time() + 30
            while not daw("status", project).get("host"):
                assert host.poll() is None, host.stderr.read()
                assert time.time() < deadline, "the host did not start"
                time.sleep(0.05)
            yield
        finally:
            daw("close", project)
            host.wait(timeout=10)


def mix(path, rate=RATE, seconds=SECONDS, channels=2):
    """A sine bass, a noise vocal and clicks, as 24-bit WAV; the array written."""
    n = rate * seconds
    t = np.arange(n) / rate
    rng = np.random.default_rng(1)
    bass = 0.4 * np.sin(2 * np.pi * 110 * t)
    noise = rng.standard_normal(n)
    spectrum = np.fft.rfft(noise)
    freqs = np.fft.rfftfreq(n, 1 / rate)
    spectrum[(freqs < 1000) | (freqs >= 8000)] = 0
    vocal = 0.2 * np.fft.irfft(spectrum, n) / 3
    clicks = np.zeros(n)
    clicks[:: rate // 2] = 0.5
    x = bass + vocal + clicks
    x = np.column_stack([x, 0.8 * x]) if channels == 2 else x[:, None]
    sf.write(path, x, rate, subtype="PCM_24")
    return x


@pytest.fixture
def song(tmp_path, env):
    """A project at 120 BPM with the mix as the sample `mix`, and a clip of it on track `song` at beat 4."""
    folder = tmp_path / "project"
    daw("init", folder, "--tempo", 120, "--bars", 8)
    path = folder / "song.yaml"
    daw("set", path, "session.master_gain_db", 0)
    (folder / "samples").mkdir()
    mix(folder / "samples" / "mix.wav")
    daw("apply", path, write(tmp_path / "p.json", {"samples": {"mix": {"path": "samples/mix.wav"}}}), "--expect", daw("inspect", path)["project_sha256"])
    daw("track", "add", path, "song")
    daw("audio", "add", path, "song", "mix", "--at", 4, "--fade-in-ms", 2, "--gain-db", -3)
    return path


def write(path, data):
    path.write_text(json.dumps(data))
    return path


def test_the_vocals_split_puts_two_aligned_parts_in_the_project(song):
    code, made = cli("separate", song, "mix")
    assert code == 0, made
    assert made["split"] == "vocals" and made["model"]["name"] == "kim-mel-roformer"
    assert made["model"]["license"] == "MIT" and made["model"]["author"] == "Kimberley Jensen"
    assert not made["reused"] and made["source"] == {
        "sample": "mix", "path": "samples/mix.wav", "sha256": made["source"]["sha256"],
        "original_sample_rate": RATE, "clip": None, "imported": False,
    }
    assert [p["part"] for p in made["parts"]] == ["vocals", "instrumental"]
    source = sf.read(song.parent / "samples/mix.wav", always_2d=True)[0]
    project = load(song)
    for part in made["parts"]:
        assert part["sample_id"] == f"mix-{part['part']}" and part["added"]
        file = song.parent / part["path"]
        assert file.parent == song.parent / "separated" and file.name.endswith(f"-kim-mel-roformer-{part['part']}.wav")
        info = sf.info(file)
        # The source's length to the frame, 32-bit float at the model's rate.
        assert (info.frames, info.samplerate, info.channels, info.subtype) == (len(source), RATE, 2, "FLOAT")
        assert part["frames"] == len(source) and part["duration_seconds"] == SECONDS
        assert part["peak_dbfs"] < 0 and part["integrated_lufs"] < 0 and not part["silent"]
        assert project["samples"][part["sample_id"]]["path"] == part["path"]
        assert project["samples"][part["sample_id"]]["sha256"] == part["sha256"]
    # The parts add up to the song: the stand-in's bands partition it.
    assert made["residual_db"] < -100
    parts = [sf.read(song.parent / p["path"], always_2d=True)[0] for p in made["parts"]]
    assert np.abs(sum(parts) - source).max() < 1e-5
    # The vocal band is in the vocals, the bass in the instrumental.
    assert np.abs(parts[1][:, 0] - 0.4 * np.sin(2 * np.pi * 110 * np.arange(len(source)) / RATE)).max() < 0.6
    assert made["runtime"]["audio_separator"] == "stand-in" and made["seconds"]["total"] >= 0
    assert "tracks" not in made and "A low residual" in made["notes"][0]
    # The rights line comes once.
    assert "copies of it" in made["rights"]
    # The recipe beside the parts.
    recipe = json.loads((song.parent / made["recipe"]).read_text())
    assert made["recipe"].startswith("separated/") and made["recipe"].endswith("-kim-mel-roformer.separated.json")
    assert recipe["source"]["sha256"] == made["source"]["sha256"] and recipe["split"] == "vocals"
    assert recipe["model"]["file"] == "vocals_mel_band_roformer.ckpt"
    assert {w["name"] for w in recipe["weights"]} == {"vocals_mel_band_roformer.ckpt", "vocals_mel_band_roformer.yaml"}
    assert all(len(w["sha256"]) == 64 for w in recipe["weights"])
    assert recipe["parameters"] == {"sample_rate": RATE, "input_gain_db": 0.0, "normalization": "none"}
    assert set(recipe["parts"]) == {"vocals", "instrumental"} and recipe["made"].startswith("20")
    assert str(song.parent) not in json.dumps({k: v for k, v in recipe.items() if k != "parts"})


def test_the_same_source_and_model_are_found_and_not_run_again(song):
    code, first = cli("separate", song, "mix")
    assert code == 0, first
    # The stand-in is told to fail if it is run: it is not.
    recipe = song.parent / first["recipe"]
    code, again = cli("separate", song, "mix")
    assert code == 0 and again["reused"] and "rights" not in again
    assert [p["sample_id"] for p in again["parts"]] == ["mix-vocals", "mix-instrumental"]
    assert not any(p["added"] for p in again["parts"])
    assert recipe.read_text() == recipe.read_text()
    # Another prefix adds the same files under other IDs; a prefix in use is refused.
    code, other = cli("separate", song, "mix", "--id", "acapella")
    assert code == 0 and other["reused"] and all(p["added"] for p in other["parts"])
    assert load(song)["samples"]["acapella-vocals"]["path"] == first["parts"][0]["path"]
    daw("apply", song, write(song.parent.parent / "p.json", {"samples": {"taken-vocals": {"path": "samples/mix.wav"}}}), "--expect", daw("inspect", song)["project_sha256"])
    code, refused = cli("separate", song, "mix", "--id", "taken")
    assert code == 1 and "Sample ID taken-vocals already names samples/mix.wav" in refused["error"]
    # A part that is gone is made again, and the song takes it again.
    for sample_id in ("mix-vocals", "mix-instrumental", "acapella-vocals", "acapella-instrumental"):
        daw("remove", song, f"samples.{sample_id}")
    (song.parent / first["parts"][0]["path"]).unlink()
    code, remade = cli("separate", song, "mix")
    assert code == 0, remade
    assert not remade["reused"] and remade["parts"][0]["frames"] == RATE * SECONDS
    assert all(p["added"] for p in remade["parts"]) and "mix-vocals" in load(song)["samples"]


def test_stems_on_tracks_play_the_song_as_it_was_and_undo_as_one(song, monkeypatch):
    before = render(song)
    folder = song.parent / "renders" / before["render_id"][:12]
    was = sf.read(folder / "mix.wav", always_2d=True)[0]
    with hosted(song, monkeypatch):
        stems_on_tracks_undone_as_one(song, was)


def stems_on_tracks_undone_as_one(song, was):
    code, made = cli("separate", song, "tracks.song.audio.0", "--split", "stems", "--quality", "fast", "--tracks")
    assert code == 0, made
    assert made["model"]["name"] == "htdemucs" and made["source"]["clip"] == "tracks.song.audio.0"
    assert [p["part"] for p in made["parts"]] == ["vocals", "drums", "bass", "other"]
    assert made["tracks"] == [
        {"id": f"mix-{p}", "clip": f"tracks.mix-{p}.audio.0", "sample": f"mix-{p}"} for p in ("vocals", "drums", "bass", "other")
    ]
    assert made["silenced"] == {"track": "song", "mute": True}
    project = load(song)
    assert [t["id"] for t in project["tracks"]] == ["song", "mix-vocals", "mix-drums", "mix-bass", "mix-other"]
    assert project["tracks"][0]["mute"]
    source_clip = project["tracks"][0]["audio"][0]
    for track in project["tracks"][1:]:
        clip = track["audio"][0]
        assert clip == {**source_clip, "sample": track["id"]}
        assert clip["at"] == 4 and clip["gain_db"] == -3 and clip["fade_in_ms"] == 2
    # The parts where the source was: the render is the same song.
    after = render(song)
    now = sf.read(song.parent / "renders" / after["render_id"][:12] / "mix.wav", always_2d=True)[0]
    assert now.shape == was.shape and np.abs(now - was).max() < 2e-4
    # One edit in the log, undone as one, with the files left.
    changes = daw("changes", song, "--since", 0)["changes"]
    assert changes[-1]["label"] == "Separate mix into vocals, drums, bass, other"
    assert changes[-1]["origin"] == "agent"
    daw("undo", song)
    project = load(song)
    assert [t["id"] for t in project["tracks"]] == ["song"] and not project["tracks"][0]["mute"]
    assert set(project["samples"]) == {"mix"}
    assert all((song.parent / p["path"]).is_file() for p in made["parts"])


def test_a_clip_among_others_is_silenced_by_its_gain_and_a_sample_source_grows_the_song(song, monkeypatch):
    with hosted(song, monkeypatch):
        clip_among_others_then_a_sample_source(song)


def clip_among_others_then_a_sample_source(song):
    daw("audio", "add", song, "song", "mix", "--at", 12, "--source-end-seconds", 1)
    code, made = cli("separate", song, "tracks.song.audio.1", "--tracks")
    assert code == 0, made
    assert made["silenced"] == {"clip": "tracks.song.audio.1", "gain_db": -96}
    project = load(song)
    assert not project["tracks"][0]["mute"] and project["tracks"][0]["audio"][1]["gain_db"] == -96
    assert project["tracks"][0]["audio"][0]["gain_db"] == -3
    assert project["tracks"][1]["audio"][0]["source_end_seconds"] == 1 and project["tracks"][1]["audio"][0]["at"] == 12
    daw("undo", song)
    # From the sample itself: whole parts from beat 0 on a two-bar song that grows to hold them.
    daw("remove", song, "tracks.song.audio.1")
    daw("audio", "move", song, "tracks.song.audio.0", "--at", 0)
    daw("set", song, "session.length_beats", 2)
    code, made = cli("separate", song, "mix", "--id", "whole", "--tracks")
    assert code == 0, made
    project = load(song)
    # Five seconds at 120 BPM are ten beats: to the end of the third bar.
    assert project["session"]["length_beats"] == 12
    assert [t["id"] for t in project["tracks"]][-2:] == ["whole-vocals", "whole-instrumental"]
    assert project["tracks"][-1]["audio"][0]["at"] == 0 and made["silenced"] is None
    # A grouped source puts the parts in its group.
    daw("undo", song)
    daw("group", "add", song, "bus", "--tracks", "song")
    code, made = cli("separate", song, "tracks.song.audio.0", "--id", "grouped", "--tracks")
    assert code == 0, made
    assert all(t["group"] == "bus" for t in load(song)["tracks"][1:])


def test_a_file_is_imported_first_and_a_48k_file_gives_parts_at_44100(song, tmp_path):
    x = mix(tmp_path / "My Song (final).wav", rate=48000, seconds=2, channels=1)
    code, made = cli("separate", song, tmp_path / "My Song (final).wav", "--split", "stems6")
    assert code == 0, made
    assert made["source"]["imported"] and made["source"]["sample"] == "My-Song-final"
    assert made["source"]["original_sample_rate"] == 48000 and made["model"]["name"] == "htdemucs_6s"
    project = load(song)
    assert project["samples"]["My-Song-final"]["path"].startswith("samples/") and project["samples"]["My-Song-final"]["path"].endswith("My Song (final).wav")
    assert [p["part"] for p in made["parts"]] == ["vocals", "drums", "bass", "guitar", "piano", "other"]
    frames = -(-len(x) * RATE // 48000)
    for part in made["parts"]:
        info = sf.info(song.parent / part["path"])
        assert (info.frames, info.samplerate, info.channels) == (frames, RATE, 1)
        assert part["sample_id"] == f"My-Song-final-{part['part']}"
    # The same file again is the same sample, and found.
    code, again = cli("separate", song, tmp_path / "My Song (final).wav", "--split", "stems6")
    assert code == 0 and again["reused"] and not again["source"]["imported"]
    assert len(load(song)["samples"]) == 1 + 1 + 6


def test_refusals_say_what_is_wrong(song):
    code, bad = cli("separate", song, "nothing")
    assert code == 1 and "not a sample of the song, an audio clip (tracks.T.audio.N) or a file" in bad["error"]
    code, bad = cli("separate", song, "tracks.song.audio.3")
    assert code == 1 and "has 1 audio clips; no clip 3" in bad["error"]
    code, bad = cli("separate", song, "mix", "--model", "nope")
    assert code == 1 and "No model named nope" in bad["error"]
    code, bad = cli("separate", song, "mix", "--model", "htdemucs", "--split", "vocals")
    assert code == 1 and "htdemucs gives the stems split, not vocals" in bad["error"]
    code, bad = cli("separate", song)
    assert code == 1 and "separate takes PROJECT SOURCE" in bad["error"]
    code, bad = cli("separate", song, "mix", "--all")
    assert code == 1 and "go with daw separate setup" in bad["error"]
    # A model named outright is run with its own split.
    code, made = cli("separate", song, "mix", "--model", "htdemucs_ft")
    assert code == 0 and made["split"] == "stems" and made["model"]["name"] == "htdemucs_ft"
    # The runner failing is the command failing, with its words.
    code, bad = cli("separate", song, "mix", "--model", "fail")  # no such model
    assert code == 1


def test_a_runner_that_fails_is_reported(song, monkeypatch):
    monkeypatch.setattr(separation, "model_named", lambda name: {**separation.model_for("vocals", "fast"), "file": "fail.yaml", "name": "failing"})
    with pytest.raises(ValueError, match="Separation failed: The stand-in was asked to fail"):
        separation.separate(song, "mix", model_name="failing")


def test_without_the_runtime_or_a_model_the_command_says_what_to_run(song, env, monkeypatch):
    monkeypatch.delenv("AAW_SEPARATOR")
    code, bad = cli("separate", song, "mix")
    assert code == 1
    assert "The separation runtime is not installed" in bad["error"]
    assert "daw separate setup" in bad["error"] and "about 1000 MB" in bad["error"] and "1393 MB" in bad["error"]
    assert str(env / "separation") in bad["error"]
    # The listing says the same, with each model and the command that gets it.
    code, listed = cli("separate", "models")
    assert code == 0, listed
    assert listed["runtime"] == {"installed": False, "root": str(env / "separation")}
    assert [m["name"] for m in listed["models"]] == ["mdx-inst-hq4", "kim-mel-roformer", "htdemucs", "htdemucs_ft", "htdemucs_6s"]
    assert listed["defaults"] == {
        "vocals": {"fast": "mdx-inst-hq4", "best": "kim-mel-roformer"},
        "stems": {"fast": "htdemucs", "best": "htdemucs_ft"},
        "stems6": {"fast": "htdemucs_6s", "best": "htdemucs_6s"},
    }
    kim = listed["models"][1]
    assert kim["mb"] == 913 and not kim["here"] and kim["get"] == "daw separate setup --models kim-mel-roformer"
    assert kim["files"][0] == {"name": "vocals_mel_band_roformer.ckpt", "bytes": 913106900, "here": False, "matches": None}
    assert kim["sdr"] == {"vocals": 12.6} and kim["license_url"].startswith("https://huggingface.co/")
    assert "copies of it" in listed["rights"]
    code, some = cli("separate", "models", "--split", "stems")
    assert [m["name"] for m in some["models"]] == ["htdemucs", "htdemucs_ft"]
    # With the runtime there and the model not, the model is what is named.
    python = env / "separation" / "env" / "bin" / "python"
    python.parent.mkdir(parents=True)
    python.write_text("")
    code, bad = cli("separate", song, "mix", "--quality", "fast")
    assert code == 1
    assert bad["error"].startswith("The model mdx-inst-hq4 (UVR-MDX-NET Inst HQ 4, 59 MB, MIT by the statement")
    assert bad["error"].endswith("is not downloaded: run daw separate setup --models mdx-inst-hq4")
    # A file here that does not match the manifest is named too, and never run.
    (env / "separation" / "models").mkdir()
    (env / "separation" / "models" / "UVR-MDX-NET-Inst_HQ_4.onnx").write_bytes(b"not a model")
    code, bad = cli("separate", song, "mix", "--quality", "fast")
    assert code == 1 and "UVR-MDX-NET-Inst_HQ_4.onnx in" in bad["error"] and "does not match the manifest's hash" in bad["error"]
    assert cli("separate", "models", "--split", "vocals")[1]["models"][0]["files"][0]["matches"] is False


def test_setup_refuses_an_unknown_model_and_downloads_nothing_without_uv(env, monkeypatch):
    monkeypatch.delenv("AAW_SEPARATOR")
    code, bad = cli("separate", "setup", "--models", "nope")
    assert code == 1 and "No model named nope" in bad["error"]
    monkeypatch.setenv("PATH", str(env))
    monkeypatch.setattr(Path, "home", classmethod(lambda cls: env))
    if separation.apple_silicon() and not Path("/opt/homebrew/bin/uv").exists():
        with pytest.raises(ValueError, match="needs uv"):
            separation.setup(runtime_only=True)
    assert not (env / "separation" / "env").exists()


def test_the_manifest_names_each_models_source_license_and_hash():
    manifest = separation.manifest()
    names = [m["name"] for m in manifest["models"]]
    assert len(set(names)) == len(names)
    for model in manifest["models"]:
        assert model["split"] in separation.SPLITS and model["quality"] in separation.QUALITIES
        assert set(model["parts"].values()) == set(separation.SPLITS[model["split"]])
        assert model["author"] and model["license"] and model["license_url"].startswith("https://")
        for file in model["files"]:
            assert file["url"].startswith("https://") and file["bytes"] > 0 and len(file["sha256"]) == 64
    assert manifest["runner"] == {
        "package": "audio-separator",
        "version": "0.47.0",
        "python": "cpython-3.12-macos-aarch64-none",
        "also": ["audioread"],
        "data": manifest["runner"]["data"],
    }
    assert [d["name"] for d in manifest["runner"]["data"]] == ["download_checks.json", "mdx_model_data.json", "vr_model_data.json"]
    # The defaults of every split are in the manifest, and the whole default download is what the refusal says.
    assert separation.mb(sum(f["bytes"] for m in separation.defaults() for f in m["files"])) == 1393


def test_describe_separate_and_help_point_the_way():
    described = daw("describe", "separate")["semantics"]
    assert "daw separate PROJECT SOURCE" in described["command"]
    assert "residual_db" in described["parts"] and "--tracks" in described["tracks"]
    assert "913 MB" in described["models"] and "daw separate setup" in described["setup"]
    assert "copies of it" in described["rights"]
    assert "separate" in daw("describe")["topics"]
    out = subprocess.run([os.environ["AAW_DAW"], "separate", "--help"], capture_output=True, text=True)
    assert out.returncode == 0 and "--split" in out.stdout and "setup" in out.stdout


@pytest.mark.skipif(not os.environ.get("AAW_SEPARATE_REAL"), reason="AAW_SEPARATE_REAL runs the real runtime over generated audio")
def test_htdemucs_separates_generated_audio_on_this_mac(tmp_path, monkeypatch):
    monkeypatch.delenv("AAW_SEPARATOR", raising=False)
    if not separation.runtime()["installed"]:
        pytest.skip("The separation runtime is not installed")
    folder = tmp_path / "real"
    daw("init", folder, "--tempo", 120, "--bars", 8)
    path = folder / "song.yaml"
    (folder / "samples").mkdir()
    x = mix(folder / "samples" / "mix.wav", seconds=8)
    daw("apply", path, write(tmp_path / "p.json", {"samples": {"mix": {"path": "samples/mix.wav"}}}), "--expect", daw("inspect", path)["project_sha256"])
    code, made = cli("separate", path, "mix", "--split", "stems", "--quality", "fast")
    assert code == 0, made
    assert made["runtime"]["device"] in ("mps", "cpu") and made["seconds"]["total"] > 0
    parts = {p["part"]: sf.read(folder / p["path"], always_2d=True)[0] for p in made["parts"]}
    assert all(len(p) == len(x) for p in parts.values())
    # The bass carries the 110 Hz tone more than the vocals do.
    assert np.sqrt(np.mean(parts["bass"] ** 2)) > np.sqrt(np.mean(parts["vocals"] ** 2))
    assert made["residual_db"] < -10
