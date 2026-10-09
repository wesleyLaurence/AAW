"""Overlap between stems: generated songs whose contests are known."""

from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import overlap
from agent_daw.model import save, validate
from agent_daw.perception import compare, listen, measured, overlap_of
from helpers import SR, cli, daw, render

TEMPO = 120
BEAT = SR * 60 // TEMPO


def tone(hz, first_bar, last_bar, bars=8, amp=0.3, phase=0.0):
    """A tone over some bars of a song, counted from 1, faded 10 ms at each end."""
    x = np.zeros(bars * 4 * BEAT)
    start, end = (first_bar - 1) * 4 * BEAT, last_bar * 4 * BEAT
    x[start:end] = amp * np.sin(2 * np.pi * hz * np.arange(end - start) / SR + phase)
    ramp = np.linspace(0, 1, SR // 100)
    x[start : start + len(ramp)] *= ramp
    x[end - len(ramp) : end] *= ramp[::-1]
    return x


def hits(hz, bars=8, amp=0.5, decay=14.0, every=1.0):
    """A drum: a tone that dies away, struck every so many beats."""
    x = np.zeros(bars * 4 * BEAT)
    t = np.arange(BEAT // 2) / SR
    hit = amp * np.sin(2 * np.pi * hz * t) * np.exp(-t * decay)
    for at in range(0, len(x), round(every * BEAT)):
        x[at : at + len(hit)] += hit[: len(x) - at]
    return x


def song(directory, parts, **more):
    """A song that plays each part's audio once on a track of its name, and its render."""
    directory.mkdir(parents=True, exist_ok=True)
    beats = len(next(iter(parts.values()))) // BEAT
    tracks = []
    for name, x in parts.items():
        sf.write(directory / f"{name}.wav", x, SR, subtype="DOUBLE")
        tracks.append(
            {
                "id": name,
                "pads": {"x": {"sample": name, "attack_ms": 0, "release_ms": 0}},
                "clips": [{"pattern": "once"}],
                **more.get("tracks", {}).get(name, {}),
            }
        )
    document = {
        "session": {"tempo": TEMPO, "length_beats": beats, "master_gain_db": 0, "end_fade_ms": 0},
        "samples": {name: {"path": f"{name}.wav"} for name in parts},
        "patterns": {
            "once": {"length_beats": beats, "events": [{"at": 0, "pad": "x", "velocity": 127}]}
        },
        "tracks": tracks,
        **{k: v for k, v in more.items() if k != "tracks"},
    }
    save(validate(document), directory / "song.yaml")
    return Path(render(directory / "song.yaml")["directory"])


def named(report, a, b):
    return [p for p in report["overlap"]["pairs"] if (p["a"], p["b"]) == (a, b)]


def test_two_tones_in_one_band_are_a_pair_where_they_meet(tmp_path):
    folder = song(
        tmp_path,
        {
            "low": tone(80, 1, 8),
            "under": tone(80, 5, 8, amp=0.25, phase=np.pi / 2),
            "high": tone(2000, 1, 8),
        },
    )
    report = listen(folder, images=False)
    (pair,) = report["overlap"]["pairs"]
    assert (pair["a"], pair["b"], pair["band_hz"]) == ("low", "under", [45, 89])
    # Half the time either sounds, and all of the time the second does.
    assert pair["fraction"] == pytest.approx(0.5, abs=0.02)
    assert pair["fraction_of_b"] == pytest.approx(1, abs=0.03)
    assert pair["contested_beats"] == pytest.approx(16, abs=0.5)
    assert pair["level_difference_db"] == pytest.approx(20 * np.log10(0.3 / 0.25), abs=0.3)
    assert not pair["keyed"]
    # With no sections, the bars where it is worst: 5 to 8.
    assert pair["worst"] == [{"at_beat": 16.0, "length_beats": 16.0, "fraction": 1.0}]
    assert report["overlap"]["pairs_omitted"] == 0
    assert report["overlap"]["cell_beats"] == 0.25
    assert "not masking" in report["methods"]["overlap"]
    # daw describe listen says what each field of a pair means.
    said = " ".join(daw("describe", "listen")["semantics"].values())
    assert all(field in said for field in [*pair, "pairs_omitted", "cell_beats", "other_bands_hz"])


def test_sections_are_where_it_is_worst_and_a_preview_keeps_the_songs_beats(tmp_path):
    folder = song(
        tmp_path,
        {"low": tone(80, 1, 8), "under": tone(80, 5, 8, amp=0.25, phase=1.0)},
        sections=[
            {"id": "first", "at": 0, "length_beats": 16},
            {"id": "second", "at": 16, "length_beats": 16},
        ],
    )
    (pair,) = listen(folder, images=False)["overlap"]["pairs"]
    assert pair["worst"] == [
        {"section": "second", "at_beat": 16.0, "length_beats": 16.0, "fraction": 1.0}
    ]
    preview = Path(render(tmp_path / "song.yaml", section="second")["directory"])
    (pair,) = listen(preview, images=False)["overlap"]["pairs"]
    assert pair["fraction"] == pytest.approx(1, abs=0.03)
    assert pair["worst"][0]["at_beat"] == 16.0


def kick_and_bass(directory, keyed):
    effects = [
        {
            "type": "compressor",
            "threshold_db": -40,
            "ratio": 20,
            "attack_ms": 1,
            "release_ms": 60,
            "sidechain": "kick",
        }
    ]
    return song(
        directory,
        {"kick": hits(60), "bass": tone(62, 1, 8, amp=0.3)},
        tracks={"bass": {"effects": effects}} if keyed else {},
        sections=[
            {"id": "verse", "at": 0, "length_beats": 16},
            {"id": "drop", "at": 16, "length_beats": 16},
        ],
    )


def test_a_bass_keyed_by_the_kick_overlaps_it_less(tmp_path):
    plain, keyed = kick_and_bass(tmp_path / "plain", False), kick_and_bass(tmp_path / "keyed", True)
    (before,) = named(listen(plain, images=False), "kick", "bass")
    assert before["band_hz"] == [45, 89] and not before["keyed"]
    assert before["fraction"] >= 0.25
    # The kick is contested most of the time it sounds; the bass is alone between hits.
    assert before["fraction_of_a"] > before["fraction_of_b"]
    report = compare(plain, keyed, images=False)
    (row,) = report["overlap"]["pairs"]
    assert (row["a"], row["b"], row["band_hz"]) == ("kick", "bass", [45, 89])
    assert row["before"] == before["fraction"]
    assert row["after"] < row["before"] - 0.1 and row["delta"] == pytest.approx(
        row["after"] - row["before"], abs=1e-3
    )
    assert (row["keyed_before"], row["keyed_after"]) == (False, True)
    assert set(row["sections"]) == {"verse", "drop"}
    assert all(s["delta"] < 0 for s in row["sections"].values())
    # Compared the other way, the pair the first no longer names is still followed.
    back = compare(keyed, plain, images=False)["overlap"]["pairs"][0]
    assert back["delta"] == pytest.approx(-row["delta"], abs=1e-3)


def test_a_group_and_a_return_are_not_set_against_their_own_tracks(tmp_path):
    folder = song(
        tmp_path,
        {"kick": hits(60, decay=6, amp=0.3), "tom": hits(66, decay=6, amp=0.3), "lead": tone(2000, 1, 8, amp=0.1)},
        groups=[{"id": "drums"}],
        returns=[{"id": "room", "effects": [{"type": "delay", "time_beats": "1/16", "feedback_percent": 0}]}],
        tracks={
            "kick": {"group": "drums", "sends": [{"to": "room", "gain_db": 0}]},
            "tom": {"group": "drums"},
        },
    )
    report, _, grid = measured(folder)
    assert grid.related("kick", "drums") and grid.related("drums", "tom")
    assert grid.related("room", "kick") and grid.related("drums", "room")
    assert not grid.related("room", "tom") and not grid.related("kick", "tom")
    assert not grid.related("lead", "room") and not grid.related("lead", "drums")
    pairs = {(p["a"], p["b"]) for p in report["overlap"]["pairs"]}
    # Two tracks of one group are a pair, and the room against a track that does not feed it.
    assert ("kick", "tom") in pairs and ("tom", "room") in pairs
    assert not pairs & {("kick", "drums"), ("tom", "drums"), ("kick", "room"), ("drums", "room")}
    # The stems that sum to the mix are the lead, the group and the return.
    mix = sf.read(folder / "mix.wav", always_2d=True)[0]
    assert grid.summed() == pytest.approx(grid.cells(mix), rel=1e-3, abs=1e-12)


def test_a_groups_pair_gives_way_to_its_tracks(tmp_path):
    folder = song(
        tmp_path,
        {"kick": hits(60, decay=6), "bass": tone(62, 1, 8)},
        groups=[{"id": "drums"}],
        tracks={"kick": {"group": "drums"}},
    )
    report, _, grid = measured(folder)
    assert grid.ranges("drums", "bass")
    assert [(p["a"], p["b"]) for p in report["overlap"]["pairs"]] == [("kick", "bass")]


def test_one_pair_by_band_and_section_through_the_command(tmp_path):
    folder = song(
        tmp_path,
        {"low": tone(80, 1, 8), "under": tone(80, 5, 8, amp=0.25, phase=1.0), "high": tone(2000, 1, 8)},
        sections=[
            {"id": "first", "at": 0, "length_beats": 16},
            {"id": "second", "at": 16, "length_beats": 16},
        ],
    )
    code, report = cli("listen", folder, "--overlap", "low", "under")
    assert code == 0, report
    assert report["kind"] == "overlap" and (report["a"], report["b"]) == ("low", "under")
    assert len(report["bands_hz"]) == 26 and report["bands_hz"][:3] == [[20, 45], [45, 89], [89, 112]]
    assert report["bands_hz"][-1] == [17800, 20000]
    whole = report["whole"]
    assert whole["fraction"][1] == pytest.approx(0.5, abs=0.02)
    # A mono sample at the center is 3 dB down in each channel.
    assert whole["a_dbfs"][1] == pytest.approx(20 * np.log10(0.3) - 6.02, abs=0.3)
    # Nothing of either sounds at 2 kHz, where the third stem is.
    assert whole["fraction"][15] is None and whole["level_difference_db"][15] is None
    assert report["sections"]["first"]["fraction"][1] == 0
    assert report["sections"]["second"]["fraction"][1] == pytest.approx(1, abs=0.03)
    (found,) = report["ranges"]
    assert found["band_hz"] == [45, 89] and found["sections"] == {"first": 0.0, "second": 1.0}
    assert Path(report["images"][0]).read_bytes().startswith(b"\x89PNG")
    assert Path(report["images"][0]).name == "overlap-low-under.png"
    assert Path(report["report_path"]).is_file()
    # A pair that never meets has no ranges, and is still a report.
    apart = overlap_of(folder, "low", "high", images=False)
    assert apart["ranges"] == [] and apart["images"] == []
    # The Rust daw passes the flag on.
    assert daw("listen", folder, "--overlap", "low", "under", "--no-images")["ranges"] == report["ranges"]
    code, error = cli("listen", folder, "--overlap", "low", "bass")
    assert code == 1 and "low, under, high" in error["error"]
    with pytest.raises(ValueError, match="two different stems"):
        overlap_of(folder, "low", "low")
    code, error = cli("listen", folder / "mix.wav", "--overlap", "low", "under")
    assert code == 0  # mix.wav beside a report is the render
    plain = tmp_path / "plain.wav"
    sf.write(plain, tone(80, 1, 2, bars=2), SR)
    code, error = cli("listen", plain, "--overlap", "low", "under")
    assert code == 1 and "stems" in error["error"]
    assert listen(plain, images=False)["overlap"] is None


def test_a_cell_is_a_sixteenth_or_the_next_note_up_that_lasts_100_ms(tmp_path):
    def grid(tempo, signature="4/4"):
        project = {"session": {"tempo": tempo, "time_signature": signature}}
        return overlap.Grid(SR * 4, SR, project, {"tracks": {}}, 0, [])

    assert grid(120).cell_beats == 0.25 and grid(150).cell_beats == 0.25
    assert grid(151).cell_beats == 0.5 and grid(301).cell_beats == 1
    # Bars follow the song's meter.
    assert list(grid(120, "3/4").bars[:14]) == [0] * 12 + [1] * 2
    assert overlap.BANDS[0] == (20.0, pytest.approx(44.67, abs=0.01))
    assert overlap.runs([False, True, True, False, True]) == [(1, 2), (4, 4)]
    assert [overlap.hz(v) for v in (44.67, 891.3, 1122.0, 17782.8)] == [45, 891, 1120, 17800]


def test_a_hit_on_the_grid_is_in_its_own_cell(tmp_path):
    project = {"session": {"tempo": TEMPO}}
    grid = overlap.Grid(4 * BEAT, SR, project, {"tracks": {}}, 0, [])
    click = np.zeros((4 * BEAT, 1))
    click[BEAT : BEAT + 48] = 0.5  # a millisecond, on the second beat
    cells = grid.cells(click).sum(axis=1)
    assert np.argmax(cells) == 4
    assert 10 * np.log10(cells[3] / cells[4]) < -14
    # A tone that fills a band reads as its mean-square power there.
    steady = 0.5 * np.sin(2 * np.pi * 1000 * np.arange(4 * BEAT) / SR)[:, None]
    cells = grid.cells(steady)
    assert 10 * np.log10(cells[4:12, 12].mean()) == pytest.approx(-9.03, abs=0.1)
