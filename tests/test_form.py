"""`daw samples form`: a generated song whose sections are known, intro, verse,
chorus, verse, chorus and the chorus a tone up, read as its form with the key
change; the parts `daw separate` made naming the sections; and the checks of
the meter and of the bars by the drums."""

import json

import numpy as np
import pytest
import soundfile as sf

from helpers import chord_song, cli

RATE = 44100
BPM = 120.0
BEAT = 60 / BPM
Am, F, C, G, Dm, E = [57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62], [50, 53, 57], [52, 56, 59]
VERSE = [Am, F, C, G] * 2
CHORUS = [F, G, C, Am, F, G, E, E]


def up(chords, semitones):
    return [[n + semitones for n in c] for c in chords]


def section(chords, level, seed):
    return level * chord_song(None, chords, bpm=BPM, seed=seed)


# The song's sections: name, bars, chords, level. The chorus is louder.
FORM = [
    ("intro", 4, None, 0.5),
    ("verse", 8, VERSE, 0.5),
    ("chorus", 8, CHORUS, 1.0),
    ("verse", 8, VERSE, 0.5),
    ("chorus", 8, CHORUS, 1.0),
    ("chorus", 8, up(CHORUS, 2), 1.0),
]


def make_song(folder):
    """The mix, and a voice part that sings in every section but the intro."""
    pieces, voice = [], []
    for i, (name, bars, chords, level) in enumerate(FORM):
        x = section(chords, level, i) if chords else level * chord_song(None, [], bpm=BPM, seed=i)[: round(bars * 4 * BEAT * RATE)]
        pieces.append(x)
        t = np.arange(len(x)) / RATE
        voice.append(0.0 * t if name == "intro" else 0.3 * np.sin(2 * np.pi * 440 * t) * (0.6 + 0.4 * np.sin(2 * np.pi * t)))
    tail = np.zeros(round(2 * RATE))
    mix = np.concatenate([*pieces, tail])
    sung = np.concatenate([*voice, tail])
    sf.write(folder / "song.wav", 0.8 * mix / np.abs(mix).max(), RATE, subtype="PCM_16")
    return folder / "song.wav", 0.5 * sung


def starts():
    """Each section's first bar, from bar 1 at the song's start."""
    out, bar = [], 1
    for _, bars, _, _ in FORM:
        out.append(bar)
        bar += bars
    return out


@pytest.fixture(scope="module")
def song(tmp_path_factory):
    folder = tmp_path_factory.mktemp("form")
    path, voice = make_song(folder)
    return path, voice


@pytest.fixture(scope="module")
def heard(song):
    code, out = cli("samples", "form", song[0])
    assert code == 0, out
    return out


def near_start(sections, bar, within=1):
    return next((s for s in sections if abs(s["bars"][0] - bar) <= within), None)


def test_the_sections_and_which_are_the_same(heard):
    sections = heard["sections"]
    begin = starts()
    found = [near_start(sections, b) for b in begin]
    assert all(found), [s["bars"] for s in sections]
    intro, verse1, chorus1, verse2, chorus2, moved = found
    # The verses are one letter and the choruses another; the last chorus is the
    # chorus a tone up, a key change.
    assert verse1["label"] == verse2["label"]
    assert chorus1["label"] == chorus2["label"] != verse1["label"]
    assert moved["label"] == chorus1["label"][0] + "′"
    assert moved["like"]["how"] == "moved" and moved["like"]["semitones"] == 2
    assert heard["form"] == " ".join(s["label"] for s in sections)
    # With no parts beside it, the louder music that comes back is the chorus.
    assert chorus1["name"] == chorus2["name"] == "chorus"
    assert verse1["name"] == "verse"
    assert heard["named_by"].startswith("level and repetition")
    assert heard["parts"] == {}


def test_the_key_change(heard):
    changes = heard["key_changes"]
    assert len(changes) == 1
    change = changes[0]
    assert change["semitones"] == 2 and abs(change["bar"] - starts()[5]) <= 1
    assert change["heard_as"].endswith("+2 semitones")
    assert heard["key"] in ("A minor", "C major")


def test_each_section_says_its_bars_and_level(heard):
    sections = heard["sections"]
    assert sections[0]["bars"][0] == 1 and sections[0]["starts_by"] == "the song's start"
    for a, b in zip(sections, sections[1:]):
        assert b["bars"][0] == a["bars"][1] + 1
        assert b["seconds"] == pytest.approx(a["until_seconds"], abs=1e-3)
    assert sum(s["count"] for s in sections) == heard["bar_count"]
    chorus = near_start(sections, starts()[2])
    verse = near_start(sections, starts()[1])
    assert chorus["level_db"] > verse["level_db"] + 3


def test_the_checks_with_no_parts(heard):
    checks = heard["checks"]
    assert checks["meter"] == "4/4" and checks["heard_as"] == "4/4"
    assert checks["beat"]["divides_in"] == 2
    assert checks["beat_changes_length"] == []
    assert checks["drums"]["used"] is False and "daw separate" in checks["drums"]["why"]
    assert "warnings" not in heard


def test_the_parts_name_the_sections(song, tmp_path):
    """A song with the parts daw separate made of it, by its recipe: the voice
    starts the verse and the intro has none."""
    path, voice = song
    project = tmp_path / "project"
    (project / "samples").mkdir(parents=True)
    (project / "separated").mkdir()
    source = project / "samples" / "song.wav"
    source.write_bytes(path.read_bytes())
    mix, _ = sf.read(path)
    drums = 0.6 * chord_song(None, [], bpm=BPM)
    drums = np.resize(drums, len(mix))
    sf.write(project / "separated" / "abc-m-vocals.wav", voice[: len(mix)], RATE, subtype="PCM_16")
    sf.write(project / "separated" / "abc-m-drums.wav", drums, RATE, subtype="PCM_16")
    record = {
        "source": {"path": "samples/song.wav", "sha256": "0"},
        "parts": {
            "vocals": {"path": "separated/abc-m-vocals.wav", "sha256": "0"},
            "drums": {"path": "separated/abc-m-drums.wav", "sha256": "0"},
        },
        "made": "2026-11-16T00:00:00+00:00",
    }
    (project / "separated" / "abc-m.separated.json").write_text(json.dumps(record))
    code, out = cli("samples", "form", source)
    assert code == 0, out
    assert set(out["parts"]) == {"vocals", "drums"}
    assert out["named_by"].startswith("the voice")
    sections = out["sections"]
    assert sections[0]["name"] == "intro" and sections[0]["sung"] == 0
    verse = near_start(sections, starts()[1])
    assert verse["sung"] == 1 and verse["name"] == "verse"
    assert near_start(sections, starts()[2])["name"] == "chorus"
    assert out["checks"]["drums"]["used"] is True
    assert out["checks"]["drums"]["cycle"]["kick"] and out["checks"]["drums"]["bars_off"] == []


def test_a_silent_part_is_no_voice(song, tmp_path):
    from agent_daw import form, library

    path, _ = song
    project = tmp_path / "p"
    (project / "samples").mkdir(parents=True)
    (project / "separated").mkdir()
    source = project / "samples" / "song.wav"
    source.write_bytes(path.read_bytes())
    mix, _ = sf.read(path)
    sf.write(project / "separated" / "x-vocals.wav", 1e-5 * np.random.default_rng(0).standard_normal(len(mix)), RATE)
    record = {"source": {"path": "samples/song.wav"}, "parts": {"vocals": {"path": "separated/x-vocals.wav"}}}
    (project / "separated" / "x.separated.json").write_text(json.dumps(record))
    parts = form.parts_of(source)
    assert set(parts) == {"vocals"}
    out = form.measure(source, library.beat_map(library.default_db(), source), parts)
    assert out["silent_parts"] == ["vocals"]
    assert "no voice" in out["named_by"]


def test_a_picture_of_the_bars(song, tmp_path):
    code, out = cli("samples", "form", song[0], "--picture", tmp_path / "form.png")
    assert code == 0, out
    assert (tmp_path / "form.png").stat().st_size > 10_000 and out["picture"].endswith("form.png")
    code, out = cli("samples", "form", song[0], "--picture", tmp_path / "form.jpg")
    assert code != 0 and "png" in json.dumps(out)


def test_a_beat_that_divides_in_three(tmp_path):
    """Hats on the thirds of each beat: the map counts four beats a bar, and the
    form says the song may be in 12/8."""
    x = chord_song(None, (VERSE + CHORUS) * 2, bpm=80, grid=1 / 3)
    sf.write(tmp_path / "song.wav", x, RATE, subtype="PCM_16")
    code, out = cli("samples", "form", tmp_path / "song.wav")
    assert code == 0, out
    assert out["checks"]["beat"]["divides_in"] == 3
    assert out["checks"]["heard_as"] == "12/8"
    assert any("12/8" in w for w in out["warnings"])


def test_a_map_whose_beat_changes_length():
    from agent_daw import form

    times = list(np.arange(32) * 0.5) + list(16 + np.arange(32) * 0.25) + list(24 + np.arange(32) * 0.5)
    rows = [{"seconds": t, "bar": i // 4 + 1, "beat": i % 4 + 1} for i, t in enumerate(times)]
    found = form.pulse({"beats": rows})
    assert len(found) == 1
    assert found[0]["from_bar"] in (9, 10) and found[0]["to_bar"] in (16, 17)
    assert found[0]["times_the_usual"] == pytest.approx(0.5)


def test_the_drums_check_the_bars():
    """A part whose bars the map has a beat late from bar 5 to bar 8: the kick
    and snare there play as if each bar started a beat earlier."""
    from agent_daw import form
    from test_drums import drums

    import tempfile
    from pathlib import Path

    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / "drums.wav"
        drums(path, bars=12, fills=())
        mono, sr = sf.read(path)
    first, beat = 0.25, 0.5
    labels = []
    for i in range(48):
        # Bars 1-4 right; from beat 16 a pickup of one beat, then bars counted a beat late.
        if i < 16:
            labels.append((i // 4 + 1, i % 4 + 1))
        elif i == 16:
            labels.append((4, 5))
        elif i < 33:
            j = i - 17
            labels.append((j // 4 + 5, j % 4 + 1))
        else:
            j = i - 33
            labels.append((9 if j < 3 else (j - 3) // 4 + 10, j + 1 if j < 3 else (j - 3) % 4 + 1))
    rows = [{"seconds": first + i * beat, "bar": b, "beat": k} for i, (b, k) in enumerate(labels)]
    report = {"beats": rows, "tempo": {"bpm": 120.0}, "beats_per_bar": 4, "duration": len(mono) / sr}
    bars = form.bar_spans(report)
    found = form.drums_check(mono, sr, report, bars)
    assert found["used"] is True
    assert found["cycle"] == {"kick": [1.0, 3.0], "snare": [2.0, 4.0]}
    off = found["bars_off"]
    assert off, found
    assert off[0]["bars"][0] == 5 and off[0]["bars"][1] >= 7 and abs(off[0]["beats"]) == 1


def test_too_short_for_a_form(tmp_path):
    sf.write(tmp_path / "short.wav", chord_song(None, [Am, F, C, G], bpm=BPM), RATE, subtype="PCM_16")
    code, out = cli("samples", "form", tmp_path / "short.wav")
    assert code != 0 and "too short" in json.dumps(out)
