//! Standard MIDI files of one part (D65): a file's notes read into a note
//! clip's, and a note clip's notes written as a file.
//!
//! A file is read when its notes are all in one file track and on one
//! channel, in a type 0 or type 1 file timed in ticks per beat. Positions are
//! ticks over ticks per beat, exact fractions of a beat. What else the file
//! has, the song does not hold: it is counted by kind and left out.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};
use serde_json::{json, Map, Value as Json};
use std::collections::{BTreeMap, VecDeque};

type Result<T> = std::result::Result<T, String>;

/// The ticks a beat is divided into in a written file. It divides by 3 for
/// triplets, by 64 for any straight note, and holds 2.025 exactly.
pub const TICKS_PER_BEAT: u16 = 960;

/// A note as a file holds it, in beats from the file's start.
#[derive(Clone, Debug, PartialEq)]
pub struct FileNote {
    pub pitch: u8,
    pub at: BigRational,
    pub duration: BigRational,
    pub velocity: u8,
}

/// What the song does not hold, in the order a reply lists it.
const KINDS: [&str; 9] = [
    "sustain pedal",
    "pitch bend",
    "controllers",
    "program changes",
    "aftertouch",
    "system exclusive",
    "tempo changes",
    "time signature changes",
    "note-offs without a note",
];

/// The one part of a file that was read.
#[derive(Debug, Default)]
pub struct Part {
    /// By start, then pitch.
    pub notes: Vec<FileNote>,
    /// The name of the file track the notes are in, if it has one.
    pub name: Option<String>,
    /// The first tempo the file sets, in beats a minute.
    pub tempo: Option<f64>,
    /// What the song does not hold, by kind, with how many there were.
    pub left_out: BTreeMap<usize, usize>,
    /// Notes with no note-off, ended where their file track ends.
    pub unended: usize,
    /// Notes whose note-off came at their note-on, made one tick long.
    pub zero_length: usize,
}

impl Part {
    /// The beat the last note ends on.
    pub fn end(&self) -> BigRational {
        self.notes.iter().map(|n| &n.at + &n.duration).max().unwrap_or_else(BigRational::zero)
    }

    /// The length of the clip the part makes: from the file's beat 0 to the
    /// end of its last note, in whole bars of the song's `meter`.
    pub fn length(&self, meter: aaw_model::Meter) -> BigRational {
        let bar = meter.bar();
        (self.end() / &bar).ceil().max(BigRational::from_integer(1.into())) * bar
    }

    /// What was left out, by kind, as a reply says it.
    pub fn left_out_json(&self) -> Json {
        Json::Object(self.left_out.iter().map(|(k, n)| (KINDS[*k].to_string(), json!(n))).collect())
    }

    /// What was adjusted to make notes of the file's, as a reply says it.
    pub fn adjusted_json(&self) -> Json {
        let mut out = Map::new();
        if self.unended > 0 {
            out.insert("notes with no note-off, ended where their track ends".into(), json!(self.unended));
        }
        if self.zero_length > 0 {
            out.insert("notes of no length, made one tick long".into(), json!(self.zero_length));
        }
        Json::Object(out)
    }
}

/// Bytes read in order.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, at: 0 }
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|end| *end <= self.bytes.len());
        let end = end.ok_or("The file ends in the middle of an event")?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A variable-length quantity: seven bits a byte, at most four bytes.
    fn var(&mut self) -> Result<u32> {
        let mut out = 0u32;
        for _ in 0..4 {
            let b = self.byte()?;
            out = (out << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(out);
            }
        }
        Err("A length in the file is longer than four bytes".into())
    }
}

/// A note sounding, waiting for its note-off.
struct Held {
    tick: u64,
    velocity: u8,
}

/// What one file track holds.
#[derive(Default)]
struct Track {
    name: Option<String>,
    /// Notes by channel, as (pitch, start tick, end tick, velocity).
    notes: BTreeMap<u8, Vec<(u8, u64, u64, u8)>>,
    tempo: Option<(u64, f64)>,
    left_out: BTreeMap<usize, usize>,
    unended: usize,
    zero_length: usize,
}

fn count(map: &mut BTreeMap<usize, usize>, kind: &str) {
    let k = KINDS.iter().position(|x| *x == kind).expect("a kind");
    *map.entry(k).or_default() += 1;
}

fn track(bytes: &[u8]) -> Result<Track> {
    let mut r = Reader::new(bytes);
    let mut out = Track::default();
    let mut tick = 0u64;
    let mut running: Option<u8> = None;
    let mut held: BTreeMap<(u8, u8), VecDeque<Held>> = BTreeMap::new();
    while !r.done() {
        tick += u64::from(r.var()?);
        let first = r.byte()?;
        let status = if first & 0x80 != 0 {
            first
        } else {
            // Running status: the byte is the first data byte of a message
            // of the status before.
            r.at -= 1;
            running.ok_or("The file has an event with no status")?
        };
        match status {
            0xff => {
                let kind = r.byte()?;
                let len = r.var()? as usize;
                let data = r.take(len)?;
                match kind {
                    0x03 if out.name.is_none() => {
                        let name = String::from_utf8_lossy(data).trim().to_string();
                        out.name = Some(name).filter(|n| !n.is_empty());
                    }
                    0x51 if len == 3 => {
                        let micros = u32::from_be_bytes([0, data[0], data[1], data[2]]);
                        if micros > 0 {
                            let bpm = 60_000_000.0 / f64::from(micros);
                            match out.tempo {
                                None => out.tempo = Some((tick, bpm)),
                                Some((_, first)) if tick > 0 && bpm != first => count(&mut out.left_out, "tempo changes"),
                                Some(_) => {}
                            }
                        }
                    }
                    0x58 if tick > 0 => count(&mut out.left_out, "time signature changes"),
                    0x2f => break,
                    _ => {}
                }
            }
            0xf0 | 0xf7 => {
                let len = r.var()? as usize;
                r.take(len)?;
                count(&mut out.left_out, "system exclusive");
            }
            0x80..=0xef => {
                running = Some(status);
                let channel = status & 0x0f;
                let a = r.byte()? & 0x7f;
                let b = if matches!(status & 0xf0, 0xc0 | 0xd0) { 0 } else { r.byte()? & 0x7f };
                match status & 0xf0 {
                    0x90 if b > 0 => held.entry((channel, a)).or_default().push_back(Held { tick, velocity: b }),
                    0x80 | 0x90 => match held.get_mut(&(channel, a)).and_then(VecDeque::pop_front) {
                        // The earliest note of the pitch still sounding ends.
                        Some(h) => {
                            let mut end = tick;
                            if end == h.tick {
                                end += 1;
                                out.zero_length += 1;
                            }
                            out.notes.entry(channel).or_default().push((a, h.tick, end, h.velocity));
                        }
                        None => count(&mut out.left_out, "note-offs without a note"),
                    },
                    0xa0 | 0xd0 => count(&mut out.left_out, "aftertouch"),
                    0xb0 if a == 64 => count(&mut out.left_out, "sustain pedal"),
                    0xb0 => count(&mut out.left_out, "controllers"),
                    0xc0 => count(&mut out.left_out, "program changes"),
                    _ => count(&mut out.left_out, "pitch bend"),
                }
            }
            other => return Err(format!("The file has a message a MIDI file cannot hold, status {other:#04x}")),
        }
    }
    // A note with no note-off ends where its track does.
    for ((channel, pitch), notes) in held {
        for h in notes {
            out.unended += 1;
            let end = tick.max(h.tick + 1);
            out.notes.entry(channel).or_default().push((pitch, h.tick, end, h.velocity));
        }
    }
    Ok(out)
}

/// Reads a file of one part. A file with notes in more than one file track
/// or on more than one channel is refused, with its parts named.
pub fn read(bytes: &[u8]) -> Result<Part> {
    let mut r = Reader::new(bytes);
    if r.take(4).ok() != Some(b"MThd") {
        return Err("This is not a MIDI file".into());
    }
    let len = r.u32()? as usize;
    let header = r.take(len.max(6))?;
    let (format, division) = (u16::from_be_bytes([header[0], header[1]]), u16::from_be_bytes([header[4], header[5]]));
    if format > 1 {
        return Err(format!("A type {format} MIDI file holds separate sequences; a type 0 or 1 file is read"));
    }
    if division & 0x8000 != 0 {
        return Err("The file is timed in SMPTE frames; a file timed in ticks per beat is read".into());
    }
    if division == 0 {
        return Err("The file's header gives no ticks per beat".into());
    }
    let mut tracks = Vec::new();
    while !r.done() {
        let kind = r.take(4)?;
        let len = r.u32()? as usize;
        let data = r.take(len)?;
        // A chunk of another kind is for another program.
        if kind == b"MTrk" {
            tracks.push(track(data)?);
        }
    }
    let parts: Vec<(usize, u8)> = tracks
        .iter()
        .enumerate()
        .flat_map(|(t, tr)| tr.notes.keys().map(move |c| (t, *c)))
        .collect();
    match parts.len() {
        0 => return Err("The file has no notes".into()),
        1 => {}
        n => {
            let named: Vec<String> = parts
                .iter()
                .map(|(t, c)| {
                    let name = tracks[*t].name.as_ref().map(|n| format!(" \"{n}\"")).unwrap_or_default();
                    format!("track {}{name} on channel {}", t + 1, c + 1)
                })
                .collect();
            return Err(format!(
                "The file has {n} parts: {}. A MIDI file of one part is read; export each part from where it was made",
                named.join(", ")
            ));
        }
    }
    let (t, channel) = parts[0];
    let ticks = BigInt::from(division);
    let beats = |tick: u64| BigRational::new(BigInt::from(tick), ticks.clone());
    let mut part = Part {
        name: tracks[t].name.clone(),
        // The tempo set earliest, in whichever track.
        tempo: tracks.iter().filter_map(|tr| tr.tempo).min_by_key(|(tick, _)| *tick).map(|(_, bpm)| bpm),
        ..Part::default()
    };
    for (i, tr) in tracks.iter_mut().enumerate() {
        for (k, n) in &tr.left_out {
            *part.left_out.entry(*k).or_default() += n;
        }
        if i == t {
            part.unended = tr.unended;
            part.zero_length = tr.zero_length;
        }
    }
    // A tempo set later in another track than the first one is a change too.
    let firsts: Vec<f64> = tracks.iter().filter_map(|tr| tr.tempo.map(|(_, bpm)| bpm)).collect();
    if let Some(first) = part.tempo {
        let others = firsts.iter().filter(|bpm| **bpm != first).count();
        if others > 0 {
            *part.left_out.entry(KINDS.iter().position(|k| *k == "tempo changes").expect("a kind")).or_default() += others;
        }
    }
    let mut notes: Vec<FileNote> = tracks[t].notes[&channel]
        .iter()
        .map(|(pitch, start, end, velocity)| FileNote {
            pitch: *pitch,
            at: beats(*start),
            duration: beats(end - start),
            velocity: *velocity,
        })
        .collect();
    notes.sort_by(|a, b| a.at.cmp(&b.at).then(a.pitch.cmp(&b.pitch)));
    part.notes = notes;
    Ok(part)
}

/// What writing a clip changed of its notes.
#[derive(Debug, Default)]
pub struct Written {
    pub bytes: Vec<u8>,
    /// Notes written.
    pub notes: usize,
    /// The indices, in the notes given, of notes whose start or end was not a
    /// whole number of ticks and was rounded to the nearest.
    pub rounded: Vec<usize>,
    /// Notes that start inside a note of their pitch and end before it. A
    /// file cannot tell which note-off is whose, so read back the first
    /// note-off ends the first note, and the two lengths change places.
    pub nested: usize,
}

fn var(out: &mut Vec<u8>, mut n: u32) {
    let mut bytes = vec![(n & 0x7f) as u8];
    n >>= 7;
    while n > 0 {
        bytes.push((n & 0x7f) as u8 | 0x80);
        n >>= 7;
    }
    out.extend(bytes.iter().rev());
}

/// A beat as ticks, and whether it fell on a tick.
fn ticks(beat: &BigRational) -> (u64, bool) {
    let scaled = beat * BigRational::from_integer(BigInt::from(TICKS_PER_BEAT));
    let exact = scaled.is_integer();
    (scaled.round().to_integer().to_u64().unwrap_or(0), exact)
}

/// Writes notes as a type 0 file of one track named `name`, on channel 1,
/// with the song's tempo and time signature.
pub fn write(notes: &[FileNote], name: &str, tempo: f64, meter: aaw_model::Meter) -> Written {
    let mut written = Written::default();
    // (tick, off before on, pitch, bytes)
    let mut events: Vec<(u64, u8, u8, [u8; 3])> = Vec::new();
    let mut spans: Vec<(u8, u64, u64)> = Vec::new();
    for (i, n) in notes.iter().enumerate() {
        let (start, a) = ticks(&n.at);
        let (end, b) = ticks(&(&n.at + &n.duration));
        if !(a && b) {
            written.rounded.push(i);
        }
        let end = end.max(start + 1);
        spans.push((n.pitch, start, end));
        events.push((start, 1, n.pitch, [0x90, n.pitch & 0x7f, n.velocity.clamp(1, 127)]));
        events.push((end, 0, n.pitch, [0x80, n.pitch & 0x7f, 0x40]));
    }
    written.notes = notes.len();
    written.nested = spans
        .iter()
        .filter(|(pitch, start, end)| spans.iter().any(|(p, s, e)| p == pitch && s < start && e > end))
        .count();
    // At one tick, notes end before notes start, so a note repeated right
    // after itself is two notes.
    events.sort_by_key(|(tick, on, pitch, _)| (*tick, *on, *pitch));
    let mut body = Vec::new();
    fn meta(body: &mut Vec<u8>, kind: u8, data: &[u8]) {
        var(body, 0);
        body.extend([0xff, kind]);
        var(body, data.len() as u32);
        body.extend(data);
    }
    meta(&mut body, 0x03, name.as_bytes());
    let micros = (60_000_000.0 / tempo).round() as u32;
    meta(&mut body, 0x51, &micros.to_be_bytes()[1..]);
    // The denominator as a power of two, as the file writes it.
    meta(&mut body, 0x58, &[meter.beats as u8, meter.unit.trailing_zeros() as u8, 24, 8]);
    let mut last = 0u64;
    for (tick, _, _, bytes) in &events {
        var(&mut body, (tick - last) as u32);
        body.extend(bytes);
        last = *tick;
    }
    var(&mut body, 0);
    body.extend([0xff, 0x2f, 0x00]);
    let out = &mut written.bytes;
    out.extend(b"MThd");
    out.extend(6u32.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(TICKS_PER_BEAT.to_be_bytes());
    out.extend(b"MTrk");
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(text: &str) -> BigRational {
        aaw_model::signed_beat(&aaw_model::Beat::Str(text.into())).unwrap()
    }

    fn note(pitch: u8, at: &str, duration: &str, velocity: u8) -> FileNote {
        FileNote { pitch, at: b(at), duration: b(duration), velocity }
    }

    /// A file of tracks given as their event bytes, delta times included.
    fn file(format: u16, division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"MThd".to_vec();
        out.extend(6u32.to_be_bytes());
        out.extend(format.to_be_bytes());
        out.extend((tracks.len() as u16).to_be_bytes());
        out.extend(division.to_be_bytes());
        for t in tracks {
            out.extend(b"MTrk");
            out.extend((t.len() as u32 + 4).to_be_bytes());
            out.extend(t);
            out.extend([0, 0xff, 0x2f, 0]);
        }
        out
    }

    #[test]
    fn a_clip_written_reads_back_the_same() {
        let notes = vec![
            note(60, "0", "1", 96),
            note(64, "0", "1", 80),
            note(67, "0", "1", 88),
            note(62, "1.975", "1/2", 72),
            note(62, "2.025", "1/2", 1),
            note(72, "1/3", "1/3", 127),
            note(74, "2/3", "1/3", 100),
        ];
        let written = write(&notes, "keys", 120.0, aaw_model::Meter::COMMON);
        assert!(written.rounded.is_empty());
        let part = read(&written.bytes).unwrap();
        let mut expected = notes.clone();
        expected.sort_by(|a, b| a.at.cmp(&b.at).then(a.pitch.cmp(&b.pitch)));
        assert_eq!(part.notes, expected);
        assert_eq!(part.name.as_deref(), Some("keys"));
        assert_eq!(part.tempo, Some(120.0));
        assert!(part.left_out.is_empty());
    }

    #[test]
    fn overlapping_notes_of_one_pitch_stay_overlapping() {
        let notes = vec![note(60, "0", "2", 100), note(60, "1", "2", 90), note(60, "3", "1", 80)];
        let part = read(&write(&notes, "x", 100.0, aaw_model::Meter::COMMON).bytes).unwrap();
        assert_eq!(part.notes, notes);
    }

    #[test]
    fn a_note_inside_another_of_its_pitch_is_counted() {
        let notes = vec![note(62, "0", "1", 100), note(62, "1/2", "1/4", 90), note(62, "1/2", "1", 80)];
        assert_eq!(write(&notes, "x", 100.0, aaw_model::Meter::COMMON).nested, 1);
        assert_eq!(write(&notes[..1], "x", 100.0, aaw_model::Meter::COMMON).nested, 0);
    }

    #[test]
    fn a_position_off_the_ticks_is_rounded_and_named() {
        let notes = vec![note(60, "0", "1", 100), note(61, "1/7", "1", 100)];
        let written = write(&notes, "x", 100.0, aaw_model::Meter::COMMON);
        assert_eq!(written.rounded, vec![1]);
        let part = read(&written.bytes).unwrap();
        assert_eq!(part.notes[1].at, b("137/960"));
    }

    #[test]
    fn both_note_off_encodings_and_running_status() {
        // 96 ticks a beat: a note ended by 0x80, then two by note-on with
        // velocity 0 under running status.
        let t = vec![
            0, 0x90, 60, 100, 96, 0x80, 60, 0, //
            0, 0x90, 62, 90, 48, 62, 0, // running status, note-on velocity 0
            0, 64, 80, 48, 64, 0,
        ];
        let part = read(&file(0, 96, &[t])).unwrap();
        assert_eq!(part.notes, vec![note(60, "0", "1", 100), note(62, "1", "1/2", 90), note(64, "3/2", "1/2", 80)]);
    }

    #[test]
    fn a_type_1_file_with_a_tempo_track_is_one_part() {
        let tempo = vec![0, 0xff, 0x51, 3, 0x09, 0x27, 0xc0]; // 600000 µs: 100 BPM
        let notes = vec![0, 0xff, 0x03, 4, b'P', b'a', b'd', b's', 0, 0x91, 48, 70, 0x81, 0x40, 0x81, 48, 0];
        let part = read(&file(1, 192, &[tempo, notes])).unwrap();
        assert_eq!(part.notes, vec![note(48, "0", "1", 70)]);
        assert_eq!(part.name.as_deref(), Some("Pads"));
        assert_eq!(part.tempo, Some(100.0));
    }

    #[test]
    fn what_the_song_cannot_hold_is_counted() {
        let t = vec![
            0, 0xff, 0x51, 3, 0x09, 0x27, 0xc0, // 100 BPM
            0, 0xc0, 5, // program change
            0, 0xb0, 7, 100, // volume
            0, 0xb0, 64, 127, // pedal down
            0, 0x90, 60, 100, //
            10, 0xe0, 0, 0x40, // pitch bend
            10, 0xd0, 30, // channel pressure
            10, 0xf0, 2, 0x7e, 0xf7, // system exclusive
            10, 0xb0, 64, 0, // pedal up
            10, 0x80, 60, 0, //
            0, 0x80, 61, 0, // a note-off with no note
            0, 0xff, 0x51, 3, 0x07, 0xa1, 0x20, // a change to 120 BPM
        ];
        let part = read(&file(0, 96, &[t])).unwrap();
        assert_eq!(part.notes.len(), 1);
        assert_eq!(
            part.left_out_json(),
            json!({
                "sustain pedal": 2, "pitch bend": 1, "controllers": 1, "program changes": 1,
                "aftertouch": 1, "system exclusive": 1, "tempo changes": 1, "note-offs without a note": 1,
            })
        );
        assert_eq!(part.tempo, Some(100.0));
    }

    #[test]
    fn notes_without_an_end_or_a_length_are_made_whole() {
        let t = vec![0, 0x90, 60, 100, 0, 0x80, 60, 0, 0, 0x90, 62, 100, 96, 0xff, 0x01, 1, b'x'];
        let part = read(&file(0, 96, &[t])).unwrap();
        assert_eq!(part.notes, vec![note(60, "0", "1/96", 100), note(62, "0", "1", 100)]);
        assert_eq!((part.unended, part.zero_length), (1, 1));
    }

    #[test]
    fn several_parts_are_refused_and_named() {
        let a = vec![0, 0xff, 0x03, 4, b'B', b'a', b's', b's', 0, 0x90, 40, 100, 96, 0x80, 40, 0];
        let b = vec![0, 0x99, 36, 100, 96, 0x89, 36, 0];
        let err = read(&file(1, 96, &[a.clone(), b])).unwrap_err();
        assert!(err.contains("2 parts"), "{err}");
        assert!(err.contains("track 1 \"Bass\" on channel 1"), "{err}");
        assert!(err.contains("track 2 on channel 10"), "{err}");
        // Two channels in one track of a type 0 file are two parts too.
        let mixed = vec![0, 0x90, 60, 100, 0, 0x91, 64, 100, 96, 0x80, 60, 0, 0, 0x81, 64, 0];
        assert!(read(&file(0, 96, &[mixed])).unwrap_err().contains("2 parts"));
    }

    #[test]
    fn other_files_are_refused() {
        assert!(read(b"RIFF....").unwrap_err().contains("not a MIDI file"));
        assert!(read(&file(0, 0xe728, &[vec![0, 0x90, 60, 1]])).unwrap_err().contains("SMPTE"));
        assert!(read(&file(2, 96, &[vec![0, 0x90, 60, 1]])).unwrap_err().contains("type 2"));
        assert!(read(&file(0, 96, &[vec![0, 0xff, 0x03, 0]])).unwrap_err().contains("no notes"));
        let mut cut = file(0, 96, &[vec![0, 0x90, 60, 100, 96, 0x80, 60, 0]]);
        cut.truncate(cut.len() - 6);
        assert!(read(&cut).is_err());
    }
}
