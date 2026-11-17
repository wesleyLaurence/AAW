"""`daw transcribe --words`: the words a stand-in Whisper hears in a voice whose
loud stretches are known, against a generated song whose beats are known: lines
on the beat map, the stretches in doubt read again on short clips, the lines that
come back, the words kept beside the voice, and the form named by them."""

import json
import shlex
import shutil
import sys
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw import separation, words
from helpers import beat_song, chord_song, cli

STAND_IN = Path(__file__).with_name("stand_in_words.py")
RATE = 44100
BPM = 120.0
FIRST = 0.137
BEAT = 60 / BPM
TURBO, LARGE = words.FIRST, words.AGAIN


@pytest.fixture
def whisper(monkeypatch, tmp_path):
    """The stand-in, and the file a test writes what it hears to."""
    script = tmp_path / "heard.json"
    monkeypatch.setenv("AAW_WORDS", shlex.join([sys.executable, str(STAND_IN)]))
    monkeypatch.setenv("AAW_WORDS_SCRIPT", str(script))
    monkeypatch.setenv("AAW_LIBRARY", str(tmp_path / "library.sqlite"))
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    return script


@pytest.fixture(scope="module")
def made(tmp_path_factory):
    """A song of 16 bars at 120 BPM whose first downbeat is 0.137 s in."""
    folder = tmp_path_factory.mktemp("song")
    beat_song(folder / "song.wav", bpm=BPM, bars=16, first=FIRST)
    return folder / "song.wav"


def at(beat):
    """The time of a beat counted from bar 1."""
    return FIRST + beat * BEAT


def voice(path, seconds, loud):
    """A voice of `seconds`, a tone where `loud` lists (from, to) and near silence
    elsewhere."""
    t = np.arange(round(seconds * RATE)) / RATE
    x = 1e-4 * np.sin(2 * np.pi * 300 * t)
    for a, b in loud:
        inside = (t >= a) & (t < b)
        x[inside] = 0.3 * np.sin(2 * np.pi * 300 * t[inside])
    sf.write(path, x, RATE, subtype="PCM_16")
    return path


def segment(*said, p=0.95):
    """A segment of words, each (start, end, word) or (start, end, word, p)."""
    out = [[a, b, f" {w}", (rest[0] if rest else p)] for a, b, w, *rest in said]
    return {"text": "".join(w[2] for w in out).strip(), "words": out}


def spoken(start, text, each=0.25, p=0.95):
    """A segment of `text`'s words one after another from `start`."""
    return segment(*[(start + i * each, start + (i + 1) * each - 0.02, w, p) for i, w in enumerate(text.split())])


def hears(script, **models):
    script.write_text(json.dumps({TURBO if k == "turbo" else LARGE: v for k, v in models.items()}))


def calls(script):
    path = Path(f"{script}.calls")
    return [json.loads(line) for line in path.read_text().splitlines()] if path.is_file() else []


def part(made, folder, loud):
    """A song with the voice part daw separate would make of it, by its recipe: a
    tone where `loud` lists; the part's path."""
    project = folder / "project"
    (project / "samples").mkdir(parents=True)
    (project / "separated").mkdir()
    source = project / "samples" / "song.wav"
    shutil.copy(made, source)
    path = voice(project / "separated" / "0123456789ab-m-vocals.wav", sf.info(source).duration, loud)
    record = {
        "source": {"path": "samples/song.wav", "sha256": "0"},
        "parts": {"vocals": {"path": "separated/0123456789ab-m-vocals.wav", "sha256": "0"}},
    }
    (project / "separated" / "0123456789ab-m.separated.json").write_text(json.dumps(record))
    return path


def song_copy(made, folder):
    path = folder / "song.wav"
    shutil.copy(made, path)
    return path


def test_each_line_is_placed_on_the_beat_map(made, tmp_path, whisper):
    path = song_copy(made, tmp_path)
    hears(
        whisper,
        turbo=[
            spoken(at(0), "I like to welcome all"),
            spoken(at(4), "we ride at midnight."),
            spoken(at(8) + 0.03, "we ride at midnight."),
        ],
    )
    code, out = cli("transcribe", path, "--words")
    assert code == 0, out
    assert out["voice_part"] is False and out["models"]["first"]["name"] == TURBO
    assert out["tempo_bpm"] == pytest.approx(BPM, abs=0.05) and out["language"] == "en"
    assert out["word_count"] == 13 and out["sure"] == 13 and out["line_count"] == 3
    first, second, third = out["lines"]
    assert first["text"] == "I like to welcome all"
    assert (first["bar"], first["sixteenth"]) == (1, "1.1.1") and first["bars"] == [1, 1]
    assert (second["bar"], third["bar"], third["sixteenth"]) == (2, 3, "3.1.1")
    assert [w["word"] for w in first["words"]] == ["I", "like", "to", "welcome", "all"]
    # Words half a beat apart: the third is on beat 2.
    assert first["words"][1]["beat"] == 1.5 and first["words"][2]["beat"] == 2.0
    assert out["hook"] == "we ride at midnight." and out["repeats"][0]["count"] == 2
    assert out["repeats"][0]["bars"] == [2, 3]
    # No voice part beside it: the mix was read, and the lead voice only.
    assert any("whole mix" in w for w in out["warnings"]) and any("lead voice" in w for w in out["warnings"])


def test_the_words_are_kept_beside_the_voice(made, tmp_path, whisper):
    path = song_copy(made, tmp_path)
    hears(whisper, turbo=[spoken(at(0), "one two three")])
    code, out = cli("transcribe", path, "--words")
    assert code == 0 and out["kept"] is False
    assert out["words_file"] == str(tmp_path / "song.words.json")
    assert len(calls(whisper)) == 1
    code, out = cli("transcribe", path, "--words", "--near", "1", "--window", "2")
    assert code == 0 and out["kept"] is True and len(calls(whisper)) == 1
    assert out["near"]["of"] == 1 and len(out["lines"]) == 1
    code, out = cli("transcribe", path, "--words", "--again")
    assert code == 0 and out["kept"] is False and len(calls(whisper)) == 2
    code, out = cli("transcribe", path, "--words", "--language", "es")
    assert code == 0 and out["kept"] is False and calls(whisper)[-1]["language"] == "es"


def test_a_loud_stretch_with_no_words_is_read_again(made, tmp_path, whisper):
    path = part(made, tmp_path, [(1, 1.7), (6, 10)])
    hears(
        whisper,
        turbo=[spoken(1.0, "first line here"), spoken(9.0, "last line")],
        # The larger model hears the skipped words, and the first line too.
        again=[spoken(1.0, "first line here"), spoken(6.2, "the words it skipped")],
    )
    code, out = cli("transcribe", path, "--words")
    assert code == 0, out
    again = calls(whisper)[-1]
    assert again["model"] == LARGE and len(again["spans"]) == 1
    (a, b), = again["spans"]
    assert 6 <= b - a <= 10 and a <= 6.0 and b >= 9.0
    (done,) = out["read_again"]
    assert done["why"] == "loud voice with no words" and done["kept"] == "again"
    assert done["first"] == "" and done["again"] == "the words it skipped"
    texts = [line["text"] for line in out["lines"]]
    assert texts == ["first line here", "the words it skipped", "last line"]
    assert out["lines"][1]["read_again"] is True and "read_again" not in out["lines"][0]
    assert out["models"]["again"]["name"] == LARGE


def test_an_unsure_line_keeps_the_surer_reading(made, tmp_path, whisper):
    path = part(made, tmp_path, [(1, 1.8), (5, 5.8)])
    hears(
        whisper,
        turbo=[spoken(1.0, "the night is gold", p=0.4), spoken(5.0, "keep this one", p=0.5)],
        again=[spoken(1.0, "the night is cold", p=0.9), spoken(5.0, "keep that one", p=0.3)],
    )
    code, out = cli("transcribe", path, "--words")
    assert code == 0, out
    first, second = out["read_again"]
    assert first["why"].startswith("unsure") and first["kept"] == "again"
    assert second["kept"] == "first" and second["again"] == "keep that one"
    assert [line["text"] for line in out["lines"]] == ["the night is cold", "keep this one"]


def test_clips_that_overlap_hear_a_word_once(made, tmp_path, whisper):
    path = part(made, tmp_path, [(2, 16)])
    hears(whisper, turbo=[spoken(1.0, "start"), spoken(16.5, "end")], again=[spoken(3.0, " ".join(["la"] * 48))])
    code, out = cli("transcribe", path, "--words")
    assert code == 0, out
    spans = calls(whisper)[-1]["spans"]
    assert len(spans) == 2 and spans[0][1] > spans[1][0]
    assert all(b - a <= 10 for a, b in spans)
    assert out["word_count"] == 2 + 48


def test_a_word_stretched_over_a_stretch_does_not_hide_it():
    """Whisper can end a word seconds after it was sung: the stretch after it is
    still read again."""
    first = words.words_of([segment((1.0, 1.4, "the"), (1.4, 12.0, "end."))], "f")
    assert first[-1]["end"] == pytest.approx(1.4 + words.LONGEST_WORD)
    loud = np.zeros(int(14 / words.FRAME))
    (doubt,) = words.doubts(first, loud, 14.0)
    assert doubt["from"] == pytest.approx(3.4) and doubt["why"] == "loud voice with no words"


def test_once_and_a_missing_second_model(made, tmp_path, whisper, monkeypatch):
    path = part(made, tmp_path, [(1, 8)])
    hears(whisper, turbo=[spoken(1.0, "only this")], again=[spoken(4.0, "and this")])
    code, out = cli("transcribe", path, "--words", "--once")
    assert code == 0 and out["read_again"] == [] and len(calls(whisper)) == 1
    # Read once and kept: a call that wants the doubts read again hears it again.
    code, out = cli("transcribe", path, "--words")
    assert code == 0 and out["kept"] is False and out["read_again"] and calls(whisper)[-1]["model"] == LARGE
    code, out = cli("transcribe", path, "--words", "--once")
    assert code == 0 and out["kept"] is True
    n = len(calls(whisper))
    monkeypatch.setenv("AAW_WORDS_WITHOUT", LARGE)
    code, out = cli("transcribe", path, "--words", "--again")
    assert code == 0 and out["models"]["again"] is None
    assert any("not read again" in w and "--models whisper-large-v3" in w for w in out["warnings"])
    assert all(c["model"] == TURBO for c in calls(whisper)[n:])
    # Once the model is here, the kept words are read again where in doubt.
    monkeypatch.delenv("AAW_WORDS_WITHOUT")
    code, out = cli("transcribe", path, "--words")
    assert code == 0 and out["kept"] is False and calls(whisper)[-1]["model"] == LARGE


def test_a_song_is_read_by_its_voice_part(made, tmp_path, whisper):
    project = tmp_path / "project"
    (project / "samples").mkdir(parents=True)
    (project / "separated").mkdir()
    source = project / "samples" / "song.wav"
    shutil.copy(made, source)
    seconds = sf.info(source).duration
    voice(project / "separated" / "0123456789ab-m-vocals.wav", seconds, [(at(4), at(8))])
    record = {
        "source": {"path": "samples/song.wav", "sha256": "0"},
        "parts": {"vocals": {"path": "separated/0123456789ab-m-vocals.wav", "sha256": "0"}},
    }
    (project / "separated" / "0123456789ab-m.separated.json").write_text(json.dumps(record))
    hears(whisper, turbo=[spoken(at(4), "sung in bar two")])
    lyrics = tmp_path / "lyrics.txt"
    code, out = cli("transcribe", source, "--words", "--lyrics", lyrics)
    assert code == 0, out
    assert out["voice_part"] is True and out["path"].endswith("0123456789ab-m-vocals.wav")
    assert out["song"] == str(source.resolve()) and out["lines"][0]["bar"] == 2
    assert out["words_file"].endswith("separated/0123456789ab-m-vocals.words.json")
    assert not any("whole mix" in w for w in out["warnings"])
    text = lyrics.read_text()
    assert "bar   2  sung in bar two" in text and "0:02.1" in text
    # The part itself is placed on its song's beat map too.
    code, out = cli("transcribe", project / "separated" / "0123456789ab-m-vocals.wav", "--words")
    assert code == 0 and out["lines"][0]["bar"] == 2 and out["kept"] is True


@pytest.mark.parametrize(
    "args, said",
    [
        (["--words", "--line"], "--line is for notes"),
        (["--words", "--drums"], "--drums is for notes"),
        (["--words", "--midi", "x.mid"], "--midi is for notes"),
        (["--lyrics", "x.txt"], "--lyrics goes with --words"),
        (["--once", "--again"], "--once, --again go with --words"),
        (["--words", "--lyrics", "/no/such/folder/x.txt"], "there is no folder"),
    ],
)
def test_what_goes_with_words(made, tmp_path, whisper, args, said):
    code, out = cli("transcribe", song_copy(made, tmp_path), *args)
    assert code != 0 and said in out["error"], out


def test_a_failing_model_says_why(made, tmp_path, whisper):
    whisper.write_text(json.dumps({TURBO: {"fail": "out of memory"}}))
    code, out = cli("transcribe", song_copy(made, tmp_path), "--words")
    assert code != 0 and "Whisper failed: out of memory" in out["error"]


def test_without_the_environment(made, tmp_path, monkeypatch):
    monkeypatch.delenv("AAW_WORDS", raising=False)
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.setenv("AAW_LIBRARY", str(tmp_path / "library.sqlite"))
    code, out = cli("transcribe", song_copy(made, tmp_path), "--words")
    assert code != 0 and "daw separate setup" in out["error"]


def test_lines_and_what_comes_back():
    said = words.words_of(
        [
            segment((0.0, 0.3, "Hello,"), (0.3, 0.6, "there."), (0.6, 0.9, "New"), (2.0, 2.3, "after"), (2.3, 2.6, "pause")),
            segment(*[(3.0 + i * 0.4, 3.38 + i * 0.4, w) for i, w in enumerate("one two three four five six seven eight, nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen".split())]),
        ],
        "f",
    )
    texts = [words.text(line) for line in words.lines(said)]
    # A sentence's end, a pause, a new segment, a comma once long, and a line
    # longer than MOST_LINE split.
    assert texts[:3] == ["Hello, there.", "New", "after pause"]
    assert texts[3] == "one two three four five six seven eight,"
    assert all(line[-1]["end"] - line[0]["start"] <= words.MOST_LINE for line in words.lines(said))
    rows = [{"text": t, "seconds": float(i), "bar": i + 1} for i, t in enumerate(
        ["It's my town", "something else", "its my town!", "Its My Town", "ya", "ya"]
    )]
    (group,) = words.repeats(rows)
    assert group["count"] == 3 and group["bars"] == [1, 3, 4] and group["text"] == "It's my town"


def test_a_cached_model_file_is_linked_not_downloaded(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.setenv("HF_HUB_CACHE", str(tmp_path / "hub"))
    body = b"weights" * 100
    snapshot = tmp_path / "hub" / "models--org--model" / "snapshots" / "abc"
    snapshot.mkdir(parents=True)
    (snapshot / "weights.npz").write_bytes(body)
    import hashlib

    spec = {
        "name": "model/weights.npz",
        "url": "https://example.invalid/weights.npz",
        "huggingface": {"repo": "org/model", "revision": "abc", "file": "weights.npz"},
        "bytes": len(body),
        "sha256": hashlib.sha256(body).hexdigest(),
    }
    fetched = []

    def download(url, target, expected=None):
        fetched.append(url)
        raise ValueError("offline")

    monkeypatch.setattr(separation, "download", download)
    separation.fetch(spec)
    kept = separation.models_dir() / "model" / "weights.npz"
    assert kept.read_bytes() == body and kept.stat().st_ino == (snapshot / "weights.npz").stat().st_ino
    assert fetched == []
    # Another hash is not taken from the cache: it is downloaded.
    with pytest.raises(ValueError):
        separation.fetch({**spec, "sha256": "0" * 64})
    assert fetched == [spec["url"]]


def test_the_whisper_models_are_in_the_manifest():
    names = {m["name"]: m for m in separation.analysis()}
    for name in (TURBO, LARGE):
        model = names[name]
        assert model["package"] == "mlx-whisper==0.4.3" and model["license"].startswith("MIT")
        assert all(f["name"].startswith(f"{name}/") and len(f["sha256"]) == 64 for f in model["files"])
    assert "mlx-whisper==0.4.3" in separation.manifest()["runner"]["also"]


# --- the form named by the words ----------------------------------------------------


VERSE = "cold rain falls on empty streets".split()  # each bar's own: cold3 rain3 ...


def one_beat_song(path):
    """32 bars of one four-chord loop at 120 BPM: verse and hook are the same music."""
    loop = [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]]
    x = chord_song(None, loop * 8, bpm=BPM, seed=1)
    x = np.concatenate([x, np.zeros(2 * RATE)])
    sf.write(path, 0.8 * x / np.abs(x).max(), RATE, subtype="PCM_16")
    return path


def test_the_words_name_the_chorus(tmp_path, whisper):
    project = tmp_path / "project"
    (project / "samples").mkdir(parents=True)
    (project / "separated").mkdir()
    source = one_beat_song(project / "samples" / "song.wav")
    seconds = sf.info(source).duration
    bar = 4 * BEAT
    voice(project / "separated" / "0123456789ab-m-vocals.wav", seconds, [(0, 32 * bar)])
    (project / "separated" / "0123456789ab-m.separated.json").write_text(
        json.dumps({"source": {"path": "samples/song.wav"}, "parts": {"vocals": {"path": "separated/0123456789ab-m-vocals.wav"}}})
    )
    # Verses of new words in bars 1-8 and 17-24, the same hook in 9-16 and 25-32.
    said = []
    for b in range(32):
        hook = 8 <= b < 16 or 24 <= b
        line = "nothing in the world can hold us" if hook else " ".join(f"{w}{b}" for w in VERSE)
        said.append(spoken(b * bar + 0.1, line, each=0.2))
    hears(whisper, turbo=said)
    code, out = cli("transcribe", source, "--words")
    assert code == 0 and out["hook"] == "nothing in the world can hold us", out
    code, out = cli("samples", "form", source)
    assert code == 0, out
    assert out["named_by"].startswith("the words") and out["words_file"].endswith("0123456789ab-m-vocals.words.json")
    # Sections start where the hook comes in and ends, whatever bar the map counts.
    def at_time(t):
        return next((s for s in out["sections"] if abs(s["seconds"] - t) < BEAT), None)

    starts = [at_time(b * bar) for b in (8, 16, 24)]
    assert all(starts), [(s["bars"], s["seconds"]) for s in out["sections"]]
    assert starts[0]["starts_by"] == "the hook coming in" and starts[1]["starts_by"] == "the hook ending"
    hooks = [s for s in out["sections"] if s["hook"] == 1]
    verses = [s for s in out["sections"] if s["hook"] == 0 and s["count"] >= 4]
    assert {s["name"] for s in hooks} == {"chorus"} and {s["name"] for s in verses} <= {"verse", "intro"}
    assert len({s["label"][0] for s in hooks}) == 1 and not {s["label"][0] for s in hooks} & {s["label"][0] for s in verses}
    assert hooks[0]["same_words"] >= 0.9 and "nothing in the world" in hooks[0]["words"]


def test_the_form_without_words_says_how_to_get_them(tmp_path, whisper):
    project = tmp_path / "project"
    (project / "samples").mkdir(parents=True)
    (project / "separated").mkdir()
    source = one_beat_song(project / "samples" / "song.wav")
    voice(project / "separated" / "0123456789ab-m-vocals.wav", sf.info(source).duration, [(0, 60)])
    (project / "separated" / "0123456789ab-m.separated.json").write_text(
        json.dumps({"source": {"path": "samples/song.wav"}, "parts": {"vocals": {"path": "separated/0123456789ab-m-vocals.wav"}}})
    )
    code, out = cli("samples", "form", source)
    assert code == 0, out
    assert "daw transcribe song.wav --words" in out["named_by"] and "words_file" not in out
    assert all("hook" not in s for s in out["sections"])
