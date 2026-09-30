"""The Rust engine must render effect-free songs as engine.py does.

Songs are generated with generated audio (tones, noise bursts and clicks) in every
sample format the library reads. Skipped when no Rust toolchain is installed;
AAW_PARITY_SONGS sets how many songs are rendered.
"""

import json
import os
import random
import subprocess

import numpy as np
import pytest
import soundfile as sf
import yaml

from agent_daw.engine import render, schedule
from agent_daw.model import Project, load
from test_rust_model_parity import rust_daw  # noqa: F401  (session fixture)

FORMATS = [
    ("wav", "PCM_16"),
    ("wav", "PCM_24"),
    ("wav", "PCM_32"),
    ("wav", "FLOAT"),
    ("wav", "DOUBLE"),
    ("wav", "PCM_U8"),
    ("aiff", "PCM_16"),
    ("flac", "PCM_24"),
]
RATES = [22050, 44100, 48000, 96000]
# -120 dBFS: the plan's tolerance for audio that is not bit-identical.
TOLERANCE = 1e-6


def make_samples(rng, directory):
    """Short tones, noise bursts and clicks in random formats and rates."""
    samples = {}
    for i in range(rng.randint(2, 4)):
        kind = rng.choice(["tone", "noise", "click"])
        ext, subtype = rng.choice(FORMATS)
        rate = rng.choice(RATES)
        frames = rng.randint(rate // 20, rate // 2)
        channels = rng.choice([1, 2])
        t = np.arange(frames) / rate
        if kind == "tone":
            x = np.sin(2 * np.pi * rng.uniform(80, 900) * t) * rng.uniform(0.1, 0.4)
        elif kind == "noise":
            x = np.random.default_rng(i).standard_normal(frames) * 0.15 * np.exp(-t * 20)
        else:
            x = np.zeros(frames)
            x[:: max(1, frames // 7)] = 0.5
        x = np.stack([x, x * rng.uniform(0.3, 1)], axis=1) if channels == 2 else x
        name = f"s{i}"
        sf.write(directory / f"{name}.{ext}", x, rate, subtype=subtype)
        samples[name] = {"path": f"{name}.{ext}", "root_note": rng.choice(["A3", "C2", "E4"]), "seconds": frames / rate}
    return samples


def make_song(rng, directory):
    samples = make_samples(rng, directory)
    length = rng.choice([8, 16])
    pads = {}
    for pad in ["a", "b", "g"]:
        name = rng.choice(list(samples))
        p = {"sample": name}
        if pad == "g":
            p["mode"] = "gate"
        for field, low, high in [("gain_db", -12, 3), ("pan", -1, 1), ("transpose", -7, 7), ("attack_ms", 0, 20), ("release_ms", 0, 60)]:
            if rng.random() < 0.4:
                p[field] = round(rng.uniform(low, high), rng.choice([0, 2, 6]))
        if rng.random() < 0.3:
            seconds = samples[name]["seconds"]
            p["start_seconds"] = round(seconds * rng.uniform(0, 0.3), 4)
            p["end_seconds"] = round(seconds * rng.uniform(0.5, 1), 4)
        for flag in ["reverse", "mono"]:
            if rng.random() < 0.25:
                p[flag] = True
        if rng.random() < 0.3:
            p["choke_group"] = "c"
        if rng.random() < 0.2:
            p["source_bpm"] = rng.choice([90, 120.5, 140])
        pads[pad] = p
    patterns = {}
    for n in range(2):
        steps = {pad: "".join(rng.choice(".x.3x9") for _ in range(16)) for pad in rng.sample(["a", "b"], rng.randint(1, 2))}
        events = []
        for _ in range(rng.randint(0, 4)):
            pad = rng.choice(["a", "b", "g"])
            e = {"at": rng.choice([0, "1/3", 1.5, "5/4", 2, 3.75]), "pad": pad}
            if pad == "g" or rng.random() < 0.3:
                e["duration"] = rng.choice([0.25, "1/3", 1, 2.5])
            if rng.random() < 0.4:
                e["note"] = rng.choice(["C3", "G#2", "D4"])
            if rng.random() < 0.3:
                e["velocity"] = rng.randint(1, 127)
            if rng.random() < 0.2:
                e["transpose"] = rng.choice([-12, 0.5, 5])
            events.append(e)
        patterns[f"p{n}"] = {"length_beats": 4, "steps": steps, "events": events, "swing": rng.choice([0.5, 0.58, 2 / 3])}
    tracks = []
    for i in range(rng.randint(1, 5)):
        t = {"id": f"t{i}", "pads": pads}
        clips = []
        for _ in range(rng.randint(1, 3)):
            repeats = rng.choice([1, 2])
            at = rng.randrange(0, length - 4 * repeats + 1)
            c = {"pattern": rng.choice(list(patterns)), "at": rng.choice([at, str(at)]), "repeats": repeats}
            if rng.random() < 0.3:
                c["velocity_scale"] = round(rng.uniform(0.2, 1.5), 3)
            clips.append(c)
        t["clips"] = clips
        for field, low, high in [("gain_db", -18, 0), ("pan", -1, 1)]:
            if rng.random() < 0.5:
                t[field] = round(rng.uniform(low, high), rng.choice([0, 3, 9]))
        for flag in ["mute", "solo"]:
            if rng.random() < 0.15:
                t[flag] = True
        tracks.append(t)
    session = {
        "tempo": rng.choice([120, 97.5, 174, rng.uniform(70, 180)]),
        "length_beats": length,
        "sample_rate": rng.choice([44100, 48000]),
        "master_gain_db": rng.choice([-12, -18, -9.5]),
    }
    if rng.random() < 0.5:
        session["end_fade_ms"] = rng.choice([0, 1, 20.8, 500])
    song = {
        "session": session,
        "samples": {k: {"path": v["path"], "root_note": v["root_note"]} for k, v in samples.items()},
        "patterns": patterns,
        "tracks": tracks,
        "sections": [{"id": "a", "at": 0, "length_beats": 4}, {"id": "b", "at": "10/3", "length_beats": 4}],
    }
    path = directory / "song.yaml"
    path.write_text(yaml.safe_dump(song))
    return path


def python_render(path, output, **kwargs):
    try:
        return render(path, output, **kwargs)
    except ValueError as e:
        return {"error": str(e)}


def rust_render(daw, path, output, track=None, section=None):
    cmd = [daw, "render", str(path), "--output", str(output)]
    cmd += ["--track", track] if track else []
    cmd += ["--section", section] if section else []
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode:
        return {"error": json.loads(out.stderr)["error"]}
    return json.loads(out.stdout)


def compare_audio(a, b):
    x, rx = sf.read(a, dtype="float64", always_2d=True)
    y, ry = sf.read(b, dtype="float64", always_2d=True)
    assert rx == ry and x.shape == y.shape, (a, x.shape, y.shape)
    return float(np.max(np.abs(x - y))) if x.size else 0.0, a.read_bytes() == b.read_bytes()


def test_renders_match_python(rust_daw, tmp_path):
    rng = random.Random(2)
    identical = compared = 0
    worst = 0.0
    for i in range(int(os.environ.get("AAW_PARITY_SONGS", 12))):
        song_dir = tmp_path / f"song{i}"
        song_dir.mkdir()
        path = make_song(rng, song_dir)
        p = load(path)
        target = rng.choice([{}, {}, {"track": rng.choice(p.tracks).id}, {"section": rng.choice(["a", "b"])}])
        py = python_render(path, song_dir / "py", **{"track_id": target.get("track"), "section": target.get("section")})
        rs = rust_render(rust_daw, path, song_dir / "rs", **target)
        assert ("error" in py) == ("error" in rs), (i, py.get("error"), rs.get("error"))
        if "error" in py:
            assert py["error"] == rs["error"]
            continue
        files = ["mix.wav", *[f"stems/{t}.wav" for t in py["tracks"]]]
        assert sorted(py["tracks"]) == sorted(rs["tracks"])
        for f in files:
            diff, same = compare_audio(song_dir / "py" / f, song_dir / "rs" / f)
            worst = max(worst, diff)
            identical += same
            compared += 1
            assert diff <= TOLERANCE, (i, f, diff)
        for t in py["tracks"]:
            assert py["tracks"][t]["events"] == rs["tracks"][t]["events"]
        assert json.loads(json.dumps(py["target"])) == rs["target"]
        assert py["mix"]["frames"] == rs["mix"]["frames"]
        assert (song_dir / "py" / "song.snapshot.yaml").read_bytes() == (song_dir / "rs" / "song.snapshot.yaml").read_bytes()
    assert compared >= 20
    # Most files should be byte-identical; the rest differ by float rounding only.
    assert identical >= compared * 0.8, (identical, compared, worst)


def test_schedules_match_python(rust_daw, tmp_path):
    rng = random.Random(9)
    for i in range(10):
        song_dir = tmp_path / f"song{i}"
        song_dir.mkdir()
        path = make_song(rng, song_dir)
        expected = [
            {"start": t.start, "track": t.track, "pad": t.pad, "cutoff": t.cutoff}
            for t in schedule(Project.model_validate(yaml.safe_load(path.read_text())))
        ]
        out = subprocess.run([rust_daw, "schedule", str(path)], capture_output=True, text=True, check=True)
        assert json.loads(out.stdout) == expected


def test_render_refuses_what_it_cannot_process(rust_daw, tmp_path):
    rng = random.Random(4)
    path = make_song(rng, tmp_path)
    song = yaml.safe_load(path.read_text())
    song["tracks"][0]["effects"] = [{"type": "limiter"}]
    path.write_text(yaml.safe_dump(song))
    result = rust_render(rust_daw, path, tmp_path / "out")
    assert "does not render track effects yet" in result["error"]
    # A bypassed effect is not processed, so the song renders.
    song["tracks"][0]["effects"][0]["bypass"] = True
    path.write_text(yaml.safe_dump(song))
    assert "error" not in rust_render(rust_daw, path, tmp_path / "out2")
