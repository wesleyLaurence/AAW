"""A song's timeline in beats and seconds: conversions, end alignment and fitting its length."""

import numpy as np
import pytest
import soundfile as sf

from agent_daw.model import load, save
from helpers import SR, cli, daw


@pytest.fixture
def song(tmp_path):
    """At 96 BPM: a 1.5 s sound on beats 0 and 6, and a quarter-second hit on beat 9."""
    sf.write(tmp_path / "long.wav", 0.3 * np.ones(round(1.5 * SR)), SR)
    sf.write(tmp_path / "hit.wav", 0.3 * np.ones(SR // 4), SR)
    save(
        {
            "session": {"tempo": 96, "length_beats": 32},
            "samples": {"long": {"path": "long.wav"}, "hit": {"path": "hit.wav"}},
            "patterns": {
                "a": {"length_beats": 10, "events": [{"at": 0, "pad": "x"}, {"at": 6, "pad": "x"}]},
                "b": {"length_beats": 10, "events": [{"at": 9, "pad": "h"}]},
            },
            "tracks": [
                {"id": "pads", "pads": {"x": {"sample": "long"}, "half": {"sample": "long", "end_seconds": 0.75}}, "clips": [{"pattern": "a"}]},
                {"id": "hits", "pads": {"h": {"sample": "hit"}}, "clips": [{"pattern": "b"}]},
            ],
        },
        tmp_path / "song.yaml",
    )
    return tmp_path / "song.yaml"


def test_places_are_given_in_beats_seconds_and_bars(song):
    code, report = cli("timeline", song, "--seconds", "0:05", "12.5", "--beats", 16, "1/3")
    assert code == 0, report
    assert report["tempo"] == 96
    assert report["length"] == {"beats": 32, "seconds": 20, "time": "0:20.000", "bar": 9, "beat": 1}
    # The last sound is the hit on beat 9, a quarter of a second long.
    assert report["sound_ends"]["beats"] == pytest.approx(9.4) and report["sound_ends"]["time"] == "0:05.875"
    assert report["tracks"]["pads"]["hits"] == 2
    assert report["tracks"]["pads"]["first_sound"]["beats"] == 0
    assert report["tracks"]["pads"]["last_sound_ends"]["beats"] == pytest.approx(6 + 2.4)
    assert report["seconds"] == [
        {"beats": 8, "seconds": 5, "time": "0:05.000", "bar": 3, "beat": 1},
        {"beats": 20, "seconds": 12.5, "time": "0:12.500", "bar": 6, "beat": 1},
    ]
    assert report["beats"][0] == {"beats": 16, "seconds": 10, "time": "0:10.000", "bar": 5, "beat": 1}
    assert report["beats"][1]["seconds"] == pytest.approx(0.208333)
    # The Rust binary passes the command on.
    assert daw("timeline", song, "--beats", 16)["beats"] == report["beats"][:1]


def test_a_sound_is_placed_to_end_on_a_beat(song):
    code, report = cli("timeline", song, "--end-at", 16, "--pad", "pads.x")
    assert code == 0, report
    aligned = report["end_aligned"]
    assert aligned["pad"] == "pads.x" and aligned["length_seconds"] == 1.5
    assert aligned["length_beats"] == pytest.approx(2.4)
    assert aligned["starts"]["beats"] == pytest.approx(13.6) and aligned["ends"]["beats"] == 16
    # A pad's own end shortens it.
    code, report = cli("timeline", song, "--end-at", "16", "--pad", "pads.half")
    assert report["end_aligned"]["starts"]["beats"] == pytest.approx(14.8)
    for bad, reason in [
        (("--end-at", 1, "--pad", "pads.x"), "cannot end at beat 1"),
        (("--end-at", 8, "--pad", "pads.none"), "Unknown pad"),
        (("--end-at", 8), "go together"),
        (("--seconds", "soon"), "Not a time"),
    ]:
        code, result = cli("timeline", song, *bad)
        assert code == 1 and reason in result["error"]


def test_the_length_is_fitted_to_where_the_sound_ends(song):
    code, report = cli("timeline", song, "--fit")
    assert code == 0, report
    # Up to the next whole beat after the sound ends at beat 9.4.
    assert report["fitted"] == {"length_beats": 10, "tail_beats": 0}
    assert report["length"]["beats"] == 10 and load(song)["session"]["length_beats"] == 10
    code, report = cli("timeline", song, "--fit", "--tail", 2)
    assert report["fitted"]["length_beats"] == 12 and load(song)["session"]["length_beats"] == 12
    # Fitting again changes nothing.
    before = song.read_text()
    code, report = cli("timeline", song, "--fit", "--tail", 2)
    assert code == 0 and song.read_text() == before
    code, result = cli("timeline", song, "--fit", "--tail", -1)
    assert code == 1 and "tail must be" in result["error"]
    # A session holds its clips whole: a longer pattern keeps the song longer.
    daw("set", song, "session.length_beats", 32)
    daw("set", song, "patterns.b.length_beats", 16)
    code, report = cli("timeline", song, "--fit")
    assert report["fitted"]["length_beats"] == 16 and "a clip reaches beat 16" in report["fitted"]["held_by_clips"]
