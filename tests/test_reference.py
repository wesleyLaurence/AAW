"""A reference: a song analyzed once, and a mix compared with it section by section.

The songs are generated: a noise burst on each beat, a chord and a sub tone, a
quiet half and then a loud one. A mix is the same song with its sub turned up,
played by a pad and rendered, so its sections are the project's.
"""

import json
import shutil
import subprocess
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import reference
from agent_daw.model import digest, save, validate
from helpers import cli, daw, render

RATE = 48000
BPM = 120
BARS = 16
LEAD = 0.25  # seconds of silence before a reference's first beat


def song(sub=1.0, lead=0.0, wide=0.0):
    """Sixteen bars at 120 BPM: eight quiet, eight four times as loud. `sub`
    scales the 45 Hz tone alone; `wide` at 1 puts the chord out of phase between
    the channels, at the level it had."""
    rng = np.random.default_rng(7)
    beat = 60 / BPM
    n = round(BARS * 4 * beat * RATE)
    t = np.arange(n) / RATE
    level = np.where(t < BARS * 2 * beat, 0.25, 1.0)
    hits = np.zeros(n)
    burst = np.exp(-np.arange(round(0.08 * RATE)) / RATE / 0.02)
    for k in range(BARS * 4):
        at = round(k * beat * RATE)
        hits[at : at + len(burst)] += 0.2 * rng.standard_normal(len(burst)) * burst
    chord = 0.05 * sum(np.sin(2 * np.pi * hz * t) for hz in (110, 440, 660))
    low = 0.1 * sub * np.sin(2 * np.pi * 45 * t)
    left = level * (hits + chord + low)
    right = level * (hits + (1 - 2 * wide) * chord + low)
    silence = np.zeros((round(lead * RATE), 2))
    return np.vstack([silence, np.column_stack([left, right])])


def write(path, x):
    sf.write(path, x, RATE, subtype="FLOAT")
    return path


@pytest.fixture
def workspace(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_WORKSPACE", str(tmp_path / "workspace"))
    return tmp_path / "workspace"


@pytest.fixture
def club(tmp_path, workspace):
    """A reference whose first downbeat is its first beat, at 0.25 s."""
    source = write(tmp_path / "Club Song.wav", song(lead=LEAD))
    return reference.add(source, "club-ref", bpm=BPM, downbeat=LEAD)


def mix(tmp_path, **changes):
    """A render of the song with two sections, each half of it."""
    folder = tmp_path / "mix"
    folder.mkdir()
    write(folder / "source.wav", song(**changes))
    project = validate(
        {
            "session": {"tempo": BPM, "length_beats": BARS * 4, "end_fade_ms": 0},
            "samples": {"song": {"path": "source.wav"}},
            "patterns": {
                "all": {"length_beats": BARS * 4, "events": [{"at": 0, "pad": "song"}]}
            },
            "tracks": [
                {"id": "song", "pads": {"song": {"sample": "song"}}, "clips": [{"pattern": "all"}]}
            ],
            "sections": [
                {"id": "verse", "at": 0, "length_beats": BARS * 2},
                {"id": "drop", "at": BARS * 2, "length_beats": BARS * 2},
            ],
        }
    )
    save(project, folder / "song.yaml")
    render(folder / "song.yaml")
    return folder / "renders" / "latest.json"


def test_a_reference_is_its_measurements_and_not_its_audio(tmp_path, workspace, club):
    folder = workspace / "library" / "references" / "club-ref"
    assert sorted(p.name for p in folder.iterdir()) == ["beats.json", "reference.json"]
    source = tmp_path / "Club Song.wav"
    assert club["source"]["path"] == str(source.resolve())
    assert club["source"]["sha256"] == digest(source)
    assert club["source"]["available"] and club["path"] == str(folder)
    assert club["beat_map"]["tempo"]["bpm"] == pytest.approx(BPM, abs=0.05)
    assert club["beat_map"]["first_downbeat_seconds"] == pytest.approx(LEAD, abs=0.005)
    assert club["mix"]["integrated_lufs"] < 0
    # The measured sections are the beat map's phrases: the quiet half, then the loud.
    assert club["sections_from"] == "measurement"
    first, last = club["sections"][0], club["sections"][-1]
    assert first["id"] == "bar-1" and first["at"] == pytest.approx(0, abs=0.01)
    assert first["label"] == "low" and last["label"] == "high"
    assert last["relative_lu"] - first["relative_lu"] == pytest.approx(12, abs=1)
    change = next(s for s in club["sections"] if s["label"] == "high")
    assert change["start_seconds"] == pytest.approx(LEAD + 16, abs=0.01)
    assert change["bar"] == 9 and change["at"] == pytest.approx(32, abs=0.01)
    assert last["end_seconds"] == pytest.approx(LEAD + 32, abs=0.01)
    # A section holds what daw listen measures of it, but for the true peak.
    assert set(club["mix"]) - set(first["audio"]) == {"estimated_true_peak_dbtp"}
    listed = reference.listing()
    assert [r["name"] for r in listed] == ["club-ref"]
    assert listed[0]["sections"] == [s["id"] for s in club["sections"]]
    assert reference.shown(reference.load("Club Ref")) == club


def test_a_name_is_taken_until_replaced_and_a_reference_is_removed(tmp_path, workspace, club):
    source = tmp_path / "Club Song.wav"
    with pytest.raises(ValueError, match="A reference named club-ref is saved"):
        reference.add(source, "Club Ref")
    again = reference.add(source, "Club Ref", replace=True)
    assert again["name"] == "club-ref"
    # Without a name, the file's.
    assert reference.add(source)["name"] == "club-song"
    with pytest.raises(ValueError, match="No reference named other .saved: club-ref, club-song"):
        reference.load("other")
    with pytest.raises(ValueError, match="letters or digits"):
        reference.add(source, "!!")
    assert reference.remove("club-song")["source"] == str(source.resolve())
    assert [r["name"] for r in reference.listing()] == ["club-ref"]
    assert source.is_file()


def test_a_sub_six_db_heavier_is_reported_by_section(tmp_path, club):
    # Named as the song's are, the reference's sections are matched by ID.
    named = reference.sections(
        "club-ref", [{"id": "verse", "at": 0}, {"id": "drop", "at": BARS * 2}]
    )
    assert named["sections_from"] == "request"
    assert [s["id"] for s in named["sections"]] == ["verse", "drop"]
    assert named["sections"][1]["start_seconds"] == pytest.approx(LEAD + 16, abs=0.01)
    assert named["sections"][1]["length_beats"] == pytest.approx(BARS * 2, abs=0.01)
    report = reference.compare(mix(tmp_path, sub=2.0), "club-ref")
    assert report["matching"]["sections"] == "id"
    assert report["direction"] == "mix minus reference"
    assert report["unmatched_sections"] == {"mix": [], "reference": []}
    for scope in (report["whole"], report["sections"]["verse"], report["sections"]["drop"]):
        bands = scope["delta"]["band_db"]
        assert bands["sub"] == pytest.approx(6.02, abs=0.2)
        assert all(abs(v) < 0.2 for band, v in bands.items() if band != "sub")
        assert scope["delta"]["band_fraction"]["sub"] > 0.1
    drop = report["sections"]["drop"]
    assert drop["reference_section"] == "drop" and drop["reference"]["label"] == "high"
    assert drop["mix"]["start_seconds"] == 16 and drop["mix"]["end_seconds"] == 32
    # Each side's drop is as far over its own file: the contour is the reference's.
    assert drop["delta"]["relative_lu"] == pytest.approx(0, abs=0.3)
    assert report["contour"]["delta"] == pytest.approx(0, abs=0.5)
    sub = [line for line in report["observations"] if line.startswith("sub (20–60 Hz)")]
    assert sub == [
        "sub (20–60 Hz): 6.0 dB above the reference's balance over the whole mix; "
        "by section, drop +6.0 dB, verse +6.0 dB."
    ]
    assert not any(line.startswith(("low", "mid", "high", "air")) for line in report["observations"])
    # The report is kept in the render's analysis folder, and nowhere else.
    path = Path(report["report_path"])
    assert path.name == "reference-club-ref.json"
    assert path.parents[2] == Path(report["mix"]["audio_path"]).parent
    assert json.loads(path.read_text())["observations"] == report["observations"]
    json.dumps(report, allow_nan=False)


def test_without_shared_ids_sections_are_matched_by_loudness(tmp_path, club):
    report = reference.compare(mix(tmp_path, sub=2.0), "club-ref")
    assert report["matching"]["sections"] == "loudness"
    loudest = max(club["sections"], key=lambda s: s["audio"]["integrated_lufs"])
    quietest = min(club["sections"], key=lambda s: s["audio"]["integrated_lufs"])
    assert report["sections"]["drop"]["reference_section"] == loudest["id"]
    assert report["sections"]["verse"]["reference_section"] == quietest["id"]
    assert report["sections"]["drop"]["delta"]["band_db"]["sub"] == pytest.approx(6, abs=0.3)
    others = {s["id"] for s in club["sections"]} - {loudest["id"], quietest["id"]}
    assert report["unmatched_sections"] == {"mix": [], "reference": sorted(others)}
    assert f"drop against {loudest['id']} +6.0 dB" in "".join(report["observations"])
    # Back to the measured sections after naming them.
    reference.sections("club-ref", [{"id": "verse", "seconds": 0}])
    assert reference.sections("club-ref", measured_again=True)["sections"] == club["sections"]


def test_a_file_without_sections_is_compared_whole(tmp_path, club):
    quiet = write(tmp_path / "quiet.wav", song(wide=1) * 10 ** (-6 / 20))
    report = reference.compare(quiet, "club-ref")
    assert report["matching"]["sections"] == "none" and report["sections"] == {}
    assert report["mix"]["render_id"] is None
    delta = report["whole"]["delta"]
    assert delta["integrated_lufs"] == pytest.approx(-6, abs=0.1)
    assert delta["estimated_true_peak_dbtp"] == pytest.approx(-6, abs=0.1)
    # A level all over is no change of balance; the chord out of phase is one of width.
    assert all(abs(v) < 0.2 for v in delta["band_db"].values())
    assert delta["side_energy_fraction"] > 0.05 and delta["stereo_correlation"] < 0
    # The largest first, by how many times over its threshold each is.
    lines = report["observations"]
    assert sorted(line.split(":")[0] for line in lines) == ["loudness", "side energy", "true peak"]
    assert "points more of the power in the sides" in lines[0]
    assert any(line.startswith("loudness: the mix is 6.0 LU quieter than club-ref,") for line in lines)
    assert Path(report["report_path"]).parents[2] == tmp_path
    # The same file as the reference differs in nothing.
    same = reference.compare(tmp_path / "Club Song.wav", "club-ref")
    assert same["observations"] == []


def test_a_band_one_side_lacks_is_said_and_not_measured(tmp_path, workspace):
    t = np.arange(6 * RATE) / RATE
    tone = lambda hz: 0.1 * np.sin(2 * np.pi * hz * t)  # noqa: E731
    full = np.column_stack([tone(45) + tone(1000) + tone(5000)] * 2)
    thin = np.column_stack([tone(1000) + tone(5000)] * 2)
    added = reference.add(write(tmp_path / "full.wav", full), "full")
    # Too short for a beat map: one whole, with the reason kept.
    assert added["beat_map"] is None and added["sections"] == []
    assert "full.wav" in added["beat_map_problem"]
    report = reference.compare(write(tmp_path / "thin.wav", thin), "full")
    assert report["whole"]["delta"]["band_db"] == dict.fromkeys(report["whole"]["delta"]["band_db"])
    assert any(
        line.startswith("sub (20–60 Hz): the mix has nothing there") for line in report["observations"]
    )
    # Its sections can still be named in seconds, and not in beats.
    named = reference.sections("full", [{"id": "b", "seconds": "0:04"}, {"id": "a", "seconds": 0}])
    assert [(s["id"], s["start_seconds"], s["end_seconds"]) for s in named["sections"]] == [
        ("a", 0, 4),
        ("b", 4, 6),
    ]
    assert "at" not in named["sections"][0]
    with pytest.raises(ValueError, match="no beat map; give seconds"):
        reference.sections("full", [{"id": "a", "at": 0}])
    with pytest.raises(ValueError, match="silent"):
        reference.add(write(tmp_path / "silence.wav", np.zeros((RATE, 2))), "silence")


def test_sections_by_hand_are_checked(tmp_path, club):
    for spec, message in [
        ({"id": "a"}, "Sections are a JSON list"),
        ([], "Sections are a JSON list"),
        ([{"at": 0}], "an object with an id"),
        ([{"id": "a"}], "takes at, in the reference's beats, or seconds"),
        ([{"id": "a", "at": 0, "seconds": 1}], "takes at, in the reference's beats, or seconds"),
        ([{"id": "a", "at": 0, "length_beats": 8}], "and nothing else"),
        ([{"id": "a", "at": 0}, {"id": "a", "at": 8}], "each used once"),
        ([{"id": "a", "at": 0}, {"id": "b", "at": 0}], "holds no audio"),
        ([{"id": "a", "seconds": "9:00"}], "holds no audio"),
        ([{"id": "a", "seconds": "soon"}], "Not a time"),
    ]:
        with pytest.raises(ValueError, match=message):
            reference.sections("club-ref", spec)
    assert reference.sections("club-ref")["sections_from"] == "measurement"
    # Naming sections measures the person's file again, so it must be as it was.
    source = tmp_path / "Club Song.wav"
    write(source, song(lead=LEAD, sub=0.5))
    with pytest.raises(ValueError, match="is not at .* as it was; add it again"):
        reference.sections("club-ref", [{"id": "a", "at": 0}])
    source.unlink()
    with pytest.raises(ValueError, match="add it again"):
        reference.sections("club-ref", [{"id": "a", "at": 0}])
    # What was measured is still there to compare with and to read.
    assert not reference.shown(reference.load("club-ref"))["source"]["available"]
    assert reference.compare(write(tmp_path / "mix.wav", song()), "club-ref")["whole"]


def test_the_commands(tmp_path, workspace):
    source = write(tmp_path / "Club Song.wav", song(lead=LEAD))
    code, added = cli("reference", "add", source, "--name", "club-ref", "--bpm", BPM, "--downbeat", LEAD)
    assert code == 0, added
    assert added["beat_map"]["requested"] == {"bpm": BPM, "downbeat": LEAD, "meter": None}
    # The Rust daw passes the commands on to Python.
    assert [r["name"] for r in daw("reference", "list")] == ["club-ref"]
    assert daw("reference", "show", "club-ref") == added
    spec = json.dumps([{"id": "verse", "at": 0}, {"id": "drop", "seconds": LEAD + 16}])
    named = daw("reference", "sections", "club-ref", spec)
    assert [s["id"] for s in named["sections"]] == ["verse", "drop"]
    assert daw("reference", "sections", "club-ref") == named
    pointer = mix(tmp_path, sub=2.0)
    report = daw("compare", pointer, "--reference", "club-ref")
    assert report["kind"] == "reference_comparison"
    assert report["sections"]["drop"]["delta"]["band_db"]["sub"] == pytest.approx(6, abs=0.2)
    assert cli("compare", pointer, "--reference", "club-ref", "--no-images")[1]["whole"] == report["whole"]
    assert daw("reference", "sections", "club-ref", "--measured")["sections"] == added["sections"]
    for args, message in [
        (("compare", pointer), "two renders, or one render and --reference NAME"),
        (("compare", pointer, pointer, "--reference", "club-ref"), "two renders, or one render"),
        (("compare", pointer, "--reference", "other"), "No reference named other"),
        (("reference", "sections", "club-ref", "verse"), "Sections are a JSON list"),
        (("reference", "sections", "club-ref", spec, "--measured"), "JSON or --measured"),
        (("reference", "add", source, "--name", "club-ref"), "is saved; give another --name"),
        (("reference", "add", source, "--name", "fast", "--bpm", 500), "bpm must be"),
        (("reference", "add", tmp_path / "missing.wav"), "No file at"),
    ]:
        with pytest.raises(ValueError, match=message):
            daw(*args)
    assert daw("reference", "remove", "club-ref")["removed"] == "club-ref"
    assert daw("reference", "list") == []
    # Two renders are compared as before.
    assert daw("compare", pointer, pointer, "--no-images")["kind"] == "compare"


def test_a_compressed_song_is_a_reference_and_no_copy_is_kept(tmp_path, workspace):
    if not shutil.which("afconvert"):
        pytest.skip("No AAC encoder here")
    wav = tmp_path / "song.wav"
    sf.write(wav, song(lead=LEAD), RATE, subtype="PCM_16")
    source = tmp_path / "Bought Song.m4a"
    subprocess.run(
        ["afconvert", "-f", "m4af", "-d", "aac", "-b", "256000", str(wav), str(source)], check=True
    )
    added = reference.add(source, bpm=BPM)
    assert added["name"] == "bought-song" and added["source"]["decoder"]
    assert added["source"]["sha256"] == digest(source)
    assert added["source"]["decoded_sha256"] != added["source"]["sha256"]
    assert added["beat_map"]["tempo"]["bpm"] == pytest.approx(BPM, abs=0.05)
    assert added["source"]["duration_seconds"] == pytest.approx(LEAD + 32, abs=0.1)
    kept = [p.name for p in (workspace / "library" / "references" / "bought-song").iterdir()]
    assert sorted(kept) == ["beats.json", "reference.json"]
    assert sorted(p.name for p in tmp_path.iterdir()) == ["Bought Song.m4a", "song.wav", "workspace"]
    named = reference.sections("bought-song", [{"id": "all", "seconds": 0}])
    assert named["sections"][0]["end_seconds"] == pytest.approx(LEAD + 32, abs=0.1)
    report = reference.compare(wav, "bought-song")
    assert all(abs(v) < 0.5 for v in report["whole"]["delta"]["band_db"].values())
    assert abs(report["whole"]["delta"]["integrated_lufs"]) < 0.2
