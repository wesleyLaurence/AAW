//! `tempo.follow`: the grid laid on a song. An audio clip that plays its file
//! at its own speed, a song played live, has its downbeats (or its beats)
//! pinned to the session's bars, and the master's tempo lane is written so
//! each pin falls on its bar: a step wherever the tempo the pins need moves
//! by more than 0.05 BPM from the one playing. The clip is moved so its first
//! pin is on a bar line; nothing is stretched.
//!
//! Each step is worked out from where the bar it starts on actually falls
//! under the steps already written, not from the file alone, so a step left
//! out or a tempo rounded to 0.001 BPM is made up at the next step. A step is
//! left out only where it is within 0.05 BPM of the tempo playing and its pin
//! lands within a millisecond without it, so no pin is further off than that.
//!
//! With `keep_time` the song around the clip keeps its sound: every beat of
//! everything else placed in beats is moved to the beat that sounds at its
//! second now, and the tempo before the first pin is fitted so that the
//! clip stays at its second too.

use crate::command::{exact, number, Edit, Outcome, Pin, Span};
use crate::tree::{Loc, Node, Step};
use aaw_model::beat::float_fraction;
use aaw_model::value::Value;
use aaw_model::TempoMap;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::ToPrimitive;
use serde_json::{json, Value as Json};

type Result<T> = std::result::Result<T, String>;

/// A step within this much of the tempo playing is left out, if the pin it
/// leads to still lands within `CLOSE_SECONDS` of its beat.
const SAME_BPM: f64 = 0.05;
const CLOSE_SECONDS: f64 = 0.001;

/// A tempo written to the lane, to 0.001 BPM.
fn bpm3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A beat to a millionth, written as an exact decimal.
fn decimal(x: f64) -> BigRational {
    float_fraction((x * 1e6).round() / 1e6)
}

fn f(x: &BigRational) -> f64 {
    x.to_f64().unwrap_or(f64::NAN)
}

fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

fn ms(x: f64) -> f64 {
    (x * 1e6).round() / 1e3
}

fn in_range(bpm: f64) -> bool {
    (20.0..=400.0).contains(&bpm)
}

impl Edit<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tempo_follow(&mut self, clip: &str, pins: &[Pin], bpm: f64, bar: Option<i64>, keep_time: bool, out: &mut Outcome) -> Result<()> {
        let loc = self.audio_clip(clip).ok_or_else(|| format!("{clip} is not an audio clip: the tempo follows an audio clip, tracks.T.audio.N"))?;
        let node = self.node(&loc).clone();
        let name = self.text(&loc);
        if number(node.get("source_bpm")).is_some() {
            return Err(format!(
                "{name} is stretched to the song's tempo by its source_bpm: the tempo follows a clip that plays its file at its own speed. Set its source_bpm to null first"
            ));
        }
        if node.get("loop_beats").is_some_and(|n| !matches!(n, Node::Leaf(Value::None))) {
            return Err(format!("{name} is looped: the tempo follows a clip that plays its file once"));
        }
        if !in_range(bpm) {
            return Err(format!("A tempo is 20 to 400 BPM, not {}", aaw_model::pyfmt::float_repr(bpm)));
        }
        let old = self.tempo();
        let span = Span::of(&node, &old)?;
        let repitch = match node.get("stretch") {
            Some(Node::Leaf(Value::Str(s))) if s.as_str() != "repitch" => 1.0,
            _ => 2f64.powf(number(node.get("transpose")).unwrap_or(0.0) / 12.0),
        };
        let bar_len = self.meter().bar();

        // The pins the clip plays, each with its beat from the first.
        let mut kept: Vec<&Pin> = pins
            .iter()
            .filter(|p| p.seconds >= span.start - 1e-9 && span.end.is_none_or(|e| p.seconds <= e + 1e-9))
            .collect();
        kept.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
        if kept.len() < 2 {
            return Err(format!(
                "{name} plays {} of the {} beats given: the tempo needs two or more to follow",
                kept.len(),
                pins.len()
            ));
        }
        let offset = |p: &Pin| -> Result<BigRational> {
            if p.beats < 1 || p.beat < 1 || p.beat > p.beats {
                return Err(format!("Beat {} of a bar of {} is not a beat of it", p.beat, p.beats));
            }
            let within = BigRational::new(BigInt::from(p.beat - 1), BigInt::from(p.beats));
            Ok((BigRational::from_integer(BigInt::from(p.bar)) + within) * &bar_len)
        };
        // Beats from the bar line of the first pin's bar: a clip cut into a
        // bar keeps its first pin where it is in the bar.
        let first = BigRational::from_integer(BigInt::from(kept[0].bar)) * &bar_len;
        let mut beats = Vec::with_capacity(kept.len());
        for (i, p) in kept.iter().enumerate() {
            let b = offset(p)? - &first;
            if i > 0 && (b <= beats[i - 1] || p.seconds <= kept[i - 1].seconds) {
                return Err(format!("The beats given go back at {} s: each must be later than the one before, in the file and in bars", p.seconds));
            }
            beats.push(b);
        }
        let into_bar = f(&beats[0]);
        let lead_in = (kept[0].seconds - span.start) / repitch;

        // The bar the first pin's bar starts on, and the tempo before the pin.
        let bar_at = |n: i64| BigRational::from_integer(BigInt::from(n - 1)) * &bar_len;
        let pinned = |n: i64| f(&bar_at(n)) + into_bar;
        let (n, opening) = if keep_time {
            let heard = old.seconds(f(&span.at)) + lead_in;
            let lowest = if into_bar > 0.0 { 1 } else { 2 };
            if heard < 1e-6 {
                if bar.is_some_and(|b| b != 1) || into_bar > 0.0 {
                    return Err(format!("{name}'s first beat is at the song's start, so it is on bar 1"));
                }
                (1, bpm3(bpm))
            } else {
                let n = match bar {
                    Some(b) if b < lowest => {
                        return Err(format!("{name}'s first beat is {heard:.3} s into the song: with --keep-time it is on bar {lowest} or later"))
                    }
                    Some(b) => b,
                    // The bar whose tempo before it is nearest the song's own.
                    None => (((bpm * heard / 60.0 - into_bar) / f(&bar_len)).round() as i64 + 1).max(lowest),
                };
                (n, bpm3(pinned(n) * 60.0 / heard))
            }
        } else {
            let opening = bpm3(bpm);
            let need = lead_in * opening / 60.0;
            let fits = ((need - into_bar) / f(&bar_len) - 1e-9).ceil().max(0.0) as i64 + 1;
            match bar {
                Some(b) if b < 1 => return Err(format!("Bars count from 1, not {b}")),
                Some(b) if b < fits => {
                    return Err(format!(
                        "{name} has {lead_in:.3} s before its first beat, {} beats at {}: its first beat's bar is bar {fits} or later",
                        round6(need),
                        aaw_model::pyfmt::float_repr(opening)
                    ))
                }
                Some(b) => (b, opening),
                None => (fits, opening),
            }
        };
        if !in_range(opening) {
            return Err(format!(
                "On bar {n} the tempo before {name}'s first beat would be {opening} BPM; a tempo is 20 to 400: give another --bar"
            ));
        }
        let start = bar_at(n);
        let at = decimal((pinned(n) - lead_in * opening / 60.0).max(0.0));

        // The steps, each from where its beat falls under those before it.
        let clip_seconds = f(&at) * 60.0 / opening;
        let heard: Vec<f64> = kept.iter().map(|p| clip_seconds + (p.seconds - span.start) / repitch).collect();
        let mut steps: Vec<(BigRational, f64)> = vec![(BigRational::from_integer(0.into()), opening)];
        let mut playing = opening;
        let mut now = pinned(n) * 60.0 / opening;
        for j in 0..kept.len() - 1 {
            let length = f(&(&beats[j + 1] - &beats[j]));
            let need = length * 60.0 / (heard[j + 1] - now);
            if !need.is_finite() || !in_range(need) {
                return Err(format!(
                    "From {} s of the file the beats need {} BPM; a tempo is 20 to 400. daw samples beats FILE --near {} shows the beats there",
                    kept[j].seconds,
                    if need.is_finite() { format!("{need:.1}") } else { "no".into() },
                    kept[j].seconds
                ));
            }
            let need = bpm3(need);
            // Left out only where the next pin still lands within a millisecond.
            let late = length * 60.0 * (1.0 / playing - 1.0 / need);
            if (need - playing).abs() > SAME_BPM || late.abs() > CLOSE_SECONDS {
                let beat = &start + &beats[j];
                match steps.last_mut() {
                    Some(last) if last.0 == beat => last.1 = need,
                    _ => steps.push((beat, need)),
                }
                playing = need;
            }
            now += length * 60.0 / playing;
        }

        let new = TempoMap::from_points(opening, steps.clone());
        // How far the farthest pin is from its beat under the steps written.
        let off = beats.iter().zip(&heard).map(|(b, t)| (new.seconds(f(&(&start + b))) - t).abs()).fold(0.0, f64::max);

        let mut moved = 0;
        if keep_time {
            moved = self.keep_time(&old, &new, &loc)?;
        }
        self.write_tempo(steps.clone())?;
        self.set_leaf(&loc, "at", crate::command::beat_value(&at));
        let stretched = self.stretched(&old, &new);

        out.label = format!("Follow the tempo of {name}");
        out.report.insert("clip".into(), json!(name));
        out.report.insert("bar".into(), json!(n));
        out.report.insert("at".into(), crate::command::to_json(&crate::command::beat_value(&at)));
        out.report.insert("first_beat_seconds".into(), json!(kept[0].seconds));
        out.report.insert("pins".into(), json!(kept.len()));
        out.report.insert(
            "steps".into(),
            Json::Array(
                new.steps()
                    .iter()
                    .map(|s| {
                        let (b, beat) = self.meter().bar_beat(s.beat_f64());
                        json!({"at": crate::command::to_json(&crate::command::beat_value(&s.beat)), "bar": b, "beat": beat, "seconds": round6(s.seconds_f64()), "bpm": s.bpm})
                    })
                    .collect(),
            ),
        );
        out.report.insert("largest_off_ms".into(), json!(ms(off)));
        if keep_time {
            out.report.insert("moved".into(), json!(moved));
        }
        out.report.insert("stretched".into(), Json::Array(stretched));
        Ok(())
    }

    /// Moves everything placed in beats but the clip at `except` to the
    /// beat that sounds under `new` at the second it sounds at under `old`;
    /// a clip moves as a whole, its notes and its length kept in beats.
    fn keep_time(&mut self, old: &TempoMap, new: &TempoMap, except: &Loc) -> Result<usize> {
        let to = |b: &BigRational| decimal(new.beat(old.seconds(f(b))));
        let mut places: Vec<(Loc, &'static str)> = Vec::new();
        let tracks = self.root.get("tracks").map_or(0, |t| t.items().len());
        for t in 0..tracks {
            let track = vec![Step::Key("tracks".into()), Step::Index(t)];
            for key in ["clips", "audio"] {
                for i in 0..self.node(&track).get(key).map_or(0, |l| l.items().len()) {
                    let loc = [track.clone(), vec![Step::Key(key.into()), Step::Index(i)]].concat();
                    if &loc != except {
                        places.push((loc, "at"));
                    }
                }
            }
        }
        let mut owners: Vec<Loc> = (0..tracks).map(|t| vec![Step::Key("tracks".into()), Step::Index(t)]).collect();
        for key in ["groups", "returns"] {
            for i in 0..self.root.get(key).map_or(0, |n| n.items().len()) {
                owners.push(vec![Step::Key(key.into()), Step::Index(i)]);
            }
        }
        if self.root.get("master").is_some() {
            owners.push(vec![Step::Key("master".into())]);
        }
        for owner in owners {
            for i in 0..self.node(&owner).get("automation").map_or(0, |l| l.items().len()) {
                let lane = [owner.clone(), vec![Step::Key("automation".into()), Step::Index(i)]].concat();
                if self.node(&lane).field("param") == Some(aaw_model::TEMPO_PARAM) {
                    continue;
                }
                for p in 0..self.node(&lane).get("points").map_or(0, |l| l.items().len()) {
                    places.push(([lane.clone(), vec![Step::Key("points".into()), Step::Index(p)]].concat(), "at"));
                }
            }
        }
        for i in 0..self.root.get("markers").map_or(0, |l| l.items().len()) {
            places.push((vec![Step::Key("markers".into()), Step::Index(i)], "at"));
        }
        let mut moved = 0;
        for (loc, key) in places {
            let Some(b) = exact(self.node(&loc).get(key)) else { continue };
            let b2 = to(&b);
            if b2 != b {
                self.set_leaf(&loc, key, crate::command::beat_value(&b2));
                moved += 1;
            }
        }
        for i in 0..self.root.get("sections").map_or(0, |l| l.items().len()) {
            let loc = vec![Step::Key("sections".into()), Step::Index(i)];
            let a = exact(self.node(&loc).get("at")).unwrap_or_default();
            let e = &a + exact(self.node(&loc).get("length_beats")).unwrap_or_default();
            let (a2, e2) = (to(&a), to(&e));
            if a2 != a || e2 != e {
                self.set_leaf(&loc, "at", crate::command::beat_value(&a2));
                self.set_leaf(&loc, "length_beats", crate::command::beat_value(&(&e2 - &a2)));
                moved += 1;
            }
        }
        let session = vec![Step::Key("session".into())];
        if let Some(len) = exact(self.node(&session).get("length_beats")) {
            self.set_leaf(&session, "length_beats", crate::command::beat_value(&to(&len)));
        }
        Ok(moved)
    }

    /// Every audio clip and pad stretched to the song's tempo by its
    /// `source_bpm`, with the tempos it now plays at, where they changed:
    /// the command cannot tell a clip's own tempo from a ratio written to
    /// make it follow another's.
    fn stretched(&self, old: &TempoMap, new: &TempoMap) -> Vec<Json> {
        let mut out = Vec::new();
        let tracks = self.root.get("tracks").map_or(&[][..], Node::items);
        for (t, track) in tracks.iter().enumerate() {
            for (i, clip) in track.node.get("audio").map_or(&[][..], Node::items).iter().enumerate() {
                let Some(own) = number(clip.node.get("source_bpm")) else { continue };
                let Some(at) = exact(clip.node.get("at")) else { continue };
                let from = f(&at);
                let end = Span::of(&clip.node, &std::sync::Arc::new(new.clone())).ok().and_then(|s| s.end_beat()).map_or(from, |e| f(&e));
                let mut tempos: Vec<f64> = vec![new.tempo_at(from)];
                tempos.extend(new.changes_between(from, end).map(|s| s.bpm));
                let (lo, hi) = tempos.iter().fold((f64::MAX, f64::MIN), |(l, h), &x| (l.min(x), h.max(x)));
                let was = old.tempo_at(from);
                if (lo - was).abs() < 1e-9 && (hi - was).abs() < 1e-9 {
                    continue;
                }
                let loc = vec![Step::Key("tracks".into()), Step::Index(t), Step::Key("audio".into()), Step::Index(i)];
                let pct = |x: f64| ((x / own - 1.0) * 1000.0).round() / 10.0;
                out.push(json!({
                    "clip": self.text(&loc),
                    "source_bpm": own,
                    "was_bpm": was,
                    "now_bpm": [lo, hi],
                    "speed_percent": [pct(lo), pct(hi)],
                }));
            }
            let pads = track.node.get("instrument").and_then(|i| i.get("sampler")).and_then(|s| s.get("pads"));
            if let Some(Node::Map(pads)) = pads {
                for (id, pad) in pads {
                    if let Some(own) = number(pad.get("source_bpm")) {
                        let (lo, hi) = new.bounds();
                        out.push(json!({
                            "pad": format!("tracks.{}.instrument.sampler.pads.{id}", track.node.field("id").unwrap_or("?")),
                            "source_bpm": own,
                            "now_bpm": [lo, hi],
                        }));
                    }
                }
            }
        }
        out
    }
}
