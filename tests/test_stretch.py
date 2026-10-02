"""A song played faster at its own pitch: `stretch: preserve_pitch` on a pad."""

import shutil

import numpy as np
import pytest
import soundfile as sf

from agent_daw.model import load, save
from helpers import SR, beat_song, cli, daw, render, stem

FASTER = 1.04


def tone_song(folder, tempo=120.0, stretch="preserve_pitch", stretcher="signalsmith"):
    """Four seconds of a 440 Hz tone with a click each half second, as a pad at 120 BPM."""
    folder.mkdir()
    t = np.arange(4 * SR) / SR
    x = 0.4 * np.sin(2 * np.pi * 440 * t)
    x[:: SR // 2] += 0.5
    sf.write(folder / "tone.wav", np.column_stack([x, x]), SR, subtype="PCM_24")
    save(
        {
            "session": {"tempo": tempo, "length_beats": 8, "master_gain_db": 0, "end_fade_ms": 0, "stretcher": stretcher},
            "samples": {"tone": {"path": "tone.wav"}},
            "patterns": {"once": {"length_beats": 8, "events": [{"at": 0, "pad": "x", "velocity": 127}]}},
            "tracks": [
                {
                    "id": "tone",
                    "pads": {"x": {"sample": "tone", "source_bpm": 120, "stretch": stretch, "attack_ms": 0, "release_ms": 0}},
                    "clips": [{"pattern": "once"}],
                }
            ],
        },
        folder / "song.yaml",
    )
    return folder / "song.yaml"


def pitch(x):
    """The strongest frequency of the left channel, to a tenth of a hertz."""
    left = x[:, 0] * np.hanning(len(x))
    spectrum = np.abs(np.fft.rfft(left, 1 << 20))
    return round(float(np.argmax(spectrum)) * SR / (1 << 20), 1)


@pytest.mark.parametrize("stretcher", ["signalsmith", "rubberband"])
def test_a_faster_tempo_keeps_the_pitch_and_shortens_the_sound(tmp_path, stretcher):
    if stretcher == "rubberband" and not shutil.which("rubberband"):
        pytest.skip("No rubberband here")
    song = tone_song(tmp_path / "song", 120 * FASTER, stretcher=stretcher)
    report = render(song, tmp_path / "out")
    x = stem(tmp_path / "out", "tone")
    assert pitch(x) == pytest.approx(440, abs=0.5)
    # The sound ends where four seconds at 1.04 times the speed end.
    sounding = np.flatnonzero(np.abs(x[:, 0]) > 0.01)
    assert sounding[-1] / SR == pytest.approx(4 / FASTER, abs=0.01)
    # The clicks, a beat apart in the song, are a beat apart in the session.
    late = [np.argmax(np.abs(np.diff(x[round(k * SR / 2 / FASTER) - 480 :, 0]))[:960]) - 480 for k in (1, 3, 5)]
    assert max(abs(v) for v in late) / SR < 0.002
    assert report["dependencies"]["stretcher"].startswith(stretcher)
    assert "preserve_pitch" in report["pitch_policy"]
    # The same song renders the same.
    again = render(song, tmp_path / "again")
    assert again["audio_sha256"] == report["audio_sha256"]


def test_repitch_is_the_default_and_moves_the_pitch(tmp_path):
    song = tone_song(tmp_path / "song", 120 * FASTER, stretch="repitch")
    report = render(song, tmp_path / "out")
    assert pitch(stem(tmp_path / "out", "tone")) == pytest.approx(440 * FASTER, abs=0.5)
    assert "stretcher" not in report["dependencies"]
    assert "stretch" not in song.read_text()


def test_at_its_own_tempo_nothing_is_stretched(tmp_path):
    stretched = render(tone_song(tmp_path / "a"), tmp_path / "a-out")
    plain = render(tone_song(tmp_path / "b", stretch="repitch"), tmp_path / "b-out")
    assert np.array_equal(stem(tmp_path / "a-out", "tone"), stem(tmp_path / "b-out", "tone"))
    assert "stretcher" not in stretched["dependencies"] and "stretcher" not in plain["dependencies"]


def test_check_warns_of_a_stretch_that_can_be_heard(tmp_path):
    song = tone_song(tmp_path / "song", 120 * FASTER)
    assert cli("check", song)[1]["warnings"] == []
    daw("set", song, "session.tempo", 132)
    assert cli("check", song)[1]["warnings"] == [
        "tone.x: stretched +10.0% from 120 BPM; more than about 8% can be heard"
    ]
    project = load(song)
    project["tracks"][0]["pads"]["x"]["source_bpm"] = None
    save(project, song)
    assert "nothing is stretched" in cli("check", song)[1]["warnings"][0]


def test_a_missing_stretcher_is_named(tmp_path, monkeypatch):
    song = tone_song(tmp_path / "song", 120 * FASTER, stretcher="rubberband")
    monkeypatch.setenv("PATH", str(tmp_path))
    with pytest.raises(ValueError, match="rubberband program was not found"):
        render(song, tmp_path / "out")


def test_an_edit_sped_up_keeps_its_joins(tmp_path):
    """The edit of test_joins, its parts following the tempo, four percent faster."""
    folder = tmp_path / "edit"
    folder.mkdir()
    beat_song(folder / "song.wav", bars=12)
    cli("samples", "--db", folder / "index.sqlite", "beats", folder / "song.wav")
    lead, beat, first = 0.005, 0.5, 0.137
    follow = {"sample": "song", "source_bpm": 120, "stretch": "preserve_pitch"}
    save(
        {
            "session": {"tempo": 120, "sample_rate": 44100, "length_beats": 46, "master_gain_db": -3},
            "samples": {"song": {"path": "song.wav"}},
            "patterns": {
                "edit": {
                    "length_beats": 46,
                    "events": [{"at": 4 - lead / beat, "pad": "first"}, {"at": 20 - lead / beat, "pad": "second"}],
                }
            },
            "tracks": [
                {
                    "id": "song",
                    "pads": {
                        "first": {**follow, "start_seconds": first - lead, "end_seconds": first - lead + 16 * beat + 0.012, "attack_ms": 2, "release_ms": 12},
                        "second": {**follow, "start_seconds": first - lead + 24 * beat, "attack_ms": 4, "release_ms": 20},
                    },
                    "clips": [{"pattern": "edit"}],
                }
            ],
        },
        folder / "song.yaml",
    )
    # The tempo at which the 23-second song lasts 22.115 seconds: four percent more.
    code, helper = cli("timeline", folder / "song.yaml", "--tempo-for", 23 / FASTER)
    assert helper["tempo_for"]["tempo"] == pytest.approx(124.8, abs=0.001)
    assert helper["tempo_for"]["change_percent"] == pytest.approx(4, abs=0.001)
    assert [p["pad"] for p in helper["tempo_for"]["pads_that_follow_tempo"]] == ["song.first", "song.second"]
    daw("set", folder / "song.yaml", "session.tempo", helper["tempo_for"]["tempo"])
    render(folder / "song.yaml")
    code, report = cli("joins", folder / "renders" / "latest.json", "--no-excerpts")
    assert code == 0, report
    join = report["joins"][0]
    assert report["length"]["seconds"] == pytest.approx(23 / FASTER, abs=0.001)
    # By the beat map the beat still carries across, eight beats skipped; by the
    # audio the stretched transients either side sit alike, to half a millisecond.
    assert abs(join["grid"]["interval_error_ms"]) < 0.1 and join["grid"]["source_beats_skipped"] == 8
    assert abs(join["measured"]["interval_error_ms"]) < 0.5
    assert abs(join["measured"]["before_ms"]) < 2.5
    assert join["flags"] == []
