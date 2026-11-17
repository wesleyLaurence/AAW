"""`daw samples sound`: each estimate checked on renders the engine made with
known settings. A reverb's decay, a delay's spacing and feedback, a sidechain's
depth and release, an equalizer's bands and a compressor's hold on the level
are read back from the render; a dry render reads as none of them. Then the
command on a generated song with the parts `daw separate` would make of it."""

import json

import numpy as np
import pytest
import soundfile as sf

from helpers import SR, chord_song, cli, run_chain
from agent_daw import sound, spectrum

BPM = 120
BEAT = 60 / BPM


def saw(seconds, note, amp):
    t = np.arange(int(seconds * SR)) / SR
    return amp * (2 * ((t * 440 * 2 ** ((note - 69) / 12)) % 1) - 1)


def phrases(count=12, seed=1):
    """A voice-like part: phrases of syllables of lengths that vary, then rests."""
    g = np.random.default_rng(seed)
    pieces = []
    for k in range(count):
        note = saw(g.uniform(1.0, 2.2), 60 + k % 5, 0.3)
        n = np.arange(len(note))
        env = np.minimum(1, n / (0.01 * SR)) * np.minimum(1, (len(note) - n) / (0.02 * SR))
        env *= 0.6 + 0.4 * (np.sin(2 * np.pi * g.uniform(3, 6) * n / SR + g.uniform(0, 6.28)) > 0)
        pieces += [note * env, np.zeros(int(g.uniform(1.6, 3.0) * SR))]
    x = np.concatenate(pieces)
    return np.column_stack([x, x])


def kick_track(seconds, first=0.25):
    """A kick on every beat, and its times."""
    t = np.arange(int(0.15 * SR)) / SR
    hit = 0.9 * np.sin(2 * np.pi * (50 + 100 * np.exp(-t * 40)) * t) * np.exp(-t * 20)
    x = np.zeros(int(seconds * SR))
    times = np.arange(first, seconds - 1, BEAT)
    for at in times:
        i = int(at * SR)
        x[i : i + len(hit)] += hit
    return np.column_stack([x, x]), times


def pink(seconds=12, seed=0):
    noise = np.random.default_rng(seed).standard_normal(int(seconds * SR))
    f = np.fft.rfftfreq(len(noise), 1 / SR)
    spec = np.fft.rfft(noise)
    spec[1:] /= np.sqrt(f[1:])
    spec[0] = 0
    x = np.fft.irfft(spec, len(noise))
    x *= 0.1 / x.std()
    return np.column_stack([x, x])


def mono(x):
    return x.mean(axis=1)


@pytest.fixture(scope="module")
def voice():
    return phrases()


# --- tails -----------------------------------------------------------------------


@pytest.mark.parametrize("decay", [0.6, 2.0])
def test_a_reverbs_decay_is_read_from_its_tails(tmp_path, voice, decay):
    fx = [{"type": "reverb", "decay_seconds": decay, "mix_percent": 30}]
    y, _ = run_chain(tmp_path / "chain", fx, voice, tempo=BPM)
    found = sound.tails(mono(y), SR, BEAT)
    assert found["decay_seconds"] == pytest.approx(decay, rel=0.15)
    assert found["consistent"] is True
    assert found["decays_fitted"] >= 8
    assert found["echoes"]["heard"] is False


def test_a_dry_part_has_no_tail_and_no_repeats(tmp_path, voice):
    y, _ = run_chain(tmp_path / "chain", [], voice, tempo=BPM)
    found = sound.tails(mono(y), SR, BEAT)
    assert found["ends"] >= 8 and "decay_seconds" not in found
    assert found["echoes"]["heard"] is False


@pytest.mark.parametrize("beats,feedback,division", [(0.75, 40, "dotted 1/8"), (1.0, 50, "1/4"), (0.5, 30, "1/8")])
def test_a_delays_spacing_and_feedback(tmp_path, voice, beats, feedback, division):
    fx = [{"type": "delay", "time_beats": beats, "feedback_percent": feedback, "mix_percent": 30}]
    y, _ = run_chain(tmp_path / "chain", fx, voice, tempo=BPM)
    echoes = sound.tails(mono(y), SR, BEAT)["echoes"]
    assert echoes["heard"] is True
    assert echoes["spacing_beats"] == pytest.approx(beats, abs=0.03)
    assert echoes["nearest"]["division"] == division
    assert echoes["feedback_percent"] == pytest.approx(feedback, abs=8)
    # The first repeat at 30 percent wet under 70 dry: 7.4 dB under.
    assert echoes["first_repeat_db"] == pytest.approx(-7.4, abs=1.5)


# --- pumping ----------------------------------------------------------------------


def pad_over_kicks(tmp_path, effects, seconds=24):
    pad = sum(saw(seconds, n, 0.15) for n in (57, 60, 64))
    kick, times = kick_track(seconds)
    y, report = run_chain(tmp_path / "chain", effects, np.column_stack([pad, pad]), keys={"kick": kick}, tempo=BPM)
    return y, pad, times


@pytest.mark.parametrize("threshold,release", [(-30, 100), (-12, 60)])
def test_a_sidechains_dip_and_release(tmp_path, threshold, release):
    fx = [{"type": "compressor", "threshold_db": threshold, "ratio": 8, "attack_ms": 1, "release_ms": release, "sidechain": "kick"}]
    y, pad, times = pad_over_kicks(tmp_path, fx)
    # What the compressor did: the output's level against the pad's, after each kick.
    out, step = sound.follow(mono(y), SR)
    dry, _ = sound.follow(pad, SR)
    rows = np.array([(out - dry)[int(t / step) : int(t / step) + 400] for t in times[2:-2]])
    truth = np.median(rows, axis=0)
    deepest = int(np.argmin(truth))
    back = int(np.flatnonzero(truth[deepest:] >= truth[deepest] / np.e)[0])
    found = sound.pumping(mono(y), SR, times, BEAT)
    assert found["heard"] is True and found["share_dipping"] >= 0.9
    assert found["depth_db"] == pytest.approx(-truth[deepest], abs=1.5)
    assert found["release_ms"] == pytest.approx(back * step * 1000, rel=0.25)
    assert found["half_a_kick_later_db"] < 1


def test_a_pad_no_kick_ducks_does_not_pump(tmp_path):
    y, _, times = pad_over_kicks(tmp_path, [])
    found = sound.pumping(mono(y), SR, times, BEAT)
    assert found["heard"] is False and found["depth_db"] < 1


# --- tone --------------------------------------------------------------------------


BANDS = [
    {"shape": "highpass", "freq_hz": 120, "slope_db_per_octave": 24},
    {"shape": "bell", "freq_hz": 300, "gain_db": -4, "q": 2.0},
    {"shape": "high_shelf", "freq_hz": 6000, "gain_db": -6, "q": 0.71},
]


def curve_between(bands, hz):
    c = sound.eq_curve(bands, hz)
    return c - c.mean()


def test_the_eq_that_made_a_tone_is_found(tmp_path):
    y, _ = run_chain(tmp_path / "chain", [{"type": "eq", "bands": BANDS}], pink(), tempo=BPM)
    found = sound.tone(spectrum.third_octaves(y.astype(np.float64), SR))
    hz = np.array([c for c in spectrum.CENTERS if 40 <= c <= 12500], float)
    assert np.abs(curve_between(found["eq"]["bands"], hz) - curve_between(BANDS, hz)).max() < 0.75
    shapes = sorted(b["shape"] for b in found["eq"]["bands"])
    assert shapes == ["bell", "high_shelf", "highpass"]
    assert found["plays_hz"][0] <= 125 and found["plays_hz"][1] >= 16000


def test_the_eq_from_one_part_to_another(tmp_path):
    """With --against, the bands that turn the part given into the part read."""
    other = [{"shape": "bell", "freq_hz": 2000, "gain_db": 5, "q": 1.0}]
    y, _ = run_chain(tmp_path / "a", [{"type": "eq", "bands": BANDS}], pink(), tempo=BPM)
    z, _ = run_chain(tmp_path / "b", [{"type": "eq", "bands": BANDS + other}], pink(seed=1), tempo=BPM)
    levels = [spectrum.third_octaves(v.astype(np.float64), SR) for v in (y, z)]
    found = sound.tone(levels[1], against=levels[0])
    assert found["eq"]["from"].startswith("the part given")
    hz = np.array([c for c in spectrum.CENTERS if 200 <= c <= 10000], float)
    assert np.abs(curve_between(found["eq"]["bands"], hz) - curve_between(other, hz)).max() < 1.0


# --- dynamics and width -------------------------------------------------------------


def test_compression_narrows_a_held_parts_level_range(tmp_path, voice):
    """A compressor holding a voice-like part narrows how far its level moves;
    against the dry part the difference says so. On drums its attack decides
    whether the crest rises or falls, so no setting is read from it."""
    ranges, made = [], []
    for i, ratio in enumerate((1, 4, 20)):
        fx = [] if ratio == 1 else [{"type": "compressor", "threshold_db": -24, "ratio": ratio, "attack_ms": 5, "release_ms": 80}]
        y, _ = run_chain(tmp_path / f"c{i}", fx, voice)
        made.append(y)
        ranges.append(sound.dynamics(mono(y), SR)["level_range_db"])
    assert ranges[0] - ranges[1] >= 1.5 and ranges[2] <= ranges[1] + 0.1
    sf.write(tmp_path / "dry.wav", made[0].astype(np.float32), SR, subtype="FLOAT")
    sf.write(tmp_path / "held.wav", made[2].astype(np.float32), SR, subtype="FLOAT")
    found = sound.measure(tmp_path / "held.wav", None, {}, against=tmp_path / "dry.wav")
    assert found["sound"]["mix"]["dynamics"]["against"]["level_range_db"] < -1.5


def test_width_whole_by_band_and_by_section():
    g = np.random.default_rng(0)
    half = 4 * SR
    centered = np.repeat(g.standard_normal(half)[:, None], 2, axis=1)
    wide = g.standard_normal((half, 2))
    x = 0.1 * np.concatenate([centered, wide]).astype(np.float32)
    sections = [
        {"label": "A", "name": "verse", "bars": [1, 2], "seconds": 0.0, "until_seconds": 4.0},
        {"label": "B", "name": "chorus", "bars": [3, 4], "seconds": 4.0, "until_seconds": 8.0},
    ]
    found = sound.width(x, SR, sections)
    a, b = found["by_section"]
    assert a["side_share"] == pytest.approx(0, abs=0.01) and a["correlation"] == pytest.approx(1, abs=0.01)
    assert b["side_share"] == pytest.approx(0.5, abs=0.03) and abs(b["correlation"]) < 0.05
    assert found["side_share"] == pytest.approx(0.25, abs=0.03)
    assert set(found["side_share_by_band"]) == {"low", "mid", "high"}


# --- the command -------------------------------------------------------------------


@pytest.fixture(scope="module")
def project(tmp_path_factory):
    """A song of 32 bars and the parts daw separate would have made of it: a
    voice with a reverb, drums with a kick on every beat and a pad the kick ducks."""
    folder = tmp_path_factory.mktemp("sound")
    project = folder / "project"
    (project / "samples").mkdir(parents=True)
    (project / "separated").mkdir()
    song = chord_song(None, [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]] * 8, bpm=BPM)
    seconds = len(song) / 44100
    rate = 44100
    n = len(song)

    def at_rate(x):
        # The parts at the song's rate, as the separation writes them.
        idx = np.minimum((np.arange(n) * SR / rate).astype(int), len(x) - 1)
        return x[idx]

    kick, times = kick_track(seconds + 1, first=0.0)
    kicks_only = at_rate(kick)
    pad = sum(saw(seconds + 1, k, 0.1) for k in (57, 60, 64))
    duck = [{"type": "compressor", "threshold_db": -30, "ratio": 8, "attack_ms": 1, "release_ms": 100, "sidechain": "kick"}]
    ducked, _ = run_chain(folder / "pad", duck, np.column_stack([pad, pad]), keys={"kick": kick}, tempo=BPM)
    wet, _ = run_chain(folder / "voice", [{"type": "reverb", "decay_seconds": 1.2, "mix_percent": 30}], phrases(8), tempo=BPM)
    parts = {"drums": kicks_only, "other": at_rate(ducked), "vocals": at_rate(wet)}
    record = {"source": {"path": "samples/song.wav", "sha256": "0"}, "parts": {}, "made": "2026-11-16T00:00:00+00:00"}
    for name, x in parts.items():
        path = f"separated/abc-m-{name}.wav"
        sf.write(project / path, x.astype(np.float32), rate, subtype="FLOAT")
        record["parts"][name] = {"path": path, "sha256": "0"}
    mix = 0.5 * np.column_stack([song, song]) + sum(parts.values())
    sf.write(project / "samples" / "song.wav", 0.8 * mix / np.abs(mix).max(), rate, subtype="PCM_16")
    (project / "separated" / "abc-m.separated.json").write_text(json.dumps(record))
    return project


@pytest.fixture(scope="module")
def heard(project):
    code, out = cli("samples", "sound", project / "samples" / "song.wav")
    assert code == 0, out
    return out


def test_the_command_reads_the_song_and_each_part(heard):
    assert set(heard["sound"]) == {"mix", "drums", "other", "vocals"}
    assert set(heard["parts"]) == {"drums", "other", "vocals"}
    assert heard["kicks"]["from"] == "the drums part's kicks" and heard["kicks"]["count"] >= 40
    assert heard["sections"] and heard["sections"][0]["bars"][0] == 1
    for name, found in heard["sound"].items():
        assert found["tone"]["eq"]["bands"], name
        assert len(found["width"]["by_section"]) == len(heard["sections"])
        if name != "mix":
            assert "against_mix_db" in found["dynamics"]
    assert heard["sound"]["drums"]["pumping"]["heard"] is False
    assert "pumping" not in heard["sound"]["mix"]


def test_the_command_hears_the_duck_and_the_reverb(heard):
    other = heard["sound"]["other"]["pumping"]
    assert other["heard"] is True and other["depth_db"] > 6
    assert heard["sound"]["vocals"]["pumping"]["heard"] is False
    assert "the_mix_dips" in heard["kicks"] and other["in_the_mix"] == heard["kicks"]["the_mix_dips"]["heard"]
    tails = heard["sound"]["vocals"]["tails"]
    assert tails["decay_seconds"] == pytest.approx(1.2, rel=0.2)


def test_one_part_against_a_file_and_a_picture(project, tmp_path):
    against = project / "separated" / "abc-m-other.wav"
    code, out = cli(
        "samples", "sound", project / "samples" / "song.wav", "--part", "other",
        "--against", against, "--picture", tmp_path / "sound.png",
    )  # fmt: skip
    assert code == 0, out
    assert set(out["sound"]) == {"other"}
    found = out["sound"]["other"]
    # The part against itself: nothing to change.
    assert found["tone"]["eq"]["bands"] == []
    assert all(abs(v) < 0.05 for v in found["dynamics"]["against"].values())
    assert (tmp_path / "sound.png").stat().st_size > 20_000


def test_what_the_command_refuses(project, tmp_path):
    song = project / "samples" / "song.wav"
    code, out = cli("samples", "sound", song, "--part", "guitar")
    assert code != 0 and "guitar" in out["error"] and "other" in out["error"]
    code, out = cli("samples", "sound", song, "--picture", tmp_path / "x.jpg")
    assert code != 0 and "png" in out["error"]
    code, out = cli("samples", "sound", song, "--against", tmp_path / "missing.wav")
    assert code != 0 and "missing.wav" in out["error"]


def test_a_song_with_no_parts(tmp_path):
    x = chord_song(None, [[57, 60, 64], [53, 57, 60]] * 8, bpm=BPM)
    sf.write(tmp_path / "song.wav", x, 44100, subtype="PCM_16")
    code, out = cli("samples", "sound", tmp_path / "song.wav")
    assert code == 0, out
    assert set(out["sound"]) == {"mix"} and out["parts"] == {}
    assert "daw separate" in out["no_parts"]
    assert out["kicks"]["from"] == "the song's kick band"
