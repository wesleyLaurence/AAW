//! The renderer's guarantees: output independent of block partition, a start
//! mid-song equal to the same frames of a render from the start, and no
//! allocation while processing.

mod common;

use aaw_engine::program::Program;
use aaw_engine::render::{Frame, Renderer};
use common::song;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// The mix and stems for consecutive blocks of the given sizes, from `from`.
fn render(program: &Arc<Program>, from: usize, sizes: &mut dyn FnMut() -> usize) -> (Vec<Frame>, Vec<Vec<Frame>>) {
    let mut r = Renderer::new(program.clone(), from, 4096);
    let mut mix = Vec::new();
    let mut stems = vec![Vec::new(); program.tracks.len()];
    let mut block = vec![[0.0; 2]; 4096];
    while !r.finished() {
        let n = sizes().min(program.total - r.position());
        r.process(&mut block[..n], |t, post| stems[t].extend_from_slice(post));
        mix.extend_from_slice(&block[..n]);
    }
    (mix, stems)
}

#[test]
fn output_does_not_depend_on_block_partition() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    let whole = render(&program, 0, &mut || 4096);
    let mut s = 3u64;
    let random = render(&program, 0, &mut || {
        s = s.wrapping_mul(2862933555777941757).wrapping_add(3037000493);
        1 + (s >> 33) as usize % 700
    });
    assert_eq!(whole.0, random.0);
    assert_eq!(whole.1, random.1);
    assert!(whole.0.iter().any(|f| f[0] != 0.0), "the song is silent");
}

#[test]
fn playing_from_the_middle_picks_up_sounding_voices() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    let whole = render(&program, 0, &mut || 512);
    // Mid-note in both the bass and a reversed layer voice.
    for from in [12345, 60000, 90001] {
        let tail = render(&program, from, &mut || 128);
        assert_eq!(&whole.0[from..], &tail.0[..], "from frame {from}");
        assert!(tail.0[..64].iter().any(|f| f[0] != 0.0), "nothing sounding at {from}");
    }
}

#[test]
fn processing_never_allocates() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    for from in [0, 30000] {
        let mut r = Renderer::new(program.clone(), from, 128);
        let mut block = vec![[0.0; 2]; 128];
        assert_no_alloc::assert_no_alloc(|| {
            while !r.finished() {
                r.process(&mut block, |_, _| {});
            }
        });
    }
}

#[test]
fn a_remixed_program_plays_as_a_compiled_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = common::write_song(dir.path());
    let base = common::program(&path, |_| {});
    let edits: [fn(&mut aaw_model::Project); 2] = [
        |p| {
            p.tracks[0].gain_db = -7.5;
            p.tracks[0].pan = 0.25;
            p.tracks[1].mute = true;
            p.session.master_gain_db = -3.0;
        },
        |p| {
            p.tracks[1].solo = true;
            p.tracks[2].solo = true;
            p.tracks[2].mute = true;
        },
    ];
    for edit in edits {
        let compiled = common::program(&path, edit);
        let mut project = aaw_model::load(&path, true).unwrap();
        edit(&mut project);
        let remixed = Arc::new(base.remix(&project).expect("the same tracks"));
        let (expected, heard) = (render(&compiled, 0, &mut || 512), render(&remixed, 0, &mut || 512));
        assert_eq!(expected, heard);
        assert_ne!(expected.0, render(&base, 0, &mut || 512).0, "the edit is audible");
    }
    // Other tracks need a compile.
    let mut project = aaw_model::load(&path, true).unwrap();
    project.tracks[0].id = "sub".into();
    assert!(base.remix(&project).is_none());
    project.tracks.pop();
    assert!(base.remix(&project).is_none());
}
