//! The song's length, which is its content: the next whole bar after the
//! last sound. A host sets `session.length_beats` to it after every edit, so
//! nothing a person places is ever past the end (D130). A song saved with a
//! longer length keeps it until its next edit.

use crate::rules::owners;
use crate::schema::{AudioClip, Project};
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};

impl AudioClip {
    /// Beats of the clip's fade out that sound after its end: the fade, no
    /// more than the file holds past the end, and none for a clip that plays
    /// to its file's end and fades over the last of it. `file` is the file's
    /// length in seconds when known.
    pub fn tail_beats(&self, tempo: f64, file: Option<f64>) -> f64 {
        let fade = self.fade_out_ms / 1000.0 * tempo / 60.0;
        match (self.source_end_seconds, file) {
            (Some(end), Some(file)) => fade.min(((file - end) / self.seconds_per_beat(tempo)).max(0.0)),
            (Some(_), None) => fade,
            (None, _) => 0.0,
        }
    }

    /// The beat the clip's sound ends on, its tail included: a looped clip's
    /// own length, else to its end in the file or the file's end. None when
    /// the clip plays to the end of a file whose length is not known.
    pub fn sound_end(&self, tempo: f64, file: Option<f64>) -> Option<f64> {
        let at = self.at_exact().to_f64().unwrap_or(0.0);
        let played = match self.length_exact() {
            Some(length) => length.to_f64().unwrap_or(0.0),
            None => {
                let until = match (self.source_end_seconds, file) {
                    (Some(end), Some(file)) => end.min(file),
                    (Some(end), None) => end,
                    (None, Some(file)) => file,
                    (None, None) => return None,
                };
                ((until - self.source_start_seconds) / self.seconds_per_beat(tempo)).max(0.0)
            }
        };
        Some(at + played + self.tail_beats(tempo, file))
    }
}

/// The beat the song's content ends on: the latest of every clip's end (an
/// audio clip's with its tail, a looped clip's with its loop), every
/// automation point, every section's end and every marker; zero for an
/// empty song. `file_seconds` is the length of a sample's file, which an
/// audio clip that plays to its file's end needs; such a clip whose file
/// cannot be read counts from its start.
pub fn content_end(p: &Project, file_seconds: &dyn Fn(&str) -> Option<f64>) -> BigRational {
    let tempo = p.session.tempo;
    let mut end = BigRational::zero();
    let mut reach = |beat: BigRational| {
        if beat > end {
            end = beat;
        }
    };
    for t in &p.tracks {
        for c in &t.clips {
            if let Some(pattern) = p.patterns.get(&c.pattern) {
                reach(c.at_exact() + pattern.length_exact() * BigRational::from_integer(c.repeats.into()));
            }
        }
        for c in &t.audio {
            let sound = c.sound_end(tempo, file_seconds(&c.sample)).and_then(BigRational::from_float);
            reach(sound.unwrap_or_else(|| c.at_exact()));
        }
        if let Some(midi) = &t.midi {
            for c in &midi.clips {
                reach(c.at_exact() + c.length_exact());
            }
        }
    }
    for owner in owners(p) {
        for lane in owner.automation() {
            for point in &lane.points {
                reach(point.at_exact());
            }
        }
    }
    for s in &p.sections {
        reach(s.at_exact() + s.length_exact());
    }
    for m in &p.markers {
        reach(m.at_exact());
    }
    end
}

/// The song's length as the host keeps it: the next whole bar after
/// `content_end`, and one bar at the least, so an empty song still has a
/// ruler.
pub fn fitted_length(p: &Project, file_seconds: &dyn Fn(&str) -> Option<f64>) -> BigRational {
    let bar = p.session.meter().bar();
    let end = content_end(p, file_seconds);
    ((end / &bar).ceil() * &bar).max(bar)
}
