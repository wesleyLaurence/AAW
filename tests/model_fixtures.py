"""Generated song documents the model is tested on, valid and not.

Everything here is synthetic: seeded random projects that cover the schema, the
same projects with one field broken, and hand-written YAML edge cases. No
personal projects or sample libraries are involved.
"""

import copy
import json
import random
import re

import yaml

TRICKY_TEXT = [
    "Untitled",
    "My Song",
    "it's",
    'say "hi"',
    "colon: here",
    "# hash",
    "- dash",
    "é unicode 🎵",
    "yes",
    "null",
    "123",
    "1.5",
    "1e5",
    "tab\there",
    "multi\nline",
    "",
    "@at",
    "~",
    "key: value, [x] {y}",
    "  leading",
    "trailing ",
    "a" * 130,
    " ".join(["word"] * 40),
    "back\\slash",
    "non breaking",
    "bell\x07",
    "0x1F",
    "2001-12-14",
    "<<",
    "=",
    "---",
    "!tag",
    "&anchor",
    "*alias",
    "%percent",
    "long " + "é" * 120,
]
KEY_NAMES = ["kick", "snare 1", "yes", "123", "null", "a: b", "é", "x" * 124, "type", "at", "1/4"]
NOTES = ["C2", "F#1", "Bb3", "a0", "G9", "c-1"]
AWKWARD_FLOATS = [0.1 + 0.2, 1e-05, 1e16, 123456789.123, 1 / 3, 2.5e-7, 1e15, 5e-324, 0.1]


def number(rng, low, high, integral=False, strings=True):
    """A value in [low, high] written as an int, a float or a numeric string."""
    options = [low, high, (low + high) / 2, rng.uniform(low, high)]
    options += [x for x in AWKWARD_FLOATS if low <= x <= high]
    if low <= 0 <= high:
        options += [0, -0.0, 0.0]
    x = rng.choice(options)
    if integral:
        x = int(round(x))
    form = rng.random()
    if form < 0.3 and float(x).is_integer():
        return int(x)
    if form < 0.4 and strings:
        return str(x)
    return float(x) if not integral else x


def beat(rng, value):
    """An exact beat position in a random written form."""
    from fractions import Fraction

    f = Fraction(value).limit_denominator(48)
    forms = [str(f)]
    if f.denominator == 1:
        forms += [int(f), f"{int(f)}"]
    if f.denominator in (1, 2, 4, 5, 8, 10):
        forms += [float(f), str(float(f))]
    return rng.choice(forms)


def effect(rng, kind, index, tracks, owner_is_master=False):
    e = {"type": kind}
    if rng.random() < 0.6:
        e["id"] = f"fx{index}"
    if kind == "filter":
        e["mode"] = rng.choice(["highpass", "lowpass"])
        e["cutoff_hz"] = number(rng, 10, 20000)
        if rng.random() < 0.5:
            e["slope_db_per_octave"] = rng.choice([12, 24, 36, 48, 24.0])
    elif kind == "eq":
        e["bands"] = [
            {
                "shape": rng.choice(["bell", "low_shelf", "high_shelf"]),
                "freq_hz": number(rng, 20, 20000),
                "gain_db": number(rng, -24, 24),
                **({"q": number(rng, 0.1, 18)} if rng.random() < 0.5 else {}),
            }
            for _ in range(rng.randint(1, 4))
        ]
    elif kind == "compressor":
        e["threshold_db"] = number(rng, -60, 0)
        for field, low, high in [
            ("ratio", 1, 20),
            ("attack_ms", 0, 500),
            ("release_ms", 1, 5000),
            ("knee_db", 0, 24),
            ("makeup_db", -24, 24),
        ]:
            if rng.random() < 0.4:
                e[field] = number(rng, low, high)
        if tracks and not owner_is_master and rng.random() < 0.4:
            e["sidechain"] = rng.choice(tracks)
    elif kind == "limiter":
        for field, low, high in [("ceiling_db", -24, -0.1), ("release_ms", 1, 2000), ("lookahead_ms", 0.5, 20)]:
            if rng.random() < 0.5:
                e[field] = number(rng, low, high)
    elif kind == "delay":
        e["time_beats"] = rng.choice(["1/4", 0.5, "3/8", 1, "1/3", "2"])
        if rng.random() < 0.5:
            e["feedback_percent"] = number(rng, 0, 95)
        if rng.random() < 0.3:
            e["lowcut_hz"] = number(rng, 10, 500)
        if rng.random() < 0.3:
            e["highcut_hz"] = number(rng, 1000, 20000)
        if rng.random() < 0.3:
            e["ping_pong"] = rng.choice([True, False, "yes", 1])
        if rng.random() < 0.5:
            e["mix_percent"] = number(rng, 0, 100)
    else:
        for field, low, high in [
            ("decay_seconds", 0.1, 12),
            ("predelay_ms", 0, 250),
            ("damping_hz", 500, 20000),
            ("lowcut_hz", 20, 2000),
            ("width_percent", 0, 100),
            ("mix_percent", 0, 100),
        ]:
            if rng.random() < 0.3:
                e[field] = number(rng, low, high)
        if rng.random() < 0.3:
            e["seed"] = rng.choice([0, 7, 4294967295, "12", 3.0])
    if rng.random() < 0.2:
        e["bypass"] = rng.choice([True, False, "off"])
    return e


AUTOMATABLE = {
    "filter": [("cutoff_hz", 10, 20000)],
    "eq": [("freq_hz", 20, 20000), ("gain_db", -24, 24), ("q", 0.1, 18)],
    "compressor": [("threshold_db", -60, 0), ("makeup_db", -24, 24)],
    "delay": [("feedback_percent", 0, 95), ("mix_percent", 0, 100)],
    "reverb": [("mix_percent", 0, 100)],
}


def lanes(rng, owner, length, kind):
    """Automation lanes with valid targets for a track, return or the master."""
    targets = []
    if kind == "master":
        targets.append(("gain_db", -96, 24))
    else:
        targets += [("gain_db", -96, 24), ("pan", -1, 1)]
    if kind == "track":
        targets += [(f"sends.{s['to']}.gain_db", -96, 12) for s in owner.get("sends", [])]
    for i, e in enumerate(owner.get("effects", [])):
        ref = e.get("id", str(i)) if rng.random() < 0.7 else str(i)
        for field, low, high in AUTOMATABLE.get(e["type"], []):
            if e["type"] == "eq":
                band = rng.randrange(len(e["bands"]))
                targets.append((f"effects.{ref}.bands.{band}.{field}", low, high))
            else:
                targets.append((f"effects.{ref}.{field}", low, high))
    rng.shuffle(targets)
    result, seen = [], set()
    for param, low, high in targets[: rng.randint(0, 3)]:
        # Different refs can name one effect; keep one lane per resolved target.
        key = param.split(".")[-1] + str(param.count("."))
        if key in seen:
            continue
        seen.add(key)
        times = sorted(rng.choice([0, 1, 2, 4, length / 2, length]) for _ in range(rng.randint(1, 4)))
        times = [t for i, t in enumerate(times) if i < 2 or times[i - 2] != t]
        points = []
        for t in times:
            p = {"at": beat(rng, t), "value": number(rng, low, high, strings=False)}
            if rng.random() < 0.3:
                p["curve"] = rng.choice(["linear", "hold"])
            points.append(p)
        result.append({"param": param, "points": points})
    return result


def random_project(rng):
    length = rng.choice([32, 64, 128])
    session = {}
    if rng.random() < 0.8:
        session["tempo"] = rng.choice([120, 174.5, "140", rng.uniform(60, 200), 90.0])
    session["length_beats"] = rng.choice([length, float(length), str(length)])
    for field, value in [
        ("title", rng.choice(TRICKY_TEXT)),
        ("sample_rate", rng.choice([44100, 48000, 48000.0])),
        ("master_gain_db", number(rng, -96, 24)),
        ("end_fade_ms", number(rng, 0, 10000)),
        ("time_signature", "4/4"),
    ]:
        if rng.random() < 0.4:
            session[field] = value
    pitched = rng.random() < 0.5
    names = rng.sample(KEY_NAMES, rng.randint(1, 4))
    samples = {}
    for name in names:
        s = {"path": f"samples/{rng.randrange(1000)}_x.wav"}
        if rng.random() < 0.3:
            s["sha256"] = "".join(rng.choice("0123456789abcdef") for _ in range(64))
        if rng.random() < 0.3:
            s["source"] = rng.choice(TRICKY_TEXT)
        if pitched or rng.random() < 0.3:
            s["root_note"] = rng.choice(NOTES)
        samples[name] = s
    return_ids = [f"r{i}" for i in range(rng.randint(0, 2))]
    track_ids = rng.sample(["drums", "bass", "Keys-2", "a_b", "t1", "t2", "lead"], rng.randint(1, 4))
    tracks = []
    for i, tid in enumerate(track_ids):
        pads = {}
        for pad in ["k", "s", "g"]:
            pd = {"sample": rng.choice(names)}
            if pad == "g":
                pd["mode"] = "gate"
            elif rng.random() < 0.2:
                pd["mode"] = "one_shot"
            for field, low, high in [
                ("gain_db", -96, 24),
                ("pan", -1, 1),
                ("transpose", -36, 36),
                ("attack_ms", 0, 10000),
                ("release_ms", 0, 10000),
            ]:
                if rng.random() < 0.25:
                    pd[field] = number(rng, low, high)
            if rng.random() < 0.2:
                pd["start_seconds"] = number(rng, 0, 1)
                pd["end_seconds"] = rng.choice([2, 2.5, "3"])
            if rng.random() < 0.2:
                pd["choke_group"] = rng.choice(["hats", "bass", "é"])
            for flag in ["reverse", "mono"]:
                if rng.random() < 0.15:
                    pd[flag] = rng.choice([True, False, "yes", "off", 1, 0])
            if rng.random() < 0.15:
                pd["source_bpm"] = number(rng, 20, 400)
            pads[pad] = pd
        # A Drum Rack: the kick and snare on their General MIDI keys, and a
        # gated pad played at the notes' pitches over two octaves.
        keymap = [{"notes": 36, "pad": "k"}, {"notes": "D2", "pad": "s"}, {"notes": [48, 72], "pad": "g", "pitched": True}]
        sampler = {"pads": pads, "map": keymap}
        if rng.random() < 0.5:
            sampler["drum_rack"] = rng.choice([True, False])
        t = {"id": tid, "type": "midi", "instrument": {"sampler": sampler}}
        for field, low, high in [("gain_db", -96, 24), ("pan", -1, 1)]:
            if rng.random() < 0.4:
                t[field] = number(rng, low, high)
        for flag in ["mute", "solo"]:
            if rng.random() < 0.2:
                t[flag] = rng.choice([True, False, "on", 0])
        clips = []
        for _ in range(rng.randint(0, 3)):
            clen = rng.choice([4, 8, 2])
            start = rng.randrange(0, int(length - clen) + 1)
            c = {"length_beats": rng.choice([clen, float(clen), str(clen)])}
            if start or rng.random() < 0.3:
                c["at"] = beat(rng, start)
            if rng.random() < 0.2:
                c["loop_beats"] = beat(rng, rng.choice([1, 2, 0.5]))
            notes = []
            for _ in range(rng.randint(0, 4)):
                pitch = rng.choice([36, "C2", 38, "D2", 60, "E4", 49])
                n = {"pitch": pitch, "at": beat(rng, rng.randrange(clen * 4) / 4), "duration": beat(rng, rng.choice([0.25, 0.5, 1, 1 / 3]))}
                if rng.random() < 0.4:
                    n["velocity"] = rng.choice([1, 64, 127, "100", 90.0])
                notes.append(n)
            if notes or rng.random() < 0.3:
                c["notes"] = notes
            if rng.random() < 0.15:
                c["active"] = rng.choice([True, False])
            clips.append(c)
        if clips or rng.random() < 0.2:
            t["clips"] = clips
        others = track_ids[:i]
        effects = [
            effect(rng, rng.choice(["filter", "eq", "compressor", "limiter", "delay", "reverb"]), j, others)
            for j in range(rng.randint(0, 3))
        ]
        if effects:
            t["effects"] = effects
        sends = [{"to": r} for r in return_ids if rng.random() < 0.6]
        for s in sends:
            if rng.random() < 0.4:
                s["gain_db"] = number(rng, -96, 12)
            if rng.random() < 0.3:
                s["pre_fader"] = rng.choice([True, False])
        if sends:
            t["sends"] = sends
        auto = lanes(rng, t, length, "track")
        if auto:
            t["automation"] = auto
        tracks.append(t)
    project = {"session": session, "samples": samples, "tracks": tracks}
    if rng.random() < 0.5:
        project = {"schema_version": 1, **project}
    returns = []
    for rid in return_ids:
        r = {"id": rid}
        effects = [effect(rng, rng.choice(["delay", "reverb", "eq", "compressor"]), j, track_ids) for j in range(rng.randint(0, 2))]
        if effects:
            r["effects"] = effects
        if rng.random() < 0.3:
            r["gain_db"] = number(rng, -96, 24)
        if rng.random() < 0.2:
            r["mute"] = True
        auto = lanes(rng, r, length, "return")
        if auto:
            r["automation"] = auto
        returns.append(r)
    if returns:
        project["returns"] = returns
    sections = []
    at = 0
    for n in range(rng.randint(0, 3)):
        size = rng.choice([4, 8, 16])
        if at + size > length:
            break
        sections.append({"id": rng.choice([f"s{n}", "drop", "intro"]) + str(n), "at": beat(rng, at), "length_beats": beat(rng, size)})
        at += size
    if sections:
        project["sections"] = sections
    if rng.random() < 0.5:
        master = {}
        effects = [effect(rng, rng.choice(["limiter", "eq", "filter", "reverb"]), j, [], True) for j in range(rng.randint(0, 2))]
        if effects:
            master["effects"] = effects
        auto = lanes(rng, master, length, "master")
        if auto:
            master["automation"] = auto
        project["master"] = master
    return project


BAD_VALUES = [None, "abc", -1e9, 1e9, [], {}, True, "1/0", "-1", 10**21, "1e400", [1], {"a": 1}, 0, "", 2.5, "x" * 3]


def paths(data, prefix=()):
    """Every location in a nested document."""
    yield prefix
    if isinstance(data, dict):
        for k, v in data.items():
            yield from paths(v, prefix + (k,))
    elif isinstance(data, list):
        for i, v in enumerate(data):
            yield from paths(v, prefix + (i,))


def mutate(rng, data):
    """The document with one location broken: replaced, deleted or given an extra key."""
    data = copy.deepcopy(data)
    locations = [p for p in paths(data) if p]
    where = rng.choice(locations)
    parent = data
    for part in where[:-1]:
        parent = parent[part]
    last = where[-1]
    action = rng.random()
    if action < 0.6:
        parent[last] = copy.deepcopy(rng.choice(BAD_VALUES))
    elif action < 0.8 and isinstance(parent, dict):
        del parent[last]
    elif isinstance(parent, dict):
        parent[rng.choice(["bogus", "extra", 1])] = 1
    else:
        parent.append(copy.deepcopy(rng.choice(BAD_VALUES)))
    return data


def dump(rng, data):
    """YAML in one of several styles, so the loader sees more than one layout."""
    style = rng.random()
    if style < 0.15:
        text = json.dumps(data, ensure_ascii=rng.random() < 0.5)
        # libyaml, and so the Rust loader, rejects escaped surrogate pairs that
        # PyYAML keeps as lone surrogates; see engine/README.md.
        if re.search(r"\\ud[89ab]", text):
            text = json.dumps(data, ensure_ascii=False)
        return text
    if style < 0.3:
        return yaml.safe_dump(data, default_flow_style=True, allow_unicode=True, width=rng.choice([40, 80, 1000]))
    if style < 0.45:
        return yaml.safe_dump(data, default_flow_style=None, sort_keys=False, explicit_start=True)
    return yaml.safe_dump(data, sort_keys=False, allow_unicode=rng.random() < 0.7)


EDGE_CASES = [
    # Loader: YAML 1.1 scalars, merge keys, anchors and tags.
    "session: {tempo: 0o140}",
    "session: {tempo: 0140, length_beats: 1:30}",
    "session: {tempo: 1_2_0, length_beats: 0x40}",
    "session: {tempo: '120', length_beats: '64'}",
    "session: {tempo: 1.2e+2, length_beats: 1e2}",
    "session: {tempo: 1.2e2}",
    "session: {tempo: .inf}",
    "session: {tempo: -.inf}",
    "session: {tempo: .nan}",
    "session: {title: 2001-12-14}",
    "session: {title: 2001-12-14 21:59:43.10 -5}",
    "session: {title: !!str 123, tempo: !!float '130'}",
    "session: {title: !!binary aGVsbG8=}",
    "session: {tempo: !!int '0b1111000'}",
    "session: {title: !!python/str x}",
    "session: !!set {a, b}",
    "session: {}\ntracks: !!omap [{a: 1}]",
    "defaults: &d {tempo: 120}\nsession:\n  <<: *d\n  length_beats: 8\n",
    "session:\n  <<: [{tempo: 100}, {tempo: 120, title: merged}]\n",
    "session: {tempo: 120, tempo: 130}",
    "session: {1: 2}",
    "session: {tempo: 120}\n---\nsession: {}\n",
    "",
    "# only a comment\n",
    "[1, 2]",
    "just text",
    "session: {title: ~}",
    "session: {title: null, tempo: Null}",
    "session: {title: =}",
    "session: {title: <<}",
    "session: {title: [unclosed}",
    "session:\n\ttempo: 120\n",
    "? [complex]\n: key\n",
    "session: &a {title: x}\nother: *a\n",
    "session: {title: *undefined}",
    "session: {title: 'single ''quoted'''}",
    'session: {title: "double \\u00e9 \\t tab"}',
    "session: {title: |\n    literal\n    block\n}",
    "session:\n  title: >\n    folded\n    text\n",
    # Coercions the model applies, as pydantic did.
    "session: {tempo: ' 120 ', sample_rate: 44100.0, length_beats: true}",
    "session: {tempo: '1_20'}",
    "session: {tempo: '1__20'}",
    "session: {tempo: yes}",
    "session: {sample_rate: '44100'}",
    "session: {time_signature: 3/4}",
    "session: {length_beats: '0'}",
    "session: {length_beats: '-4'}",
    "session: {length_beats: 1/0}",
    "session: {length_beats: ' 16 '}",
    "session: {length_beats: '1e1'}",
    "session: {length_beats: 1_6}",
    "schema_version: 2\nsession: {}",
    "schema_version: 1.0\nsession: {}",
    "schema_version: true\nsession: {}",
    "session: {}\nsamples: {s: {path: a.wav, sha256: ABC}}",
    "session: {}\nsamples: {s: {path: a.wav, root_note: H2}}",
    "session: {}\nsamples: {s: {path: a.wav, root_note: C10}}",
    "session: {}\nsamples: {s: {path: 5}}",
    "session: {}\nsamples: {1: {path: a.wav}}",
    # A song of patterns is refused: a file of one is converted on read (D191).
    "session: {}\npatterns: {p: {length_beats: 4, steps: {k: 'x...'}}}",
    # Note clips.
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: 128, duration: 1}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: H2, duration: 1}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: 36, duration: 0}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 0}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: abc}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: 36, at: 4, duration: 1}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: 36, at: -1, duration: 1}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: 36, duration: 1, velocity: 100.5}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: 36, duration: 1, velocity: '1e2'}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, notes: [{pitch: 36, at: [1], duration: {}}]}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4, loop_beats: 0}]}]",
    "session: {}\ntracks: [{id: a, type: midi, clips: [{id: c, length_beats: 4}, {id: c, at: 4, length_beats: 4}]}]",
    "session: {}\ntracks: [{id: 1a, pads: {}}]",
    "session: {}\ntracks: [{id: a, pads: {}}, {id: a, pads: {}}]",
    "session: {}\ntracks: [{id: a}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: fuzz}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{mode: lowpass}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [5]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: 5}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: filter, mode: lowpass, cutoff_hz: 5}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: filter, mode: bandpass, cutoff_hz: 50, slope_db_per_octave: '12'}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: eq, bands: []}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: delay, time_beats: 20}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: delay, time_beats: 1/100000}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: delay, time_beats: 1, lowcut_hz: 500, highcut_hz: 400}]}]",
    "session: {tempo: 20}\ntracks: [{id: a, pads: {}, effects: [{type: delay, time_beats: 16}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: reverb, seed: 4294967296}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: reverb, seed: 1.5}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: limiter, id: x}, {type: limiter, id: x}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: compressor, threshold_db: -10, sidechain: a}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: compressor, threshold_db: -10, sidechain: b}]}, {id: b, pads: {}, effects: [{type: compressor, threshold_db: -10, sidechain: a}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: compressor, threshold_db: -10, sidechain: r}]}]\nreturns: [{id: r}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: compressor, threshold_db: -10, sidechain: nope}]}]",
    "session: {}\nmaster: {effects: [{type: compressor, threshold_db: -10, sidechain: a}]}\ntracks: [{id: a, pads: {}}]",
    "session: {}\ntracks: [{id: a, pads: {}, sends: [{to: r}, {to: r}]}]\nreturns: [{id: r}]",
    "session: {}\ntracks: [{id: a, pads: {}, sends: [{to: q}]}]\nreturns: [{id: r}]",
    "session: {}\ntracks: [{id: a, pads: {}}]\nreturns: [{id: a}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: gain_db, points: [{at: 0, value: 99}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: gain_db, points: [{at: 20, value: 0}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: gain_db, points: [{at: 2, value: 0}, {at: 1, value: 0}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: gain_db, points: [{at: 1, value: 0}, {at: 1, value: 0}, {at: 1, value: 0}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: gain_db, points: []}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: gain_db, points: [{at: 0, value: 0}]}, {param: gain_db, points: [{at: 0, value: 0}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: width, points: [{at: 0, value: 0}]}]}]",
    "session: {}\nreturns: [{id: r, automation: [{param: sends.x.gain_db, points: [{at: 0, value: 0}]}]}]",
    "session: {}\nmaster: {automation: [{param: pan, points: [{at: 0, value: 0}]}]}",
    "session: {}\ntracks: [{id: a, pads: {}, sends: [{to: r}], automation: [{param: sends.r.pan, points: [{at: 0, value: 0}]}]}]\nreturns: [{id: r}]",
    "session: {}\ntracks: [{id: a, pads: {}, automation: [{param: sends.r.gain_db, points: [{at: 0, value: 0}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: limiter}], automation: [{param: effects.0.ceiling_db, points: [{at: 0, value: -1}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: eq, bands: [{shape: bell, freq_hz: 100, gain_db: 0}]}], automation: [{param: effects.0.gain_db, points: [{at: 0, value: 0}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: eq, bands: [{shape: bell, freq_hz: 100, gain_db: 0}]}], automation: [{param: effects.0.bands.3.gain_db, points: [{at: 0, value: 0}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: filter, mode: lowpass, cutoff_hz: 100}], automation: [{param: effects.9.cutoff_hz, points: [{at: 0, value: 100}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: filter, mode: lowpass, cutoff_hz: 100}], automation: [{param: effects.0.cutoff_hz.x, points: [{at: 0, value: 100}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: filter, id: f, mode: lowpass, cutoff_hz: 100}], automation: [{param: effects.f.cutoff_hz, points: [{at: 0, value: 100}]}, {param: effects.0.cutoff_hz, points: [{at: 0, value: 100}]}]}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: filter, mode: lowpass, cutoff_hz: 100}], automation: [{param: effects.0.cutoff_hz, points: [{at: 0, value: 5}]}]}]",
    "session: {}\nsections: [{id: a, at: 0, length_beats: 4}, {id: a, at: 4, length_beats: 4}]",
    "session: {}\nsections: [{id: a, at: 14, length_beats: 4}]",
    "session: {}\nsections: [{id: a, at: x, length_beats: 0}]",
    "session: {}\nsections: [{id: a, at: 0, length_beats: 0}]",
    # Pads on a track and pattern clips are refused as patterns are.
    "session: {}\ntracks: [{id: a, pads: {k: {sample: s}}}]",
    "session: {}\ntracks: [{id: a, pads: {}, clips: [{pattern: p}]}]",
    # Drum Racks.
    "session: {}\ntracks: [{id: a, type: midi, instrument: {sampler: {drum_rack: true, pads: {k: {sample: s}}, map: [{notes: 36, pad: k}]}}}]",
    "session: {length_beats: 4}\nsamples: {s: {path: a.wav}}\ntracks: [{id: a, type: midi, instrument: {sampler: {drum_rack: true, pads: {k: {sample: s}}, map: [{notes: 36, pad: k}]}}, clips: [{id: c, at: 1, length_beats: 4, notes: [{pitch: 36, duration: 1/4}]}]}]",
    "session: {}\nsamples: {s: {path: a.wav}}\ntracks: [{id: a, type: midi, instrument: {sampler: {drum_rack: true, pads: {k: {sample: s}}, map: [{notes: 36, pad: q}]}}}]",
    "session: {}\nsamples: {s: {path: a.wav}}\ntracks: [{id: a, type: midi, instrument: {sampler: {drum_rack: true, pads: {k: {sample: s}}, map: [{notes: 36, pad: k}]}}, clips: [{id: c, length_beats: 4, notes: [{pitch: C3, duration: 1}]}]}]",
    "session: {}\nsamples: {s: {path: a.wav}}\ntracks: [{id: a, type: midi, instrument: {sampler: {drum_rack: true, pads: {k: {sample: s, mode: gate}}, map: [{notes: [36, 60], pad: k, pitched: true}]}}, clips: [{id: c, length_beats: 4, notes: [{pitch: C3, duration: 1}]}]}]",
    "session: {}\nsamples: {s: {path: a.wav}}\ntracks: [{id: a, type: midi, instrument: {sampler: {drum_rack: true, pads: {k: {sample: s, start_seconds: 2, end_seconds: 1}}, map: [{notes: 36, pad: k}]}}}]",
    "session: {}\nsamples: {s: {path: a.wav}}\ntracks: [{id: a, type: midi, instrument: {sampler: {drum_rack: true, pads: {k: {sample: s, mode: loop}}, map: [{notes: 36, pad: k}]}}}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: " + json.dumps([{"type": "limiter"}] * 33) + "}]",
    "session: {}\ntracks: [{id: a, pads: {}, effects: [{type: eq, bands: " + json.dumps([{"shape": "x"}] * 17) + "}]}]",
    "session: {}\ntracks: {a: 1}",
    "session: 5",
    "session: {bogus: 1, tempo: x}\nextra: 2",
    "session: {}\ntracks: [{id: a, pads: {}, gain_db: 1e400}]",
    "session: {}\ntracks: [{id: a, pads: {}, mute: 2}]",
    "session: {}\ntracks: [{id: a, pads: {}, mute: 2.0, solo: 0.5}]",
    "session: {}\ntracks: [{id: a, pads: {}, mute: ' yes'}]",
]


def corpus(size=300, seed=20260930):
    """(name, yaml text) pairs: valid random projects, broken copies, edge cases."""
    rng = random.Random(seed)
    out = []
    for i in range(size):
        data = random_project(rng)
        out.append((f"random-{i}", dump(rng, data)))
        out.append((f"broken-{i}", dump(rng, mutate(rng, data))))
    out += [(f"edge-{i}", text) for i, text in enumerate(EDGE_CASES)]
    return out
