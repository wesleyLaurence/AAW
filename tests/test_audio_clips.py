"""Audio clips: an edit of a song built with `daw audio`, checked as it is rendered.

The song is generated at 120 BPM with its first downbeat 0.137 s in.
"""

import numpy as np
import pytest
import soundfile as sf

from agent_daw.model import load
from helpers import beat_song, cli, daw, render, stem

FIRST = 0.137


@pytest.fixture
def song(tmp_path):
    """A song of twelve bars in a project, as one audio clip from its first
    downbeat on beat 4, starting 5 ms before it."""
    beat_song(tmp_path / "original.wav", bars=12)
    path = tmp_path / "edit" / "song.yaml"
    daw("init", path.parent, "--tempo", 120, "--bars", 14)
    daw("set", path, "session.sample_rate", 44100)
    daw("set", path, "session.master_gain_db", 0)
    db = tmp_path / "index.sqlite"
    code, asset = cli("samples", "--db", db, "import", tmp_path / "original.wav", "--project", path, "--id", "song")
    assert code == 0, asset
    assert cli("samples", "--db", db, "beats", path.parent / asset["path"])[0] == 0
    daw("track", "add", path, "song")
    made = daw("audio", "add", path, "song", "song", "--at", 4, "--source-start-seconds", FIRST, "--lead-ms", 5, "--fade-in-ms", 2)
    assert made["path"] == "tracks.song.audio.0" and made["label"] == "Add audio clip of song to song"
    return path


def test_a_cut_makes_a_join_that_passes_every_check(song):
    # Beats 16 to 24 of the song, which play on session beats 20 to 28, come out.
    cut = daw("audio", "cut", song, "song", "--from", 20, "--to", 28)
    assert cut["label"] == "Cut beats 20 to 28 from song and close the gap"
    assert load(song)["tracks"][0]["audio"] == [
        {**clip, "gain_db": 0, "fade_curve": "equal_power", "source_bpm": None, "stretch": "repitch"}
        for clip in (
            {"sample": "song", "at": 4, "source_start_seconds": FIRST, "source_end_seconds": FIRST + 8, "lead_ms": 5, "fade_in_ms": 2, "fade_out_ms": 12},
            {"sample": "song", "at": 20, "source_start_seconds": FIRST + 12, "source_end_seconds": None, "lead_ms": 5, "fade_in_ms": 4, "fade_out_ms": 8},
        )
    ]
    # inspect lists them, and the timeline knows where their sound is.
    listed = daw("inspect", song)["tracks"][0]["audio"]
    assert [(c["ref"], c["at"]) for c in listed] == [("tracks.song.audio.0", 4), ("tracks.song.audio.1", 20)]
    code, fitted = cli("timeline", song, "--fit")
    assert code == 0 and fitted["fitted"]["length_beats"] == 47
    assert fitted["tracks"]["song"]["first_sound"]["seconds"] == pytest.approx(2 - 0.005)
    # The song plays at its own level: nothing is turned down on the way.
    report = render(song)
    original = sf.read(song.parent / load(song)["samples"]["song"]["path"])[0]
    assert report["mix"]["peak_dbfs"] == pytest.approx(20 * np.log10(np.abs(original).max()), abs=0.01)
    code, checked = cli("joins", song.parent / "renders" / "latest.json", "--no-excerpts")
    assert code == 0, checked
    join = checked["joins"][0]
    assert len(checked["joins"]) == 1 and join["flags"] == []
    assert join["parts"] == ["audio.0", "audio.1"] and join["at_beats"] == pytest.approx(19.99)
    assert abs(join["grid"]["interval_error_ms"]) < 0.1 and join["grid"]["source_beats_skipped"] == 8
    assert abs(join["measured"]["interval_error_ms"]) < 0.5
    assert join["fade"]["out_ms"] == 12 and join["fade"]["overlap_ms"] == pytest.approx(12, abs=0.05)


def test_a_split_clip_plays_as_the_whole_one_did(song, tmp_path):
    daw("set", song, "session.end_fade_ms", 0)
    render(song, tmp_path / "whole")
    for n, beat in enumerate([11, "53/4", 20.37, 31]):
        made = daw("audio", "split", song, f"tracks.song.audio.{n}", "--at", beat)
        assert made["path"] == f"tracks.song.audio.{n + 1}"
    assert len(load(song)["tracks"][0]["audio"]) == 5
    render(song, tmp_path / "split")
    assert np.array_equal(stem(tmp_path / "whole", "song"), stem(tmp_path / "split", "song"))
    # A split is not an edit: nothing of the song is skipped at any of the four.
    code, checked = cli("joins", tmp_path / "split", "--no-excerpts")
    assert [j["grid"]["source_beats_skipped"] for j in checked["joins"]] == [0, 0, 0, 0]
    assert checked["flagged"] == []


def test_an_edit_of_clips_keeps_its_joins_when_sped_up(song):
    daw("audio", "cut", song, "song", "--from", 20, "--to", 28)
    for n in (0, 1):
        daw("set", song, f"tracks.song.audio.{n}.source_bpm", 120)
        daw("set", song, f"tracks.song.audio.{n}.stretch", "preserve_pitch")
    daw("set", song, "session.tempo", 124.8)
    assert cli("check", song)[1]["warnings"] == []
    render(song)
    code, checked = cli("joins", song.parent / "renders" / "latest.json", "--no-excerpts")
    join = checked["joins"][0]
    assert join["flags"] == [] and join["grid"]["source_beats_skipped"] == 8
    assert abs(join["grid"]["interval_error_ms"]) < 0.1 and abs(join["measured"]["interval_error_ms"]) < 0.5
    # The clips still sit on their beats, which are closer together now.
    assert join["at_beats"] == pytest.approx(20 - 0.005 * 124.8 / 60, abs=1e-4)
    # Past about eight percent, check says so of each clip.
    daw("set", song, "session.tempo", 132)
    assert cli("check", song)[1]["warnings"] == [
        f"song.audio.{n}: stretched +10.0% from 120 BPM; more than about 8% can be heard" for n in (0, 1)
    ]


def test_trims_and_crossfades_through_the_command_line(song):
    daw("audio", "trim", song, "tracks.song.audio.0", "--start", 8, "--end", 24)
    clip = load(song)["tracks"][0]["audio"][0]
    assert (clip["at"], clip["source_start_seconds"], clip["source_end_seconds"]) == (8, FIRST + 2, FIRST + 10)
    daw("audio", "add", song, "song", "song", "--at", 24, "--source-start-seconds", FIRST + 16)
    made = daw("audio", "crossfade", song, "tracks.song.audio.1", "--ms", 20, "--lead-ms", 8)
    assert made["label"] == "Crossfade audio clips at beat 24"
    first, second = load(song)["tracks"][0]["audio"]
    assert first["fade_out_ms"] == 20 and (second["lead_ms"], second["fade_in_ms"]) == (8, 7)
    with pytest.raises(ValueError, match="not inside audio clip"):
        daw("audio", "split", song, "tracks.song.audio.0", "--at", 2)
    with pytest.raises(ValueError, match="A cut runs from a beat to a later one"):
        daw("audio", "cut", song, "song", "--from", 12, "--to", 12)
