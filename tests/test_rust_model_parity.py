"""The Rust model must accept, write and fingerprint songs exactly as model.py does.

Skipped when no Rust toolchain is installed. Set AAW_RUST_DAW to test a prebuilt
binary, or AAW_PARITY_SIZE to run a larger generated corpus.
"""

import hashlib
import json
import os
import random
import shutil
import subprocess
from pathlib import Path

import pytest
import yaml
from pydantic import ValidationError

from agent_daw.cli import execute, parser
from agent_daw.model import LEGACY_FIELDS, Project, ProjectDumper, _sha, _without, project_hash
from model_fixtures import corpus, dump, random_project

ENGINE = Path(__file__).resolve().parents[1] / "engine"


@pytest.fixture(scope="session")
def rust_daw():
    override = os.environ.get("AAW_RUST_DAW")
    if override:
        return Path(override)
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo")
    if not Path(cargo).exists():
        pytest.skip("Rust toolchain not installed")
    subprocess.run([cargo, "build", "--quiet", "-p", "aaw-cli"], cwd=ENGINE, check=True)
    return ENGINE / "target" / "debug" / "daw"


def python_result(text):
    try:
        data = yaml.safe_load(text)
    except Exception:
        return {"valid": False, "kind": "yaml"}
    try:
        p = Project.model_validate(data)
    except ValidationError as e:
        errors = [
            {"loc": ".".join(str(x) for x in err["loc"]), "type": err["type"], "msg": err["msg"]}
            for err in e.errors()
        ]
        return {"valid": False, "kind": "validation", "errors": errors}
    saved = p.model_dump(mode="json", exclude_none=True, exclude_defaults=True)
    canonical = yaml.dump(
        {"schema_version": 1, **saved},
        Dumper=ProjectDumper,
        sort_keys=False,
        allow_unicode=True,
        width=110,
    )
    full, fields, forms = p.model_dump(mode="json"), {}, [project_hash(p)]
    for added in [{}, *LEGACY_FIELDS]:
        fields |= added
        forms.append(_sha(_without(full, fields)))
    return {"valid": True, "yaml": canonical, "project_sha256": forms[0], "fingerprints": forms}


def rust_results(daw, files):
    out = subprocess.run([daw, "model", *map(str, files)], capture_output=True, text=True, check=True)
    results = []
    for entry in json.loads(out.stdout):
        if entry["valid"]:
            keep = ["valid", "yaml", "project_sha256", "fingerprints"]
        elif entry["kind"] == "validation":
            keep = ["valid", "kind", "errors"]
        else:
            keep = ["valid", "kind"]
        results.append({k: entry[k] for k in keep})
    return results


def compare(daw, docs, tmp_path, expected=None):
    """Documents whose Rust result differs from the Python one."""
    expected = expected or [python_result(text) for _, text in docs]
    files = []
    for name, text in docs:
        f = tmp_path / f"{name}.yaml"
        f.write_text(text)
        files.append(f)
    rust = rust_results(daw, files)
    return [
        (name, text, p, r)
        for (name, text), p, r in zip(docs, expected, rust, strict=True)
        if p != r
    ]


def report(mismatches):
    lines = [f"{len(mismatches)} documents differ between the Python and Rust models"]
    for name, text, p, r in mismatches[:5]:
        lines += [f"--- {name}", text[:1500], f"python: {json.dumps(p)[:1500]}", f"rust:   {json.dumps(r)[:1500]}"]
    return "\n".join(lines)


def test_model_parity(rust_daw, tmp_path):
    docs = corpus(int(os.environ.get("AAW_PARITY_SIZE", 200)))
    results = [python_result(text) for _, text in docs]
    # The corpus must exercise both outcomes, not only rejections.
    assert sum(r["valid"] for r in results) > len(docs) // 3
    assert sum(not r["valid"] for r in results) > len(docs) // 4
    mismatches = compare(rust_daw, docs, tmp_path, results)
    assert not mismatches, report(mismatches)


def test_canonical_form_is_a_fixed_point(rust_daw, tmp_path):
    docs = [(n, t) for n, t in corpus(60, seed=7) if python_result(t)["valid"]]
    first = rust_results(rust_daw, [_write(tmp_path / f"a-{n}.yaml", t) for n, t in docs])
    again = rust_results(rust_daw, [_write(tmp_path / f"b-{n}.yaml", r["yaml"]) for (n, _), r in zip(docs, first)])
    assert [r["yaml"] for r in first] == [r["yaml"] for r in again]
    assert [r["project_sha256"] for r in first] == [r["project_sha256"] for r in again]


def _write(path, text):
    path.write_text(text)
    return path


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
    full = Project.model_validate(data).model_dump(mode="json")
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


def _run_python(argv):
    try:
        return 0, execute(parser().parse_args(argv))
    except Exception:
        return 1, None


def _run_rust(daw, argv):
    out = subprocess.run([daw, *argv], capture_output=True, text=True)
    return out.returncode, json.loads(out.stdout) if out.returncode == 0 else None


def test_fmt_and_apply_match_python(rust_daw, tmp_path):
    rng = random.Random(11)
    outcomes = []
    for _ in range(40):
        data = random_project(rng)
        for sample in data.get("samples", {}).values():
            if "sha256" in sample:
                sample["sha256"] = hashlib.sha256(sample["path"].encode()).hexdigest()
        try:
            Project.model_validate(data)
        except ValidationError:
            continue
        text = dump(rng, data)
        for patch in _patches(rng, data):
            n = len(outcomes)
            py_song = _project_dir(tmp_path / f"py{n}", data, text)
            rs_song = _project_dir(tmp_path / f"rs{n}", data, text)
            assert _run_python(["fmt", str(py_song)])[0] == 0
            assert _run_rust(rust_daw, ["fmt", str(rs_song)])[0] == 0
            assert py_song.read_text() == rs_song.read_text()
            sha = project_hash(Project.model_validate(yaml.safe_load(py_song.read_text())))
            patch_file = tmp_path / f"patch{n}.json"
            patch_file.write_text(json.dumps(patch))
            py = _run_python(["apply", str(py_song), str(patch_file), "--expect", sha])
            rs = _run_rust(rust_daw, ["apply", str(rs_song), str(patch_file), "--expect", sha])
            assert py == rs, (patch, py, rs)
            assert py_song.read_text() == rs_song.read_text(), patch
            stale = _run_rust(rust_daw, ["apply", str(rs_song), str(patch_file), "--expect", "0" * 64])
            assert stale[0] == 1
            outcomes.append(py[0])
    # Both accepted and rejected edits must be compared.
    assert outcomes.count(0) >= 20 and outcomes.count(1) >= 10, outcomes
