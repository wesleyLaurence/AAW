"""The song model must keep doing what the Python model did when it was retired.

`golden_model.json` pins, for a generated corpus of valid songs, the same songs
with one field broken and YAML edge cases, what pydantic and PyYAML did with each
document at the cutover: its canonical YAML and fingerprints, or its errors'
locations, types and messages. It pins `fmt` and `apply` on generated edits the
same way. Existing projects, render reports and `--expect` SHAs depend on these.

After a deliberate change to the model, AAW_UPDATE_GOLDEN=1 accepts what it
does now. AAW_PARITY_SIZE checks a larger corpus for crashes and fixed points.
"""

import hashlib
import json
import os
import random
import subprocess
from pathlib import Path

import pytest

from agent_daw.model import fingerprints, load, parse, project_hash, to_yaml, validate
from model_fixtures import corpus, dump, random_project

GOLDEN = Path(__file__).with_name("golden_model.json")
UPDATE = bool(os.environ.get("AAW_UPDATE_GOLDEN"))


def short(data):
    return hashlib.sha256(json.dumps(data, sort_keys=True).encode()).hexdigest()[:16]


def text_hash(text):
    return hashlib.sha256(text.encode()).hexdigest()[:16]


def digest(parts):
    return hashlib.sha256("\0".join(parts).encode()).hexdigest()


def golden():
    return json.loads(GOLDEN.read_text())


def update(key, value):
    data = golden() if GOLDEN.exists() else {}
    data[key] = value
    GOLDEN.write_text(json.dumps(data, indent=1) + "\n")


def results(daw, files):
    """What `daw model` says of each file, without paths and error summaries."""
    out = subprocess.run([daw, "model", *map(str, files)], capture_output=True, text=True, check=True)
    kept = []
    for entry in json.loads(out.stdout):
        if entry["valid"]:
            keep = ["valid", "yaml", "project_sha256", "fingerprints"]
        elif entry["kind"] == "validation":
            keep = ["valid", "kind", "errors"]
        else:
            keep = ["valid", "kind"]
        kept.append({k: entry[k] for k in keep})
    return kept


def _write(path, text):
    path.write_text(text)
    return path


def test_corpus_matches_the_pinned_results(rust_daw, tmp_path):
    docs = corpus(200)
    found = results(rust_daw, [_write(tmp_path / f"{name}.yaml", text) for name, text in docs])
    # The corpus must exercise both outcomes, not only rejections.
    assert sum(r["valid"] for r in found) > len(docs) // 3
    assert sum(not r["valid"] for r in found) > len(docs) // 4
    pinned = {"documents": digest(text for _, text in docs), "results": [short(r) for r in found]}
    if UPDATE:
        return update("corpus", pinned)
    expected = golden()["corpus"]
    assert pinned["documents"] == expected["documents"], "the corpus generator no longer writes the pinned documents"
    changed = [(name, text, r) for (name, text), r, a, b in zip(docs, found, pinned["results"], expected["results"], strict=True) if a != b]
    lines = [f"{len(changed)} documents are handled differently than at the cutover"]
    for name, text, r in changed[:5]:
        lines += [f"--- {name}", text[:1500], json.dumps(r)[:1500]]
    assert not changed, "\n".join(lines)


def test_canonical_form_is_a_fixed_point(rust_daw, tmp_path):
    docs = corpus(int(os.environ.get("AAW_PARITY_SIZE", 60)), seed=7)
    first = results(rust_daw, [_write(tmp_path / f"a-{n}.yaml", t) for n, t in docs])
    valid = [r for r in first if r["valid"]]
    assert len(valid) > len(docs) // 3
    again = results(rust_daw, [_write(tmp_path / f"b-{i}.yaml", r["yaml"]) for i, r in enumerate(valid)])
    assert [r["yaml"] for r in valid] == [r["yaml"] for r in again]
    assert [r["project_sha256"] for r in valid] == [r["project_sha256"] for r in again]


def test_python_reads_songs_as_the_cli_does(rust_daw, tmp_path):
    docs = corpus(40, seed=3)
    files = [_write(tmp_path / f"{name}.yaml", text) for name, text in docs]
    out = subprocess.run([rust_daw, "model", *map(str, files)], capture_output=True, text=True, check=True)
    for (name, text), entry in zip(docs, json.loads(out.stdout), strict=True):
        if entry["valid"]:
            song = parse(text)
            assert to_yaml(song) == entry["yaml"], name
            assert fingerprints(song) == entry["fingerprints"], name
            assert project_hash(song) == entry["project_sha256"], name
            # The full dump is the song again.
            assert validate(song) == song, name
        else:
            with pytest.raises(ValueError) as refused:
                parse(text)
            assert str(refused.value) == entry["error"], name


def _project_dir(root, data, text):
    """A project whose samples are placeholder files holding their own path."""
    root.mkdir(parents=True)
    for sample in data.get("samples", {}).values():
        f = root / sample["path"]
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_bytes(sample["path"].encode())
    song = root / "song.yaml"
    song.write_text(text)
    return song


def _patches(rng, data):
    full = validate(data)
    patches = [
        {"session": {"tempo": rng.choice([133, 90.5, "150", 1000])}},
        {"session": {"title": rng.choice(["patched é", "it's: here", None])}},
        {"sections": None},
        {"master": {"effects": [{"type": "limiter", "ceiling_db": -2}]}},
        {"patterns": {name: {"swing": 0.6} for name in list(full["patterns"])[:1]}},
        {"samples": {name: None for name in list(full["samples"])[:1]}},
        {"tracks": full["tracks"][:1]},
        {"tracks": [{**full["tracks"][0], "gain_db": -3, "effects": []}]},
        {"bogus": 1},
        {"schema_version": 1},
    ]
    return rng.sample(patches, 3)


def _run(daw, argv):
    out = subprocess.run([daw, *map(str, argv)], capture_output=True, text=True)
    return out.returncode, json.loads(out.stdout) if out.returncode == 0 else None


def test_fmt_and_apply_match_the_pinned_edits(rust_daw, tmp_path):
    rng = random.Random(11)
    cases, outcomes = [], []
    for _ in range(40):
        data = random_project(rng)
        for sample in data.get("samples", {}).values():
            if "sha256" in sample:
                sample["sha256"] = hashlib.sha256(sample["path"].encode()).hexdigest()
        try:
            validate(data)
        except ValueError:
            continue
        text = dump(rng, data)
        for patch in _patches(rng, data):
            n = len(outcomes)
            song = _project_dir(tmp_path / f"song{n}", data, text)
            assert _run(rust_daw, ["fmt", song])[0] == 0
            formatted = song.read_text()
            sha = project_hash(load(song))
            patch_file = tmp_path / f"patch{n}.json"
            patch_file.write_text(json.dumps(patch))
            stale = _run(rust_daw, ["apply", song, patch_file, "--expect", "0" * 64])
            assert stale[0] == 1 and song.read_text() == formatted
            status, result = _run(rust_daw, ["apply", song, patch_file, "--expect", sha])
            cases.append(text_hash(text) + short(patch))
            outcomes.append([status, text_hash(formatted), text_hash(song.read_text()), result["project_sha256"][:16] if result else None])
    # Both accepted and rejected edits must be compared.
    statuses = [o[0] for o in outcomes]
    assert statuses.count(0) >= 20 and statuses.count(1) >= 10, statuses
    pinned = {"cases": digest(cases), "outcomes": outcomes}
    if UPDATE:
        return update("edits", pinned)
    expected = golden()["edits"]
    assert pinned["cases"] == expected["cases"], "the generator no longer writes the pinned edits"
    assert outcomes == expected["outcomes"]
