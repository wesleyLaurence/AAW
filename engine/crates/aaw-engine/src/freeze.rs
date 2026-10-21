//! A track frozen to audio: what its instrument, inserts and their lanes
//! make, rendered once to a file that plays in their place.

use crate::program::{compile_scoped, Cache, Scope, StemKind};
use crate::render::Renderer;
use aaw_model::Project;
use std::path::Path;
use std::sync::Arc;

/// Renders what track `track` of `p` sounds like as its inserts leave it,
/// before its fader, pan, mute, solo and sends, over the whole song: stereo
/// frames at the song's rate, one per frame of the song's length. The
/// track's own `frozen` is set aside, so a frozen track renders from its
/// content again; the tracks that key its sidechains play along as they
/// are. The file this makes is what the track plays while frozen.
pub fn freeze_track(p: &Project, directory: &Path, track: &str, block_size: usize) -> Result<Vec<[f32; 2]>, String> {
    if block_size == 0 {
        return Err("block_size must be positive".into());
    }
    let mut thawed = p.clone();
    let t = thawed
        .tracks
        .iter_mut()
        .find(|t| t.id == track)
        .ok_or_else(|| format!("Unknown track: {track}"))?;
    t.frozen = None;
    let program = Arc::new(compile_scoped(&thawed, directory, &mut Cache::default(), Scope::Channel(track))?);
    let index = program
        .stem_index(StemKind::Track, track)
        .ok_or_else(|| format!("{track} has no stem to freeze"))?;
    let total = program.total;
    if total == 0 {
        return Err("The song has no frames to freeze".into());
    }
    let mut out = vec![[0.0f32; 2]; total];
    let mut block = vec![[0.0; 2]; block_size];
    let mut renderer = Renderer::new(program.clone(), 0, block_size);
    renderer.tap_before_faders();
    // The output trails the transport by the program's latency; rendering
    // that much further brings out the song's last frames, each stem block
    // placed by the timeline frame it starts at.
    while !renderer.finished() {
        let n = block_size.min(program.total + program.latency - renderer.position());
        renderer.render(&mut block[..n], |i, x, at| {
            if i != index {
                return;
            }
            let lo = (-at).clamp(0, x.len() as i64) as usize;
            let hi = (total as i64 - at).clamp(0, x.len() as i64) as usize;
            for (k, f) in (lo..hi).zip(&x[lo..hi]) {
                let dst = &mut out[(at + k as i64) as usize];
                dst[0] = f[0] as f32;
                dst[1] = f[1] as f32;
            }
        });
    }
    if !out.iter().all(|f| f[0].is_finite() && f[1].is_finite()) {
        return Err(format!("{track}: the freeze has samples that are not numbers"));
    }
    Ok(out)
}
