"""Generated songs with generated audio, for tests that render.

Samples are short tones, noise bursts and clicks in every format the engine
reads; songs use every pad, pattern, clip and mixer field, and `add_mix` gives
one effects, sends, returns and automation. Nothing here is personal material.
"""

from fractions import Fraction

import numpy as np
import soundfile as sf
import yaml

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


EFFECTS = ["filter", "eq", "compressor", "limiter", "delay", "reverb"]


def make_mix_song(rng, directory, kinds=EFFECTS):
    path = make_song(rng, directory)
    song = yaml.safe_load(path.read_text())
    add_mix(rng, song, kinds)
    path.write_text(yaml.safe_dump(song))
    return path
