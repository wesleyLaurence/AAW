//! How a device is shared: its folder zipped, `SLUG/` and the files under
//! it, and installed from that zip, from a folder, or from the workspace
//! into a project, so the song carries the device it plays.
//!
//! A pack holds what the code hash counts, so the device installed from it
//! has the same code, and every cache made with it stays good. The same
//! folder packs to the same bytes: the entries are in order, at one time,
//! with the same modes.

use super::{find, files, hex, project_dir, read, Installed, Where};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

/// The most a device's files may come to, packed or installed.
pub const MAX_BYTES: u64 = 256 * 1024 * 1024;
/// The most files a device may have.
pub const MAX_FILES: usize = 4096;

fn io(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

/// Why a real-time device cannot be shared as it is, or None: a shared
/// one carries its `device.wasm`, built from the `device.rs` beside it, so
/// it plays where no compiler is.
fn unbuilt(device: &Installed) -> Option<String> {
    let m = &device.manifest;
    if m.run != "realtime" {
        return None;
    }
    let build = format!("daw plugin build {} builds it", m.slug);
    if !device.dir.join(&m.entry).is_file() {
        return Some(format!("{} has no {}: {build}, and a shared real-time device carries it so that no compiler is needed to play it", m.slug, m.entry));
    }
    device.stale().then(|| format!("{} was changed after {} was built: {build} again, so what is shared plays what the source says", m.source.as_deref().unwrap_or_default(), m.entry))
}

/// What the person is told of the code they install: an offline device's
/// Python runs as they do, a real-time one in a sandbox.
fn trust(device: &Installed) -> (bool, String) {
    if device.manifest.run == "realtime" {
        (true, "Its device.wasm runs in the engine's sandbox: it imports nothing but math, reaches no file and no network, and is stopped when it traps or runs past its deadline.".into())
    } else {
        (false, format!("Its {} runs as you, in this Mac's Python, whenever a song plays it, and may read and write your files: read it, or know its author, before a song plays it.", device.manifest.entry))
    }
}

/// What a reply says of a device: what it is, by whom, where it is.
fn summary(device: &Installed) -> Json {
    let m = &device.manifest;
    let mut out = json!({
        "slug": m.slug, "name": m.name, "version": m.version, "kind": m.kind, "run": m.run,
        "author": m.author, "license": m.license, "folder": device.dir, "code": device.code,
    });
    out["from"] = device.json()["from"].clone();
    out
}

/// Zips the device's folder to `to`, a `.zip` file: `SLUG/device.yaml` and
/// the rest, as the folder holds them.
pub fn pack(device: &Installed, to: &Path, replace: bool) -> Result<Json> {
    let m = &device.manifest;
    if to.extension().and_then(|e| e.to_str()) != Some("zip") {
        return Err(format!("{} is not a .zip file: a device is shared as its folder zipped, such as {}-{}.zip", to.display(), m.slug, m.version));
    }
    if to.exists() && !replace {
        return Err(format!("{} is already there: --replace writes over it", to.display()));
    }
    if let Some(why) = unbuilt(device) {
        return Err(why);
    }
    let listed = files(&device.dir)?;
    if listed.len() > MAX_FILES {
        return Err(format!("{} has {} files; a device has at most {MAX_FILES}", device.dir.display(), listed.len()));
    }
    let mut total = 0u64;
    let mut names = Vec::new();
    let parent = to.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(io(parent))?;
    let mut out = tempfile::NamedTempFile::new_in(parent).map_err(io(parent))?;
    {
        let mut zip = zip::ZipWriter::new(out.as_file_mut());
        for file in &listed {
            let rel = file.strip_prefix(&device.dir).unwrap_or(file);
            let name = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            let bytes = std::fs::read(file).map_err(io(file))?;
            total += bytes.len() as u64;
            if total > MAX_BYTES {
                return Err(format!("{} comes to more than {} MB; a device is at most that", device.dir.display(), MAX_BYTES / 1024 / 1024));
            }
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(zip::DateTime::default())
                .unix_permissions(0o644)
                .large_file(bytes.len() as u64 >= 1 << 32);
            zip.start_file(format!("{}/{name}", m.slug), options).map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(io(to))?;
            names.push(name);
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    out.as_file().sync_all().map_err(io(to))?;
    out.persist(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let bytes = std::fs::read(to).map_err(io(to))?;
    let mut warnings = Vec::new();
    if m.author.is_empty() {
        warnings.push("author is empty: the manifest says who made the device, for whoever installs it".to_string());
    }
    if m.license.is_empty() {
        warnings.push("license is empty: the manifest says what others may do with the device".to_string());
    }
    let file = std::fs::canonicalize(to).unwrap_or_else(|_| to.to_path_buf());
    Ok(json!({
        "file": file,
        "bytes": bytes.len(),
        "sha256": hex(&Sha256::digest(&bytes)),
        "files": names,
        "device": summary(device),
        "warnings": warnings,
        "next": format!("daw plugin install {} installs it on another Mac, or into a project with --project", file.display()),
    }))
}

/// The devices folder a project or the workspace keeps, as a whole path.
fn places(project: Option<&Path>) -> Result<(PathBuf, Where)> {
    Ok(match project {
        Some(p) => (project_dir(&std::fs::canonicalize(p).map_err(io(p))?), Where::Project),
        None => (super::dir(), Where::Workspace),
    })
}

/// Where an install comes from.
enum Source {
    Zip(PathBuf),
    Folder(PathBuf),
    /// The workspace's device of this slug, copied into a project.
    Workspace(Installed),
}

/// Writes the zip's files under `staging`, refusing a path that leaves it,
/// a link, and more than a device may hold; returns the folder it made.
fn unzip(file: &Path, staging: &Path) -> Result<PathBuf> {
    let reader = std::fs::File::open(file).map_err(io(file))?;
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("{}: not a zip file it can read ({e})", file.display()))?;
    let mut slug: Option<String> = None;
    let (mut total, mut count) = (0u64, 0usize);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("{}: {e}", file.display()))?;
        let raw = entry.name().map_err(|e| e.to_string())?.to_string();
        let Some(path) = entry.enclosed_name() else {
            return Err(format!("{}: {raw} is a path outside the device's folder", file.display()));
        };
        let parts: Vec<String> = path.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        // What the Finder's Compress adds and what the code hash leaves out.
        if parts.first().is_some_and(|p| p == "__MACOSX") || parts.iter().any(|p| p.starts_with('.') || p == "__pycache__") {
            continue;
        }
        if entry.is_symlink() {
            return Err(format!("{}: {raw} is a link; a device's folder holds files", file.display()));
        }
        if entry.encrypted() {
            return Err(format!("{}: {raw} is encrypted", file.display()));
        }
        let top = &parts[0];
        if parts.len() == 1 && !entry.is_dir() {
            return Err(format!("{}: {raw} is not in a folder; a shared device is its folder zipped, SLUG/device.yaml and the rest", file.display()));
        }
        match &slug {
            None => slug = Some(top.clone()),
            Some(s) if s != top => return Err(format!("{}: holds {s}/ and {top}/; a shared device is one folder", file.display())),
            _ => {}
        }
        if entry.is_dir() {
            continue;
        }
        count += 1;
        total += entry.size();
        if count > MAX_FILES || total > MAX_BYTES {
            return Err(format!("{}: more than {MAX_FILES} files or {} MB; a device is at most that", file.display(), MAX_BYTES / 1024 / 1024));
        }
        let to = staging.join(&path);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(io(dir))?;
        }
        let mut bytes = Vec::new();
        (&mut entry).take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("{}: {raw}: {e}", file.display()))?;
        if bytes.len() as u64 > entry.size() {
            return Err(format!("{}: {raw} is larger than the zip says", file.display()));
        }
        std::fs::write(&to, &bytes).map_err(io(&to))?;
    }
    let slug = slug.ok_or_else(|| format!("{}: holds no device", file.display()))?;
    let folder = staging.join(&slug);
    if !folder.join("device.yaml").is_file() {
        return Err(format!("{}: {slug}/device.yaml is not in it; a shared device is its folder zipped, with its manifest", file.display()));
    }
    Ok(folder)
}

/// Copies a device's files under `staging`, in a folder of its slug.
fn copy(from: &Path, slug: &str, staging: &Path) -> Result<PathBuf> {
    let folder = staging.join(slug);
    let listed = files(from)?;
    if listed.len() > MAX_FILES {
        return Err(format!("{} has {} files; a device has at most {MAX_FILES}", from.display(), listed.len()));
    }
    let mut total = 0u64;
    for file in listed {
        total += std::fs::metadata(&file).map_err(io(&file))?.len();
        if total > MAX_BYTES {
            return Err(format!("{} comes to more than {} MB; a device is at most that", from.display(), MAX_BYTES / 1024 / 1024));
        }
        let to = folder.join(file.strip_prefix(from).unwrap_or(&file));
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(io(dir))?;
        }
        std::fs::copy(&file, &to).map_err(io(&file))?;
    }
    Ok(folder)
}

/// Gives every file under `folder` one time, so a real-time device's
/// module, built after its source where it was packed, is not taken for
/// stale where it lands.
fn one_time(folder: &Path) -> Result<()> {
    let now = std::time::SystemTime::now();
    for file in files(folder)? {
        std::fs::File::options().write(true).open(&file).and_then(|f| f.set_modified(now)).map_err(io(&file))?;
    }
    Ok(())
}

/// Installs a device: from a `.zip` a pack made, from a folder holding a
/// manifest, or, with a project, from the workspace's device of a slug,
/// copied into the project's `devices/` so the song carries it. Into the
/// project's `devices/` with `project`, else the workspace's. A device of
/// that slug already there with other code is replaced only with `replace`.
pub fn install(source: &str, project: Option<&Path>, replace: bool) -> Result<Json> {
    let given = Path::new(source);
    let from = if given.is_file() {
        Source::Zip(given.to_path_buf())
    } else if given.is_dir() {
        if !given.join("device.yaml").is_file() {
            return Err(format!("{} holds no device.yaml: a device's folder has its manifest", given.display()));
        }
        Source::Folder(given.to_path_buf())
    } else if project.is_some() {
        Source::Workspace(find(None, source).map_err(|e| format!("{source} is not a file or a folder, and {e}"))?)
    } else {
        return Err(format!("{source} is not a file or a folder: install takes a .zip a pack made, or a device's folder; with --project, the slug of a workspace's device copies it into the project"));
    };
    let (root, place) = places(project)?;
    if let Source::Folder(dir) | Source::Workspace(Installed { dir, .. }) = &from {
        if let Ok(d) = read(dir, place) {
            if let Some(why) = unbuilt(&d) {
                return Err(why);
            }
        }
    }
    std::fs::create_dir_all(&root).map_err(io(&root))?;
    let staging = tempfile::Builder::new().prefix(".installing-").tempdir_in(&root).map_err(io(&root))?;
    let staged = match &from {
        Source::Zip(file) => unzip(file, staging.path())?,
        Source::Folder(dir) => {
            let dir = std::fs::canonicalize(dir).map_err(io(dir))?;
            let slug = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            copy(&dir, &slug, staging.path())?
        }
        Source::Workspace(d) => copy(&d.dir, &d.manifest.slug, staging.path())?,
    };
    one_time(&staged)?;
    // Said of what was given, not of where it was unpacked.
    let unpacked = format!("{}/", staging.path().display());
    let incoming = read(&staged, place).map_err(|e| format!("{}: {}", given.display(), e.replace(&unpacked, "")))?;
    let slug = incoming.manifest.slug.clone();
    let target = root.join(&slug);
    let mut replaced = Json::Null;
    if target.exists() {
        if !target.join("device.yaml").is_file() {
            return Err(format!("{} is there and is not a device: move it away first", target.display()));
        }
        let there = read(&target, place).ok();
        if there.as_ref().is_some_and(|d| d.code == incoming.code) {
            let (sandboxed, trust) = trust(there.as_ref().expect("read"));
            return Ok(json!({"device": there.as_ref().map(summary), "unchanged": true, "sandboxed": sandboxed, "trust": trust}));
        }
        let version = there.as_ref().map_or_else(|| "a manifest it cannot read".to_string(), |d| format!("version {}", d.manifest.version));
        if !replace {
            return Err(format!(
                "{slug} is already installed at {}, {version}, and its code is not this one's, version {}: --replace replaces it",
                target.display(),
                incoming.manifest.version
            ));
        }
        replaced = json!({"version": there.as_ref().map(|d| d.manifest.version.clone()), "code": there.as_ref().map(|d| d.code.clone())});
        // The old folder goes into the staging folder, which is removed
        // with it once the new one is in place.
        let old = staging.path().join(".replaced");
        std::fs::rename(&target, &old).map_err(io(&target))?;
        if let Err(e) = std::fs::rename(&staged, &target) {
            let _ = std::fs::rename(&old, &target);
            return Err(format!("{}: {e}", target.display()));
        }
    } else {
        std::fs::rename(&staged, &target).map_err(io(&target))?;
    }
    let installed = read(&target, place)?;
    let (sandboxed, trust) = trust(&installed);
    let m = &installed.manifest;
    let next = if m.run == "realtime" && !target.join(&m.entry).is_file() {
        format!("daw plugin build {slug} builds it; daw plugin check {slug} runs it once")
    } else {
        format!("daw plugin check {slug}{} runs it once", project.map_or(String::new(), |p| format!(" --project {}", p.display())))
    };
    Ok(json!({
        "device": summary(&installed),
        "source": match &from {
            Source::Zip(f) => json!({"zip": std::fs::canonicalize(f).unwrap_or_else(|_| f.clone())}),
            Source::Folder(d) => json!({"folder": std::fs::canonicalize(d).unwrap_or_else(|_| d.clone())}),
            Source::Workspace(d) => json!({"workspace": d.dir}),
        },
        "replaced": replaced,
        "unchanged": false,
        "sandboxed": sandboxed,
        "trust": trust,
        "next": next,
    }))
}

/// The Trash's folder for a file, with a name not yet used there: the
/// Finder's way, `NAME 2` after `NAME`.
fn trash_for(name: &str) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set, so there is no Trash")?;
    let trash = home.join(".Trash");
    std::fs::create_dir_all(&trash).map_err(io(&trash))?;
    let mut at = trash.join(name);
    let mut n = 2;
    while at.exists() {
        at = trash.join(format!("{name} {n}"));
        n += 1;
    }
    Ok(at)
}

/// Moves a device's folder to the Trash: the project's own with `project`,
/// else the workspace's. A song that names it plays it dry, or plays the
/// workspace's where the project's was removed.
pub fn remove(project: Option<&Path>, slug_of: &str) -> Result<Json> {
    let (root, place) = places(project)?;
    let want = crate::patches::slug(slug_of);
    let folder = root.join(&want);
    if want.is_empty() || !folder.join("device.yaml").is_file() {
        let other = if project.is_some() { "without --project, the workspace's" } else { "with --project, a project's own" };
        return Err(format!("No device {want} under {}; {other} is removed; daw plugin list shows what is installed", root.display()));
    }
    let removed = read(&folder, place).map(|d| summary(&d)).unwrap_or_else(|e| json!({"slug": want, "problem": e}));
    let to = trash_for(&want)?;
    std::fs::rename(&folder, &to).map_err(|e| format!("{} could not be moved to the Trash: {e}", folder.display()))?;
    let now = find(project, &want).ok().map(|d| summary(&d));
    Ok(json!({"removed": removed, "folder": folder, "trash": to, "now": now}))
}
