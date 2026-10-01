"""What a render promises, checked on generated songs.

Songs are generated with generated audio in every sample format the engine
reads, half of them with effects, sends, returns and automation. Each is
rendered whole, in other block sizes, as a section and as one channel, and the
results are held to each other and to the song. AAW_RENDER_SONGS sets how many.
"""

import os
import random

import numpy as np
import soundfile as sf

from agent_daw.model import digest, frame, load, project_hash, schedule, to_yaml
from agent_daw.perception import analyze
from helpers import daw, render, stem
from song_fixtures import make_mix_song, make_song


def audio(path):
    return sf.read(path, dtype="float64", always_2d=True)[0]


def unsafe(error):
    return str(error).startswith("Unsafe PCM export")


def test_generated_songs_keep_the_render_invariants(tmp_path):
    rng = random.Random(5)
    rendered = previews = summed = 0
    for i in range(int(os.environ.get("AAW_RENDER_SONGS", 12))):
        directory = tmp_path / f"song{i}"
        directory.mkdir()
        path = make_mix_song(rng, directory) if i % 2 else make_song(rng, directory)
        song = load(path)
        session = song["session"]
        try:
            full = render(path, directory / "full")
        except ValueError as error:
            # A mix at or over full scale is refused and nothing is written.
            assert unsafe(error) and not (directory / "full").exists()
            continue
        rendered += 1

        # The report describes the song and the files beside it.
        channels = [c["id"] for c in song["tracks"] + song["returns"]]
        assert list(full["tracks"]) == channels
        triggers = schedule(song)
        assert daw("schedule", path) == [t._asdict() for t in triggers]
        for track in song["tracks"]:
            hits = sum(t.track == track["id"] for t in triggers)
            assert full["tracks"][track["id"]]["events"] == hits
        assert full["project_sha256"] == project_hash(song)
        assert (directory / "full" / "song.snapshot.yaml").read_text() == to_yaml(song)
        total = frame(session["length_beats"], session["tempo"], session["sample_rate"])
        mix = audio(directory / "full" / "mix.wav")
        stems = {name: stem(directory / "full", name) for name in channels}
        assert full["mix"]["frames"] == total == len(mix)
        assert all(len(x) == total for x in stems.values())
        assert digest(directory / "full" / "mix.wav") == full["audio_sha256"]
        for name in channels:
            stem_file = directory / "full" / "stems" / f"{name}.wav"
            assert digest(stem_file) == full["tracks"][name]["audio_sha256"]
        assert np.max(np.abs(mix)) < 1

        # Stems sum to the mix when no master effect comes after them.
        assert full["stems_sum_to_mix"] == (not song["master"]["effects"])
        if full["stems_sum_to_mix"]:
            assert np.max(np.abs(mix - sum(stems.values()))) < 1e-6
            summed += 1

        # No sample depends on how the render is cut into blocks.
        blocks = render(path, directory / "blocks", rng.choice([61, 127, 1000, 9999]))
        assert blocks["audio_sha256"] == full["audio_sha256"]
        for name in channels:
            assert blocks["tracks"][name]["audio_sha256"] == full["tracks"][name]["audio_sha256"]

        # A section is those frames of the whole render, tails included.
        section = render(path, directory / "section", section=rng.choice(["a", "b"]))
        first, last = section["target"]["source_frames"]
        assert np.array_equal(audio(directory / "section" / "mix.wav"), mix[first:last])
        for name in channels:
            assert np.array_equal(stem(directory / "section", name), stems[name][first:last])

        # A track or return on its own is its stem.
        name = rng.choice(channels)
        try:
            preview = render(path, directory / "preview", track_id=name)
        except ValueError as error:
            # A stem may pass full scale where the mix, after its master, does not.
            assert unsafe(error)
        else:
            previews += 1
            assert list(preview["tracks"]) == [name]
            assert preview["tracks"][name]["audio_sha256"] == full["tracks"][name]["audio_sha256"]

        # Perception verifies the render against its report and snapshot.
        report, _ = analyze(directory / "full")
        assert report["source"]["render_id"] == full["render_id"]
        assert set(report["tracks"]) == set(channels)
    assert rendered >= 8 and previews >= 5 and summed >= 4, (rendered, previews, summed)
