"""`daw transcribe --drums`: a generated drum part whose hits, grooves and fills
are known, heard as kicks, snares and hats, grouped into grooves and written as
note text, a MIDI file and a drum track in a song."""

import json
import struct

import numpy as np
import pytest
import soundfile as sf
from scipy.signal import butter, sosfilt

from helpers import beat_song, cli, daw

RATE = 44100
BPM = 120.0
BEAT = 60 / BPM
FIRST = 0.25
BARS = 16
FILLS = (7, 16)  # bars 7 and 16 end in a snare roll and drop the hats' last beat


def kit(seed=1):
    rng = np.random.default_rng(seed)

    def noise(seconds, decay, level, band):
        n = round(seconds * RATE)
        s = sosfilt(butter(4, band[1], band[0], fs=RATE, output="sos"), rng.standard_normal(n))
        return level * s / np.abs(s).max() * np.exp(-np.arange(n) / (decay * RATE))

    def kick(level=0.8):
        t = np.arange(round(0.25 * RATE)) / RATE
        sweep = np.cumsum(55 + 110 * np.exp(-t / 0.015)) / RATE
        body = level * np.sin(2 * np.pi * sweep) * np.exp(-t / 0.09)
        # A beater's click: as loud in the snare's band as a snare, gone in a few ms.
        click = noise(0.006, 0.0012, 0.6 * level / 0.8, ("bandpass", (1500, 4000)))
        body[: len(click)] += click
        return body

    def snare(level=0.5):
        t = np.arange(round(0.15 * RATE)) / RATE
        body = 0.3 * np.sin(2 * np.pi * 190 * t) * np.exp(-t / 0.02)
        return level / 0.5 * (body + noise(0.15, 0.04, 0.5, ("lowpass", 5000)))

    def hat(level=0.15):
        return noise(0.04, 0.008, level, ("highpass", 8000))

    return kick, snare, hat


def drums(path, bars=BARS, fills=FILLS, seed=1):
    """A drum part at 120 BPM: a kick on one and three, a snare on two and four
    and hats on the eighths; in the fill bars the snare plays the last beat's
    sixteenths and the hats leave it. Returns each drum's hits as (bar, cell)."""
    kick, snare, hat = kit(seed)
    x = np.zeros(round((FIRST + (bars * 4 + 2) * BEAT) * RATE))
    known = {"kick": [], "snare": [], "hat": []}

    def add(drum, bar, cell, sound):
        start = round((FIRST + ((bar - 1) * 4 + cell / 4) * BEAT) * RATE)
        sound = sound[: len(x) - start]
        x[start : start + len(sound)] += sound
        known[drum].append((bar, cell))

    for bar in range(1, bars + 1):
        fill = bar in fills
        for cell in (0, 8):
            add("kick", bar, cell, kick())
        for cell in (4,) + ((12, 13, 14, 15) if fill else (12,)):
            add("snare", bar, cell, snare(0.5 if cell in (4, 12) else 0.35))
        for cell in range(0, 12 if fill else 16, 2):
            add("hat", bar, cell, hat())
    sf.write(path, 0.9 * x / np.abs(x).max(), RATE, subtype="PCM_24")
    return known


@pytest.fixture(scope="module")
def part(tmp_path_factory):
    """The part, and the whole song's beat map it is placed on, as a stem is."""
    folder = tmp_path_factory.mktemp("drums")
    path = folder / "drums.wav"
    known = drums(path)
    beat_song(folder / "song.wav", bpm=BPM, bars=BARS + 1, first=FIRST)
    return path, known


def on(path):
    return ["--beats", path.with_name("song.wav")]


@pytest.fixture(scope="module")
def heard(part):
    path, _ = part
    code, out = cli("transcribe", path, "--drums", "--text", *on(path))
    assert code == 0, out
    return out


def cells(out, drum):
    """A drum's hits as (bar, cell) from the reply."""
    found = []
    for h in out["hits"]:
        if h["drum"] == drum:
            bar, beat, n = map(int, h["sixteenth"].split("."))
            found.append((bar, (beat - 1) * 4 + n - 1))
    return found


def test_each_drum_is_heard_on_its_sixteenths(part, heard):
    _, known = part
    assert heard["tempo_bpm"] == pytest.approx(BPM, abs=0.05)
    for drum in ("kick", "snare"):
        assert sorted(cells(heard, drum)) == sorted(known[drum]), drum
    # A hat on a kick or a snare is the hardest to hear; none is made up.
    hats = set(cells(heard, "hat"))
    assert hats <= set(known["hat"]) and len(hats) >= 0.9 * len(known["hat"])
    # The kick's click is in the snare's band, as loud as a snare, and is no snare.
    assert not set(cells(heard, "snare")) & set(known["kick"])
    assert heard["drums"]["kick"] == {
        "note": "C2",
        "midi": 36,
        "count": len(known["kick"]),
        "clearest_seconds": heard["drums"]["kick"]["clearest_seconds"],
    }
    assert (heard["drums"]["snare"]["note"], heard["drums"]["hat"]["midi"]) == ("D2", 42)
    first = heard["hits"][0]
    assert (first["drum"], first["sixteenth"], first["beats"]) == ("kick", "1.1.1", pytest.approx(0, abs=0.01))
    assert abs(first["off_grid_ms"]) <= 3 and 1 <= first["velocity"] <= 127
    # The fills' quieter snares are quieter notes.
    roll = [h for h in heard["hits"] if h["drum"] == "snare" and h["sixteenth"] == "7.4.2"]
    loud = [h for h in heard["hits"] if h["drum"] == "snare" and h["sixteenth"] == "7.2.1"]
    assert roll[0]["velocity"] < loud[0]["velocity"]


def test_the_bars_that_share_a_groove_and_the_fills(heard):
    (groove,) = heard["grooves"]
    assert groove["id"] == "A" and groove["count"] == BARS - len(FILLS)
    assert groove["bars"] == [[1, 6], [8, 15]] and groove["beats"] == 4 and groove["hits"] == 12
    # A bar of it may lack a hat the hearing missed, and has nothing else.
    assert all(set(d) == {"bar", "missing"} and all(m.startswith("hat ") for m in d["missing"]) for d in groove["differs"])
    assert heard["fills"] == [[7, 7], [16, 16]]


def test_the_part_as_note_text_a_row_a_drum(heard):
    text = heard["text"]
    assert text["grid"] == "1/4"
    kinds = [(c["id"], c["kind"], c["bars"]) for c in text["clips"]]
    assert kinds == [
        ("groove-a-1", "groove", [1, 6]),
        ("fill-2", "fill", [7, 7]),
        ("groove-a-3", "groove", [8, 15]),
        ("fill-4", "fill", [16, 16]),
    ]
    rows = text["clips"][0]["text"].split("\n")
    assert len(rows) == 3 and text["clips"][0]["loop_bars"] == 1
    strip = lambda row: [cell.split("@")[0] for cell in row.split()]
    assert strip(rows[0]) == ["c2", ".", ".", ".", ".", ".", ".", ".", "c2", ".", ".", ".", ".", ".", ".", "."]
    assert strip(rows[1]) == [".", ".", ".", ".", "d2", ".", ".", ".", ".", ".", ".", ".", "d2", ".", ".", "."]
    assert strip(rows[2]) == ["f#2", "."] * 8
    fill = strip(text["clips"][1]["text"].split("\n")[1])
    assert fill[12:] == ["d2"] * 4
    code, out = cli("transcribe", "unused.wav", "--drums", "--grid", "1/3", "--text")
    assert code != 0


def test_a_midi_file_of_every_hit_as_played(part, tmp_path):
    path, known = part
    target = tmp_path / "drums.mid"
    code, out = cli("transcribe", path, *on(path), "--drums", "--midi", target)
    assert code == 0, out
    assert out["midi"] == {"path": str(target.resolve()), "bar_one_beat": 0.0, "as_played": True}
    data = target.read_bytes()
    assert data[:4] == b"MThd" and struct.unpack(">H", data[12:14])[0] == 960
    ons = [data[i + 1] for i in range(22, len(data) - 2) if data[i] == 0x90 and data[i + 2] > 0]
    assert sorted(set(ons)) == [36, 38, 42]
    assert len(ons) == out["count"] >= 0.95 * sum(len(v) for v in known.values())


def test_a_drum_track_in_a_song_as_one_edit(part, tmp_path):
    path, known = part
    folder = tmp_path / "song"
    daw("init", folder, "--tempo", "120")
    song = folder / "song.yaml"
    code, out = cli("transcribe", path, *on(path), "--drums", "--project", song, "--at", "4")
    assert code == 0, out
    written = out["clip"]
    assert written["track"] == "drums" and written["bar_one_at"] == 4
    assert {d: p["note"] for d, p in written["pads"].items()} == {"kick": "C2", "snare": "D2", "hat": "F#2"}
    track = daw("get", folder, "tracks.drums")
    sampler = track["instrument"]["sampler"]
    assert sorted(sampler["pads"]) == ["hat", "kick", "snare"]
    assert {m["pad"]: m["notes"] for m in sampler["map"]} == {"kick": 36, "snare": 38, "hat": 42}
    for d in ("kick", "snare", "hat"):
        sample = daw("get", folder, f"samples.drums-{d}")
        assert (folder / sample["path"]).is_file()
    clips = {c["id"]: c for c in track["clips"]}
    groove = clips["groove-a-1"]
    assert (groove["at"], groove["length_beats"], groove["loop_beats"]) == (4, 24, 4)
    assert len(groove["notes"]) == 12
    assert (clips["fill-2"]["at"], clips["fill-2"]["length_beats"], clips["fill-2"].get("loop_beats")) == (28, 4, None)
    assert clips["groove-a-3"]["at"] == 32 and clips["fill-4"]["at"] == 64
    # Every hit is in the song where it is heard: each groove note on its sixteenth.
    for n in groove["notes"]:
        assert abs(n.get("at", 0) * 4 - round(n.get("at", 0) * 4)) < 0.04
    # A part refused leaves the song as it was: here a second one would take
    # the first one's sample names on a track it names.
    daw("track", "add", folder, "kit", "--type", "midi")
    before = song.read_text()
    daw("set", folder, "samples.kit-kick", json.dumps(daw("get", folder, "samples.drums-kick")))
    before = song.read_text()
    code, out = cli("transcribe", path, *on(path), "--drums", "--project", song, "--track", "kit")
    assert code != 0 and "already has a sample kit-kick" in out["error"]
    assert song.read_text() == before


def test_as_played_is_one_clip(part, tmp_path):
    path, known = part
    folder = tmp_path / "song"
    daw("init", folder)
    code, out = cli("transcribe", path, *on(path), "--drums", "--as-played", "--project", folder / "song.yaml")
    assert code == 0, out
    (clip,) = out["clip"]["clips"]
    assert clip["kind"] == "played" and clip["bars"] == [1, BARS]
    notes = daw("get", folder, "tracks.drums")["clips"][0]["notes"]
    assert len(notes) == out["count"]


def test_a_drum_track_where_an_audio_clip_plays_the_part(part, tmp_path):
    path, _ = part
    folder = tmp_path / "song"
    daw("init", folder, "--tempo", "120")
    cli("samples", "import", path, "--project", folder, "--id", "stem")
    daw("track", "add", folder, "orig")
    daw("audio", "add", folder, "orig", "stem", "--at", "8")
    code, out = cli("transcribe", "tracks.orig.audio.0", *on(path), "--drums", "--project", folder / "song.yaml")
    assert code == 0, out
    # Bar 1 lands where the clip plays the part's first downbeat, FIRST in.
    assert out["clip"]["bar_one_at"] == pytest.approx(8 + FIRST / BEAT, abs=0.01)
    assert out["left_out"] == {"outside_the_clip": 0}


def test_onto_a_midi_track_of_the_song(part, tmp_path):
    path, _ = part
    folder = tmp_path / "song"
    daw("init", folder)
    daw("track", "add", folder, "kit", "--type", "midi")
    daw("track", "add", folder, "vox")
    song = folder / "song.yaml"
    code, out = cli("transcribe", path, *on(path), "--drums", "--project", song, "--track", "kit")
    assert code == 0, out
    assert out["clip"]["track"] == "kit"
    assert daw("get", folder, "tracks.kit")["instrument"]["sampler"]["map"]
    code, out = cli("transcribe", path, *on(path), "--drums", "--project", song, "--track", "vox")
    assert code != 0 and "not a MIDI track" in out["error"]
    code, out = cli("transcribe", path, *on(path), "--drums", "--project", song, "--track", "none")
    assert code != 0 and "No track none" in out["error"]
    # The kit's Sampler is kept; a second part on it adds clips only.
    before = daw("get", folder, "tracks.kit")["instrument"]
    code, out = cli("transcribe", path, *on(path), "--drums", "--project", song, "--track", "kit", "--at", "64")
    assert code == 0, out
    assert out["clip"]["pads"] == {} and daw("get", folder, "tracks.kit")["instrument"] == before
    ids = [c["id"] for c in daw("get", folder, "tracks.kit")["clips"]]
    assert len(ids) == len(set(ids)) and "groove-a-1-2" in ids


@pytest.mark.parametrize(
    "extra, message",
    [
        (["--line"], "--line is for pitched parts"),
        (["--range", "C1-C3"], "--range is for pitched parts"),
        (["--quantize", "1/4", "--text"], "--quantize is for pitched parts"),
    ],
)
def test_what_a_drum_part_refuses(part, extra, message):
    path, _ = part
    code, out = cli("transcribe", path, *on(path), "--drums", *extra)
    assert code != 0 and message in out["error"]


def test_as_played_needs_drums_and_something_written(part):
    path, _ = part
    code, out = cli("transcribe", path, *on(path), "--as-played", "--text")
    assert code != 0 and "--as-played goes with --drums" in out["error"]
    code, out = cli("transcribe", path, *on(path), "--drums", "--as-played")
    assert code != 0 and "--text, --midi or --project" in out["error"]


def test_near_a_time(part):
    path, _ = part
    code, out = cli("transcribe", path, *on(path), "--drums", "--near", "0:04", "--window", "1")
    assert code == 0, out
    assert out["hits"] and all(abs(h["seconds"] - 4) <= 1 for h in out["hits"])
    assert out["near"]["of"] > len(out["hits"])
    assert "text" not in out and "clip" not in out


def test_silence_has_no_drums(tmp_path):
    from agent_daw import drums as module

    found = module.heard(np.zeros(RATE * 2, dtype=np.float32), RATE)
    assert found["moments"] == [] and found["drums"] == []


def test_the_clips_of_grooves_fills_and_empty_bars():
    from agent_daw import drums as module

    hit = [{"drum": "kick"}]
    labels = {1: "A", 2: "A", 3: "A", 4: "B", 5: "A", 6: "B", 7: "A", 8: "B", 11: "A"}
    by_bar = {b: hit for b in (*labels, 10, 12, 13)}
    shape = {"labels": labels, "first": 1, "last": 13, "by_bar": by_bar}
    made = [(c["kind"], c["bars"][0], c["bars"][-1], c["loop_bars"]) for c in module.clips([], None, shape)]
    # A run of A, then B and A in turn looped two bars, the B left over, a bar
    # with no hit skipped, and the fills around an A.
    assert made == [
        ("groove", 1, 3, 1),
        ("groove", 4, 7, 2),
        ("groove", 8, 8, 1),
        ("fill", 10, 10, None),
        ("groove", 11, 11, 1),
        ("fill", 12, 13, None),
    ]
