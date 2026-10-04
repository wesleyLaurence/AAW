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
