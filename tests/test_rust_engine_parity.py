"""The Rust engine must render songs as engine.py does.

Songs are generated with generated audio (tones, noise bursts and clicks) in every
sample format the library reads, with and without effects, sends, returns and
automation. Effect-free renders are byte-identical; with effects, audio matches
within -120 dBFS. Reverb tails come from a different noise generator (D40), so
they are compared on decay time, spectrum and energy. Skipped when no Rust
toolchain is installed; AAW_PARITY_SONGS sets how many songs are rendered.
"""

import json
import os
import random
import subprocess
from fractions import Fraction

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


def pick(rng, low, high):
    """A value in a range, written as an integer, a short decimal or a long one."""
    return round(rng.uniform(low, high), rng.choice([0, 1, 6]))


def make_effect(rng, kind, sources, n):
    if kind == "filter":
        e = {"mode": rng.choice(["highpass", "lowpass"]), "cutoff_hz": rng.choice([40, 180.5, 900, 4000, 12000])}
        if rng.random() < 0.6:
            e["slope_db_per_octave"] = rng.choice([12, 24, 36, 48])
    elif kind == "eq":
        e = {"bands": []}
        for _ in range(rng.randint(1, 3)):
            band = {"shape": rng.choice(["bell", "low_shelf", "high_shelf"]), "freq_hz": pick(rng, 60, 9000), "gain_db": pick(rng, -9, 9)}
            if rng.random() < 0.5:
                band["q"] = round(rng.uniform(0.3, 4), 2)
            e["bands"].append(band)
    elif kind == "compressor":
        e = {"threshold_db": pick(rng, -45, -12)}
        for field, values in [("ratio", [1, 2.5, 8, 20]), ("attack_ms", [0, 0.5, 10, 40]), ("release_ms", [5, 80, 400]), ("knee_db", [0, 3, 12]), ("makeup_db", [-3, 0, 4.5])]:
            if rng.random() < 0.5:
                e[field] = rng.choice(values)
        if sources and rng.random() < 0.6:
            e["sidechain"] = rng.choice(sources)
    elif kind == "limiter":
        e = {}
        for field, values in [("ceiling_db", [-0.5, -3, -9]), ("release_ms", [5, 60, 300]), ("lookahead_ms", [0.5, 3, 7.3])]:
            if rng.random() < 0.6:
                e[field] = rng.choice(values)
    elif kind == "delay":
        e = {"time_beats": rng.choice(["1/4", 0.5, "3/4", "1/3", 1])}
        for field, values in [("feedback_percent", [0, 30, 70]), ("lowcut_hz", [120, 400]), ("highcut_hz", [1500, 6000]), ("ping_pong", [True]), ("mix_percent", [20, 55.5, 100])]:
            if rng.random() < 0.5:
                e[field] = rng.choice(values)
    else:
        e = {"decay_seconds": rng.choice([0.3, 0.8, 1.6])}
        for field, values in [("predelay_ms", [0, 12, 40]), ("damping_hz", [2000, 9000]), ("lowcut_hz", [60, 300]), ("width_percent", [0, 60]), ("mix_percent", [25, 100]), ("seed", [3])]:
            if rng.random() < 0.4:
                e[field] = rng.choice(values)
    e["type"] = kind
    if rng.random() < 0.6:
        e["id"] = f"{kind}{n}"
    if rng.random() < 0.1:
        e["bypass"] = True
    return e


LANES = {
    "filter": [("cutoff_hz", 100, 12000)],
    "eq": [("freq_hz", 60, 10000), ("gain_db", -12, 12), ("q", 0.3, 4)],
    "compressor": [("threshold_db", -50, -5), ("makeup_db", -6, 6)],
    "delay": [("feedback_percent", 0, 80), ("mix_percent", 0, 100)],
    "reverb": [("mix_percent", 0, 100)],
}


def make_lanes(rng, owner, length, kind):
    """Lanes for a track, a return or the master: levels, sends and effect parameters."""
    targets = [("gain_db", -30, -3)]
    if kind != "master":
        targets.append(("pan", -1, 1))
    targets += [(f"sends.{s['to']}.gain_db", -40, 0) for s in owner.get("sends", [])]
    for i, e in enumerate(owner.get("effects", [])):
        ref = e["id"] if "id" in e and rng.random() < 0.7 else str(i)
        for field, low, high in LANES.get(e["type"], []):
            if e["type"] == "eq":
                targets.append((f"effects.{ref}.bands.{rng.randrange(len(e['bands']))}.{field}", low, high))
            else:
                targets.append((f"effects.{ref}.{field}", low, high))
    rng.shuffle(targets)
    lanes = []
    for param, low, high in targets[: rng.randint(0, 3)]:
        times = [rng.choice([0, "1/3", 1, 2.5, length // 2, length - 1, length]) for _ in range(rng.randint(1, 5))]
        beats = []
        # In time order; two points at one position jump, and no more may share one.
        for t in sorted(times, key=lambda t: Fraction(str(t))):
            if beats[-2:] != [t, t]:
                beats.append(t)
        value = pick(rng, low, high)
        points = []
        for t in beats:
            # Some lanes are constant, which renders as the static value.
            if rng.random() < 0.8:
                value = pick(rng, low, high)
            point = {"at": t, "value": max(low, min(high, value))}
            if rng.random() < 0.3:
                point["curve"] = "hold"
            points.append(point)
        lanes.append({"param": param, "points": points})
    return lanes


def add_mix(rng, song, kinds):
    """Gives a song effects of the given kinds, sends, returns and automation."""
    length = song["session"]["length_beats"]
    returns = [{"id": f"r{i}"} for i in range(rng.randint(0, 2))]
    ids = [t["id"] for t in song["tracks"]]
    for i, t in enumerate(song["tracks"]):
        effects = [make_effect(rng, rng.choice(kinds), ids[:i], n) for n in range(rng.randint(0, 3))]
        if effects:
            t["effects"] = effects
        sends = []
        for r in returns:
            if rng.random() < 0.6:
                send = {"to": r["id"], "gain_db": pick(rng, -20, 0)}
                if rng.random() < 0.4:
                    send["pre_fader"] = True
                sends.append(send)
        if sends:
            t["sends"] = sends
        lanes = make_lanes(rng, t, length, "track")
        if lanes:
            t["automation"] = lanes
    for r in returns:
        wet = [k for k in kinds if k in ("delay", "reverb")] or kinds
        r["effects"] = [make_effect(rng, rng.choice(wet if n == 0 else kinds), ids, n) for n in range(rng.randint(1, 2))]
        for field, low, high in [("gain_db", -12, 0), ("pan", -1, 1)]:
            if rng.random() < 0.4:
                r[field] = pick(rng, low, high)
        if rng.random() < 0.1:
            r["mute"] = True
        lanes = make_lanes(rng, r, length, "return")
        if lanes:
            r["automation"] = lanes
    if returns:
        song["returns"] = returns
    master = {}
    effects = [make_effect(rng, rng.choice([k for k in kinds if k != "delay"]), [], n) for n in range(rng.randint(0, 2))]
    if rng.random() < 0.7:
        effects.append({"type": "limiter", "ceiling_db": -1})
    if effects:
        master["effects"] = effects
    lanes = make_lanes(rng, master, length, "master")
    if lanes:
        master["automation"] = lanes
    if master:
        song["master"] = master


DRY = ["filter", "eq", "compressor", "limiter", "delay"]


def make_mix_song(rng, directory, kinds=DRY):
    path = make_song(rng, directory)
    song = yaml.safe_load(path.read_text())
    add_mix(rng, song, kinds)
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


def close(a, b):
    return a == pytest.approx(b, rel=1e-6, abs=1e-6)


def same_effects(py, rs):
    """Whether two effect reports agree: types, bypass, latency, lanes and gain reduction."""
    assert len(py) == len(rs), (py, rs)
    for a, b in zip(py, rs):
        assert set(a) == set(b), (a, b)
        for key, value in a.items():
            assert close(value, b[key]) if isinstance(value, float) else value == b[key], (key, a, b)


def test_effects_and_automation_match_python(rust_daw, tmp_path):
    rng = random.Random(11)
    compared = 0
    worst = 0.0
    for i in range(int(os.environ.get("AAW_PARITY_SONGS", 12))):
        song_dir = tmp_path / f"song{i}"
        song_dir.mkdir()
        path = make_mix_song(rng, song_dir)
        p = load(path)
        channels = [t.id for t in p.tracks] + [r.id for r in p.returns]
        target = rng.choice([{}, {}, {"track": rng.choice(channels)}, {"section": rng.choice(["a", "b"])}])
        py = python_render(path, song_dir / "py", **{"track_id": target.get("track"), "section": target.get("section")})
        rs = rust_render(rust_daw, path, song_dir / "rs", **target)
        assert ("error" in py) == ("error" in rs), (i, py.get("error"), rs.get("error"))
        if "error" in py:
            # The peak in the message may differ in its last digit.
            assert py["error"].split(":")[0] == rs["error"].split(":")[0]
            continue
        assert list(py["tracks"]) == list(rs["tracks"])
        for f in ["mix.wav", *[f"stems/{t}.wav" for t in py["tracks"]]]:
            diff, _ = compare_audio(song_dir / "py" / f, song_dir / "rs" / f)
            worst = max(worst, diff)
            compared += 1
            assert diff <= TOLERANCE, (i, f, diff)
        for name, a in py["tracks"].items():
            b = rs["tracks"][name]
            assert a["kind"] == b["kind"]
            assert a.get("events") == b.get("events") and a.get("senders") == b.get("senders")
            assert a.get("automation") == b.get("automation")
            assert close(a["peak_dbfs"], b["peak_dbfs"])
            same_effects(a["effects"], b["effects"])
        same_effects(py["master_effects"], rs["master_effects"])
        assert py["master_automation"] == rs["master_automation"]
        assert py["stems_sum_to_mix"] == rs["stems_sum_to_mix"]
        assert json.loads(json.dumps(py["target"])) == rs["target"]
        for key in ["peak_dbfs", "rms_dbfs", "estimated_true_peak_dbtp"]:
            assert close(py["mix"][key], rs["mix"][key])
    assert compared >= 20, compared
    print(f"{compared} files, worst difference {worst:.3g}")


def write_reverb_song(directory, reverb, sample):
    """One hit of a sample at beat 0 into a return whose only effect is the reverb."""
    rate = 48000
    if sample == "click":
        x = np.zeros(2000)
        x[50] = 0.5
    else:
        x = np.random.default_rng(1).standard_normal(4 * rate) * 0.05
    sf.write(directory / "x.wav", x, rate, subtype="FLOAT")
    song = {
        "session": {"tempo": 120, "length_beats": 8, "end_fade_ms": 0, "master_gain_db": 0},
        "samples": {"x": {"path": "x.wav"}},
        "patterns": {"hit": {"length_beats": 8, "events": [{"at": 0, "pad": "x"}]}},
        "tracks": [
            {
                "id": "dry",
                "pads": {"x": {"sample": "x", "attack_ms": 0, "release_ms": 0}},
                "clips": [{"pattern": "hit"}],
                "sends": [{"to": "room"}],
            }
        ],
        "returns": [{"id": "room", "effects": [{"type": "reverb", **reverb}]}],
    }
    path = directory / "song.yaml"
    path.write_text(yaml.safe_dump(song))
    return path


def decay_seconds(x, rate):
    """Seconds to fall 60 dB, from the slope of the backward-integrated energy between -5 and -35 dB."""
    energy = np.cumsum(x[::-1] ** 2)[::-1]
    level = 10 * np.log10(np.maximum(energy / energy[0], 1e-30))
    return 2 * (np.argmax(level <= -35) - np.argmax(level <= -5)) / rate


def band_levels(x, rate):
    """Energy in each octave from 125 Hz to 8 kHz, in dB."""
    power = np.abs(np.fft.rfft(x)) ** 2
    freqs = np.fft.rfftfreq(len(x), 1 / rate)
    return [10 * np.log10(power[(freqs >= f / 2**0.5) & (freqs < f * 2**0.5)].sum()) for f in [125, 250, 500, 1000, 2000, 4000, 8000]]


@pytest.mark.parametrize(
    "reverb",
    [
        {},
        {"decay_seconds": 0.6, "predelay_ms": 25, "damping_hz": 2500, "lowcut_hz": 300},
        {"decay_seconds": 1.2, "predelay_ms": 0, "damping_hz": 12000, "lowcut_hz": 40, "width_percent": 0},
    ],
)
def test_reverb_matches_python_statistically(rust_daw, tmp_path, reverb):
    """The Rust reverb draws its noise from its own generator (D40): a tail is
    not sample-identical to Python's, but has its energy, onset, width, and over
    several seeds its decay time and spectrum."""
    rate = 48000
    decay, bands = {"py": [], "rs": []}, {"py": [], "rs": []}
    for seed in range(6):
        directory = tmp_path / str(seed)
        directory.mkdir()
        path = write_reverb_song(directory, {**reverb, "seed": seed}, "click")
        assert "error" not in python_render(path, directory / "py")
        assert "error" not in rust_render(rust_daw, path, directory / "rs")
        dry, _ = compare_audio(directory / "py" / "stems" / "dry.wav", directory / "rs" / "stems" / "dry.wav")
        assert dry == 0
        py, _ = sf.read(directory / "py" / "stems" / "room.wav", dtype="float64")
        rs, _ = sf.read(directory / "rs" / "stems" / "room.wav", dtype="float64")
        assert py.shape == rs.shape
        # The tail starts on the same frame, the hit plus the predelay, and rises
        # over the 3 ms onset. Before it there is only the transforms' rounding.
        first = lambda x: int(np.argmax(np.abs(x).max(axis=1) > 1e-9))
        start = 50 + round(reverb.get("predelay_ms", 10) * rate / 1000)
        assert start <= first(py) <= start + 20 and start <= first(rs) <= start + 20
        for c in range(2):
            # Each channel's response is normalized to the same energy.
            assert 10 * np.log10(np.sum(rs[:, c] ** 2) / np.sum(py[:, c] ** 2)) == pytest.approx(0, abs=0.05)
        correlation = lambda x: abs(np.corrcoef(x[:, 0], x[:, 1])[0, 1])
        if reverb.get("width_percent", 100) == 0:
            assert np.array_equal(rs[:, 0], rs[:, 1]) and np.array_equal(py[:, 0], py[:, 1])
        else:
            assert correlation(rs) < 0.1 and correlation(py) < 0.1
        for name, x in [("py", py), ("rs", rs)]:
            decay[name].append(decay_seconds(x[:, 0], rate))
            bands[name].append(band_levels(x[:, 0], rate))
    assert np.mean(decay["rs"]) == pytest.approx(np.mean(decay["py"]), rel=0.03)
    # One tail's low octaves hold few independent frequencies and vary by a
    # couple of dB from seed to seed in either engine.
    difference = np.abs(np.mean(bands["rs"], axis=0) - np.mean(bands["py"], axis=0))
    assert np.all(difference <= [2.5, 2.5, 1.5, 1, 1, 1, 1]), difference


def test_reverb_keeps_the_level_of_steady_noise(rust_daw, tmp_path):
    path = write_reverb_song(tmp_path, {"decay_seconds": 1.0}, "noise")
    assert "error" not in python_render(path, tmp_path / "py")
    assert "error" not in rust_render(rust_daw, path, tmp_path / "rs")
    level = lambda f: 10 * np.log10(np.mean(sf.read(f, dtype="float64")[0][2 * 48000 :] ** 2))
    dry = level(tmp_path / "rs" / "stems" / "dry.wav")
    assert level(tmp_path / "rs" / "stems" / "room.wav") == pytest.approx(dry, abs=0.5)
    assert level(tmp_path / "py" / "stems" / "room.wav") == pytest.approx(dry, abs=0.5)
