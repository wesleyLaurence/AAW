//! How a device is shared: its folder packed to a zip, the same bytes for
//! the same folder, installed elsewhere with the same code, a zip that
//! would leave its folder refused, and a real-time device packed only as
//! built.

use aaw_host::devices::{self, share, Where};
use std::io::Write;
use std::path::Path;

fn names(zip_file: &Path) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_file).unwrap()).unwrap();
    (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().unwrap().to_string()).collect()
}

/// A zip of the given entries, a `None` body a link.
fn zip_of(file: &Path, entries: &[(&str, Option<&str>)]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(file).unwrap());
    for (name, body) in entries {
        match body {
            Some(body) => {
                zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            None => zip.add_symlink(*name, "/etc/passwd", zip::write::SimpleFileOptions::default()).unwrap(),
        }
    }
    zip.finish().unwrap();
}

#[test]
fn a_device_packs_to_the_same_bytes_and_installs_elsewhere_with_the_same_code() {
    let here = tempfile::tempdir().unwrap();
    let there = tempfile::tempdir().unwrap();
    let made = devices::new("Tilt", &devices::project_dir(here.path()), false, "effect").unwrap();
    let file = here.path().join("out/tilt-0.1.zip");
    let packed = share::pack(&made, &file, false).unwrap();
    assert_eq!(names(&file), ["tilt/device.py", "tilt/device.yaml"]);
    assert_eq!(packed["files"], serde_json::json!(["device.py", "device.yaml"]));
    assert_eq!(packed["device"]["code"], made.code.as_str());
    assert!(packed["warnings"][0].as_str().unwrap().starts_with("author is empty"));
    let first = std::fs::read(&file).unwrap();
    assert!(share::pack(&made, &file, false).unwrap_err().contains("--replace writes over it"));
    share::pack(&made, &file, true).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), first, "the same folder packs to the same bytes");
    assert!(share::pack(&made, &here.path().join("tilt.tar"), false).unwrap_err().contains("is not a .zip file"));

    // Installed into another project, it has the same code, so what was
    // made with it is still good; a second install changes nothing.
    let installed = share::install(file.to_str().unwrap(), Some(there.path()), false).unwrap();
    assert_eq!(installed["device"]["code"], made.code.as_str());
    assert_eq!(installed["device"]["from"], "project");
    assert_eq!(installed["unchanged"], false);
    assert_eq!(installed["sandboxed"], false);
    assert!(installed["trust"].as_str().unwrap().contains("runs as you"));
    let landed = there.path().join("devices/tilt");
    assert_eq!(devices::read(&landed, Where::Project).unwrap().code, made.code);
    assert_eq!(share::install(file.to_str().unwrap(), Some(there.path()), false).unwrap()["unchanged"], true);
    // Nothing is left of the install beside it.
    let left: Vec<_> = std::fs::read_dir(there.path().join("devices")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left, ["tilt"]);

    // Other code of the same slug is replaced only when asked.
    std::fs::write(landed.join("device.py"), "def process(audio, sample_rate, fields):\n    return audio\n").unwrap();
    let edited = devices::read(&landed, Where::Project).unwrap().code;
    let refused = share::install(file.to_str().unwrap(), Some(there.path()), false).unwrap_err();
    assert!(refused.contains("tilt is already installed at") && refused.contains("--replace replaces it"), "{refused}");
    let replaced = share::install(file.to_str().unwrap(), Some(there.path()), true).unwrap();
    assert_eq!(replaced["replaced"]["code"], edited.as_str());
    assert_eq!(devices::read(&landed, Where::Project).unwrap().code, made.code);

    // A folder installs as a zip does.
    let third = tempfile::tempdir().unwrap();
    let copied = share::install(made.dir.to_str().unwrap(), Some(third.path()), false).unwrap();
    assert_eq!(copied["device"]["code"], made.code.as_str());
    assert!(share::install(here.path().to_str().unwrap(), Some(third.path()), false).unwrap_err().contains("holds no device.yaml"));
    assert!(share::install("/no/such/file.zip", None, false).unwrap_err().contains("is not a file or a folder"));
}

#[test]
fn a_zip_that_would_leave_its_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("song");
    std::fs::create_dir(&project).unwrap();
    let manifest = "format: 1\nname: Pass\n";
    let code = "def process(audio, sample_rate, fields):\n    return audio\n";
    let refused = |entries: &[(&str, Option<&str>)]| {
        let file = dir.path().join("bad.zip");
        zip_of(&file, entries);
        share::install(file.to_str().unwrap(), Some(&project), false).unwrap_err()
    };
    assert!(refused(&[("pass/device.yaml", Some(manifest)), ("../evil.py", Some(code))]).contains("outside the device's folder"));
    // Said of the zip, not of where it was unpacked.
    let e = refused(&[("pass/device.yaml", Some(manifest))]);
    assert!(e.ends_with("bad.zip: pass/device.yaml: the manifest's entry device.py is not there"), "{e}");
    assert!(refused(&[("pass/device.yaml", Some(manifest)), ("other/device.py", Some(code))]).contains("holds pass/ and other/"));
    assert!(refused(&[("device.yaml", Some(manifest))]).contains("is not in a folder"));
    assert!(refused(&[("pass/device.py", Some(code))]).contains("pass/device.yaml is not in it"));
    assert!(refused(&[("pass/device.yaml", Some(manifest)), ("pass/device.py", Some(code)), ("pass/link", None)]).contains("is a link"));
    assert!(refused(&[("Pass Device/device.yaml", Some(manifest)), ("Pass Device/device.py", Some(code))]).contains("is not a slug"));
    std::fs::write(dir.path().join("not.zip"), "text").unwrap();
    assert!(share::install(dir.path().join("not.zip").to_str().unwrap(), Some(&project), false).unwrap_err().contains("not a zip file"));
    // Nothing of a refused install is left.
    let left: Vec<_> = std::fs::read_dir(project.join("devices")).unwrap().collect();
    assert!(left.is_empty());

    // What the Finder's Compress adds is left out.
    let file = dir.path().join("pass.zip");
    zip_of(
        &file,
        &[
            ("pass/device.yaml", Some(manifest)),
            ("pass/device.py", Some(code)),
            ("pass/.DS_Store", Some("x")),
            ("__MACOSX/pass/._device.py", Some("x")),
            ("pass/__pycache__/device.cpython-312.pyc", Some("x")),
        ],
    );
    let installed = share::install(file.to_str().unwrap(), Some(&project), false).unwrap();
    assert_eq!(installed["device"]["slug"], "pass");
    let mut landed: Vec<_> = std::fs::read_dir(project.join("devices/pass")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    landed.sort();
    assert_eq!(landed, ["device.py", "device.yaml"]);
}

#[test]
fn a_realtime_device_is_packed_only_as_built_and_lands_not_stale() {
    let here = tempfile::tempdir().unwrap();
    let there = tempfile::tempdir().unwrap();
    let folder = devices::project_dir(here.path()).join("edge");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("device.yaml"), "format: 1\nname: Edge\nrun: realtime\nsource: device.rs\nauthor: Someone\nlicense: MIT\n").unwrap();
    std::fs::write(folder.join("device.rs"), "// the source\n").unwrap();
    let file = here.path().join("edge.zip");
    let unbuilt = devices::read(&folder, Where::Project).unwrap();
    assert!(share::pack(&unbuilt, &file, false).unwrap_err().contains("edge has no device.wasm: daw plugin build edge builds it"));
    // The module is not looked into by a pack: what plays it checks it.
    std::fs::write(folder.join("device.wasm"), b"\0asm").unwrap();
    let source = std::fs::File::options().write(true).open(folder.join("device.rs")).unwrap();
    source.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60)).unwrap();
    let stale = devices::read(&folder, Where::Project).unwrap();
    assert!(stale.stale());
    assert!(share::pack(&stale, &file, false).unwrap_err().contains("device.rs was changed after device.wasm was built"));
    assert!(share::install(folder.to_str().unwrap(), Some(there.path()), false).unwrap_err().contains("was changed after"));
    let wasm = std::fs::File::options().write(true).open(folder.join("device.wasm")).unwrap();
    wasm.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(120)).unwrap();
    let built = devices::read(&folder, Where::Project).unwrap();
    let packed = share::pack(&built, &file, false).unwrap();
    assert_eq!(packed["warnings"], serde_json::json!([]));
    // Unzipped, its files are given one time, so the module built after its
    // source is not taken for stale where it lands.
    let installed = share::install(file.to_str().unwrap(), Some(there.path()), false).unwrap();
    assert_eq!(installed["sandboxed"], true);
    let landed = devices::read(&there.path().join("devices/edge"), Where::Project).unwrap();
    assert!(!landed.stale());
    assert_eq!(landed.code, built.code);
}

#[test]
fn the_spec_s_schema_is_the_manifest_the_host_reads() {
    let schema = aaw_model::contract::device_manifest_schema();
    let keys: Vec<&str> = schema["properties"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, devices::MANIFEST_KEYS);
    let field = &schema["$defs"]["field"];
    let parts: Vec<&str> = field["properties"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(parts, devices::FIELD_PARTS);
    let types: Vec<&str> = field["properties"]["type"]["enum"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
    assert_eq!(types, devices::FIELD_TYPES);
    assert_eq!(schema["properties"]["format"]["const"], devices::FORMAT);
    // Every scaffold is a manifest of format 1, with no key the schema lacks.
    let dir = tempfile::tempdir().unwrap();
    for (name, realtime, kind) in [("A", false, "effect"), ("B", true, "effect"), ("C", false, "instrument"), ("D", true, "instrument"), ("E", false, "midi")] {
        let made = devices::new(name, dir.path(), realtime, kind).unwrap();
        assert_eq!(made.manifest.format, 1);
        let text = std::fs::read_to_string(made.dir.join("device.yaml")).unwrap();
        assert!(text.contains("daw describe plugins is the spec"), "{text}");
    }
}
