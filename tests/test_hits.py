"""A song's hits in one band, against a generated drum part whose hits are known."""

import numpy as np
import pytest
import soundfile as sf
from scipy.signal import butter, sosfilt

from agent_daw import hits
from helpers import cli

RATE = 44100
BPM = 120.0
BEAT = 60 / BPM


def drums(path, bars=16, first=0.25, seed=1):
    """A drum part at 120 BPM with a bass under it, written to `path`.

    A kick on beats one and three, a ghost kick 9 dB down on the last eighth of
    every other bar, a snare on two and four, hats on the eighths and a bass note
    held through each bar from its downbeat, in the kick's band. Bar 3's first kick
    has a beater click 8 ms before its body, and bar 5's first kick a faint blip
    30 ms before it. Returns the times of each kind.
    """
    rng = np.random.default_rng(seed)
    beats = first + BEAT * np.arange(bars * 4)
    x = np.zeros(round((beats[-1] + 2) * RATE))

    def add(at, sound):
        start = round(at * RATE)
        sound = sound[: len(x) - start]
        x[start : start + len(sound)] += sound

    def kick(level=0.8):
        t = np.arange(round(0.25 * RATE)) / RATE
        sweep = np.cumsum(55 + 110 * np.exp(-t / 0.015)) / RATE
        return level * np.sin(2 * np.pi * sweep) * np.exp(-t / 0.09)

    def noise(seconds, decay, level, band=None):
        n = round(seconds * RATE)
        s = rng.standard_normal(n)
        if band:
            s = sosfilt(butter(4, band[1], band[0], fs=RATE, output="sos"), s)
            s /= np.abs(s).max()
        return level * s * np.exp(-np.arange(n) / (decay * RATE))

    def snare():
        t = np.arange(round(0.12 * RATE)) / RATE
        body = 0.3 * np.sin(2 * np.pi * 190 * t) * np.exp(-t / 0.02)
        return body + noise(0.12, 0.03, 0.5, ("lowpass", 5000))

    def bass(hz, seconds):
        t = np.arange(round(seconds * RATE)) / RATE
        shape = np.minimum(1, t / 0.03) * np.minimum(1, (seconds - t) / 0.03)
        return 0.06 * np.sin(2 * np.pi * hz * t) * shape

    known = {"kick": [], "ghost": [], "snare": [], "hats": []}
    for i, at in enumerate(beats):
        bar, place = divmod(i, 4)
        if place in (0, 2):
            if bar == 2 and place == 0:
                add(at - 0.008, noise(0.004, 0.001, 0.25))  # the beater's click
                known["kick"].append(at - 0.008)
            else:
                known["kick"].append(at)
            if bar == 4 and place == 0:
                add(at - 0.030, noise(0.002, 0.0005, 0.003))  # a faint blip in the quiet
            add(at, kick())
        else:
            add(at, snare())
            known["snare"].append(at)
        for half in (0, 0.5):
            add(at + half * BEAT, noise(0.03, 0.006, 0.15, ("highpass", 8000)))
            known["hats"].append(at + half * BEAT)
        if place == 0:
            add(at, bass((62.0, 70.0, 58.0, 66.0)[bar % 4], 4 * BEAT * 0.95))
        if place == 3 and bar % 2:
            add(at + BEAT / 2, kick(0.8 * 10 ** (-9 / 20)))
            known["ghost"].append(at + BEAT / 2)
    sf.write(path, 0.9 * x / np.abs(x).max(), RATE, subtype="PCM_24")
    return {k: np.array(v) for k, v in known.items()}


@pytest.fixture(scope="module")
def song(tmp_path_factory):
    path = tmp_path_factory.mktemp("hits") / "drums.wav"
    return path, drums(path)


def matched(found, truth, within=0.002):
    """How many of `truth` have a found time within `within` seconds."""
    found = np.array(found)
    return sum(bool(len(found)) and np.abs(found - t).min() <= within for t in truth)


def test_the_kick_band_finds_each_kick_at_its_attack(song):
    path, known = song
    code, out = cli("samples", "hits", path, "--band", "kick")
    assert code == 0, out
    assert out["band"] == {"name": "kick", "low_hz": 45.0, "high_hz": 90.0}
    loud = [h for h in out["hits"] if h["level_db"] >= -6]
    # Every kick and nothing else within 6 dB of the loudest, each within 2 ms,
    # the one with a click from the click and the one after a blip from itself.
    assert len(loud) == len(known["kick"])
    assert matched([h["seconds"] for h in loud], known["kick"]) == len(known["kick"])
    # The ghost kicks are listed too, about 9 dB down.
    ghosts = [h for h in out["hits"] if matched([h["seconds"]], known["ghost"])]
    assert len(ghosts) == len(known["ghost"])
    assert all(-13 <= h["level_db"] <= -6 for h in ghosts)
    assert sum(c["hits"] for c in out["levels"] if c["from_db"] >= -3) == len(known["kick"])
    assert [c["hits"] for c in out["levels"] if c["from_db"] == -3.0] == [0]


def test_a_hit_is_placed_on_the_beat_map(song):
    path, known = song
    code, out = cli("samples", "hits", path, "--band", "kick", "--min-level", "-6")
    assert code == 0, out
    assert out["tempo_bpm"] == pytest.approx(BPM, abs=0.05)
    first = out["hits"][0]
    assert (first["bar"], first["beat"], first["sixteenth"]) == (1, 1.0, "1.1.1")
    assert abs(first["off_grid_ms"]) <= 2
    # Beats one and three, two beats apart, and the clicked kick 8 ms early.
    assert {h["sixteenth"].split(".", 1)[1] for h in out["hits"]} == {"1.1", "3.1"}
    assert [h["gap_beats"] for h in out["hits"][:3]] == [2.0, 2.0, 2.0]
    clicked = next(h for h in out["hits"] if h["bar"] == 2 and h["sixteenth"] == "3.1.1")
    assert clicked["off_grid_ms"] == pytest.approx(-8, abs=2)
    assert out["hits"][-1]["gap_seconds"] is None and out["hits"][-1]["gap_beats"] is None
    code, all_hits = cli("samples", "hits", path, "--band", "kick")
    ghost = min(all_hits["hits"], key=lambda h: abs(h["seconds"] - known["ghost"][0]))
    assert ghost["sixteenth"] == "2.4.3" and abs(ghost["off_grid_ms"]) <= 2


@pytest.mark.parametrize("band, kind", [("snare", "snare"), ("hats", "hats")])
def test_the_snare_and_hat_bands_find_their_hits(song, band, kind):
    path, known = song
    code, out = cli("samples", "hits", path, "--band", band)
    assert code == 0, out
    times = [h["seconds"] for h in out["hits"]]
    # The beater's click sounds in both bands, and 8 ms before bar 3's hat it
    # is one hit with it, timed from the click.
    truth = [t for t in known[kind] if abs(t - known["kick"][4] - 0.008) > 0.001]
    assert matched(times, truth) == len(truth)
    if kind == "snare":
        # The snares, and the beater's click, which sounds in the snare's band too.
        assert len(times) == len(known["snare"]) + 1
        assert matched(times, [known["kick"][4]]) == 1


def test_near_a_time_and_a_band_in_hz(song):
    path, known = song
    code, out = cli("samples", "hits", path, "--band", "45-90", "--near", "0:10", "--window", "2")
    assert code == 0, out
    assert out["band"]["name"] == "45-90"
    assert out["hits"] and all(abs(h["seconds"] - 10) <= 2 for h in out["hits"])
    assert out["near"]["of"] > out["count"]


def test_the_picture_marks_the_hits(song, tmp_path):
    path, _ = song
    picture = tmp_path / "kick.png"
    code, out = cli("samples", "hits", path, "--band", "kick", "--picture", picture)
    assert code == 0, out
    assert out["picture"] == str(picture.resolve())
    assert picture.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"
    code, out = cli("samples", "hits", path, "--band", "kick", "--near", "5", "--picture", tmp_path / "near.png")
    assert code == 0 and (tmp_path / "near.png").is_file()
    code, out = cli("samples", "hits", path, "--picture", tmp_path / "kick.jpg")
    assert code != 0 and "png" in out["error"]


def test_a_file_too_short_for_a_beat_map_has_hits_in_seconds(tmp_path):
    path = tmp_path / "loop.wav"
    known = drums(path, bars=1, first=0.1)
    code, out = cli("samples", "hits", path, "--band", "kick", "--min-level", "-6")
    assert code == 0, out
    assert out["no_beat_map"] and out["tempo_bpm"] is None
    assert matched([h["seconds"] for h in out["hits"]], known["kick"]) == len(known["kick"])
    assert "bar" not in out["hits"][0] and "gap_beats" not in out["hits"][0]


@pytest.mark.parametrize("text", ["90-45", "lows", "5-90", "45-", "45-30000"])
def test_a_band_is_named_or_two_frequencies(text):
    with pytest.raises(ValueError, match="band"):
        hits.band(text)
    assert hits.band("Kick") == ("kick", 45.0, 90.0)
    assert hits.band("100-250.5") == ("100-250.5", 100.0, 250.5)


def test_silence_has_no_hits(tmp_path):
    path = tmp_path / "quiet.wav"
    sf.write(path, np.zeros(RATE), RATE)
    found, _, _ = hits.find(np.zeros(RATE, dtype=np.float32), RATE, 45, 90)
    assert found == []
