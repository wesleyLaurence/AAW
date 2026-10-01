//! Waveform peaks: the least and greatest sample of each stretch of a track's
//! voices, at several resolutions, and the identity that says when they can
//! be kept.

mod common;

use aaw_engine::peaks::{peaks, BASE, COARSEST, STEP};
use aaw_engine::program::{compile_cached, Cache, Voice};
use common::{song, write_song};

/// The voices summed frame by frame, each from its own position.
fn summed(voices: &[Voice], total: usize) -> Vec<[f64; 2]> {
    let mut out = vec![[0.0; 2]; total];
    for v in voices {
        for i in 0..v.length {
            let Some(frame) = out.get_mut(v.start as usize + i) else { break };
            frame[0] += v.sample(i, 0);
            frame[1] += v.sample(i, 1);
        }
    }
    out
}

#[test]
fn peaks_hold_the_least_and_greatest_sample_of_each_bucket() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    for track in &program.tracks {
        let made = peaks(&track.voices, program.total);
        assert_eq!(made.frames, program.total);
        let finest = &made.levels[0];
        assert_eq!((finest.frames_per_bucket, finest.buckets()), (BASE, program.total.div_ceil(BASE)));
        let audio = summed(&track.voices, program.total);
        let mut loud = 0;
        for (bucket, frames) in audio.chunks(BASE).enumerate() {
            let lo = frames.iter().flat_map(|f| [f[0], f[1]]).fold(f64::INFINITY, f64::min);
            let hi = frames.iter().flat_map(|f| [f[0], f[1]]).fold(f64::NEG_INFINITY, f64::max);
            let (shown_lo, shown_hi) = (finest.data[2 * bucket] as f64 / 127.0, finest.data[2 * bucket + 1] as f64 / 127.0);
            // Never less than the bucket holds, and within a step of it.
            assert!(shown_lo <= lo.max(-1.0) + 1e-12 && shown_lo > lo.max(-1.0) - 1.0 / 127.0 - 1e-12, "{}: bucket {bucket}", track.id);
            assert!(shown_hi >= hi.min(1.0) - 1e-12 && shown_hi < hi.min(1.0) + 1.0 / 127.0 + 1e-12, "{}: bucket {bucket}", track.id);
            loud += usize::from(hi - lo > 0.05);
        }
        assert!(loud > 100, "{} plays", track.id);
        // Each coarser level covers the one before it, down to a size worth drawing from.
        for pair in made.levels.windows(2) {
            assert_eq!(pair[1].frames_per_bucket, pair[0].frames_per_bucket * STEP);
            assert_eq!(pair[1].buckets(), pair[0].buckets().div_ceil(STEP));
            for (bucket, fine) in pair[0].data.chunks(2 * STEP).enumerate() {
                let lo = fine.iter().step_by(2).min().unwrap();
                let hi = fine.iter().skip(1).step_by(2).max().unwrap();
                assert_eq!((pair[1].data[2 * bucket], pair[1].data[2 * bucket + 1]), (*lo, *hi));
            }
        }
        assert!(made.levels.len() > 1 && made.levels.last().unwrap().buckets() <= COARSEST);
    }
    // The hats start a beat in: before them there is silence.
    let hats = program.tracks.iter().find(|t| t.id == "hats").unwrap();
    let silent = (60.0 / 128.0 * 48000.0 / BASE as f64) as usize - 1;
    assert!(peaks(&hats.voices, program.total).levels[0].data[..2 * silent].iter().all(|x| *x == 0));
}

#[test]
fn peaks_are_cut_at_the_session_end_and_clip_at_full_scale() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    let bass = program.tracks.iter().find(|t| t.id == "bass").unwrap();
    let half = peaks(&bass.voices, program.total / 2);
    let whole = peaks(&bass.voices, program.total);
    let buckets = (program.total / 2) / BASE;
    assert_eq!(half.levels[0].data[..2 * buckets], whole.levels[0].data[..2 * buckets]);
    assert_eq!(half.levels[0].buckets(), (program.total / 2).div_ceil(BASE));
    // A sum louder than full scale is drawn as full scale.
    let loud: Vec<Voice> = bass.voices.iter().map(|v| Voice { gain: v.gain * 40.0, ..v.clone() }).collect();
    let clipped = peaks(&loud, program.total);
    assert!(clipped.levels[0].data.iter().any(|x| *x == 127) && clipped.levels[0].data.iter().any(|x| *x == -127));
    assert_eq!(peaks(&[], 1000).levels[0].data, vec![0i8; 2 * 16]);
}

#[test]
fn a_tracks_identity_changes_only_with_what_its_voices_are_made_from() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let mut cache = Cache::default();
    let mut project = aaw_model::load(&path, true).unwrap();
    let ids = |p: &aaw_model::Project, cache: &mut Cache| -> Vec<(String, u64)> {
        let program = compile_cached(p, dir.path(), cache).unwrap();
        program.tracks.iter().map(|t| (t.id.clone(), t.voices_id)).collect()
    };
    let before = ids(&project, &mut cache);
    assert_eq!(before, ids(&project, &mut Cache::default()), "the same song has the same identities");
    // Levels, pans and effects leave the voices as they are.
    project.tracks[0].gain_db = -9.0;
    project.tracks[1].pan = 0.5;
    project.tracks[2].mute = true;
    assert_eq!(before, ids(&project, &mut cache));
    // A moved clip changes its track and no other; so does a pattern, for the tracks that play it.
    project.tracks[1].clips[0].at = aaw_model::Beat::int(2);
    let moved = ids(&project, &mut cache);
    assert_eq!((moved[0] == before[0], moved[1] == before[1], moved[2] == before[2]), (true, false, true));
    project.patterns.get_mut("bass").unwrap().events.pop();
    let edited = ids(&project, &mut cache);
    assert_eq!((edited[0] == moved[0], edited[1] == moved[1], edited[2] == moved[2]), (false, true, false));
    // The session length is part of it, since nothing sounds past the end.
    project.session.length_beats = aaw_model::Beat::int(20);
    let longer = ids(&project, &mut cache);
    assert!(longer.iter().zip(&edited).all(|(a, b)| a.1 != b.1));
}
