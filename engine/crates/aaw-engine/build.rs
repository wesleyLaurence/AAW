//! Links libsndfile, the library behind Python's `soundfile`, so decoded audio
//! and written PCM match the Python engine. Set SNDFILE_LIB_DIR to choose a
//! copy; otherwise Homebrew's is used when present, then the linker's defaults.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SNDFILE_LIB_DIR");
    let dir = std::env::var("SNDFILE_LIB_DIR").ok().or_else(|| {
        let out = Command::new("brew").args(["--prefix", "libsndfile"]).output().ok()?;
        out.status
            .success()
            .then(|| format!("{}/lib", String::from_utf8_lossy(&out.stdout).trim()))
    });
    if let Some(dir) = dir {
        println!("cargo:rustc-link-search=native={dir}");
    }
    println!("cargo:rustc-link-lib=dylib=sndfile");
}
