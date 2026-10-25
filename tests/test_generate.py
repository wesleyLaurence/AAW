"""Generated audio with `daw generate sound` and `daw keys`, against a stand-in
for the ElevenLabs API on this machine: the sound saved in the generated folder
with its recipe and imported into the project, found by the library, played
by a pad and rendered with no network; the estimate, the agent's budget and
the person's requests; the edit undone under a host; the refusals; an MP3
plan decoded by import; and the key in no output and no file. A song from a
plan made by `daw generate plan`, returned as a multipart reply of a drum
track at a known tempo, imported with its beat map and cut on its bars.

No request leaves the machine: AAW_GENERATE_URL names the stand-in, and the
key comes from AAW_TEST_GENERATE_KEY, which is read only for a loopback
address. The Keychain is never touched.
"""

import json
import os
import shutil
import subprocess
import tempfile
import threading
import time
from collections import deque
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from agent_daw.model import load
from helpers import beat_song, cli, daw

KEY = "sk_test_key_that_is_never_shown_42"
RATE = 44100
BOUNDARY = "aaw-test-boundary-3c9e"
SONG_BPM = 100.0
SONG_FIRST = 0.05


def drums(seconds, folder):
    """A drum track at SONG_BPM, its first downbeat SONG_FIRST in, as the
    headerless stereo 16-bit PCM at 48 kHz the service sends for music."""
    beat_song(folder / "drums.wav", SONG_BPM, int(seconds / (240 / SONG_BPM)) + 1, SONG_FIRST, seed=3)
    x, rate = sf.read(folder / "drums.wav")
    if x.ndim > 1:
        x = x.mean(axis=1)
    frames = round(seconds * 48000)
    y = np.interp(np.arange(frames) / 48000, np.arange(len(x)) / rate, x, right=0.0)
    stereo = np.stack([y, 0.9 * y], axis=1)
    return (np.clip(stereo, -1, 1) * 32767).astype("<i2").tobytes()


def multipart(meta, audio, audio_type):
    head = f"--{BOUNDARY}\r\nContent-Type: application/json\r\n\r\n{json.dumps(meta)}\r\n--{BOUNDARY}\r\nContent-Type: {audio_type}\r\n\r\n"
    return head.encode() + audio + f"\r\n--{BOUNDARY}--\r\n".encode()


def quarters(seconds, names=("Intro", "Verse", "Chorus", "Outro")):
    """A plan of four chunks, each a quarter of `seconds`."""
    ms = round(seconds * 1000 / 4)
    return {"chunks": [{"text": f"[{n}]\n{{instrumental}}", "duration_ms": ms, "positive_styles": ["dusty soul", "Rhodes"], "negative_styles": []} for n in names]}


def pcm(seconds):
    """A 440 Hz sine of `seconds` as headerless 16-bit PCM, as the service sends it."""
    t = np.arange(round(seconds * RATE)) / RATE
    return (np.sin(2 * np.pi * 440 * t) * 0.5 * 32767).astype("<i2").tobytes()


class StandIn:
    """The service on this machine: a subscription of `tier`, a sine for every
    sound asked for, and whatever `script` says next in place of either."""

    def __init__(self, tier="pro"):
        self.tier = tier
        self.delay = 0
        self.seen = []
        self.script = deque()
        self.folder = Path(tempfile.mkdtemp(prefix="stand-in"))
        self.mp3 = None
        stand_in = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def _reply(self, status, body, headers=()):
                self.send_response(status)
                self.send_header("Content-Length", str(len(body)))
                for k, v in headers:
                    self.send_header(k, v)
                self.end_headers()
                self.wfile.write(body)

            def _handle(self, method):
                path, _, query = self.path.partition("?")
                length = int(self.headers.get("Content-Length") or 0)
                raw = self.rfile.read(length) if length else b""
                try:
                    body = json.loads(raw) if raw else None
                except ValueError:
                    body = raw.decode(errors="replace")
                stand_in.seen.append({"method": method, "path": path, "query": query, "headers": dict(self.headers), "body": body})
                if self.headers.get("xi-api-key") != KEY:
                    return self._reply(401, b'{"detail": {"status": "invalid_api_key", "message": "Invalid API key"}}')
                if stand_in.script:
                    status, payload, headers = stand_in.script.popleft()
                    return self._reply(status, payload if isinstance(payload, bytes) else json.dumps(payload).encode(), headers)
                if method == "GET" and path == "/v1/user/subscription":
                    sub = {"tier": stand_in.tier, "character_count": 500, "character_limit": 10000, "next_character_count_reset_unix": 1761955200, "status": "active"}
                    return self._reply(200, json.dumps(sub).encode())
                if method == "POST" and path == "/v1/music/plan":
                    seconds = ((body or {}).get("music_length_ms") or 38400) / 1000
                    return self._reply(200, json.dumps(quarters(seconds)).encode())
                if method == "POST" and path == "/v1/music/detailed":
                    plan = (body or {}).get("composition_plan") or quarters(((body or {}).get("music_length_ms") or 9600) / 1000)
                    seconds = sum(c["duration_ms"] for c in plan["chunks"]) / 1000
                    meta = {"composition_plan": plan, "song_metadata": {"title": "Dusty Soul Ballad", "description": "Slow", "genres": ["soul"], "languages": ["en"], "is_explicit": False}}
                    if "pcm_48000" in query:
                        audio, kind = drums(seconds, stand_in.folder), "audio/pcm"
                    else:
                        audio, kind = stand_in.mp3, "audio/mpeg"
                    headers = [("request-id", f"req-{len(stand_in.seen)}"), ("song-id", "song-xyz"), ("content-type", f"multipart/mixed; boundary={BOUNDARY}")]
                    return self._reply(200, multipart(meta, audio, kind), headers)
                if method == "POST" and path == "/v1/sound-generation":
                    time.sleep(stand_in.delay)
                    seconds = (body or {}).get("duration_seconds") or 2
                    headers = [("request-id", f"req-{len(stand_in.seen)}"), ("character-cost", str(round(seconds * 40))), ("content-type", "audio/pcm")]
                    return self._reply(200, pcm(seconds), headers)
                return self._reply(404, b'{"detail": "not here"}')

            def do_GET(self):
                self._handle("GET")

            def do_POST(self):
                self._handle("POST")

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def stop(self):
        self.server.shutdown()
        self.server.server_close()

    def sounds(self):
        return [s for s in self.seen if s["path"] == "/v1/sound-generation"]

    def songs(self):
        return [s for s in self.seen if s["path"] == "/v1/music/detailed"]


@pytest.fixture
def stand_in(tmp_path, monkeypatch):
    """The stand-in service, a data folder and a workspace of this test's own, and the test key."""
    service = StandIn()
    monkeypatch.setenv("AAW_GENERATE_URL", service.url)
    monkeypatch.setenv("AAW_TEST_GENERATE_KEY", KEY)
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.setenv("AAW_WORKSPACE", str(tmp_path / "workspace"))
    yield service
    service.stop()
    shutil.rmtree(service.folder, ignore_errors=True)


@pytest.fixture
def song(tmp_path, stand_in):
    """A project at 120 BPM, with the plan's tier already read once."""
    folder = tmp_path / "project"
    daw("init", folder, "--tempo", 120)
    assert daw("keys", "status")["tier"] == "pro"
    stand_in.seen.clear()
    return folder / "song.yaml"


def run(*args, stdin=None):
    """`daw` as a process: its status, stdout and stderr, for what is printed."""
    out = subprocess.run([os.environ["AAW_DAW"], *map(str, args)], capture_output=True, text=True, input=stdin)
    return out.returncode, out.stdout, out.stderr


def failure(*args):
    code, out, err = run(*args)
    assert code == 1, out
    return json.loads(err.strip().splitlines()[-1])["error"]


@contextmanager
def hosted(project, monkeypatch):
    """A session host for the project, so edits are undoable; closed at the end."""
    with tempfile.TemporaryDirectory(prefix="aaw", dir="/tmp") as registry:
        monkeypatch.setenv("AAW_HOST_DIR", registry)
        host = subprocess.Popen([os.environ["AAW_DAW"], "host", str(project)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.time() + 30
            while not daw("status", project).get("host"):
                assert host.poll() is None, host.stderr.read()
                assert time.time() < deadline, "the host did not start"
                time.sleep(0.05)
            yield
        finally:
            daw("close", project)
            host.wait(timeout=10)


def everything_under(root):
    text = []
    for path in Path(root).rglob("*"):
        if path.is_file():
            text.append(path.read_bytes().decode(errors="replace"))
    return "\n".join(text)


def test_a_sound_is_generated_imported_found_by_the_library_and_played(tmp_path, stand_in, song):
    generated = tmp_path / "workspace" / "library" / "generated"
    code, out, err = run("generate", "sound", song, "vinyl crackle, warm", "--bars", 2, "--loop", "--id", "crackle")
    assert code == 0, err
    made = json.loads(out)
    assert made["sample_id"] == "crackle"
    assert made["seconds"] == 4.0 and made["sample_rate"] == RATE and made["channels"] == 1
    assert made["format"] == "pcm_44100" and made["tier"] == "pro"
    assert made["bpm"] == 120.0 and made["bars"] == 2.0 and made["loop"] is True
    assert made["text_sent"] == "vinyl crackle, warm at 120 BPM"
    assert made["cost_credits"] == 160 and made["request_id"] == "req-1"
    assert made["origin"] == "agent" and made["placed"] is False
    assert made["budget"]["spent_today"] == {"agent": 4.0, "user": 0.0} and made["budget"]["left_today"] == 116.0
    assert "notes" not in made, made["notes"]
    # The request as the service documents it; the tier was read once before.
    assert [s["path"] for s in stand_in.seen] == ["/v1/sound-generation"]
    sent = stand_in.sounds()[0]
    assert sent["query"] == "output_format=pcm_44100"
    assert sent["body"] == {"text": "vinyl crackle, warm at 120 BPM", "model_id": "eleven_text_to_sound_v2", "duration_seconds": 4.0, "loop": True}
    # The project's copy plays as a WAV, with the recipe beside it, and the song has it.
    copy = song.parent / made["path"]
    assert copy.parent == song.parent / "samples" and copy.name.endswith("-crackle.wav")
    info = sf.info(copy)
    assert (info.frames, info.samplerate, info.channels) == (4 * RATE, RATE, 1)
    assert load(song)["samples"]["crackle"]["path"] == made["path"]
    recipe = json.loads((song.parent / made["recipe"]).read_text())
    assert recipe["prompt"] == "vinyl crackle, warm" and recipe["parameters"]["loop"] is True
    assert recipe["file"] == copy.name and recipe["generated_file"] == made["generated_file"]
    # The same sound is in the generated folder with its recipe, and the library found it.
    assert Path(made["generated_file"]).parent == generated and Path(made["generated_file"]).name.endswith("-crackle.wav")
    assert Path(made["generated_recipe"]).read_text() and Path(made["generated_recipe"]).name.endswith("-crackle.generated.json")
    code, folders = cli("samples", "folders", "list")
    assert code == 0 and str(generated.resolve()) in [f["path"] for f in folders]
    code, found = cli("samples", "search", "crackle")
    assert code == 0 and any(Path(r["path"]) == Path(made["generated_file"]).resolve() for r in found), found
    # Listed by the project, newest first, with the sample that plays it.
    listed = daw("generate", "list", song)["generated"]
    assert [(r["sample_id"], r["prompt"], r["seconds"]) for r in listed] == [("crackle", "vinyl crackle, warm", 4.0)]
    # A pad plays it and the song renders with nothing asked of the service.
    daw("track", "add", song, "tex")
    daw("pad", "add", song, "tex", "c", "--sample", "crackle")
    daw("pattern", "add", song, "p", "--length-beats", 4)
    daw("pattern", "steps", song, "p", "c", "x...............")
    daw("clip", "add", song, "tex", "p")
    before = len(stand_in.seen)
    rendered = daw("render", song)
    assert rendered["mix"]["peak_dbfs"] > -20
    assert len(stand_in.seen) == before
    # The key is in no reply and no file.
    assert KEY not in out + err
    assert KEY not in everything_under(tmp_path / "data") + everything_under(tmp_path / "workspace") + everything_under(song.parent)
    # A second sound without an ID is named by its words and does not take the first's name.
    again = daw("generate", "sound", song, "vinyl crackle", "--seconds", 1)
    assert again["sample_id"] == "vinyl-crackle" and again["seconds"] == 1.0
    code, found = cli("samples", "search", "crackle")
    assert code == 0 and len(found) == 2, found
    assert [r["sample_id"] for r in daw("generate", "list", song)["generated"]] == ["vinyl-crackle", "crackle"]


def test_an_estimate_sends_nothing_and_the_budget_holds_the_agent_and_not_the_person(stand_in, song):
    estimate = daw("generate", "sound", song, "a riser", "--seconds", 4, "--estimate")
    assert estimate["sent"] is False and estimate["credits"] == 160 and estimate["seconds"] == 4.0
    assert estimate["allowed"] is True and estimate["key"] is True and estimate["sample_id"] == "a-riser"
    assert estimate["budget"]["agent_seconds_per_day"] == 120
    assert stand_in.seen == []
    # No length: the most a sound can be is assumed.
    assumed = daw("generate", "sound", song, "a riser", "--estimate")
    assert assumed["seconds"] == 30 and assumed["seconds_assumed"] is True and assumed["credits"] == 1200
    # The budget, held before anything is sent.
    assert daw("keys", "budget", "--agent-seconds", 3)["budget"]["agent_seconds_per_day"] == 3
    message = failure("generate", "sound", song, "a riser", "--seconds", 4)
    assert "budget" in message and "0.0 s of 3 s" in message and "--agent-seconds" in message
    assert stand_in.sounds() == []
    assert daw("generate", "sound", song, "a riser", "--seconds", 4, "--estimate")["allowed"] is False
    # The person is not counted.
    made = daw("--origin", "user", "generate", "sound", song, "a riser", "--seconds", 4)
    assert made["origin"] == "user" and made["budget"]["spent_today"] == {"agent": 0.0, "user": 4.0}
    assert json.loads((song.parent / made["recipe"]).read_text())["origin"] == "user"
    # The agent spends what it has, then is refused.
    made = daw("generate", "sound", song, "a riser", "--seconds", 2, "--id", "riser2")
    assert made["budget"]["spent_today"]["agent"] == 2.0 and made["budget"]["left_today"] == 1.0
    message = failure("generate", "sound", song, "a riser", "--seconds", 2, "--id", "riser3")
    assert "2.0 s of 3 s" in message
    status = daw("keys", "status")
    assert status["key"] is True and status["tier"] == "pro"
    assert status["budget"]["spent_today"] == {"agent": 2.0, "user": 4.0}
    assert status["subscription"]["credits_left"] == 9500 and status["subscription"]["credits_reset"] == "2025-11-01T00:00:00Z"
    assert daw("keys", "budget")["budget"]["agent_seconds_per_day"] == 3
    assert "must be 0 to 86400" in failure("keys", "budget", "--agent-seconds", -1)
    # A budget of 0 asks every time.
    daw("keys", "budget", "--agent-seconds", 0)
    assert "0 s" in failure("generate", "sound", song, "a riser", "--seconds", 1, "--id", "riser4")


def test_bars_and_fields_are_checked_before_anything_is_sent(stand_in, song):
    assert "at most 30" in failure("generate", "sound", song, "a loop", "--bars", 16)
    assert "not both" in failure("generate", "sound", song, "a loop", "--bars", 2, "--seconds", 4)
    assert "0.5 to 30" in failure("generate", "sound", song, "a loop", "--seconds", 31)
    assert "--influence" in failure("generate", "sound", song, "a loop", "--influence", 2)
    daw("set", song, "session.time_signature", "3/4")
    waltz = daw("generate", "sound", song, "a waltz loop", "--bars", 2, "--estimate")
    assert waltz["seconds"] == 3.0 and waltz["text"] == "a waltz loop at 120 BPM"
    assert stand_in.seen == []
    # A sample's name is not taken twice.
    daw("generate", "sound", song, "a hit", "--seconds", 1, "--id", "hit")
    assert "already exists" in failure("generate", "sound", song, "a hit", "--seconds", 1, "--id", "hit")


def test_a_hosted_project_takes_the_sound_as_one_undoable_edit(stand_in, song, monkeypatch):
    with hosted(song, monkeypatch):
        made = daw("generate", "sound", song, "a whoosh", "--seconds", 1, "--id", "whoosh")
        assert made["revision"] >= 1
        status = daw("status", song)
        assert status["undo"] == {"label": "Generate whoosh", "origin": "agent"}
        assert daw("get", song, "samples.whoosh")["path"] == made["path"]
        daw("undo", song)
        assert "whoosh" not in daw("get", song, "samples")
    assert (song.parent / made["path"]).is_file() and Path(made["generated_file"]).is_file()
    assert "whoosh" not in load(song)["samples"]


def test_without_a_key_the_command_says_where_to_enter_one(stand_in, song, monkeypatch):
    monkeypatch.delenv("AAW_TEST_GENERATE_KEY")
    message = failure("generate", "sound", song, "a whoosh", "--seconds", 1)
    assert message.startswith("No ElevenLabs key is set: enter one with `daw keys set elevenlabs`")
    assert stand_in.seen == []
    status = daw("keys", "status")
    assert status["key"] is False and "daw keys set elevenlabs" in status["hint"]
    assert daw("generate", "sound", song, "a whoosh", "--seconds", 1, "--estimate")["key"] is False
    # A stand-in's key is the environment's; the Keychain is never written for one.
    code, out, err = run("keys", "set", "elevenlabs", stdin="sk_typed\n")
    assert code == 1 and "AAW_TEST_GENERATE_KEY" in err and "sk_typed" not in out + err
    assert "Unknown service" in failure("keys", "set", "openai")


def test_the_refusals_are_the_services_and_a_suggested_prompt_is_text(stand_in, song):
    stand_in.script.append((401, {"detail": {"status": "invalid_api_key", "message": "Invalid API key"}}, []))
    message = failure("generate", "sound", song, "a whoosh", "--seconds", 1)
    assert "wrong or expired" in message and "daw keys set elevenlabs" in message
    stand_in.script.append((402, {"detail": {"status": "insufficient_credits", "message": "Out of credits"}}, []))
    assert "No ElevenLabs credits are left" in failure("generate", "sound", song, "a whoosh", "--seconds", 1)
    stand_in.script.append((400, {"detail": {"status": "bad_prompt", "message": "No artists", "suggested_prompt": "a dusty soul ballad"}}, []))
    message = failure("generate", "sound", song, "a song like Marvin Gaye", "--seconds", 1)
    assert "named something it may not" in message and '"a dusty soul ballad"' in message and "not to send unasked" in message
    # The subscription's refusal is reported by keys status and spends nothing.
    stand_in.script.append((401, {"detail": {"status": "invalid_api_key", "message": "Invalid API key"}}, []))
    status = daw("keys", "status")
    assert status["key"] is True and "wrong or expired" in status["problem"]
    assert "whoosh" not in load(song)["samples"]
    assert daw("keys", "status")["budget"]["spent_today"]["agent"] == 0.0


def mp3_bytes(path):
    """A second of sine as MP3, by whichever encoder is here, or None."""
    if shutil.which("ffmpeg"):
        out = subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=330:duration=1", "-ac", "1", "-ar", str(RATE), "-b:a", "128k", str(path)],
            capture_output=True,
        )
        if out.returncode == 0:
            return path.read_bytes()
    return None


def test_a_plan_without_pcm_gets_an_mp3_that_import_decodes(tmp_path, stand_in, song, monkeypatch):
    mp3 = mp3_bytes(tmp_path / "tone.mp3")
    if mp3 is None or not (shutil.which("afconvert") or shutil.which("ffmpeg")):
        pytest.skip("no MP3 encoder or decoder here")
    stand_in.tier = "free"
    assert daw("keys", "status")["tier"] == "free"
    stand_in.script.append((200, mp3, [("request-id", "req-mp3"), ("character-cost", "40")]))
    made = daw("generate", "sound", song, "a tone", "--seconds", 1, "--id", "tone")
    assert made["format"] == "mp3_44100_128" and made["tier"] == "free"
    assert stand_in.sounds()[0]["query"] == "output_format=mp3_44100_128"
    assert Path(made["generated_file"]).suffix == ".mp3" and Path(made["generated_file"]).read_bytes() == mp3
    assert made["path"].endswith(".wav") and (song.parent / made["path"]).is_file()
    assert 0.9 < made["seconds"] < 1.2
    assert load(song)["samples"]["tone"]["source_sha256"]
    recipe = json.loads((song.parent / made["recipe"]).read_text())
    assert recipe["parameters"]["output_format"] == "mp3_44100_128" and recipe["file"] == Path(made["path"]).name
    assert daw("generate", "list", song)["generated"][0]["sample_id"] == "tone"
    # Taken into the song as a PCM sound is: the person's edit, named for the
    # sound, and the agent's budget not counted.
    stand_in.script.append((200, mp3, [("request-id", "req-mp3-2")]))
    with hosted(song, monkeypatch):
        made = daw("--origin", "user", "generate", "sound", song, "a tone", "--seconds", 1, "--id", "tone2")
        assert made["revision"] >= 1 and made["origin"] == "user"
        assert daw("status", song)["undo"] == {"label": "Generate tone2", "origin": "user"}
    assert made["budget"]["spent_today"]["agent"] == 1.0 and 0.9 < made["budget"]["spent_today"]["user"] < 1.2


def test_describe_and_help_say_what_the_agent_needs():
    topic = daw("describe", "generate")["semantics"]
    assert set(topic) >= {"command", "music", "plan", "good_for", "result", "files", "recipe", "key", "budget", "formats", "errors", "terms", "limits"}
    assert "--plan FILE" in topic["music"] and "chunks" in topic["plan"] and "beat_map" in topic["result"]
    assert "daw keys set elevenlabs" in topic["key"] and "never the product" in topic["good_for"]
    assert "generate" in daw("describe")["topics"]
    code, out, _ = run("generate", "--help")
    assert code == 0 and all(word in out for word in ["sound", "music", "plan", "list"])
    code, out, _ = run("generate", "music", "--help")
    assert code == 0 and all(flag in out for flag in ["--plan", "--seconds", "--instrumental", "--seed", "--model", "--estimate"])
    code, out, _ = run("generate", "sound", "--help")
    assert code == 0 and "--bars" in out and "--estimate" in out and "--loop" in out
    code, out, _ = run("keys", "--help")
    assert code == 0 and all(word in out for word in ["set", "status", "remove", "budget"])


def test_the_keychain_key_is_sent_to_elevenlabs_only(stand_in, song, monkeypatch):
    # The stand-in, named so the loopback check does not take it as this
    # machine: the Keychain is asked for nothing and the stand-in hears nothing.
    port = stand_in.url.rsplit(":", 1)[1]
    monkeypatch.setenv("AAW_GENERATE_URL", f"http://127.1:{port}")
    message = failure("generate", "sound", song, "a whoosh", "--seconds", 1)
    assert "neither ElevenLabs" in message and "nothing was sent" in message
    status = daw("keys", "status")
    assert status["key"] is None and "neither ElevenLabs" in status["problem"]
    code, out, err = run("keys", "set", "elevenlabs", stdin="sk_typed\n")
    assert code == 1 and "neither ElevenLabs" in err and "sk_typed" not in out + err
    assert stand_in.seen == []
    assert status["budget"]["spent_today"] == {"agent": 0.0, "user": 0.0}


def test_a_sound_that_could_not_be_saved_is_still_counted(tmp_path, stand_in, song):
    # The generated folder cannot be made: its parent is a file.
    (tmp_path / "workspace").mkdir(exist_ok=True)
    (tmp_path / "workspace" / "library").write_text("in the way")
    failure("generate", "sound", song, "a whoosh", "--seconds", 2)
    assert len(stand_in.sounds()) == 1
    assert daw("keys", "status")["budget"]["spent_today"]["agent"] == 2.0


def test_requests_at_once_cannot_spend_past_the_budget(stand_in, song):
    daw("keys", "budget", "--agent-seconds", 5)
    stand_in.delay = 0.5
    runs = [
        subprocess.Popen([os.environ["AAW_DAW"], "generate", "sound", str(song), "a hit", "--seconds", "2", "--id", f"hit{n}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for n in range(4)
    ]
    codes = [r.wait(timeout=60) for r in runs]
    for r in runs:
        r.stdout.close()
        r.stderr.close()
    assert sorted(codes) == [0, 0, 1, 1]
    assert len(stand_in.sounds()) == 2
    assert daw("keys", "status")["budget"]["spent_today"]["agent"] == 4.0


def test_a_song_from_a_plan_is_imported_with_its_beat_map_and_cut_on_its_bars(tmp_path, stand_in, song):
    # A plan, at no cost, written to a file to edit.
    plan_file = tmp_path / "plan.json"
    code, out, err = run("generate", "plan", "dusty soul ballad, Rhodes, 100 bpm", "--seconds", 38.4, "--out", plan_file)
    assert code == 0, err
    planned = json.loads(out)
    assert planned["seconds"] == 38.4 and planned["model_id"] == "music_v2_5" and "problem" not in planned
    assert [s["name"] for s in planned["sections"]] == ["Intro", "Verse", "Chorus", "Outro"]
    assert planned["sections"][2]["start_seconds"] == 19.2
    assert stand_in.seen[-1]["body"] == {"prompt": "dusty soul ballad, Rhodes, 100 bpm", "model_id": "music_v2_5", "music_length_ms": 38400}
    assert json.loads(plan_file.read_text()) == planned["plan"]
    assert daw("keys", "status")["budget"]["spent_today"] == {"agent": 0.0, "user": 0.0}

    # The song from the plan, edited.
    plan = json.loads(plan_file.read_text())
    plan["chunks"][1]["text"] = "[Verse]\nWe kept the radio low"
    plan_file.write_text(json.dumps(plan))
    code, out, err = run("generate", "music", song, "--plan", plan_file, "--seed", 7)
    assert code == 0, err
    made = json.loads(out)
    sent = stand_in.songs()[0]
    assert sent["query"] == "output_format=pcm_48000"
    assert sent["body"] == {"composition_plan": plan, "seed": 7, "model_id": "music_v2_5", "store_for_inpainting": True}
    assert made["sample_id"] == "dusty-soul-ballad" and made["title"] == "Dusty Soul Ballad"
    assert made["song_id"] == "song-xyz" and made["format"] == "pcm_48000"
    assert (made["sample_rate"], made["channels"], made["seconds"]) == (48000, 2, 38.4)
    assert made["budget"]["spent_today"]["agent"] == 38.4 and made["placed"] is False
    assert "notes" not in made, made["notes"]
    copy = song.parent / made["path"]
    info = sf.info(copy)
    assert (info.samplerate, info.channels) == (48000, 2) and abs(info.duration - 38.4) < 0.001
    assert load(song)["samples"]["dusty-soul-ballad"]["path"] == made["path"]
    recipe = json.loads((song.parent / made["recipe"]).read_text())
    assert recipe["kind"] == "music" and recipe["plan"] == plan and recipe["song_id"] == "song-xyz"
    assert recipe["parameters"]["seed"] == 7 and recipe["genres"] == ["soul"]

    # Its beat map: the drums' tempo, every bar, and each section on its bar.
    beat_map = made["beat_map"]
    assert beat_map["tempo"]["bpm"] == pytest.approx(SONG_BPM, abs=0.05)
    bars = np.array(beat_map["bars"])
    assert bars[0] == pytest.approx(SONG_FIRST, abs=0.002)
    assert np.allclose(np.diff(bars), 240 / SONG_BPM, atol=0.002)
    assert [(s["name"], s["bar"]) for s in made["sections"]] == [("Intro", 1), ("Verse", 5), ("Chorus", 9), ("Outro", 13)]
    assert made["sections"][2]["bar_seconds"] == pytest.approx(19.25, abs=0.002)

    # Listed with its title; a section cut on its bars plays at the song's tempo.
    listed = daw("generate", "list", song)["generated"][0]
    assert (listed["sample_id"], listed["kind"], listed["title"], listed["song_id"]) == ("dusty-soul-ballad", "music", "Dusty Soul Ballad", "song-xyz")
    daw("track", "add", song, "chop")
    chorus = made["sections"][2]
    daw("audio", "add", song, "chop", made["sample_id"], "--at", 0, "--source-start-seconds", chorus["bar_seconds"],
        "--source-end-seconds", bars[12], "--source-bpm", beat_map["tempo"]["bpm"], "--stretch", "preserve_pitch")
    before = len(stand_in.seen)
    assert daw("render", song)["mix"]["peak_dbfs"] > -30
    assert len(stand_in.seen) == before
    assert KEY not in out + err
    assert KEY not in everything_under(tmp_path / "data") + everything_under(tmp_path / "workspace") + everything_under(song.parent)


def test_a_song_is_checked_estimated_and_held_against_the_budget(tmp_path, stand_in, song, monkeypatch):
    estimate = daw("generate", "music", song, "dusty soul", "--seconds", 60, "--estimate")
    assert (estimate["kind"], estimate["seconds"], estimate["usd"], estimate["allowed"]) == ("music", 60.0, 0.15, True)
    assert estimate["format"] == "pcm_48000" and estimate["sent"] is False and estimate["sample_id"] == "dusty-soul"
    assumed = daw("generate", "music", song, "dusty soul", "--estimate")
    assert assumed["seconds"] == 300 and assumed["seconds_assumed"] is True and assumed["allowed"] is False
    # With no length the model's is held at five minutes, past the agent's day.
    message = failure("generate", "music", song, "dusty soul")
    assert "budget" in message and "give --seconds" in message
    # Checked before anything is sent.
    bad = tmp_path / "bad.json"
    bad.write_text(json.dumps({"chunks": [{"text": "[Intro]", "duration_ms": 1000, "positive_styles": ["soul"]}]}))
    assert "chunk 1 is 1000 ms" in failure("generate", "music", song, "--plan", bad)
    good = tmp_path / "good.json"
    good.write_text(json.dumps(quarters(12)))
    assert "not both" in failure("generate", "music", song, "soul", "--plan", good)
    assert "--seconds is for a prompt" in failure("generate", "music", song, "--plan", good, "--seconds", 12)
    assert "--seed is for a plan" in failure("generate", "music", song, "soul", "--seconds", 12, "--seed", 1)
    assert "3 to 300" in failure("generate", "music", song, "soul", "--seconds", 400)
    assert "--model must be" in failure("generate", "music", song, "soul", "--seconds", 12, "--model", "music_v9")
    assert "music_v1 takes sections" in failure("generate", "music", song, "--plan", good, "--model", "music_v1")
    assert stand_in.songs() == []
    # A prompt with a length, as the person, under a host: one undoable edit.
    with hosted(song, monkeypatch):
        made = daw("--origin", "user", "generate", "music", song, "dusty soul", "--seconds", 9.6, "--instrumental", "--id", "soul")
        assert made["revision"] >= 1 and made["origin"] == "user" and made["seconds"] == 9.6
        assert daw("status", song)["undo"] == {"label": "Generate soul", "origin": "user"}
        daw("undo", song)
        assert "soul" not in daw("get", song, "samples")
    assert stand_in.songs()[0]["body"] == {"prompt": "dusty soul", "music_length_ms": 9600, "force_instrumental": True, "model_id": "music_v2_5", "store_for_inpainting": True}
    assert made["budget"]["spent_today"] == {"agent": 0.0, "user": 9.6}
    # A refused prompt's suggestion is text, and nothing is spent.
    stand_in.script.append((400, {"detail": {"status": "bad_prompt", "message": "No artists", "data": {"prompt_suggestion": "a slow soul ballad"}}}, []))
    message = failure("generate", "music", song, "like Marvin Gaye", "--seconds", 10)
    assert "named something it may not" in message and '"a slow soul ballad"' in message
    assert daw("keys", "status")["budget"]["spent_today"]["agent"] == 0.0


def test_a_plan_without_pcm_gets_a_song_as_mp3_that_import_decodes(tmp_path, stand_in, song):
    if not shutil.which("ffmpeg"):
        pytest.skip("no MP3 encoder here")
    mp3 = tmp_path / "song.mp3"
    out = subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=220:duration=9.6", "-ac", "2", "-ar", "48000", "-b:a", "192k", str(mp3)], capture_output=True)
    if out.returncode != 0:
        pytest.skip("ffmpeg could not make an MP3")
    stand_in.mp3 = mp3.read_bytes()
    stand_in.tier = "creator"
    assert daw("keys", "status")["tier"] == "creator"
    made = daw("generate", "music", song, "a drone", "--seconds", 9.6, "--id", "drone")
    assert stand_in.songs()[0]["query"] == "output_format=mp3_48000_192"
    assert made["format"] == "mp3_48000_192" and Path(made["generated_file"]).suffix == ".mp3"
    assert made["path"].endswith(".wav") and 9.5 < made["seconds"] < 9.8
    assert load(song)["samples"]["drone"]["path"] == made["path"]
