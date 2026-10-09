"""A finer spectrum: third octaves, tilt and resonances, on audio whose answers are known."""

import json
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import spectrum
from agent_daw.model import save, validate
from agent_daw.perception import compare, listen, measure, measured
from helpers import SR, cli, daw, render

BARS = 16
BELL = {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 2500, "gain_db": 9, "q": 8}]}


def pink(rng, frames):
    """Noise with equal power in every octave."""
    found = np.fft.rfft(rng.standard_normal(frames))
    hz = np.fft.rfftfreq(frames, 1 / SR)
    hz[0] = hz[1]
    x = np.fft.irfft(found / np.sqrt(hz), frames)
    return 0.1 * x / np.abs(x).max()


def test_third_octaves_and_the_tilt_of_noise_and_a_tone():
    rng = np.random.default_rng(5)
    assert len(spectrum.CENTERS) == 31 and len(spectrum.EDGES) == 32
    assert spectrum.EDGES[0] == pytest.approx(17.78, abs=0.01) and spectrum.EDGES[-1] == 20000
    # Pink noise is level from band to band, and white climbs 3 dB an octave.
    even = spectrum.third_octaves(pink(rng, 30 * SR)[:, None], SR)
    assert spectrum.tilt(even) == pytest.approx(0, abs=0.2)
    white = spectrum.third_octaves(0.1 * rng.standard_normal((30 * SR, 2)), SR)
    assert spectrum.tilt(white) == pytest.approx(3, abs=0.2)
    assert white[20] - white[17] == pytest.approx(3.01, abs=0.3)
    # A 1 kHz tone is in the 1 kHz band and no other, at its power.
    tone = 0.5 * np.sin(2 * np.pi * 1000 * np.arange(4 * SR) / SR)[:, None]
    levels = spectrum.third_octaves(tone, SR)
    assert spectrum.CENTERS[17] == 1000 and levels[17] == pytest.approx(-9.03, abs=0.01)
    assert max(v for k, v in enumerate(levels) if k != 17) < -100
    # One band of thirty-one has no tilt.
    assert spectrum.tilt(levels) is None
    # Audio too short for two bins in a band leaves the band null, and silence all of them.
    short = spectrum.third_octaves(tone[:4000], SR)
    assert short[:6] == [None] * 6 and short[6] is not None and short[17] == pytest.approx(-9.03, abs=0.05)
    silent = spectrum.third_octaves(np.zeros((SR, 2)), SR)
    assert silent == [None] * 31 and spectrum.tilt(silent) is None
    # Every measurement holds them, and the bands add up to the whole.
    audio = measure(0.1 * rng.standard_normal((4 * SR, 2)), SR)
    assert len(audio["spectrum_db"]) == 31 and audio["tilt_db_per_octave"] == pytest.approx(3, abs=0.4)
    total = 10 * np.log10(sum(10 ** (v / 10) for v in audio["spectrum_db"]))
    # White noise at 48 kHz has a sixth of its power over 20 kHz, where no band is.
    assert total == pytest.approx(audio["rms_dbfs"] + 10 * np.log10(5 / 6), abs=0.1)


def song(directory, pitches, effects=(), per_bar=4, sections=()):
    """A Synth's plain saw playing a line of notes, and its render."""
    directory.mkdir(parents=True, exist_ok=True)
    step = 4 / per_bar
    notes = [{"pitch": int(p), "at": i * step, "duration": step * 0.9} for i, p in enumerate(pitches)]
    document = {
        "session": {"tempo": 120, "length_beats": BARS * 4, "master_gain_db": 0, "end_fade_ms": 0},
        "tracks": [
            {
                "id": "lead",
                "type": "midi",
                "gain_db": -12,
                "instrument": {"synth": {"oscillators": {"a": {}}}},
                "clips": [{"id": "line", "length_beats": BARS * 4, "notes": notes}],
                "effects": list(effects),
            }
        ],
        "sections": list(sections),
    }
    save(validate(document), directory / "song.yaml")
    return Path(render(directory / "song.yaml")["directory"])


def walk():
    """A bass line that never settles: four notes a bar from A1 to A2."""
    return np.random.default_rng(3).integers(33, 46, BARS * 4)


def resonances(folder, track="lead"):
    return listen(folder, images=False)["tracks"][track]["audio"]["resonances"]


def test_a_bell_on_a_melody_is_a_resonance_and_the_melody_is_not(tmp_path):
    assert resonances(song(tmp_path / "plain", walk())) == []
    halves = [
        {"id": "first", "at": 0, "length_beats": BARS * 2},
        {"id": "second", "at": BARS * 2, "length_beats": BARS * 2},
    ]
    belled = song(tmp_path / "bell", walk(), [BELL], sections=halves)
    report = listen(belled, images=False)
    (found,) = report["tracks"]["lead"]["audio"]["resonances"]
    assert found["freq_hz"] == pytest.approx(2500, rel=0.03) and found["note"] == "D#7"
    assert 5 <= found["q"] <= 12
    # A bell's skirt lifts what it is measured from, so it reads under its gain.
    assert 5 <= found["prominence_db"] <= 9
    assert found["present_fraction"] == 1 and found["could_be_note"] is False
    assert report["tracks"]["lead"]["audio"]["resonances_omitted"] == 0
    assert "mix" not in report or "resonances" not in report["mix"]
    assert "twelfth of an octave" in report["methods"]["resonances"]
    assert report["methods"]["third_octave_hz"] == spectrum.CENTERS
    # daw describe listen says what each field means.
    said = " ".join(daw("describe", "listen")["semantics"].values())
    assert all(field in said for field in [*found, "spectrum_db", "tilt_db_per_octave", "resonances_omitted"])

    # By section the third octaves and resonances are written, and printed when asked for.
    code, shown = cli("listen", belled, "--no-images")
    assert code == 0, shown
    written = json.loads(Path(shown["report_path"]).read_text())
    assert len(shown["tracks"]["lead"]["audio"]["spectrum_db"]) == 31
    assert shown["tracks"]["lead"]["audio"]["resonances"] == [found]
    for scope in (shown["sections"]["first"]["audio"], shown["tracks"]["lead"]["sections"]["first"]):
        assert "spectrum_db" not in scope and "resonances" not in scope
        assert scope["tilt_db_per_octave"] is not None
    assert len(written["sections"]["first"]["audio"]["spectrum_db"]) == 31
    part = written["tracks"]["lead"]["sections"]["second"]
    assert part["resonances"][0]["freq_hz"] == pytest.approx(2500, rel=0.03)
    code, section = cli("listen", belled, "--section", "second", "--no-images")
    assert code == 0, section
    assert section["kind"] == "section" and section["section"] == "second"
    assert (section["start_seconds"], section["end_seconds"]) == (16, 32)
    assert section["third_octave_hz"] == spectrum.CENTERS
    assert section["mix"] == written["sections"]["second"]["audio"]
    assert section["tracks"]["lead"] == {"kind": "track", "audio": part}
    # The Rust daw passes the flag on.
    assert daw("listen", belled, "--section", "second", "--no-images")["tracks"] == section["tracks"]
    code, error = cli("listen", belled, "--section", "third")
    assert code == 1 and "first, second" in error["error"]
    code, error = cli("listen", belled, "--section", "first", "--overlap", "lead", "lead")
    assert code == 1 and "not both" in error["error"]

    # A cut at the reported frequency, of the reported height and width, takes it away.
    cut = {
        "type": "eq",
        "bands": [
            {"shape": "bell", "freq_hz": found["freq_hz"], "gain_db": -found["prominence_db"], "q": found["q"]}
        ],
    }
    after = song(tmp_path / "cut", walk(), [BELL, cut], sections=halves)
    change = compare(belled, after, images=False)
    (row,) = change["tracks"]["lead"]["resonances"]
    assert row["freq_hz"] == found["freq_hz"] and row["before_db"] == found["prominence_db"]
    assert (row["listed_before"], row["listed_after"]) == (True, False)
    assert row["delta_db"] == pytest.approx(-found["prominence_db"], abs=2.5)
    assert row["delta_db"] == pytest.approx(row["after_db"] - row["before_db"], abs=0.11)
    # The third octave it sits in falls, with the loudness matched, and no other does.
    thirds = change["tracks"]["lead"]["loudness_matched_delta"]["spectrum_db"]
    assert thirds[21] < -3 and all(abs(v) < 1.5 for k, v in enumerate(thirds) if abs(k - 21) > 1)
    assert change["tracks"]["lead"]["actual_delta"]["tilt_db_per_octave"] == pytest.approx(0, abs=0.3)
    # A section's change has the tilt and not the thirty-one numbers.
    part = change["tracks"]["lead"]["sections"]["first"]
    assert "spectrum_db" not in part["actual_delta"] and "tilt_db_per_octave" in part["actual_delta"]


def test_a_held_note_reports_its_harmonics_as_notes(tmp_path):
    held = listen(song(tmp_path / "held", [45] * BARS * 4), images=False)["tracks"]["lead"]["audio"]
    assert len(held["resonances"]) == spectrum.LISTED and held["resonances_omitted"] > 0
    assert all(r["could_be_note"] is True and r["present_fraction"] == 1 for r in held["resonances"])
    # A2 and its partials, the most prominent first.
    assert [r["freq_hz"] for r in held["resonances"]] == pytest.approx([110, 220, 330, 440, 550], rel=0.01)
    assert [r["note"] for r in held["resonances"]][:2] == ["A2", "A3"]
    assert held["resonances"][0]["prominence_db"] > held["resonances"][-1]["prominence_db"]
    # With the bell on it too, nothing tells the bell from the note's own sound.
    belled = listen(song(tmp_path / "bell", [45] * BARS * 4, [BELL]), images=False)
    assert all(r["could_be_note"] is True for r in belled["tracks"]["lead"]["audio"]["resonances"])


def stems(directory, parts, tracks=(), **more):
    """A song that plays each part's audio once on a track of its name, and its render."""
    directory.mkdir(parents=True, exist_ok=True)
    beats = BARS * 4
    for name, x in parts.items():
        sf.write(directory / f"{name}.wav", x, SR, subtype="DOUBLE")
    document = {
        "session": {"tempo": 120, "length_beats": beats, "master_gain_db": -18, "end_fade_ms": 0},
        "samples": {name: {"path": f"{name}.wav"} for name in parts},
        "patterns": {"once": {"length_beats": beats, "events": [{"at": 0, "pad": "x", "velocity": 127}]}},
        "tracks": list(tracks),
        **more,
    }
    save(validate(document), directory / "song.yaml")
    return Path(render(directory / "song.yaml")["directory"])


def ring(rng, hz=None):
    """Noise bursts on every beat, with a tone that rings after each if asked."""
    x = np.zeros(BARS * 2 * SR)
    t = np.arange(SR // 3) / SR
    for at in range(0, len(x), SR // 2):
        hit = 0.3 * rng.standard_normal(len(t)) * np.exp(-t * 18)
        if hz:
            hit += 0.25 * np.sin(2 * np.pi * hz * t) * np.exp(-t * 8)
        x[at : at + len(t)] += hit
    return x


def test_what_the_song_says_decides_whether_a_peak_could_be_a_note(tmp_path):
    rng = np.random.default_rng(11)
    pad = {"x": {"sample": "snare", "attack_ms": 0, "release_ms": 0}}
    folder = stems(
        tmp_path,
        {"snare": ring(rng, 410), "plain": ring(rng)},
        tracks=[
            # One hit of one sample: nothing moves, so a ring could be the sound's own.
            {"id": "snare", "pads": pad, "clips": [{"pattern": "once"}]},
            # The same file as an audio clip: the song says nothing of what is in it.
            {"id": "clip", "pads": {}, "audio": [{"sample": "snare", "at": 0}]},
            {"id": "plain", "pads": {"x": {"sample": "plain"}}, "clips": [{"pattern": "once"}], "group": "kit"},
            {"id": "sent", "pads": pad, "clips": [{"pattern": "once"}], "sends": [{"to": "room", "gain_db": 0}], "group": "kit"},
        ],
        groups=[{"id": "kit"}],
        returns=[{"id": "room", "effects": [{"type": "delay", "time_beats": "1/16", "feedback_percent": 0}]}],
    )
    report, _, _, rings, _ = measured(folder)

    def first(name):
        return report["tracks"][name]["audio"]["resonances"][0]

    found = first("snare")
    assert found["freq_hz"] == pytest.approx(410, rel=0.02) and found["note"] == "G#4"
    assert found["prominence_db"] > 15 and found["present_fraction"] == 1
    # A ring is narrower than the smoothing, and reads as about that wide.
    assert 12 <= found["q"] <= 20
    assert found["could_be_note"] is True
    assert first("clip")["freq_hz"] == found["freq_hz"] and first("clip")["could_be_note"] is None
    # A group is read from its tracks and a return from what feeds it.
    assert rings.feeding("kit") == ["plain", "sent"] and rings.feeding("room") == ["sent"]
    assert first("kit")["could_be_note"] is True and first("room")["could_be_note"] is True
    assert not any(r["freq_hz"] == found["freq_hz"] for r in report["tracks"]["plain"]["audio"]["resonances"])
    # How far a stem stands out at a frequency, listed there or not.
    assert rings.height("snare", 410) == found["prominence_db"]
    assert rings.height("plain", 410) < spectrum.LEAST_DB
    # The picture: the mix's third octaves over its stems'.
    report = listen(folder)
    assert [Path(p).name for p in report["images"]] == ["overview.png", "spectrum.png"]
    assert Path(report["images"][1]).read_bytes().startswith(b"\x89PNG")


def test_notes_that_move_explain_only_their_own_partials():
    # Two bars of each of two notes, eight bars long, with nothing rendered.
    project = validate(
        {
            "session": {"tempo": 120, "length_beats": 32},
            "samples": {"kick": {"path": "kick.wav"}},
            "patterns": {"beat": {"length_beats": 4, "events": [{"at": 0, "pad": "kick"}]}},
            "tracks": [
                {
                    "id": "lead",
                    "type": "midi",
                    "instrument": {"synth": {"oscillators": {"a": {}}}},
                    "clips": [
                        {
                            "id": "line",
                            "length_beats": 32,
                            "notes": [
                                {"pitch": "A2" if bar % 4 < 2 else "E3", "at": bar * 4, "duration": 4}
                                for bar in range(8)
                            ],
                        }
                    ],
                },
                {"id": "drums", "pads": {"kick": {"sample": "kick"}}, "clips": [{"pattern": "beat", "repeats": 4}]},
                {"id": "empty", "pads": {"kick": {"sample": "kick"}}},
            ],
        }
    )
    manifest = {"tracks": {"lead": {}, "drums": {}, "empty": {}}}
    rings = spectrum.Resonances(16 * SR, SR, project, manifest, 0, [])
    assert len(rings.spans) == 8 and rings.span_beats == 4
    every = np.ones(8, dtype=bool)
    # A2 plays in half the bars: its partials could be the note, up to the sixteenth.
    assert rings.could_be_note("lead", 110, every) is True
    assert rings.could_be_note("lead", 330, every) is True
    assert rings.could_be_note("lead", 16 * 110, every) is True
    assert rings.could_be_note("lead", 3000, every) is False
    # Between two partials, a semitone and more from both, is not one.
    assert rings.could_be_note("lead", 135, every) is False
    # In the bars where only E3 plays, A2 explains nothing.
    later = np.array([False, False, True, True, False, False, True, True])
    assert rings.could_be_note("lead", 110, later) is False and rings.could_be_note("lead", 165, later) is True
    # Hits of one sample never move; a track that plays nothing says nothing.
    assert rings.could_be_note("drums", 3000, every) is True
    assert rings.could_be_note("empty", 3000, every) is None
    # A bar is the span, or as many bars as last a second.
    fast = {"session": {"tempo": 300, "time_signature": "3/4"}, "tracks": []}
    assert spectrum.Resonances(16 * SR, SR, fast, manifest, 0, []).span_beats == 6
    # Fewer than four sounding bars cannot tell what stays from what passes.
    short = spectrum.Resonances(3 * 2 * SR, SR, project, manifest, 0, [])
    short.add("lead", 0.1 * np.random.default_rng(1).standard_normal((3 * 2 * SR, 2)))
    assert short.listed("lead") == {"resonances": None, "resonances_omitted": 0}
    assert short.height("lead", 1000) is None
