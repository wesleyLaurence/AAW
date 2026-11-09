"""Key, chords and swing, against generated sounds whose harmony and groove are known."""

import json

import numpy as np
import pytest
import soundfile as sf

from agent_daw import harmony, swing
from agent_daw.analysis import analyze
from agent_daw.library import scan, search
from helpers import beat_song, chord_song, cli, daw

RATE = 44100
MAJOR = {"I": (0, ""), "ii": (2, "m"), "IV": (5, ""), "V": (7, ""), "vi": (9, "m")}
MINOR = {"i": (0, "m"), "iv": (5, "m"), "V": (7, ""), "VI": (8, ""), "III": (3, "")}


def triad(root, quality, base=48):
    r = base + root % 12
    return [r, r + (3 if quality == "m" else 4), r + 7]


def progression(tonic, numerals):
    table = MAJOR if numerals[0] == "I" else MINOR
    return [triad(tonic + table[n][0], table[n][1]) for n in numerals]


@pytest.mark.parametrize(
    "tonic, numerals, name",
    [
        (0, ["I", "IV", "V", "I"], "C major"),
        (3, ["I", "vi", "IV", "V"], "Eb major"),
        (6, ["I", "V", "vi", "IV"], "F# major"),
        (9, ["i", "iv", "V", "i"], "A minor"),
        (1, ["i", "VI", "III", "V"], "C# minor"),
        (10, ["i", "iv", "V", "i"], "Bb minor"),
    ],
)
def test_a_progression_with_drums_is_in_its_key(tonic, numerals, name):
    x = chord_song(None, progression(tonic, numerals) * 2, seed=tonic)
    key = harmony.key_of(x, RATE)
    assert key["tonal"] and key["name"] == name
    assert key["confidence"] > 0.6 and key["scale_confidence"] > 0.7
    assert key["candidates"][0]["name"] == name and len(key["candidates"]) == 4
    assert sum(key["profile"]) == pytest.approx(1, abs=0.01)


def test_drums_noise_and_one_note_have_no_key():
    rng = np.random.default_rng(3)
    t = np.arange(RATE * 3) / RATE
    for x in (
        chord_song(None, [], drums=True),
        0.1 * rng.standard_normal(RATE * 6),
        0.3 * np.sin(2 * np.pi * 110 * t) * np.exp(-t),
        np.zeros(RATE),
    ):
        key = harmony.key_of(x, RATE)
        assert not key["tonal"] and key["name"] is None
        assert "candidates" not in key and len(key["profile"]) == 12


def test_keys_are_read_and_spelled_as_a_signature_writes_them():
    assert harmony.parse_key("A minor") == (9, "minor")
    assert harmony.parse_key("Am") == harmony.parse_key("amin") == (9, "minor")
    assert harmony.parse_key("F#") == harmony.parse_key("Gb major") == (6, "major")
    assert harmony.parse_key("Bbm") == harmony.parse_key("A#min") == (10, "minor")
    assert harmony.parse_key("CM") == (0, "major")
    for wrong in ("H", "C## major", "minor", "Am7"):
        with pytest.raises(ValueError, match="A key is a note"):
            harmony.parse_key(wrong)
    assert [harmony.key_name(t, "major") for t in (1, 3, 6, 8, 10)] == [
        "Db major", "Eb major", "F# major", "Ab major", "Bb major"
    ]
    assert [harmony.key_name(t, "minor") for t in (1, 3, 8, 10)] == [
        "C# minor", "Eb minor", "G# minor", "Bb minor"
    ]


def test_a_transpose_moves_a_sample_onto_the_key_or_its_relative():
    assert harmony.transpose_to("C major", "A minor") == {
        "transpose": 0, "then_key": "C major", "relation": "relative key"
    }
    assert harmony.transpose_to("D minor", "A minor") == {
        "transpose": -5, "then_key": "A minor", "relation": "same key"
    }
    assert harmony.transpose_to("E minor", "G major")["transpose"] == 0
    assert harmony.transpose_to("E minor", "C major")["transpose"] == 5
    assert harmony.transpose_to("F# major", "C major")["transpose"] == -6
    assert harmony.transpose_to("Bb major", "C minor") == {
        "transpose": 5, "then_key": "Eb major", "relation": "relative key"
    }


def test_chords_follow_the_beats_and_change_on_the_bar():
    Am, F, C, G = [57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]
    Dm, E7, Cmaj7, Gsus4 = [50, 53, 57], [52, 56, 59, 62], [48, 52, 55, 59], [55, 60, 62]
    chords = [Am, F, C, G, Dm, E7, Cmaj7, Gsus4]
    x = chord_song(None, chords, bpm=100)
    # A C triad over an E in the bass, and two bars of silence.
    x = np.concatenate([x, chord_song(None, [C], bpm=100, drums=False, bass=40), np.zeros(round(4.8 * RATE))])
    times = np.arange(0, 41) * 0.6
    runs = harmony.chords(x, RATE, times, [i % 4 == 0 for i in range(41)], (9, "minor"))
    assert [(r["first_beat"], r["beats"], r["chord"]) for r in runs] == [
        (0, 4, "Am"), (4, 4, "F"), (8, 4, "C"), (12, 4, "G"), (16, 4, "Dm"),
        (20, 4, "E7"), (24, 4, "Cmaj7"), (28, 4, "Gsus4"), (32, 4, "C/E"), (36, 4, "N"),
    ]
    assert all(r["fit"] > 0.7 for r in runs[:-1]) and runs[-1]["fit"] is None
    # A flat key spells its chords with flats.
    flat = harmony.chords(chord_song(None, [[51, 55, 58], [56, 60, 63]], bpm=100), RATE, np.arange(9) * 0.6, [i % 4 == 0 for i in range(9)], (3, "major"))
    assert [r["chord"] for r in flat] == ["Eb", "Ab"]


@pytest.mark.parametrize("bpm", [90, 128, 174])
@pytest.mark.parametrize(
    "grid, amount, feel",
    [(0.5, 0.5, "straight"), (0.5, 0.58, "swung"), (0.5, 2 / 3, "triplet"), (0.25, 0.5, "straight"), (0.25, 0.6, "swung"), (0.25, 0.72, "swung")],
)
def test_swing_is_said_as_a_pattern_says_it(bpm, grid, amount, feel):
    x = chord_song(None, [[48, 52, 55]] * 8, bpm=bpm, swing=amount, grid=grid, seed=bpm)
    found = swing.measure(x, RATE, np.arange(33) * 60 / bpm)
    assert found["grid"] == ("1/8" if grid == 0.5 else "1/16")
    assert found["swing"] == pytest.approx(amount, abs=0.015) and found["feel"] == feel
    assert found["pattern"] == {"grid": grid, "swing": pytest.approx(round(amount, 2), abs=0.02)}
    assert found["delay_beats"] == pytest.approx((2 * amount - 1) * grid, abs=0.01)
    side = found["eighths" if grid == 0.5 else "sixteenths"]
    assert side["onsets"] >= 30 and side["late_ms"] == pytest.approx((amount - 0.5) * 60 / bpm * grid * 2000, abs=4)


def test_no_swing_without_notes_between_the_beats():
    beat = 0.5
    x = np.zeros(RATE * 10)
    t = np.arange(round(0.4 * RATE)) / RATE
    for k in range(19):
        start = round(k * beat * RATE)
        x[start : start + len(t)] += np.sin(2 * np.pi * 60 * t) * np.exp(-t * 30)
    found = swing.measure(x, RATE, np.arange(21) * beat)
    assert found["swing"] is None and found["why"] == "too few onsets between the beats"


def test_a_loop_is_measured_for_its_key_chords_and_swing(tmp_path):
    loop = tmp_path / "loop.wav"
    # D minor, two bars at 120 BPM, swung sixteenths.
    chords = [[50, 53, 57], [55, 58, 62], [57, 61, 64], [50, 53, 57]]
    sf.write(loop, chord_song(None, chords, bpm=120, beats_each=2, swing=0.62, grid=0.25), RATE)
    report = analyze(loop)
    assert report["key"]["name"] == "D minor" and report["key"]["confidence"] > 0.9
    assert [(c["at_beat"], c["chord"]) for c in report["chords"]] == [(0, "Dm"), (2, "Gm"), (4, "A"), (6, "Dm")]
    rhythm = report["rhythm"]
    assert rhythm["tempo"]["first_beat_seconds"] == 0
    assert rhythm["swing"]["grid"] == "1/16" and rhythm["swing"]["swing"] == pytest.approx(0.62, abs=0.015)


def test_search_by_measured_key(tmp_path):
    root, db = tmp_path / "lib", tmp_path / "lib.sqlite"
    root.mkdir()
    sf.write(root / "am_loop.wav", chord_song(None, progression(9, ["i", "iv", "V", "i"]), bpm=120, beats_each=2), RATE)
    sf.write(root / "c_loop.wav", chord_song(None, progression(0, ["I", "IV", "V", "I"]), bpm=120, beats_each=2), RATE)
    sf.write(root / "d_loop.wav", chord_song(None, progression(2, ["I", "IV", "V", "I"]), bpm=120, beats_each=2), RATE)
    sf.write(root / "beat.wav", chord_song(None, [], bpm=120), RATE)
    scan(root, db)
    code, rows = cli("samples", "--db", db, "search", "--measured-key", "A minor", "--measure", "10")
    assert code == 0
    # A minor's own and its relative's, which needs no transpose.
    assert [r["name"] for r in rows] == ["am_loop.wav", "c_loop.wav"]
    assert rows[0]["measured"]["key"] == "A minor" and rows[0]["measured"]["to_key"]["relation"] == "same key"
    assert rows[1]["measured"]["to_key"] == {"transpose": 0, "then_key": "C major", "relation": "relative key"}
    code, rows = cli("samples", "--db", db, "search", "--measured-key", "Am", "--key-within", "2")
    assert [(r["name"], r["measured"]["to_key"]["transpose"]) for r in rows] == [
        ("am_loop.wav", 0), ("c_loop.wav", 0), ("d_loop.wav", -2)
    ]
    # The drums have no key, and a search lists that as null.
    code, rows = cli("samples", "--db", db, "search", "beat")
    assert rows[0]["measured"]["key"] is None
    code, error = cli("samples", "--db", db, "search", "--key-within", "2")
    assert code == 1 and "--measured-key" in error["error"]
    code, error = cli("samples", "--db", db, "search", "--measured-key", "H")
    assert code == 1 and "A key is a note" in error["error"]


def test_a_beat_map_has_the_songs_swing(tmp_path):
    beat_song(tmp_path / "song.wav", 120.0, 24)
    code, report = cli("samples", "beats", tmp_path / "song.wav")
    assert code == 0
    # Its hats are even eighths from bar 9.
    assert report["swing"]["grid"] == "1/8" and report["swing"]["feel"] == "straight"
    assert report["swing"]["swing"] == pytest.approx(0.5, abs=0.015)


def test_a_songs_chords_by_bar_and_beat(tmp_path):
    Am, F, C, G = [57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]
    Dm, E = [50, 53, 57], [52, 56, 59]
    # Two bars and a beat of drums first. A beat of drums alone is like some
    # chord too, and is given one with its fit.
    song = chord_song(None, ([Am, F, C, G] * 3 + [Dm, E, Am, Am]) * 2, bpm=96, swing=0.6)
    sf.write(tmp_path / "song.wav", np.concatenate([chord_song(None, [], bpm=96)[: round(2.5 * RATE)], song]), RATE)
    code, report = cli("samples", "chords", tmp_path / "song.wav")
    assert code == 0
    assert report["key"]["name"] in ("A minor", "C major") and report["key"]["scale_confidence"] > 0.9
    assert "profile" not in report["key"] and report["tempo_bpm"] == pytest.approx(96, abs=0.01)
    first = next(i for i, c in enumerate(report["chords"]) if c["chord"] == "Am" and c["beats"] == 4)
    named = report["chords"][first:]
    assert [c["chord"] for c in named[:8]] == ["Am", "F", "C", "G", "Am", "F", "C", "G"]
    # Each chord is a bar, on the bar line.
    assert all(c["beat"] == 1 and c["beats"] == 4 for c in named[:8])
    assert named[1]["bar"] == named[0]["bar"] + 1
    assert named[1]["seconds"] == pytest.approx(named[0]["until_seconds"])
    code, near = cli("samples", "chords", tmp_path / "song.wav", "--near", "0:20", "--window", "2")
    assert 1 <= len(near["chords"]) <= 3
    assert all(c["until_seconds"] >= 18 and c["seconds"] <= 22 for c in near["chords"])


def song_with(tmp_path, notes_text=None, loop=None, **loop_clip):
    """A song at 120 BPM: a Synth track with notes from text, and an audio track
    playing a loop written as a WAV."""
    song = tmp_path / "song"
    daw("init", song, "--tempo", 120)
    if notes_text:
        daw("track", "add", song, "keys", "--type", "midi")
        daw("synth", "add", song, "keys")
        daw("clip", "add", song, "keys", "--grid", 4, "--text", notes_text)
    if loop is not None:
        sf.write(tmp_path / "loop.wav", loop, RATE)
        assert cli("samples", "--db", tmp_path / "db.sqlite", "import", tmp_path / "loop.wav", "--project", song, "--id", "loop")[0] == 0
        daw("track", "add", song, "loops")
        daw("audio", "add", song, "loops", "loop", *[str(v) for kv in loop_clip.items() for v in (f"--{kv[0].replace('_', '-')}", kv[1])])
    return song


def test_the_key_of_what_a_song_plays(tmp_path):
    song = song_with(tmp_path, "a3,c4,e4 f3,a3,c4 d3,f3,a3 e3,g#3,b3")
    code, found = cli("key", song)
    assert code == 0
    assert found["key"]["name"] == "A minor" and found["key"]["confidence"] > 0.8
    assert found["from"]["notes"] == {"keys": 48.0} and found["from"]["samples"] == []
    # A loop in E minor, moved down two semitones to D minor, is heard in D minor.
    em = chord_song(None, progression(4, ["i", "iv", "V", "i"]), bpm=120, beats_each=2)
    sample = tmp_path / "em.wav"
    sf.write(sample, em, RATE)
    code, fitted = cli("key", song, "--sample", sample, "--db", tmp_path / "db.sqlite")
    (fit,) = fitted["samples"]
    assert fit["sample"] == str(sample) and fit["path"] == str(sample.resolve())
    assert (fit["key"], fit["transpose"], fit["then_key"], fit["relation"]) == ("E minor", 5, "A minor", "same key")
    assert fit["key_confidence"] > 0.9
    # Played at the song's tempo as a loop, repitched by next to nothing.
    assert fit["source_bpm"] == pytest.approx(120, abs=0.5) and abs(fit["repitch_semitones"]) < 0.1
    assert fit["swing"] == {"grid": "1/8", "swing": pytest.approx(0.5, abs=0.02), "feel": "straight"}


def test_a_songs_samples_count_by_their_key_and_their_transpose(tmp_path):
    em = chord_song(None, progression(4, ["i", "iv", "V", "i"]) * 4, bpm=120, beats_each=2)
    song = song_with(tmp_path, loop=em, transpose=-2)
    code, found = cli("key", song)
    assert found["key"]["name"] == "D minor"
    assert found["from"]["samples"] == [
        {"sample": "loop", "tracks": ["loops"], "beats": 32.0, "transpose": -2, "key": "E minor", "heard_in": "D minor"}
    ]
    # The song's own sample by its ID fits by the key its audio has.
    code, fitted = cli("key", song, "--sample", "loop", "--db", tmp_path / "db.sqlite")
    assert fitted["samples"][0]["sample"] == "loop" and fitted["samples"][0]["transpose"] == -2
    # A muted track is not heard, and then the song has no key.
    daw("set", song, "tracks.loops.mute", "true")
    code, found = cli("key", song)
    assert not found["key"]["tonal"] and found["from"]["samples"] == []
    code, fitted = cli("key", song, "--sample", "loop", "--db", tmp_path / "db.sqlite")
    assert fitted["samples"][0]["why"].startswith("the song has no key")


def test_drums_in_a_song_add_nothing_to_its_key(tmp_path):
    song = song_with(tmp_path, "a3,c4,e4 f3,a3,c4 d3,f3,a3 e3,g#3,b3", loop=chord_song(None, [], bpm=120))
    code, found = cli("key", song)
    assert found["key"]["name"] == "A minor"
    assert [s["sample"] for s in found["from"]["without_key"]] == ["loop"]
    code, fitted = cli("key", song, "--sample", "loop", "--db", tmp_path / "db.sqlite")
    assert fitted["samples"][0]["why"].startswith("the sample has no key")
    assert "swing" in fitted["samples"][0]
