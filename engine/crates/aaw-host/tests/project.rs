//! A project as a folder: the song file a path names, Untitled projects in the
//! data folder, and a project saved under another name while a host has it
//! open, by a move and by a copy. No audio device is opened.

mod common;

use aaw_host::client::{self, Request};
use aaw_host::command::Origin;
use aaw_host::host::{self, Options};
use aaw_host::session::Session;
use aaw_host::{project, registry};
use common::{cmd, write_song};
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The registry in a short directory, as socket paths are short, and a data
/// folder that is not the person's.
fn folders() -> &'static Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::Builder::new().prefix("aaw").tempdir_in("/tmp").unwrap();
        // SAFETY: set once, before any test reads them; every test calls this first.
        unsafe {
            std::env::set_var("AAW_HOST_DIR", dir.path().join("hosts"));
            std::env::set_var("AAW_DATA_DIR", dir.path().join("data"));
        }
        dir
    })
    .path()
}

fn ask(project: &Path, j: Json) -> Result<Option<client::Reply>, String> {
    client::ask(project, &Request::new(cmd(j), Origin::Agent))
}

/// A host of the song in a folder named `a`, which the tests save elsewhere.
struct Running {
    /// The folder that holds `a`.
    root: PathBuf,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Json, String>>>,
    _dir: tempfile::TempDir,
}

fn start(path: &Path) -> (Arc<AtomicBool>, JoinHandle<Result<Json, String>>) {
    let stop = Arc::new(AtomicBool::new(false));
    let (p, s) = (path.to_path_buf(), stop.clone());
    let thread = std::thread::spawn(move || {
        host::run(
            &p,
            Options {
                audio: aaw_host::host::Audio::default(),
                play: None,
                exit_on_stop: false,
                seconds: None,
                feed: false,
                prepare: false,
                pace: host::PACE,
            },
            s,
        )
    });
    let started = Instant::now();
    while ask(path, json!({"op": "status"})).unwrap().is_none() {
        if thread.is_finished() {
            panic!("the host stopped: {:?}", thread.join().unwrap());
        }
        assert!(started.elapsed() < Duration::from_secs(10), "the host did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    (stop, thread)
}

impl Running {
    fn start() -> Running {
        folders();
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(root.join("a")).unwrap();
        let path = write_song(&root.join("a"));
        let (stop, thread) = start(&path);
        Running {
            root,
            path,
            stop,
            thread: Some(thread),
            _dir: dir,
        }
    }

    /// Sends to the host at a path, which may be one the project had.
    fn send(&self, at: &Path, j: Json) -> Result<Json, String> {
        ask(at, j).map(|r| r.expect("host running").result)
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().unwrap().unwrap();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// The registrations for paths under a folder: each path, and where it says
/// the project is now.
fn registered(under: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    registry::list()
        .into_iter()
        .filter(|l| l.project.starts_with(under))
        .map(|l| (l.project, l.now))
        .collect()
}

#[test]
fn a_project_is_named_by_its_folder_or_its_song_file() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("beat");
    let song = project::create(&folder, 100.0, None, None).unwrap();
    assert_eq!(song, folder.join("song.yaml"));
    let song_file = |p: &Path| project::song_file(p).unwrap();
    assert_eq!(song_file(&folder), song);
    assert_eq!(song_file(&song), song);
    // A path that is gone is a folder, unless it is written as a file.
    assert_eq!(song_file(&dir.path().join("gone")), dir.path().join("gone/song.yaml"));
    assert_eq!(song_file(&dir.path().join("gone/other.yaml")), dir.path().join("gone/other.yaml"));
    assert_eq!(project::shown(&song), folder);
    // The project file beside the song names it, and is the way in from outside.
    let file = folder.join("beat.aaw");
    let id = project::id_of(&song).unwrap();
    assert_eq!(id.len(), 36, "{id}");
    assert_eq!(text(&file), format!("# An AAW project. Open it with AAW; the song is in song.yaml beside it.\nformat: 1\nid: {id}\nsong: song.yaml\n"));
    assert_eq!(project::read_project_file(&file).unwrap(), project::ProjectFile { path: file.clone(), id: id.clone(), song: "song.yaml".into() });
    assert_eq!(song_file(&file), song);
    assert_eq!(project::project_file_in(&folder), Some(file.clone()));
    assert_eq!(project::ensure_project_file(&folder).unwrap().id, id, "one that is there is kept");
    // One that is gone names its folder's song, which a host may still answer for.
    assert_eq!(song_file(&dir.path().join("gone/gone.aaw")), dir.path().join("gone/song.yaml"));
    // The song's notes are beside it, and the file that imports them.
    assert_eq!(text(&folder.join("SONG.md")), "## Vision\n\n## Instructions\n\n## Direction\n\n## Decisions\n");
    assert_eq!(text(&folder.join("AGENTS.md")), "Read SONG.md before working on this project, and keep it current.\n\n@SONG.md\n");
    assert_eq!(project::shown(&folder.join("other.yaml")), folder.join("other.yaml"));
    // A second project in the folder is refused.
    assert!(project::create(&folder, 100.0, None, None).unwrap_err().contains("already exists"));
    // An empty song is eight bars of its meter long, and says its meter.
    let waltz = project::blank(100.0, None, Some("3/4")).unwrap();
    assert_eq!((waltz.session.time_signature.as_str(), waltz.session.length_exact()), ("3/4", num_rational::BigRational::from_integer(24.into())));
    let seven = project::blank(100.0, None, Some("7/8")).unwrap();
    assert_eq!(seven.session.length_exact(), num_rational::BigRational::from_integer(28.into()));
    assert!(project::blank(100.0, None, Some("5/3")).unwrap_err().contains("such as 4/4"));
    assert_eq!(aaw_model::to_yaml(&project::blank(100.0, None, Some("4/4")).unwrap()), aaw_model::to_yaml(&project::blank(100.0, None, None).unwrap()));
    // A path that is gone resolves through the folders that are there.
    let canonical = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(project::resolved(&dir.path().join("gone/song.yaml")).unwrap(), canonical.join("gone/song.yaml"));
    assert_eq!(project::resolved(&folder.join("../gone/./song.yaml")).unwrap(), canonical.join("gone/song.yaml"));
}

#[test]
fn a_project_file_that_does_not_read_is_refused_and_several_are_told_apart() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("beat");
    let song = project::create(&folder, 100.0, None, None).unwrap();
    let file = folder.join("beat.aaw");
    let refused = |text: &str| {
        std::fs::write(&file, text).unwrap();
        project::song_file(&file).unwrap_err()
    };
    let instead = format!("open {} instead", song.display());
    for (text, why) in [
        ("format: 1\nid: x\nsong: ../other/song.yaml\n", "names a song that is not a file in its folder"),
        ("format: 1\nid: x\nsong: /etc/passwd\n", "names a song that is not a file in its folder"),
        ("format: 1\nid: x\nsong: ..\n", "names a song that is not a file in its folder"),
        ("format: 1\nid: x\n", "names no song"),
        ("format: 1\nsong: song.yaml\n", "has no id"),
        ("format: 1\nid: ''\nsong: song.yaml\n", "has no id"),
        ("id: x\nsong: song.yaml\n", "no format"),
        ("format: two\nid: x\nsong: song.yaml\n", "not a whole number"),
        ("format: 2\nid: x\nsong: song.yaml\n", "written by a newer AAW"),
        ("- a list\n", "not a mapping"),
        ("{", "is not an AAW project file"),
    ] {
        let e = refused(text);
        assert!(e.contains(why) && e.contains(&instead), "{text:?}: {e}");
    }
    // A song by another name in the folder is followed.
    std::fs::write(&file, "format: 1\nid: x\nsong: other.yaml\n").unwrap();
    assert_eq!(project::song_file(&file).unwrap(), folder.join("other.yaml"));
    assert_eq!(project::id_of(&song).as_deref(), Some("x"));
    // With several, the one named for the folder is the project's, else the first by name.
    std::fs::write(folder.join("a.aaw"), "format: 1\nid: a\nsong: song.yaml\n").unwrap();
    assert_eq!(project::project_files_in(&folder), [folder.join("a.aaw"), file.clone()]);
    assert_eq!(project::project_file_in(&folder), Some(file.clone()));
    std::fs::remove_file(&file).unwrap();
    assert_eq!(project::project_file_in(&folder), Some(folder.join("a.aaw")));
    assert_eq!(project::id_of(&song).as_deref(), Some("a"));
    // A new ID is written into the file the folder has.
    let renewed = project::renew_id(&song).unwrap();
    assert_ne!(renewed, "a");
    assert_eq!(project::read_project_file(&folder.join("a.aaw")).unwrap().id, renewed);
    assert!(!file.exists());
    // Without one, a host writes one named for the folder; nothing else does.
    std::fs::remove_file(folder.join("a.aaw")).unwrap();
    assert_eq!(project::id_of(&song), None);
    assert_eq!(project::song_file(&folder).unwrap(), song);
    assert_eq!(project::project_file_in(&folder), None);
    let mut s = Session::open(&song, true).unwrap();
    s.edit(&cmd(json!({"op": "track.add", "id": "drums"})), Origin::User, None, None).unwrap();
    s.write().unwrap();
    assert_eq!(project::project_file_in(&folder), None);
    let written = project::ensure_project_file(&folder).unwrap();
    assert_eq!(written.path, file);
    assert_eq!(project::id_of(&song), Some(written.id));
    // Two IDs are never the same.
    assert_ne!(project::new_id(), project::new_id());
}

#[test]
fn untitled_projects_take_the_lowest_free_name_and_blank_ones_are_swept() {
    folders();
    let first = project::create_untitled().unwrap();
    let second = project::create_untitled().unwrap();
    let root = project::untitled_root();
    assert_eq!(first, root.join("Untitled/song.yaml"));
    assert_eq!(second, root.join("Untitled 2/song.yaml"));
    assert_eq!(text(&second), "schema_version: 1\nsession:\n  title: Untitled 2\n  tempo: 120.0\n  length_beats: 32\n");
    // The project file and the folder's icon do not make a project hold something.
    assert!(root.join("Untitled 2/Untitled 2.aaw").is_file());
    std::fs::write(root.join("Untitled 2/Icon\r"), "").unwrap();
    assert!(project::is_untitled(&first) && project::is_untitled(&second));
    assert!(!project::is_untitled(&root.join("song.yaml")));
    let blank = |song: &Path| project::untouched(song.parent().unwrap(), &text(song));
    assert!(blank(&first) && blank(&second));

    // A project with a track, one with a file beside the song and one with
    // something written in its notes hold something.
    let third = project::create_untitled().unwrap();
    let noted = project::create_untitled().unwrap();
    let notes = noted.with_file_name(project::NOTES_FILE);
    std::fs::write(&notes, text(&notes) + "\nKeep the vocal dry.\n").unwrap();
    assert!(!blank(&noted));
    project::delete_untitled(&noted).unwrap();
    let mut s = Session::open(&second, false).unwrap();
    s.edit(&cmd(json!({"op": "track.add", "id": "drums"})), Origin::User, None, None).unwrap();
    s.write().unwrap();
    std::fs::write(third.parent().unwrap().join("notes.txt"), "x").unwrap();
    std::fs::write(first.parent().unwrap().join(".DS_Store"), "x").unwrap();
    assert!(blank(&first) && !blank(&second) && !blank(&third));

    // One a host has open is left alone, whatever it holds.
    let fourth = project::create_untitled().unwrap();
    let left = project::sweep_untitled(|song| song == fourth);
    assert_eq!(left, [second.clone(), third.clone()]);
    assert!(!first.exists() && fourth.exists());
    assert_eq!(project::create_untitled().unwrap(), first);

    // Only an Untitled project is deleted this way.
    let dir = tempfile::tempdir().unwrap();
    let named = project::create(&dir.path().join("beat"), 120.0, None, None).unwrap();
    assert!(project::delete_untitled(&named).unwrap_err().contains("not an Untitled project"));
    assert!(named.exists());
    project::delete_untitled(&third).unwrap();
    assert!(!third.parent().unwrap().exists());
}

#[test]
fn a_move_keeps_the_history_and_the_old_path_answers() {
    let mut h = Running::start();
    let old = h.path.clone();
    let folder = h.root.join("My Beat");
    let new = folder.join("song.yaml");
    h.send(&old, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6})).unwrap();
    let id = project::id_of(&old).expect("the host wrote the project file");
    assert!(h.root.join("a/a.aaw").is_file());

    let moved = h.send(&old, json!({"op": "project.move", "to": folder})).unwrap();
    assert_eq!(moved["label"], json!("Saved as My Beat"));
    assert_eq!(moved["project"], json!(new));
    assert_eq!(moved["from"], json!(old));
    assert_eq!(moved["revision"], json!(2));
    // The folder is at the new place with its samples, and the song has its name.
    assert!(!h.root.join("a").exists());
    assert!(folder.join("tone.wav").is_file() && folder.join("hit.wav").is_file());
    assert!(text(&new).contains("title: My Beat") && text(&new).contains("gain_db: -6.0"));
    // The project file, which the host wrote for a project made without one,
    // is named for the new folder and keeps its ID.
    assert!(!folder.join("a.aaw").exists());
    assert_eq!(project::project_files_in(&folder), [folder.join("My Beat.aaw")]);
    assert_eq!(project::id_of(&new), Some(id.clone()));
    assert_eq!(project::song_file(&folder.join("My Beat.aaw")).unwrap(), new);

    // The host answers at the new path, with its history: the move is a
    // change and not an undo step.
    let status = h.send(&new, json!({"op": "status"})).unwrap();
    assert_eq!(status["project"], json!(new));
    assert_eq!(status["title"], json!("My Beat"));
    assert_eq!(status["revision"], json!(2));
    assert_eq!(status["undo"]["label"], json!("Set tracks.drums.gain_db: 0.0 → -6"));

    // A command sent to the old path lands, and is told where the project is.
    let reply = ask(&old, json!({"op": "undo"})).unwrap().unwrap();
    assert_eq!(reply.moved(), Some(new.as_path()));
    assert!(ask(&new, json!({"op": "status"})).unwrap().unwrap().moved().is_none());
    // Undoing the edit made before the move keeps the name.
    assert!(text(&new).contains("title: My Beat") && !text(&new).contains("gain_db: -6.0"));
    h.send(&old, json!({"op": "set", "path": "session.tempo", "value": 90})).unwrap();
    assert!(text(&new).contains("tempo: 90.0"));

    let log = h.send(&new, json!({"op": "changes", "since": 0})).unwrap();
    let ops: Vec<&str> = log["changes"].as_array().unwrap().iter().map(|c| c["op"].as_str().unwrap()).collect();
    assert_eq!(ops, ["set", "project.move", "undo", "set"]);
    assert_eq!(registered(&h.root), [(new.clone(), None), (old.clone(), Some(new.clone()))]);

    // A second move: both earlier paths still answer.
    let third = h.root.join("deeper/Third");
    h.send(&new, json!({"op": "project.move", "to": third})).unwrap();
    // A command given a project file, the one there or one the project had, lands too.
    for at in [&old, &new, &third.join("song.yaml"), &third.join("Third.aaw"), &folder.join("My Beat.aaw")] {
        let song = project::song_file(at).unwrap();
        assert_eq!(h.send(&song, json!({"op": "status"})).unwrap()["title"], json!("Third"), "{}", at.display());
    }
    assert!(!folder.exists());
    assert_eq!(project::project_files_in(&third), [third.join("Third.aaw")]);
    assert_eq!(project::id_of(&third.join("song.yaml")), Some(id));

    // Closing unregisters every path.
    h.close();
    assert_eq!(registered(&h.root), []);
    assert!(ask(&old, json!({"op": "status"})).unwrap_err().contains("No such file"));
}

#[test]
fn a_copy_leaves_the_original_which_its_path_cannot_reach_until_released() {
    let mut h = Running::start();
    let old = h.path.clone();
    let folder = h.root.join("b");
    let new = folder.join("song.yaml");
    h.send(&old, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6})).unwrap();
    let before = text(&old);

    let copied = h.send(&old, json!({"op": "project.copy", "to": folder})).unwrap();
    assert_eq!(copied["label"], json!("Saved as b"));
    assert_eq!(copied["project"], json!(new));
    // The original is as it was, and the copy has its samples and its name.
    assert_eq!(text(&old), before);
    assert!(folder.join("tone.wav").is_file() && !folder.join(".daw.lock").exists());
    assert!(text(&new).contains("title: b") && text(&new).contains("gain_db: -6.0"));
    // The copy's project file is named for it, with an ID of its own.
    assert_eq!(project::project_files_in(&folder), [folder.join("b.aaw")]);
    assert_eq!(project::project_files_in(&h.root.join("a")), [h.root.join("a/a.aaw")]);
    assert_ne!(project::id_of(&new), project::id_of(&old));
    assert!(project::id_of(&new).is_some());

    // The host carries on in the copy with its history, and a command sent
    // to the original's path follows it there.
    let reply = ask(&old, json!({"op": "set", "path": "tracks.bass.gain_db", "value": -3})).unwrap().unwrap();
    assert_eq!(reply.moved(), Some(new.as_path()));
    assert!(text(&new).contains("gain_db: -3.0"));
    assert_eq!(text(&old), before);
    h.send(&new, json!({"op": "undo"})).unwrap();
    h.send(&new, json!({"op": "undo"})).unwrap();
    assert!(!text(&new).contains("gain_db: -6.0") && !text(&new).contains("gain_db: -3.0"));
    assert!(text(&new).contains("title: b"));
    assert_eq!(text(&old), before);

    // The original cannot have a host of its own while the copy answers for it.
    let stop = Arc::new(AtomicBool::new(false));
    let options = || Options {
        audio: aaw_host::host::Audio::default(),
        play: None,
        exit_on_stop: false,
        seconds: None,
        feed: false,
        prepare: false,
        pace: host::PACE,
    };
    let refused = host::run(&old, options(), stop).unwrap_err();
    assert!(refused.contains("was saved as") && refused.contains("answers for it"), "{refused}");

    // Released, the path is the original's again.
    assert_eq!(h.send(&new, json!({"op": "project.release", "project": new})).unwrap()["released"], json!(false));
    assert_eq!(h.send(&new, json!({"op": "project.release", "project": old})).unwrap()["released"], json!(true));
    assert!(ask(&old, json!({"op": "status"})).unwrap().is_none());
    assert_eq!(registered(&h.root), [(new.clone(), None)]);
    let (stop, thread) = start(&old);
    assert_eq!(h.send(&old, json!({"op": "status"})).unwrap()["title"], json!("Untitled"));
    assert_eq!(h.send(&new, json!({"op": "status"})).unwrap()["title"], json!("b"));
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap().unwrap();
    h.close();
}

#[test]
fn a_refused_move_changes_nothing() {
    let h = Running::start();
    let old = h.path.clone();
    std::fs::create_dir(h.root.join("taken")).unwrap();
    for (to, why) in [
        (h.root.join("taken"), "already there"),
        (h.root.join("a/inside"), "inside the project"),
        (PathBuf::from("relative"), "not an absolute path"),
    ] {
        for op in ["project.move", "project.copy"] {
            let e = h.send(&old, json!({"op": op, "to": to})).unwrap_err();
            assert!(e.contains(why), "{e}");
        }
    }
    let status = h.send(&old, json!({"op": "status"})).unwrap();
    assert_eq!((status["revision"].clone(), status["title"].clone()), (json!(0), json!("Untitled")));
    assert_eq!(registered(&h.root), [(old.clone(), None)]);
    assert!(old.is_file());
}

#[test]
fn a_name_replaces_a_title_through_the_history() {
    folders();
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::create_dir(root.join("a")).unwrap();
    let mut s = Session::open(&write_song(&root.join("a")), true).unwrap();
    let edit = |s: &mut Session, j: Json| s.edit(&cmd(j), Origin::User, None, None).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6}));
    edit(&mut s, json!({"op": "set", "path": "session.title", "value": "Working title"}));
    let command = json!({"op": "project.move", "to": root.join("Named")});
    let (reply, change) = s.relocate(&root.join("Named"), false, Origin::User, command).unwrap();
    assert_eq!((change.op.as_str(), change.revision), ("project.move", 3));
    assert_eq!(reply["project"], json!(root.join("Named/song.yaml")));
    // The step that only changed the title is gone; the edit before it is next.
    assert_eq!(s.history().undo.unwrap().0, "Set tracks.drums.gain_db: 0.0 → -6");
    s.undo(Origin::User, false).unwrap();
    assert_eq!(s.project().session.title, "Named");
    assert!(s.history().undo.is_none());
    s.undo(Origin::User, true).unwrap();
    assert_eq!((s.project().session.title.as_str(), s.project().tracks[0].gain_db), ("Named", -6.0));
    s.save().unwrap();
    assert!(text(&root.join("Named/song.yaml")).contains("title: Named"));
}

#[test]
fn the_index_lists_what_the_app_knows() {
    folders();
    assert_eq!(project::index_file(), project::data_dir().join("projects.json"));
    std::fs::create_dir_all(project::data_dir()).unwrap();
    let index = json!({"projects": [
        {"path": "/music/Old/song.yaml", "title": "Old", "untitled": false, "opened": 10.0, "bookmark": "AAAA", "id": "0f0c"},
        {"path": "/data/Untitled/Untitled 2/song.yaml", "title": "Untitled 2", "untitled": true, "opened": 30.5},
        {"title": "No path"},
    ]});
    std::fs::write(project::index_file(), index.to_string()).unwrap();
    let known = project::known();
    let rows: Vec<(&str, bool, f64)> = known.iter().map(|k| (k.title.as_str(), k.untitled, k.opened)).collect();
    assert_eq!(rows, [("Untitled 2", true, 30.5), ("Old", false, 10.0)]);
    assert_eq!(known[1].song, Path::new("/music/Old/song.yaml"));
    assert_eq!((known[1].id.as_deref(), known[0].id.as_deref()), (Some("0f0c"), None));
    std::fs::write(project::index_file(), "not json").unwrap();
    assert_eq!(project::known(), []);
}
