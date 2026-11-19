//! Patterns converted (D190): a song of pads and pattern clips, converted to
//! Drum Racks and note clips when it is read, renders as the MIDI songs the
//! other tests play. That the conversion keeps every hit as it sounded was
//! measured sample for sample against the pattern songs' own renders while
//! the engine still played patterns (D190); the model's tests hold the
//! conversion to what it wrote then (D191).

mod common;

use aaw_engine::program::compile;
use aaw_engine::render::{Frame, Renderer};
use common::{write_pattern_mix_song, write_pattern_song};

use std::path::Path;

fn render(p: &aaw_model::Project, dir: &Path) -> Vec<Frame> {
    let program = std::sync::Arc::new(compile(p, dir).unwrap_or_else(|e| panic!("{e}")));
    let mut r = Renderer::new(program.clone(), 0, 4096);
    let mut out = vec![[0.0; 2]; program.total];
    for chunk in out.chunks_mut(4096) {
        r.render(chunk, |_, _, _| {});
    }
    out
}

/// The same but for the last bits of a sample, where voices are summed in
/// another order.
fn assert_same(a: &[Frame], b: &[Frame]) {
    assert!(a.iter().any(|f| f[0].abs() > 0.01), "the song is silent");
    assert_eq!(a.len(), b.len());
    let (worst, at) = a.iter().zip(b).enumerate().fold((0.0f64, 0), |(w, at), (i, (x, y))| {
        let d = (x[0] - y[0]).abs().max((x[1] - y[1]).abs());
        if d > w { (d, i) } else { (w, at) }
    });
    assert!(worst < 1e-9, "{worst} apart at frame {at}: {:?} against {:?}", a[at], b[at]);
}

#[test]
fn the_pattern_songs_converted_play_as_the_test_songs() {
    for (midi, patterns) in [
        (common::write_song as fn(&Path) -> std::path::PathBuf, write_pattern_song as fn(&Path) -> std::path::PathBuf),
        (common::write_mix_song, write_pattern_mix_song),
    ] {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let written = aaw_model::load(&midi(a.path()), true).unwrap();
        let path = patterns(b.path());
        let (converted, was) = aaw_model::convert::read(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(was);
        assert_same(&render(&written, a.path()), &render(&converted, b.path()));
        // Read from its file, it is converted the same.
        assert_eq!(aaw_model::to_yaml(&aaw_model::load(&path, true).unwrap()), aaw_model::to_yaml(&converted));
    }
}
