#!/bin/sh
# Measures what makes the person wait: runs the app through one session on a
# large song and writes every time its main thread was held past a tenth of a
# second, with what it was doing, into OUT.json (docs/features/performance.md).
#
#   ./measure.sh OUT.json [APP]
#
# APP is the app's executable, .build/release/AAW unless given (`swift build
# -c release` makes it). The first run generates, under $AAW_MEASURE_DIR or
# /tmp/aaw-measure: a song of 30 tracks, five minutes long, of 20 audio tracks
# with 160 clips of 40-second files and 10 Synth tracks with 10,240 notes; a
# library of 50,000 short samples in nested folders, indexed; and a
# conversation of 2,000 messages, about 8 MB, as its newest thread. Each run
# opens a fresh copy of the song in a window of 1440 by 900 and opens the
# browser, searches the library, opens Folders, drags a clip and undoes it,
# plays four seconds on BlackHole 2ch, saves, takes eight edits and eight
# transposes from `daw` in a terminal, then scrolls and zooms for 240 frames.
# AAW_STALL_MS=16 counts shorter work as well.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=$1
app=${2:-"$here/.build/release/AAW"}
dir=${AAW_MEASURE_DIR:-/tmp/aaw-measure}
daw="$root/engine/target/release/daw"
[ -x "$daw" ] || (cd "$root/engine" && cargo build --release -p aaw-cli)
mkdir -p "$dir"

if [ ! -f "$dir/song/song.yaml" ]; then
    (cd "$root" && uv run python - "$dir" <<'EOF'
import json, pathlib, subprocess, sys
import numpy as np, soundfile as sf

dir = pathlib.Path(sys.argv[1])
song, audio = dir / "song", dir / "audio"
audio.mkdir(parents=True, exist_ok=True)

def daw(*args):
    subprocess.run(["uv", "run", "daw", *map(str, args)], check=True, capture_output=True)

daw("init", song, "--tempo", 100)
rate, noise = 44100, np.random.default_rng(1)
for i in range(20):
    t = np.arange(rate * 40) / rate
    x = 0.3 * np.sin(2 * np.pi * (80 + 37 * i) * t) * (0.5 + 0.5 * np.sin(2 * np.pi * 0.25 * t))
    x = x + 0.05 * noise.standard_normal(len(t))
    sf.write(audio / f"loop{i:02d}.wav", np.stack([x, 0.9 * x], 1).astype(np.float32), rate, subtype="PCM_24")
    daw("samples", "import", audio / f"loop{i:02d}.wav", "--project", song, "--id", f"loop{i:02d}")
commands = []
for i in range(20):
    commands.append({"op": "track.add", "id": f"audio{i:02d}", "type": "audio"})
    for c in range(8):
        commands.append({"op": "audio.add", "track": f"audio{i:02d}", "sample": f"loop{(i + c) % 20:02d}", "at": c * 64, "source_end_seconds": 36})
for m in range(10):
    commands.append({"op": "synth.add", "track": f"synth{m}"})
    for c in range(8):
        clip = f"s{m}c{c}"
        commands.append({"op": "clip.add", "track": f"synth{m}", "id": clip, "at": c * 64, "length_beats": 64})
        notes = [{"pitch": 48 + (k * 7 + m) % 24, "at": k / 2, "duration": 0.5, "velocity": 90} for k in range(128)]
        commands.append({"op": "note.add", "clip": f"tracks.synth{m}.clips.{clip}", "notes": notes})
(dir / "batch.json").write_text(json.dumps({"label": "Large song", "commands": commands}))
daw("batch", song, dir / "batch.json")
EOF
    )
fi

if [ ! -f "$dir/data/library.sqlite" ]; then
    python3 - "$dir/library" <<'EOF'
import pathlib, random, struct, sys, wave
root = pathlib.Path(sys.argv[1])
random.seed(2)
names = ["kick", "snare", "hat", "clap", "perc", "bass", "pad", "lead", "vox", "fx"]
frames = struct.pack("<1100h", *[random.randint(-3000, 3000) for _ in range(1100)])
for p in range(50):
    for s, name in enumerate(names):
        folder = root / f"Pack {p:02d}" / f"{name}s"
        folder.mkdir(parents=True, exist_ok=True)
        for k in range(100):
            with wave.open(str(folder / f"P{p:02d} {name} {k:03d} {80 + k % 60}bpm.wav"), "wb") as w:
                w.setnchannels(1); w.setsampwidth(2); w.setframerate(22050); w.writeframes(frames)
EOF
    mkdir -p "$dir/data"
    (cd "$root" && uv run daw samples --db "$dir/data/library.sqlite" folders add "$dir/library" >/dev/null)
fi

# The copy the session edits, and the conversation in its folder of Claude
# Code's record, which the panel shows as the newest thread.
rm -rf "$dir/run"
cp -cR "$dir/song" "$dir/run" 2>/dev/null || cp -R "$dir/song" "$dir/run"
claude="$dir/claude/projects/$(python3 -c 'import os, re, sys; print(re.sub(r"[^A-Za-z0-9]", "-", os.path.realpath(sys.argv[1])))' "$dir/run")"
if [ ! -d "$claude" ]; then
    mkdir -p "$claude"
    python3 - "$claude" <<'EOF'
import json, pathlib, sys
words = "The kick sits under the bass at bar 41. " * 100
lines = []
for n in range(1000):
    lines.append(json.dumps({"type": "user", "message": {"role": "user", "content": f"Message {n}: {words}"}}))
    lines.append(json.dumps({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": f"Answer {n}: {words}"}]}}))
(pathlib.Path(sys.argv[1]) / "00000000-0000-4000-8000-000000000000.jsonl").write_text("\n".join(lines) + "\n")
EOF
fi

# The browser opens on Samples, as the clicks below expect.
defaults write AAW browser.section Samples

# The agent's edits, from a terminal, once the window has opened.
(
    sleep 18
    for i in 1 2 3 4 5 6 7 8; do
        "$daw" set "$dir/run" "tracks.audio0$i.gain_db" "-$i" >/dev/null
        "$daw" note transpose "$dir/run" "tracks.synth$i.clips.s${i}c0" --by 1 >/dev/null
    done
) &
edits=$!

CLAUDE_CONFIG_DIR="$dir/claude" AAW_AGENT=/usr/bin/true AAW_DATA_DIR="$dir/data" AAW_OUTPUT_DEVICE="BlackHole 2ch" \
    "$app" "$dir/run/song.yaml" --size 1440x900 \
    --click 23,20 --wait 1 \
    --click 171,144 --type kick --wait 2 \
    --click 27,350 --wait 2 \
    --click 31,210 --wait 1 \
    --drag 737,200,807,200 --wait 1 --key cmd+z --wait 1 \
    --key space --wait 4 --key space \
    --key cmd+s --wait 6 \
    --measure "$out" 2>/dev/null
wait $edits

python3 - "$out" <<'EOF'
import json, sys
stalls = json.load(open(sys.argv[1]))["stalls"]
print(f"{stalls['count']} stalls, the longest {stalls['max_ms']:.0f} ms")
for stall in stalls["stalls"]:
    print(f"  {stall['ms']:.0f} ms at {stall['at_s']} s")
EOF
