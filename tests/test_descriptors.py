"""Sound descriptors: what a sample is like in numbers and words, search by them, and samples like a chosen one, on generated sounds whose answers are known."""

import json
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf
from scipy.signal import butter, sosfilt

from agent_daw import descriptors
from agent_daw.analysis import analyze
from agent_daw.library import analyze as analyze_indexed, analyze_all, like, scan, search
from agent_daw.model import save, validate
from agent_daw.perception import listen
from helpers import SR, daw, render


def seconds(length):
    return np.arange(round(SR * length)) / SR


def kick(length, hz=50.0, ring=None, amp=0.8):
    """A sweep that falls to `hz` and dies away over `ring` seconds."""
    t = seconds(length)
    phase = 2 * np.pi * np.cumsum(hz + 3 * hz * np.exp(-t / 0.008)) / SR
    return amp * np.sin(phase) * np.exp(-t / (ring or length / 5))


def hat(length, ring, seed=0, cut=6000):
    """Noise over `cut` Hz that dies away over `ring` seconds."""
    noise = np.random.default_rng(seed).standard_normal(round(SR * length))
    noise = sosfilt(butter(4, cut, "highpass", fs=SR, output="sos"), noise)
    return 0.3 * noise * np.exp(-seconds(length) / ring)


def pad(length=4.0):
    """A chord that swells for most of a second and fades for one."""
    t = seconds(length)
    notes = sum(np.sin(2 * np.pi * hz * k * t) / k for hz in (220, 277.18, 329.63) for k in (1, 2, 3, 4))
    return 0.1 * notes * np.minimum(1, t / 0.8) * np.minimum(1, (length - t) / 1.0)


def tone(hz, length=1.0, amp=0.5):
    return amp * np.sin(2 * np.pi * hz * seconds(length))


def drum_loop(bpm=128, bars=2):
    out = np.zeros(round(bars * 4 * 60 / bpm * SR))
    for eighth in range(bars * 8):
        at = round(eighth * 30 / bpm * SR)
        for hit in ([kick(0.2, 55)] if eighth % 2 == 0 else []) + [hat(0.03, 0.005, eighth)]:
            hit = hit[: len(out) - at]
            out[at : at + len(hit)] += 0.5 * hit
    return out


def write(path, x):
    path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(path, x, SR, subtype="FLOAT")
    return path


SOUNDS = {
    "kick_short": lambda: kick(0.12),
    "kick_long": lambda: kick(1.5),
    "hat_closed": lambda: hat(0.06, 0.008),
    "hat_open": lambda: hat(0.6, 0.12, seed=1),
    "pad": pad,
}


@pytest.fixture(scope="module")
def sounds(tmp_path_factory):
    folder = tmp_path_factory.mktemp("sounds")
    return {name: analyze(write(folder / f"{name}.wav", make()))["sound"] for name, make in SOUNDS.items()}


def test_each_number_puts_the_generated_sounds_in_order(sounds):
    def order(field):
        return sorted(sounds, key=lambda name: sounds[name][field])

    assert order("centroid_hz") == ["kick_long", "kick_short", "pad", "hat_closed", "hat_open"]
    assert sounds["kick_long"]["centroid_hz"] < 100 and sounds["hat_closed"]["centroid_hz"] > 6000
    assert order("low_fraction")[-2:] == ["kick_short", "kick_long"]
    assert sounds["kick_long"]["low_fraction"] > 0.9 and sounds["hat_open"]["low_fraction"] == 0
    assert sounds["pad"]["low_fraction"] < 0.01
    # A drum starts within a few milliseconds and a swell takes hundreds.
    assert all(sounds[name]["attack_ms"] < 3 for name in sounds if name != "pad")
    assert 300 < sounds["pad"]["attack_ms"] < 800
    assert order("decay_ms") == ["hat_closed", "kick_short", "hat_open", "kick_long", "pad"]
    assert sounds["hat_closed"]["decay_ms"] < 31 and sounds["kick_short"]["decay_ms"] < 100
    assert sounds["kick_long"]["decay_ms"] == pytest.approx(690, abs=30)
    # The pad sounds for the rest of its four seconds once it has risen.
    assert sounds["pad"]["decay_ms"] > 3000
    assert order("noisiness") == ["pad", "kick_long", "kick_short", "hat_closed", "hat_open"]
    assert sounds["pad"]["noisiness"] < 0.05 and sounds["kick_long"]["noisiness"] < 0.25
    assert sounds["hat_closed"]["noisiness"] > 0.8 and sounds["hat_open"]["noisiness"] > 0.8
    # A short hit is all attack; a pad has none.
    assert order("punch_db") == ["pad", "kick_long", "hat_open", "kick_short", "hat_closed"]
    assert sounds["hat_closed"]["punch_db"] > 40 and sounds["pad"]["punch_db"] < 0


def test_the_words_are_a_category_s_own(sounds):
    def said(name, category):
        return descriptors.words(sounds[name], category)

    assert said("kick_short", "kick")["decay_ms"] == "short"
    assert said("kick_long", "kick")["decay_ms"] == "long"
    assert said("hat_closed", "hat")["decay_ms"] == "short"
    assert said("hat_open", "hat")["decay_ms"] == "long"
    assert said("hat_open", "hat") == {
        "centroid_hz": "bright",
        "low_fraction": "thin",
        "attack_ms": "sharp",
        "decay_ms": "long",
        "noisiness": "noisy",
    }
    # A kick has no line of its own for brightness, so it is dark as any sample is.
    assert said("kick_long", "kick")["centroid_hz"] == "dark"
    assert said("kick_long", "kick")["low_fraction"] == "boomy"
    # A long kick is not long beside every sample: the word is the category's.
    assert said("kick_long", None)["decay_ms"] == "medium"
    assert said("pad", None) == {
        "centroid_hz": "warm",
        "low_fraction": "thin",
        "attack_ms": "slow",
        "decay_ms": "medium",
        "noisiness": "tonal",
    }
    # A line belongs to the word above it.
    low, high = descriptors.lines("kick")["decay_ms"]
    assert descriptors.words({"decay_ms": low}, "kick") == {"decay_ms": "medium"}
    assert descriptors.words({"decay_ms": high}, "kick") == {"decay_ms": "long"}
    assert descriptors.category(sounds["hat_closed"], False) == "hat"
    assert descriptors.category(sounds["kick_short"], False) == "kick"
    assert descriptors.category(sounds["hat_open"], False) == "hat"
    # A loop is no one sound, and a boom of a second and a half no kick.
    assert descriptors.category(sounds["hat_closed"], True) is None
    assert descriptors.category(sounds["kick_long"], False) is None
    assert descriptors.category(sounds["pad"], False) is None


def test_noise_and_tones_whatever_band_they_fill(tmp_path):
    def read(name, x):
        return analyze(write(tmp_path / f"{name}.wav", x))["sound"]

    noise = 0.2 * np.random.default_rng(3).standard_normal(2 * SR)
    white = read("white", noise)
    # Steady noise starts at once and sounds to its end, wherever its highest sample is.
    assert white["noisiness"] > 0.9 and white["attack_ms"] < 1
    assert white["decay_ms"] == pytest.approx(2000, abs=5)
    high = read("high", sosfilt(butter(4, 6000, "highpass", fs=SR, output="sos"), noise))
    assert high["noisiness"] > 0.85 and high["centroid_hz"] > 3 * white["centroid_hz"] / 4
    for hz in (60, 1000, 8000):
        sine = read(f"sine{hz}", tone(hz))
        assert sine["noisiness"] < 0.01 and sine["centroid_hz"] == pytest.approx(hz, rel=0.01)
        assert sine["low_fraction"] == (1.0 if hz < 120 else 0.0)
    # A sawtooth is a tone too: every harmonic repeats with the first.
    assert read("saw", 0.3 * (2 * ((110 * seconds(1)) % 1) - 1))["noisiness"] < 0.01


def test_loudness_is_of_the_loudest_400_ms(tmp_path):
    def loudness(name, x):
        return analyze(write(tmp_path / f"{name}.wav", x))["sound"]["loudness_lufs"]

    # A 1 kHz tone at half of full scale on one channel reads -9.0.
    assert loudness("tone", tone(1000)) == pytest.approx(-9.0, abs=0.1)
    assert loudness("quiet", tone(1000, amp=0.25)) == pytest.approx(-15.0, abs=0.1)
    # A tenth of a second of it is a quarter of the 400 ms: 6 dB less.
    assert loudness("brief", tone(1000, 0.1)) == pytest.approx(-15.0, abs=0.1)
    # Silence around it changes nothing, and the same on two channels is 3 dB more.
    late = np.concatenate([np.zeros(SR), tone(1000, 0.1), np.zeros(SR)])
    assert loudness("late", late) == pytest.approx(-15.0, abs=0.1)
    assert loudness("stereo", np.column_stack([tone(1000)] * 2)) == pytest.approx(-6.0, abs=0.1)
    # More channels than the engine plays are read as their sum.
    assert loudness("four", np.column_stack([tone(1000)] * 4)) == pytest.approx(-9.0, abs=0.1)


def test_a_loop_and_silence(tmp_path):
    report = analyze(write(tmp_path / "loop_128.wav", drum_loop()))
    loop = report["sound"]
    assert report["rhythm"]["kind"] == "loop" and report["measured_category"] is None
    assert loop["attack_ms"] is None and loop["decay_ms"] is None and loop["punch_db"] is None
    assert loop["centroid_hz"] > 100 and 0 < loop["low_fraction"] < 1 and loop["loudness_lufs"] < 0
    assert list(descriptors.words(loop, None)) == ["centroid_hz", "low_fraction", "noisiness"]
    silent = analyze(write(tmp_path / "silent.wav", np.zeros(SR)))
    assert silent["sound"] == dict.fromkeys(descriptors.FIELDS)
    assert silent["measured_category"] is None and descriptors.words(silent["sound"], "kick") == {}


def test_a_sample_s_punch_is_its_hit_s_in_daw_listen(tmp_path):
    x = np.column_stack([kick(0.25, ring=0.08, amp=0.5)] * 2)
    sf.write(tmp_path / "kick.wav", x, SR, subtype="DOUBLE")
    document = {
        "session": {"tempo": 120, "length_beats": 16, "master_gain_db": 0, "end_fade_ms": 0},
        "samples": {"kick": {"path": "kick.wav"}},
        "patterns": {"bar": {"length_beats": 4, "events": [{"at": 0, "pad": "kick", "velocity": 127}]}},
        "tracks": [
            {
                "id": "kick",
                "pads": {"kick": {"sample": "kick", "attack_ms": 0, "release_ms": 0}},
                "clips": [{"pattern": "bar", "repeats": 4}],
            }
        ],
    }
    save(validate(document), tmp_path / "song.yaml")
    report = listen(Path(render(tmp_path / "song.yaml")["directory"]), images=False)
    sample = analyze(tmp_path / "kick.wav")["sound"]
    assert sample["punch_db"] == pytest.approx(report["tracks"]["kick"]["hits"]["punch_db"], abs=0.2)


@pytest.fixture
def pack(tmp_path):
    """A folder of drums named for what they are, two named for nothing, a pad
    and a loop, and its index, with nothing measured."""
    folder = tmp_path / "pack"
    for name, x in {
        "kick_short": kick(0.12),
        "kick_short_too": kick(0.13, 52),
        "kick_long": kick(1.5),
        "hihat_closed": hat(0.06, 0.008),
        "hihat_open": hat(0.6, 0.12, seed=1),
        "tick": hat(0.05, 0.006, seed=2),
        "thud": kick(0.2, 48),
        "swell": pad(),
        "loop_128": drum_loop(),
    }.items():
        write(folder / f"{name}.wav", x)
    db = tmp_path / "index.sqlite"
    scan(folder, db)
    return folder, db


def names(rows):
    return [row["name"].removesuffix(".wav") for row in rows]


def test_search_sorts_and_filters_by_what_was_measured(pack):
    folder, db = pack
    # Nothing is measured: a sort by a measurement is refused, and says what to do.
    with pytest.raises(ValueError, match="None of the 3 matching samples is measured.*--measure N"):
        search(db, "kick", sort="punch")
    # By name and by length nothing need be.
    assert names(search(db, "kick")) == ["kick_long", "kick_short", "kick_short_too"]
    assert names(search(db, "kick", reverse=True)) == ["kick_short_too", "kick_short", "kick_long"]
    assert names(search(db, sort="duration", limit=2)) == ["swell", "loop_128"]
    assert names(search(db, "kick", ranges={"duration": (None, 0.125)})) == ["kick_short"]
    # --measure measures what the search needs and keeps it.
    notes = []
    found = search(db, "kick", sort="punch", measure=2, notes=notes)
    assert names(found) == ["kick_short", "kick_long"]
    assert "1 of the 3 matching samples are not measured" in notes[0]
    assert analyze_indexed(db, folder / "kick_short.wav")["cached"]
    assert names(search(db, "kick", sort="punch", measure=20)) == ["kick_short", "kick_short_too", "kick_long"]
    summary = analyze_all(db)
    assert summary["analyzed"] == 6 and summary["unchanged"] == 3 and not summary["errors"]

    row = search(db, "kick_long")[0]
    assert row["category"] == "kick" and row["measured"]["words_among"] == "kick"
    assert row["measured"]["words"]["decay_ms"] == "long" and row["measured"]["decay_ms"] > 600
    assert set(descriptors.FIELDS) < set(row["measured"])
    # The most first, and the other way round.
    assert names(search(db, sort="centroid", limit=3)) == ["hihat_open", "hihat_closed", "tick"]
    assert names(search(db, sort="decay", reverse=True, limit=2)) == ["tick", "hihat_closed"]
    assert names(search(db, sort="loudness", limit=1)) == ["kick_long"]
    # A loop has no attack, decay or punch, and is left out of a sort by one.
    assert "loop_128" not in names(search(db, sort="punch"))
    assert "loop_128" in names(search(db, sort="noisiness"))
    # A word is true of a sample among its category, and several words are all true.
    assert names(search(db, "kick", words=["short"])) == ["kick_short", "kick_short_too"]
    assert names(search(db, words=["long", "noisy"])) == ["hihat_open"]
    assert names(search(db, words=["slow"])) == ["swell"]
    assert names(search(db, ranges={"decay_ms": (None, 60)}, sort="decay")) == [
        "kick_short_too",
        "kick_short",
        "hihat_closed",
        "tick",
    ]
    assert names(search(db, ranges={"centroid_hz": (300, 1000), "attack_ms": (100, None)})) == ["swell"]


def test_a_sample_named_for_nothing_is_of_the_category_it_measures_as(pack):
    folder, db = pack
    analyze_all(db)
    assert names(search(db, category="hat")) == ["hihat_closed", "hihat_open", "tick"]
    assert names(search(db, category="kick")) == ["kick_long", "kick_short", "kick_short_too", "thud"]
    tick = search(db, "tick")[0]
    assert tick["category"] == "other" and tick["measured"]["category"] == "hat"
    assert tick["measured"]["words_among"] == "hat"
    # A pad measures as nothing in particular, and its words are every sample's.
    swell = search(db, "swell")[0]
    assert swell["measured"]["category"] is None and swell["measured"]["words_among"] == "any"
    assert names(search(db, category="other")) == ["loop_128", "swell", "thud", "tick"]


def test_like_lists_the_nearest_of_a_category(pack):
    folder, db = pack
    with pytest.raises(ValueError, match="None of the 2 samples to compare with is measured"):
        like(db, folder / "kick_short.wav")
    found = like(db, folder / "kick_short.wav", measure=20)
    assert found["among"] == "kick" and found["compared"] == 2 and found["not_measured"] == 0
    assert names(found["like"]) == ["kick_short_too", "kick_long"]
    # One named for nothing is a kick once it is measured.
    analyze_all(db)
    found = like(db, folder / "kick_short.wav")
    assert found["compared"] == 3 and names(found["like"]) == ["kick_short_too", "thud", "kick_long"]
    distances = [row["distance"] for row in found["like"]]
    assert distances == sorted(distances) and distances[0] < 0.5 < distances[-1]
    assert found["sample"]["measured"]["words"]["decay_ms"] == "short"
    # A file that is not in the library, such as a project's copy, by its name's category.
    copy = write(folder.parent / "song" / "samples" / "0123456789ab_hihat.wav", hat(0.06, 0.008, seed=7))
    assert names(like(db, copy)["like"]) == ["hihat_closed", "tick", "hihat_open"]
    # One named for nothing is held against what it measures as, or every sample.
    assert like(db, folder / "tick.wav")["among"] == "hat"
    swell = like(db, folder / "swell.wav")
    assert swell["among"] == "any" and swell["compared"] == 7
    everything = like(db, folder / "kick_short.wav", category="any", limit=2)
    assert names(everything["like"]) == ["kick_short_too", "thud"] and everything["compared"] == 7
    # A loop is held against loops, and there is no other here; silence against nothing.
    assert like(db, folder / "loop_128.wav", category="any")["like"] == []
    with pytest.raises(ValueError, match="silent"):
        like(db, write(folder.parent / "silent.wav", np.zeros(SR)))
    # The same sound is no distance from itself, and loudness does not count.
    assert descriptors.distance(found["sample"]["measured"], found["sample"]["measured"]) == 0
    quiet = analyze(write(folder.parent / "quiet.wav", 0.1 * kick(0.12)))["sound"]
    assert descriptors.distance(quiet, found["sample"]["measured"]) < 0.05
    assert quiet["loudness_lufs"] == pytest.approx(found["sample"]["measured"]["loudness_lufs"] - 20, abs=0.1)


def test_many_samples_are_measured_together_and_a_run_goes_on(tmp_path):
    folder = tmp_path / "many"
    for n in range(20):
        write(folder / f"kick_{n:02d}.wav", kick(0.1 + n / 100))
    (folder / "broken.wav").write_bytes(b"not audio")
    db = tmp_path / "index.sqlite"
    scan(folder, db)
    analyze_indexed(db, folder / "kick_00.wav")
    # One file goes missing after the library was read.
    (folder / "kick_19.wav").unlink()
    summary = analyze_all(db)
    assert summary["analyzed"] == 18 and summary["unchanged"] == 1
    assert [Path(e["path"]).name for e in summary["errors"]] == ["kick_19.wav"]
    # What was measured is kept, and the next run has only the missing file left.
    again = analyze_all(db)
    assert again["analyzed"] == 0 and again["unchanged"] == 19 and len(again["errors"]) == 1
    decays = [row["measured"]["decay_ms"] for row in search(db, limit=100) if row["measured"]]
    assert decays == sorted(decays) and len(decays) == 19


def run(*args):
    out = subprocess.run([sys.executable, "-m", "agent_daw.cli", *map(str, args)], capture_output=True, text=True)
    return out.returncode, out.stdout, out.stderr


def test_the_commands(pack):
    folder, db = pack
    code, _, err = run("samples", "--db", db, "search", "kick", "--sort", "punch")
    assert code == 1 and "daw samples analyze --all" in json.loads(err)["error"]
    code, out, err = run("samples", "--db", db, "search", "kick", "--sort", "punch", "--measure", 2, "--limit", 1)
    assert code == 0 and names(json.loads(out)) == ["kick_short"]
    assert "1 of the 3 matching samples are not measured" in json.loads(err)["note"]
    code, out, err = run("samples", "--db", db, "analyze", "--all")
    assert code == 0 and json.loads(out)["analyzed"] == 7 and err == ""
    code, out, err = run("samples", "--db", db, "search", "--short", "--bright", "--category", "hat")
    assert (code, err) == (0, "") and names(json.loads(out)) == ["hihat_closed", "tick"]
    code, out, _ = run("samples", "--db", db, "search", "--max-decay", 300, "--min-low", 0.5, "--sort", "decay")
    assert code == 0 and names(json.loads(out)) == ["thud", "kick_short_too", "kick_short"]
    code, out, _ = run("samples", "--db", db, "like", folder / "kick_short.wav", "--limit", 1)
    found = json.loads(out)
    assert code == 0 and names(found["like"]) == ["kick_short_too"] and found["among"] == "kick"
    code, out, _ = run("samples", "--db", db, "like", search(db, "tick")[0]["id"], "--category", "any")
    assert code == 0 and names(json.loads(out)["like"])[:2] == ["hihat_closed", "hihat_open"]
    code, _, err = run("samples", "--db", db, "search", "--sort", "brightness")
    assert code == 2 and "centroid" in err
    # The topic prints the lines the words are read against, from the one file.
    described = daw("describe", "samples")
    assert described["lines"] == descriptors.LINES
    assert set(described["lines"]["any"]) == set(descriptors.WORDS)
    for word in sum(descriptors.WORDS.values(), []):
        assert word in described["semantics"]["words"]
