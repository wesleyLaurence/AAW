//! Favorites: a file in the workspace library that keeps a sample, a patch,
//! a rack, an instrument rack or a device kind at hand, in the order added,
//! each checked against the library when added and said to be missing when
//! the library no longer has it.

use aaw_host::favorites::{self, Favorite, Kind};
use aaw_host::{patches, racks};
use std::path::Path;
use std::sync::OnceLock;

/// The workspace every test writes into: a folder of this test run's, set
/// once before any test reads it, so the person's library is never written.
/// The tests of this file share one favorites file, so each uses names of
/// its own.
fn workspace() -> &'static Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: set once, before any test reads it; every test calls this first.
        unsafe { std::env::set_var("AAW_WORKSPACE", dir.path()) };
        dir
    })
    .path()
}

fn names(all: &[Favorite]) -> Vec<(Kind, &str)> {
    all.iter().map(|f| (f.kind, f.what.as_str())).collect()
}

#[test]
fn favorites_are_checked_added_in_order_listed_and_removed() {
    let ws = workspace();
    // A patch of the factory's, a device kind, a saved rack and a sample file.
    let (_, added, new) = favorites::add(Kind::Patch, "soft pad").unwrap();
    assert!(new);
    assert_eq!(added, Favorite { kind: Kind::Patch, what: "Soft Pad".into() });
    let (_, _, new) = favorites::add(Kind::Device, "Arpeggiator").unwrap();
    assert!(new);
    let chain = racks::validate(&aaw_model::yaml_load::load("- {type: limiter}\n").unwrap()).unwrap();
    racks::save("Fav Glue", "", &[], &chain, "user", false).unwrap();
    favorites::add(Kind::Rack, "fav glue").unwrap();
    let sample = ws.join("fav-kick.wav");
    std::fs::write(&sample, b"RIFF").unwrap();
    let (all, added, _) = favorites::add(Kind::Sample, sample.to_str().unwrap()).unwrap();
    assert_eq!(added.what, std::fs::canonicalize(&sample).unwrap().to_string_lossy());
    assert!(favorites::file().is_file());
    assert_eq!(
        names(&all)[..3],
        [(Kind::Patch, "Soft Pad"), (Kind::Device, "arpeggiator"), (Kind::Rack, "Fav Glue")]
    );

    // Added again, by its slug: not twice, and still where it was.
    let (all, have, new) = favorites::add(Kind::Patch, "SOFT-PAD").unwrap();
    assert!(!new && have.what == "Soft Pad");
    assert_eq!(all.iter().filter(|f| f.kind == Kind::Patch).count(), 1);
    assert!(favorites::has(&all, Kind::Patch, "soft pad") && !favorites::has(&all, Kind::InstrumentRack, "soft pad"));

    // What the library does not have is refused with the way to find it.
    assert!(favorites::add(Kind::Patch, "no such patch").unwrap_err().contains("daw patch list"));
    assert!(favorites::add(Kind::Rack, "no such rack").unwrap_err().contains("daw rack list"));
    assert!(favorites::add(Kind::InstrumentRack, "no such").unwrap_err().contains("instrument rack list"));
    assert!(favorites::add(Kind::Device, "piano").unwrap_err().contains("not a device kind"));
    assert!(favorites::add(Kind::Sample, "/no/such/file.wav").unwrap_err().contains("No file"));

    // Listed with what the library has now; the rack's file gone makes it missing.
    let listed = favorites::listed(&all);
    let rack = listed.iter().find(|l| l.favorite.kind == Kind::Rack).unwrap();
    assert!(!rack.missing && rack.slug == "fav-glue" && rack.name == "Fav Glue");
    std::fs::remove_file(racks::dir().join("fav-glue.yaml")).unwrap();
    let listed = favorites::listed(&all);
    let rack = listed.iter().find(|l| l.favorite.kind == Kind::Rack).unwrap();
    assert!(rack.missing && rack.name == "Fav Glue");
    let kick = listed.iter().find(|l| l.favorite.kind == Kind::Sample).unwrap();
    assert!(!kick.missing && kick.name == "fav-kick.wav");

    // Removed by the name as the file has it, or by the file's path; one
    // not there is refused.
    let (all, removed) = favorites::remove(Kind::Rack, "Fav Glue").unwrap();
    assert_eq!(removed.what, "Fav Glue");
    assert!(!favorites::has(&all, Kind::Rack, "fav glue"));
    let (all, _) = favorites::remove(Kind::Sample, sample.to_str().unwrap()).unwrap();
    assert!(!all.iter().any(|f| f.kind == Kind::Sample));
    assert!(favorites::remove(Kind::Rack, "Fav Glue").unwrap_err().contains("not a favorite rack"));
    assert_eq!(favorites::read().unwrap(), all);
    // The factory patch is still a patch.
    assert!(patches::find("Soft Pad").is_ok());
}

#[test]
fn a_file_that_cannot_be_read_is_said() {
    workspace();
    // Written by hand with a kind that is none: read refuses it, naming the file.
    let text = favorites::text_of(&[Favorite { kind: Kind::Device, what: "synth".into() }]);
    assert!(favorites::parse(&text).is_ok());
    assert!(favorites::parse("favorites:\n- {kind: song, name: x}\n").unwrap_err().contains("not a kind"));
}
