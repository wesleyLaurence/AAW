from pathlib import Path
import numpy as np
import soundfile as sf
from agent_daw import library
from agent_daw.cli import parser


def test_shared_sources_and_scoped_search(tmp_path, monkeypatch):
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.delenv("AAW_LIBRARY", raising=False)
    db = library.default_db()
    roots = [tmp_path / "drums", tmp_path / "drums-old", tmp_path / "a_%"]
    for root in roots:
        root.mkdir()
        sf.write(root / "kick.wav", np.zeros(100), 44100)
        library.scan(root, db)
    assert len(library.folders(db)) == 3
    assert len(library.search(db, "kick")) == 3
    assert len(library.search(db, roots=[roots[0]])) == 1
    assert len(library.search(db, roots=[roots[0], roots[2]])) == 2
    elsewhere = tmp_path / "another-project"
    elsewhere.mkdir()
    monkeypatch.chdir(elsewhere)
    assert parser().parse_args(["samples", "search"]).db == db
    assert len(library.search(db)) == 3
    library.remove_folder(db, roots[0])
    assert len(library.search(db)) == 2
    assert (roots[0] / "kick.wav").is_file()
    (roots[1] / "kick.wav").unlink()
    library.scan(roots[1], db)
    assert len(library.search(db)) == 1


def test_nested_sources_survive_removal_and_refresh(tmp_path):
    db = tmp_path / "index.sqlite"
    parent = tmp_path / "samples"
    child = parent / "nested"
    child.mkdir(parents=True)
    sf.write(child / "hit.wav", np.zeros(100), 44100)
    library.scan(parent, db)
    library.scan(child, db)
    assert len(library.search(db)) == 1
    library.remove_folder(db, parent)
    assert len(library.search(db)) == 1
    library.remove_folder(db, child)
    assert library.search(db) == []
    assert (child / "hit.wav").exists()


def test_unavailable_source_and_explicit_override(tmp_path, monkeypatch):
    root = tmp_path / "offline"
    root.mkdir()
    db = tmp_path / "index.sqlite"
    library.scan(root, db)
    root.rmdir()
    assert library.folders(db) == [{"path": str(root), "available": False}]
    monkeypatch.setenv("AAW_LIBRARY", str(db))
    assert library.default_db() == db


def test_folder_commands_share_sources_with_search(tmp_path, monkeypatch):
    from helpers import cli
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.delenv("AAW_LIBRARY", raising=False)
    root = tmp_path / "library"
    root.mkdir()
    sf.write(root / "hit.wav", np.zeros(100), 44100)
    code, added = cli("samples", "folders", "add", root)
    assert code == 0 and added["indexed"] == 1
    code, rows = cli("samples", "search", "hit", "--folder", root)
    assert code == 0 and len(rows) == 1
    assert cli("samples", "folders", "refresh")[1][0]["unchanged"] == 1
    assert cli("samples", "folders", "remove", root) == (0, [])
    assert (root / "hit.wav").exists()


def test_file_symlink_belongs_to_its_registered_folder(tmp_path):
    source = tmp_path / "original.wav"
    sf.write(source, np.zeros(100), 44100)
    root = tmp_path / "library"
    root.mkdir()
    (root / "linked.wav").symlink_to(source)
    db = tmp_path / "index.sqlite"
    library.scan(root, db)
    assert len(library.search(db, roots=[root])) == 1
    library.remove_folder(db, root)
    assert library.search(db) == []
    assert source.exists() and (root / "linked.wav").is_symlink()


def test_search_by_path_keeps_only_those_files(tmp_path):
    db = tmp_path / "index.sqlite"
    root = tmp_path / "drums"
    root.mkdir()
    for name in ["kick", "snare", "hat"]:
        sf.write(root / f"{name}.wav", np.zeros(100), 44100)
    library.scan(root, db)
    # The favorites' files, and one the index does not have, which is left out.
    found = library.search(db, paths=[root / "snare.wav", root / "kick.wav", tmp_path / "none.wav"])
    assert [r["name"] for r in found] == ["kick.wav", "snare.wav"]
    assert found[0]["path"] == str((root / "kick.wav").resolve())
    assert len(library.search(db, "kick", paths=[root / "snare.wav"])) == 0
    args = parser().parse_args(["samples", "search", "--path", "a.wav", "--path", "b.wav"])
    assert [str(p) for p in args.path] == ["a.wav", "b.wav"]


def test_tree_lists_subfolders_and_the_samples_directly_in_a_folder(tmp_path):
    db = tmp_path / "index.sqlite"
    root = tmp_path / "samples"
    for rel in ["top 10.wav", "top 2.wav", "bass/sub.wav", "drums/kicks/k1.wav",
                "drums/kicks/k2.wav", "drums/snare.wav", "Drums Old/hat.wav"]:
        (root / rel).parent.mkdir(parents=True, exist_ok=True)
        sf.write(root / rel, np.zeros(100), 44100)
    (root / "empty").mkdir()
    (root / "bass" / "notes.txt").write_text("not audio")
    library.scan(root, db)
    (whole,) = library.tree(db)
    assert whole["path"] == str(root.resolve())
    # By name as the Finder orders them, case aside; an empty folder is left out.
    assert [(f["name"], f["count"]) for f in whole["folders"]] == [("bass", 1), ("drums", 3), ("Drums Old", 1)]
    assert whole["folders"][1]["path"] == str(root.resolve() / "drums")
    assert [s["name"] for s in whole["samples"]] == ["top 2.wav", "top 10.wav"]
    assert "measured" in whole["samples"][0]
    drums, kicks = library.tree(db, [root / "drums", root / "drums" / "kicks"])
    assert [(f["name"], f["count"]) for f in drums["folders"]] == [("kicks", 2)]
    assert [s["name"] for s in drums["samples"]] == ["snare.wav"]
    assert kicks["folders"] == [] and [s["name"] for s in kicks["samples"]] == ["k1.wav", "k2.wav"]
    # "drums" never takes in "Drums Old" or a folder whose name starts the same.
    assert library.tree(db, [root / "Drums Old"])[0]["samples"][0]["name"] == "hat.wav"
    try:
        library.tree(db, [tmp_path])
        raise AssertionError("a folder outside the registered ones")
    except ValueError as e:
        assert "Not in a registered sample folder" in str(e)


def test_tree_command(tmp_path, monkeypatch):
    from helpers import cli
    monkeypatch.setenv("AAW_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.delenv("AAW_LIBRARY", raising=False)
    root = tmp_path / "library"
    (root / "bass").mkdir(parents=True)
    sf.write(root / "bass" / "sub.wav", np.zeros(100), 44100)
    cli("samples", "folders", "add", root)
    code, found = cli("samples", "tree", root, root / "bass")
    assert code == 0 and [f["name"] for f in found[0]["folders"]] == ["bass"]
    assert [s["name"] for s in found[1]["samples"]] == ["sub.wav"]
    code, found = cli("samples", "tree")
    assert code == 0 and found[0]["folders"][0]["count"] == 1
