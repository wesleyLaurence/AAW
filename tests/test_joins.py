"""Checks of a rendered edit: the joins between parts of a song, and the length.

The song is generated at 120 BPM with its first downbeat 0.137 s in. An edit keeps
its first sixteen beats, skips eight, and keeps the rest, cutting 5 ms before the
beat with a 12 ms fade out over a 4 ms fade in, as two pads of one sample.
"""

import json

import numpy as np
import pytest
import soundfile as sf

from agent_daw.model import save
from helpers import beat_song, cli, daw, render

BEAT = 0.5
FIRST = 0.137
LEAD = 0.005  # how long before the beat a part starts


def edit(folder, shift=0.0, enters=24, out_ms=12, in_ms=4, cut=0.0, mapped=True, bars=12):
    """An edit of the generated song, rendered. `shift` moves the second part's
    place in the song by seconds, `enters` is the beat of the song it starts on,
    and `cut` moves the join later than the beat."""
    folder.mkdir()
    beat_song(folder / "song.wav", bars=bars)
    if mapped:
        code, made = cli("samples", "--db", folder / "index.sqlite", "beats", folder / "song.wav")
        assert code == 0 and made["tempo"]["bpm"] == pytest.approx(120, abs=0.002)
    lead = LEAD - cut
    rest = bars * 4 - enters
    save(
        {
            "session": {"tempo": 120, "sample_rate": 44100, "length_beats": 4 + 16 + rest + 2, "master_gain_db": -3},
            "samples": {"song": {"path": "song.wav"}, "hat": {"path": "hat.wav"}},
            "patterns": {
                "edit": {
                    "length_beats": 4 + 16 + rest + 2,
                    "events": [
                        {"at": 4 - lead / BEAT, "pad": "first"},
                        {"at": 20 - lead / BEAT, "pad": "second"},
                    ],
                },
                "hats": {"length_beats": 4, "grid": "1/4", "steps": {"h": "x.x.x.x.x.x.x.x."}},
            },
            "tracks": [
                {
                    "id": "song",
                    "pads": {
                        "first": {
                            "sample": "song",
                            "start_seconds": FIRST - lead,
                            "end_seconds": FIRST - lead + 16 * BEAT + out_ms / 1000,
                            "attack_ms": 2,
                            "release_ms": out_ms,
                        },
                        "second": {
                            "sample": "song",
                            "start_seconds": FIRST - lead + enters * BEAT + shift,
                            "attack_ms": in_ms,
                            "release_ms": 20,
                        },
                    },
                    "clips": [{"pattern": "edit"}],
                },
                # A drum hit after another is not a join.
                {"id": "hats", "gain_db": -12, "pads": {"h": {"sample": "hat"}}, "clips": [{"pattern": "hats", "repeats": 4}]},
            ],
        },
        folder / "song.yaml",
    )
    noise = np.random.default_rng(1).standard_normal(4410) * np.exp(-np.arange(4410) / 400)
    sf.write(folder / "hat.wav", 0.3 * noise / np.abs(noise).max(), 44100)
    return render(folder / "song.yaml")["render_id"][:12]


def test_a_join_on_the_beat_passes_every_check(tmp_path):
    folder = tmp_path / "edit"
    name = edit(folder)
    code, report = cli("joins", folder / "renders" / "latest.json", "--limit", 30)
    assert code == 0, report
    assert report["tempo"] == 120 and report["flagged"] == [] and len(report["joins"]) == 1
    join = report["joins"][0]
    assert (join["track"], join["sample"], join["pads"]) == ("song", "song", ["first", "second"])
    assert join["at_seconds"] == pytest.approx(20 * BEAT - LEAD, abs=1e-4)
    assert join["fade"] == {"seconds": 0.012, "out_ms": 12, "in_ms": 4, "overlap_ms": pytest.approx(12, abs=0.05)}
    # By the song's beat map: beat 16 of the song leaves and beat 25 enters, eight
    # beats skipped, a session beat apart to within a tenth of a millisecond.
    grid = join["grid"]
    assert abs(grid["interval_error_ms"]) < 0.1 and abs(grid["before_ms"]) < 0.1
    assert (grid["session_beats_between"], grid["source_beats_skipped"]) == (1, 8)
    assert (grid["leaves_at"]["bar"], grid["leaves_at"]["beat"]) == (4, 4)
    assert (grid["enters_at"]["bar"], grid["enters_at"]["beat"]) == (7, 1)
    assert grid["enters_after_fade_in_ms"] == pytest.approx(1, abs=0.1)
    # By the audio: the transients either side sit the same distance from the beat.
    heard = join["measured"]
    assert abs(heard["interval_error_ms"]) < 0.5 and heard["before_transients"] == 8
    assert join["step"]["ratio"] < 2 and abs(join["level"]["change_db"]) < 3
    # An excerpt of the mix around the join, for a person to hear.
    excerpt = folder / "renders" / name / "joins" / "join-01.wav"
    assert join["excerpt"] == {"audio": str(excerpt.resolve()), "from_seconds": pytest.approx(8, abs=0.01)}
    assert sf.info(excerpt).duration == pytest.approx(4)
    # The length is the whole file's, against the limit.
    assert report["length"]["seconds"] == 23 and report["length"]["within_limit"]
    assert 20 < report["length"]["sound_ends_seconds"] <= 23
    code, tight = cli("joins", folder / "renders" / name, "--limit", 20, "--no-excerpts")
    assert tight["length"]["within_limit"] is False and tight["length"]["over_by_seconds"] == 3
    assert "excerpt" not in tight["joins"][0]
    # The Rust binary passes the command on.
    assert daw("joins", folder / "renders" / name, "--no-excerpts")["joins"] == tight["joins"]


def test_a_join_off_the_beat_is_flagged(tmp_path):
    edit(tmp_path / "late", shift=-0.007)
    code, report = cli("joins", tmp_path / "late" / "renders" / "latest.json", "--no-excerpts")
    join = report["joins"][0]
    assert report["flagged"] == [1]
    assert join["grid"]["interval_error_ms"] == pytest.approx(7, abs=0.1)
    assert join["measured"]["interval_error_ms"] == pytest.approx(7, abs=0.5)
    assert any("slips +7.0" in flag for flag in join["flags"])
    assert any("Transients sit" in flag for flag in join["flags"])


def test_skipped_beats_that_are_not_whole_bars_are_flagged(tmp_path):
    edit(tmp_path / "odd", enters=22)
    code, report = cli("joins", tmp_path / "odd" / "renders" / "latest.json", "--no-excerpts")
    join = report["joins"][0]
    assert join["grid"]["source_beats_skipped"] == 6 and abs(join["grid"]["interval_error_ms"]) < 0.1
    assert join["flags"] == ["6 beats of the song are skipped, not whole bars"]


def test_a_splice_through_a_note_shows_as_a_step(tmp_path):
    # Cut a fifth of a second after the beat, where the bass sounds, with no fades.
    edit(tmp_path / "click", out_ms=0, in_ms=0, cut=0.2)
    code, report = cli("joins", tmp_path / "click" / "renders" / "latest.json", "--no-excerpts")
    join = report["joins"][0]
    assert join["step"]["ratio"] > 2 and any("possible click" in flag for flag in join["flags"])
    assert abs(join["grid"]["interval_error_ms"]) < 0.1


def test_without_a_beat_map_the_audio_is_still_measured(tmp_path):
    edit(tmp_path / "bare", mapped=False)
    code, report = cli("joins", tmp_path / "bare" / "renders" / "latest.json", "--no-excerpts")
    join = report["joins"][0]
    assert code == 0 and join["grid"] is None
    assert join["flags"] == ["No beat map of song: run daw samples beats on it to check the beat across the join"]
    assert abs(join["measured"]["interval_error_ms"]) < 0.5


def test_only_a_whole_render_is_checked(tmp_path):
    folder = tmp_path / "edit"
    edit(folder)
    preview = render(folder / "song.yaml", tmp_path / "preview", track_id="song")
    assert preview["target"]["track"] == "song"
    code, result = cli("joins", tmp_path / "preview")
    assert code == 1 and "whole song" in result["error"]
    code, result = cli("joins", folder / "song.wav")
    assert code == 1 and "need a render" in result["error"]
    report = json.loads(next((folder / "renders").glob("*/report.json")).read_text())
    assert report["target"]["track"] is None
