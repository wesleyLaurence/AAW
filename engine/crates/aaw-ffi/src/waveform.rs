//! Waveforms for the app's clips: the peaks of each track's voices, worked
//! out on a thread of their own after each revision is compiled and kept by
//! the track's identity, so that an edit redoes only the tracks whose audio
//! it changed and a fader or a knob redoes none. An audio clip draws its file
//! instead, whose peaks are worked out once for the file.

use crate::files::Files;
use crate::SongObserver;
use aaw_engine::peaks::{file_peaks, peaks, Peaks};
use aaw_engine::program::{Program, Voice};
use aaw_host::session::Doc;
use aaw_host::tree::Node;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// One resolution of a track's peaks.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PeakLevel {
    pub frames_per_bucket: u32,
    /// For each bucket two signed bytes: its least and its greatest sample,
    /// with full scale at 127.
    pub data: Vec<u8>,
}

/// What a track's clips play, before its effects and fader, as peaks.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TrackPeaks {
    /// Equal for tracks whose audio is the same.
    pub identity: u64,
    /// Frames to a beat, to place the peaks on the timeline.
    pub frames_per_beat: f64,
    /// The session length the peaks cover, in frames.
    pub frames: u64,
    /// From the finest to the coarsest.
    pub levels: Vec<PeakLevel>,
}

/// The identity of a track's audio at a revision.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TrackWave {
    /// The track's key.
    pub track: u64,
    pub identity: u64,
}

/// A sample file as it is, as peaks: what an audio clip of it draws.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FilePeaks {
    /// The file's identity, as its `FileView` and its clips have it.
    pub identity: u64,
    /// Frames of the file to a second.
    pub frames_per_second: f64,
    pub frames: u64,
    /// From the finest to the coarsest.
    pub levels: Vec<PeakLevel>,
}

/// The waveforms of a revision: which audio each track has, and the peaks of
/// audio the app has not been sent before. The peaks of a track whose audio
/// is still being worked out follow in a later update for the same revision.
/// The app may forget peaks no track of the latest update has. A track of
/// audio clips alone is not listed: an audio clip draws its file.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Waveforms {
    pub revision: u64,
    pub tracks: Vec<TrackWave>,
    pub peaks: Vec<TrackPeaks>,
    /// The identities of the files audio clips play at the revision.
    pub files: Vec<u64>,
    /// The peaks of files the app has not been sent before.
    pub file_peaks: Vec<FilePeaks>,
}

struct Track {
    key: u64,
    identity: u64,
    voices: Arc<Vec<Voice>>,
}

struct Job {
    revision: u64,
    tracks: Vec<Track>,
    /// The files audio clips play, by identity.
    files: Vec<(u64, PathBuf)>,
    total: usize,
    frames_per_beat: f64,
}

/// Hands each compiled revision to the thread that works out its peaks.
pub struct Worker {
    jobs: Sender<Job>,
}

/// Jobs whose peaks stay kept after the last one that used them, so that undo
/// and redo find them.
const KEEP: u64 = 16;

impl Worker {
    pub fn new(observer: Arc<dyn SongObserver>) -> Worker {
        let (jobs, queue) = channel();
        let spawned = std::thread::Builder::new().name("aaw-waveforms".into()).spawn(move || run(queue, observer));
        // Without the thread there are no waveforms, and the song still plays.
        drop(spawned);
        Worker { jobs }
    }

    /// Asks for the waveforms of a revision's program. `doc` is that revision
    /// of the song in `directory`, which gives each track its key.
    pub fn compiled(&self, revision: u64, program: &Program, doc: &Doc, files: &Files, directory: &Path) {
        let tree = doc.tree();
        // A track of audio clips alone has nothing that draws its peaks.
        let keys: HashMap<&str, u64> = doc
            .project
            .tracks
            .iter()
            .zip(tree.get("tracks").map(Node::items).unwrap_or(&[]))
            .filter(|(t, _)| !t.clips.is_empty() || t.audio.is_empty())
            .map(|(t, item)| (t.id.as_str(), item.handle))
            .collect();
        let tracks = program
            .tracks
            .iter()
            .filter_map(|t| {
                Some(Track {
                    key: *keys.get(t.id.as_str())?,
                    identity: t.voices_id,
                    voices: t.voices.clone(),
                })
            })
            .collect();
        let mut played: Vec<(u64, PathBuf)> = Vec::new();
        for clip in doc.project.tracks.iter().flat_map(|t| &t.audio) {
            let Some(sample) = doc.project.samples.get(&clip.sample) else { continue };
            let Some(file) = files.of(directory, sample) else { continue };
            if played.iter().all(|(identity, _)| *identity != file.identity) {
                played.push((file.identity, directory.join(&sample.path)));
            }
        }
        let _ = self.jobs.send(Job {
            revision,
            tracks,
            files: played,
            total: program.total,
            frames_per_beat: program.rate as f64 * 60.0 / program.tempo,
        });
    }
}

fn levels(peaks: &Peaks) -> Vec<PeakLevel> {
    peaks
        .levels
        .iter()
        .map(|l| PeakLevel {
            frames_per_bucket: l.frames_per_bucket as u32,
            data: l.data.iter().map(|x| *x as u8).collect(),
        })
        .collect()
}

fn file_record(identity: u64, peaks: &Peaks, rate: u32) -> FilePeaks {
    FilePeaks {
        identity,
        frames_per_second: f64::from(rate),
        frames: peaks.frames as u64,
        levels: levels(peaks),
    }
}

fn record(identity: u64, frames_per_beat: f64, peaks: &Peaks) -> TrackPeaks {
    TrackPeaks {
        identity,
        frames_per_beat,
        frames: peaks.frames as u64,
        levels: levels(peaks),
    }
}

fn run(queue: Receiver<Job>, observer: Arc<dyn SongObserver>) {
    // Peaks by identity, with the job that last used them.
    let mut kept: HashMap<u64, (u64, Arc<Peaks>)> = HashMap::new();
    // The identities whose peaks the app has.
    let mut sent: HashSet<u64> = HashSet::new();
    // The same for files, whose peaks are kept with their rate.
    let mut kept_files: HashMap<u64, (u64, Arc<(Peaks, u32)>)> = HashMap::new();
    let mut sent_files: HashSet<u64> = HashSet::new();
    let mut count = 0u64;
    let mut waiting: Option<Job> = None;
    loop {
        let Some(mut job) = waiting.take().or_else(|| queue.recv().ok()) else { return };
        // Only the latest revision is worth drawing.
        while let Ok(newer) = queue.try_recv() {
            job = newer;
        }
        count += 1;
        let wanted: HashSet<u64> = job.tracks.iter().map(|t| t.identity).collect();
        sent.retain(|id| wanted.contains(id));
        let played: HashSet<u64> = job.files.iter().map(|(identity, _)| *identity).collect();
        sent_files.retain(|id| played.contains(id));
        let update = |peaks: Vec<TrackPeaks>, file_peaks: Vec<FilePeaks>| Waveforms {
            revision: job.revision,
            tracks: job.tracks.iter().map(|t| TrackWave { track: t.key, identity: t.identity }).collect(),
            peaks,
            files: job.files.iter().map(|(identity, _)| *identity).collect(),
            file_peaks,
        };

        // What is kept goes at once; the rest as each is worked out.
        let mut ready = Vec::new();
        let mut missing: Vec<&Track> = Vec::new();
        for t in &job.tracks {
            match kept.get_mut(&t.identity) {
                Some((used, peaks)) => {
                    *used = count;
                    if sent.insert(t.identity) {
                        ready.push(record(t.identity, job.frames_per_beat, peaks));
                    }
                }
                None if missing.iter().all(|m| m.identity != t.identity) => missing.push(t),
                None => {}
            }
        }
        let mut ready_files = Vec::new();
        let mut missing_files: Vec<&(u64, PathBuf)> = Vec::new();
        for file in &job.files {
            match kept_files.get_mut(&file.0) {
                Some((used, peaks)) => {
                    *used = count;
                    if sent_files.insert(file.0) {
                        ready_files.push(file_record(file.0, &peaks.0, peaks.1));
                    }
                }
                None => missing_files.push(file),
            }
        }
        observer.waveforms(update(ready, ready_files));

        // Files first: a file's peaks are what a clip just dropped is waiting
        // for, and they are read once. A file that cannot be read has none.
        for (identity, path) in missing_files {
            if waiting.is_none() {
                waiting = queue.try_recv().ok();
            }
            if waiting.is_some() {
                break;
            }
            let Ok(made) = file_peaks(path) else { continue };
            let made = Arc::new(made);
            kept_files.insert(*identity, (count, made.clone()));
            if sent_files.insert(*identity) {
                observer.waveforms(update(Vec::new(), vec![file_record(*identity, &made.0, made.1)]));
            }
        }

        // A newer revision makes this one's remaining tracks not worth doing.
        let stale = AtomicBool::new(waiting.is_some());
        let next = AtomicUsize::new(0);
        let (done, results) = channel::<(u64, Peaks)>();
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4).min(missing.len());
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let done = done.clone();
                let (missing, stale, next, total) = (&missing, &stale, &next, job.total);
                scope.spawn(move || {
                    while !stale.load(Relaxed) {
                        let Some(track) = missing.get(next.fetch_add(1, Relaxed)) else { break };
                        let _ = done.send((track.identity, peaks(&track.voices, total)));
                    }
                });
            }
            drop(done);
            for (identity, peaks) in results {
                let peaks = Arc::new(peaks);
                kept.insert(identity, (count, peaks.clone()));
                if waiting.is_none() {
                    waiting = queue.try_recv().ok();
                    stale.store(waiting.is_some(), Relaxed);
                }
                if waiting.is_none() && sent.insert(identity) {
                    observer.waveforms(update(vec![record(identity, job.frames_per_beat, &peaks)], Vec::new()));
                }
            }
        });
        kept.retain(|_, (used, _)| *used + KEEP > count);
        kept_files.retain(|_, (used, _)| *used + KEEP > count);
    }
}
