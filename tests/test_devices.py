"""Devices of the person's own, through `daw`: a plugin scaffolded, checked and
listed; added to a chain by its slug and held to its manifest; its cache
made by its code at a render; a lane on a field; what `daw check` says of one
that is missing or at another version."""

import json
import os

import numpy as np
import pytest
import yaml
from helpers import SR, cli, daw, level_db, run_chain, tone


@pytest.fixture
def workspace(tmp_path, monkeypatch):
    """A workspace of the test's own, where `daw plugin new` puts a plugin."""
    ws = tmp_path / "workspace"
    monkeypatch.setenv("AAW_WORKSPACE", str(ws))
    return ws


def test_a_plugin_is_scaffolded_checked_and_listed(workspace):
    made = daw("plugin", "new", "Tilt EQ")
    assert made["slug"] == "tilt-eq" and made["name"] == "Tilt EQ" and made["from"] == "workspace"
    folder = workspace / "devices" / "tilt-eq"
    assert (folder / "device.yaml").is_file() and (folder / "device.py").is_file()
    assert "daw plugin check tilt-eq" in made["next"]
    with pytest.raises(ValueError, match="already there"):
        daw("plugin", "new", "Tilt EQ")
    listed = daw("plugin", "list")
    assert [d["slug"] for d in listed["devices"]] == ["tilt-eq"]
    assert [f["name"] for f in listed["devices"][0]["fields"]] == ["gain_db", "tone_hz", "warmth_percent", "mode", "invert"]
    assert daw("plugin", "list", "reverb")["devices"] == []
    shown = daw("plugin", "show", "tilt-eq")
    gain = shown["fields"][0]
    assert (gain["type"], gain["min"], gain["max"], gain["default"], gain["lane"], gain["unit"]) == ("db", -24, 24, 0, True, "dB")
    assert shown["fields"][3] == {"name": "mode", "label": "Mode", "type": "choice", "choices": ["stereo", "mono"], "default": "stereo"}
    checked = daw("plugin", "check", "tilt-eq")
    assert checked["ok"] is True and checked["frames_out"] == 24000 and checked["channels"] == 2
    # A manifest that does not read is refused in words.
    (folder / "device.yaml").write_text("format: 1\nname: Tilt EQ\nfields: {gain: {type: db}}\n")
    with pytest.raises(ValueError, match="min is required"):
        daw("plugin", "show", "tilt-eq")
    assert "min is required" in daw("plugin", "list")["problems"][0]


def test_a_device_in_a_chain_renders_through_its_code_and_a_lane_moves_its_field(tmp_path, workspace):
    daw("plugin", "new", "Tilt EQ")
    x = tone(1000)
    device = {"type": "device", "name": "tilt-eq", "fields": {"gain_db": -6}}
    out, reports = run_chain(tmp_path / "gain", [device], x)
    assert abs(level_db(out) - level_db(x) + 6.02) < 0.05
    assert reports[0]["type"] == "device"
    # The cache is under the project's .daw/devices, and a render again
    # reads it rather than running the code: the same bytes.
    caches = list((tmp_path / "gain" / ".daw" / "devices").glob("*.wav"))
    assert len(caches) == 1
    again, _ = run_chain(tmp_path / "gain2", [device], x)
    assert np.array_equal(out, again)
    # A lane on gain_db: -24 at the start to 0 at the end, so the second
    # half is louder than the first.
    moved, _ = run_chain(tmp_path / "lane", [device], x, automation={0: {"fields.gain_db": [(0, -24), (2, 0)]}})
    half = len(x) // 2
    assert level_db(moved[half:]) - level_db(moved[:half]) > 8
    # The warmth bends the tone: harmonics appear.
    warm, _ = run_chain(tmp_path / "warm", [{**device, "fields": {"gain_db": 0, "warmth_percent": 100}}], x)
    spectrum = np.abs(np.fft.rfft(warm[:, 0] * np.hanning(len(warm))))
    assert spectrum[3000] > spectrum[1000] * 0.01
    # A field outside the manifest's range, or a field it lacks, is refused.
    with pytest.raises(ValueError, match="gain_db is -24 to 24"):
        run_chain(tmp_path / "loud", [{**device, "fields": {"gain_db": 40}}], x)
    with pytest.raises(ValueError, match="no field colour"):
        run_chain(tmp_path / "colour", [{**device, "fields": {"colour": 1}}], x)
    # Code that fails refuses the render with the reason.
    (workspace / "devices" / "tilt-eq" / "device.py").write_text("def process(audio, sample_rate, fields):\n    raise ValueError('no sound today')\n")
    with pytest.raises(ValueError, match="no sound today"):
        run_chain(tmp_path / "broken", [device], x)


def test_a_device_is_added_by_slug_from_the_terminal_and_the_check_warns_of_one_missing(tmp_path, workspace):
    daw("plugin", "new", "Tilt EQ")
    project = tmp_path / "song"
    daw("init", project)
    daw("track", "add", project, "bass")
    added = daw("effect", "add", project, "tracks.bass", "--type", "tilt-eq", "--gain-db", "-3", "--mode", "mono")
    assert added["label"] == "Add tilt-eq to tracks.bass"
    effect = daw("get", project, "tracks.bass.effects.0")
    assert effect["type"] == "device" and effect["name"] == "tilt-eq" and effect["version"] == "0.1"
    assert effect["fields"]["gain_db"] == -3 and effect["fields"]["mode"] == "mono" and effect["fields"]["tone_hz"] == 20000
    daw("set", project, "tracks.bass.effects.0.fields.gain_db", "-9")
    with pytest.raises(ValueError, match="one of stereo, mono"):
        daw("set", project, "tracks.bass.effects.0.fields.mode", "wide")
    daw("lane", "set", project, "tracks.bass", "effects.0.fields.gain_db", '[{"at": 0, "value": -12}]')
    with pytest.raises(ValueError, match="does not take a lane on tone_hz"):
        daw("lane", "set", project, "tracks.bass", "effects.0.fields.tone_hz", '[{"at": 0, "value": 800}]')
    # Its state without a host: waiting, nothing having run.
    status = daw("plugin", "status", project)
    assert status["host"] is False
    assert status["devices"] == [{"effect": "tracks.bass.effects.0", "device": "tilt-eq", "state": "waiting"}]
    # The check: nothing to say while it is installed at the song's version;
    # another version, and a device that is not there, are warnings.
    code, report = cli("check", project)
    assert code == 0
    assert [w for w in report["warnings"] if w["code"].startswith("device")] == []
    song = yaml.safe_load((project / "song.yaml").read_text())
    song["tracks"][0]["effects"][0]["version"] = "9"
    song["master"] = {"effects": [{"type": "device", "name": "gone"}]}
    (project / "song.yaml").write_text(yaml.safe_dump(song))
    code, report = cli("check", project)
    assert code == 0
    codes = {w["code"]: w for w in report["warnings"]}
    assert "installed at 0.1; the song was written with 9" in codes["device-version"]["message"]
    assert codes["device-version"]["paths"] == ["tracks.bass.effects.0"]
    assert "gone is not installed" in codes["device-missing"]["message"]
    assert daw("plugin", "status", project)["devices"][1]["state"] == "missing"
    # The describe says what a device is, and the effect's help how to add one.
    described = daw("describe", "effects")
    assert "daw plugin new" in described["semantics"]["device"]
    assert "device-missing" in daw("describe", "check")["codes"]


def test_a_project_s_own_device_shadows_the_workspace_s(tmp_path, workspace):
    daw("plugin", "new", "Tilt EQ")
    project = tmp_path / "song"
    daw("init", project)
    own = daw("plugin", "new", "Tilt EQ", "--project", project)
    assert own["from"] == "project" and own["folder"].startswith(str(project))
    listed = daw("plugin", "list", "--project", project)
    assert [(d["slug"], d["from"]) for d in listed["devices"]] == [("tilt-eq", "project")]
    assert daw("plugin", "list")["devices"][0]["from"] == "workspace"
    # The project's copy is what the song uses: changed there, it is found.
    (project / "devices" / "tilt-eq" / "device.yaml").write_text(
        (project / "devices" / "tilt-eq" / "device.yaml").read_text().replace('version: "0.1"', 'version: "0.2"')
    )
    assert daw("plugin", "show", "tilt-eq", "--project", project)["version"] == "0.2"
    assert daw("plugin", "show", "tilt-eq")["version"] == "0.1"
    daw("track", "add", project, "bass")
    daw("effect", "add", project, "tracks.bass", "--type", "tilt-eq")
    assert daw("get", project, "tracks.bass.effects.0.version") == "0.2"
    assert os.environ["AAW_WORKSPACE"] == str(workspace)
    assert json.loads(json.dumps(listed))["project"].endswith("devices")


def test_a_realtime_plugin_is_built_run_in_the_song_and_rebuilt(tmp_path, workspace):
    made = daw("plugin", "new", "Live Tilt", "--realtime")
    if "build_error" in made:
        if "not installed" in made["build_error"] or "Could not start" in made["build_error"]:
            pytest.skip("no Rust for WebAssembly here")
        raise AssertionError(made["build_error"])
    assert made["run"] == "realtime" and made["entry"] == "device.wasm" and made["source"] == "device.rs"
    assert made["build"]["ok"] is True and made["build"]["latency"] == 0
    assert "daw plugin build live-tilt" in made["next"]
    folder = workspace / "devices" / "live-tilt"
    assert all((folder / f).is_file() for f in ("device.yaml", "device.rs", "device.wasm"))
    # In a chain it runs as the song plays, and makes no cache.
    x = tone(1000)
    device = {"type": "device", "name": "live-tilt", "fields": {"gain_db": -6}}
    out, reports = run_chain(tmp_path / "gain", [device], x)
    assert abs(level_db(out) - level_db(x) + 6.02) < 0.05
    assert reports[0]["type"] == "device"
    assert not list((tmp_path / "gain").glob(".daw/devices/*.wav"))
    # A lane on its gain, and one on its tone, which moves on a log scale.
    moved, _ = run_chain(tmp_path / "lane", [device], x, automation={0: {"fields.gain_db": [(0, -24), (2, 0)]}})
    half = len(x) // 2
    assert level_db(moved[half:]) - level_db(moved[:half]) > 8
    dark, _ = run_chain(tmp_path / "dark", [device], x, automation={0: {"fields.tone_hz": [(0, 200), (2, 200)]}})
    assert level_db(dark) < level_db(out) - 6
    # Its state without a host: ready, its code running.
    project = tmp_path / "song"
    daw("init", project)
    daw("track", "add", project, "bass")
    daw("effect", "add", project, "tracks.bass", "--type", "live-tilt")
    assert daw("plugin", "status", project)["devices"] == [{"effect": "tracks.bass.effects.0", "device": "live-tilt", "state": "ready"}]
    # A changed source is said to be stale until built; one that does not
    # compile is refused with rustc's words.
    source = folder / "device.rs"
    os.utime(folder / "device.wasm", (1, 1))
    assert "daw plugin build live-tilt" in daw("plugin", "check", "live-tilt")["stale"]
    source.write_text(source.read_text().replace("fn latency(&self) -> usize {\n        0", "fn latency(&self) -> usize {\n        0 +"))
    with pytest.raises(ValueError, match="device.rs does not build"):
        daw("plugin", "build", "live-tilt")
    # An offline plugin has nothing to build.
    daw("plugin", "new", "Tilt EQ")
    with pytest.raises(ValueError, match="runs offline"):
        daw("plugin", "build", "tilt-eq")
