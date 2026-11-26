//! `daw tempo follow`: the beat map of an audio clip's file, measured by
//! `daw samples beats`, turned into the pins of `tempo.follow`, and the
//! map's segments put on the session's bars in the reply.

use crate::{files, route, text, Cli, Song};
use aaw_host::command::{Command, Pin};
use aaw_host::python;
use clap::ValueEnum;
use serde_json::{json, Value as Json};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;

/// How often the tempo may step.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum By {
    /// A step a bar, each bar's tempo its own length: a song played live.
    Bar,
    /// A step a segment of the beat map: a song that keeps each tempo steadily.
    Segment,
    /// A step a beat: a song that drifts within the bar.
    Beat,
}

impl By {
    fn name(self) -> &'static str {
        match self {
            By::Bar => "bar",
            By::Segment => "segment",
            By::Beat => "beat",
        }
    }
}

pub fn run(cli: &Cli, project: &Song, clip: &str, by: By, bar: Option<i64>, keep_time: bool) -> Result<Json> {
    let song = files(project);
    let dir = song.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let found = route(cli, project, Command::Get { path: clip.to_string() }, None)?;
    let Some(sample) = found.get("sample").and_then(Json::as_str) else {
        return Err(format!("{clip} is not an audio clip: the tempo follows an audio clip, tracks.T.audio.N"));
    };
    let start = found.get("source_start_seconds").and_then(Json::as_f64).unwrap_or(0.0);
    let asset = route(cli, project, Command::Get { path: format!("samples.{sample}") }, None)?;
    let file = dir.join(asset.get("path").and_then(Json::as_str).ok_or_else(|| format!("Sample {sample} has no path"))?);
    let file = file.canonicalize().map_err(|e| format!("{}: {e}", file.display()))?;
    let meter = aaw_model::load(&song, false).map_err(text)?.session.time_signature;
    let map = python::run("samples", &["beats", &file.to_string_lossy(), "--meter", &meter, "--all"])?;

    let beats: Vec<(f64, i64, i64)> = map["beats"]
        .as_array()
        .ok_or("The beat map has no beats")?
        .iter()
        .filter_map(|b| Some((b["seconds"].as_f64()?, b["bar"].as_i64()?, b["beat"].as_i64()?)))
        .filter(|b| b.1 >= 1)
        .collect();
    let mut per_bar: BTreeMap<i64, i64> = BTreeMap::new();
    for b in &beats {
        *per_bar.entry(b.1).or_default() += 1;
    }
    let downbeats: Vec<(f64, i64)> = beats.iter().filter(|b| b.2 == 1).map(|b| (b.0, b.1)).collect();
    let segments: Vec<Json> = map["tempo"]["segments"].as_array().cloned().unwrap_or_default();
    let pin = |seconds: f64, bar: i64| Pin { seconds, bar, beat: 1, beats: 1 };
    let pins: Vec<Pin> = match by {
        By::Bar => downbeats.iter().map(|&(s, b)| pin(s, b)).collect(),
        By::Beat => beats.iter().map(|&(s, b, k)| Pin { seconds: s, bar: b, beat: k, beats: per_bar[&b] }).collect(),
        By::Segment => {
            // The downbeat each segment starts on, and the last, which ends
            // the last segment.
            let mut out: Vec<Pin> = Vec::new();
            for seg in &segments {
                let Some(b) = seg["bar"].as_i64() else { continue };
                if let Some(&(s, b)) = downbeats.iter().find(|d| d.1 == b) {
                    out.push(pin(s, b));
                }
            }
            if let Some(&(s, b)) = downbeats.last()
                && out.last().is_none_or(|p| p.bar != b)
            {
                out.push(pin(s, b));
            }
            // A clip cut from inside a segment starts where it is cut.
            if let Some(&(s, b)) = downbeats.iter().find(|d| d.0 >= start - 1e-9)
                && !out.iter().any(|p| p.bar == b)
            {
                out.push(pin(s, b));
                out.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
            }
            out
        }
    };
    // The tempo before the first downbeat the clip plays: its segment's.
    let first = downbeats.iter().find(|d| d.0 >= start - 1e-9).copied();
    let segment_of = |bar: i64| segments.iter().rev().find(|s| s["bar"].as_i64().is_some_and(|b| b <= bar)).or(segments.first());
    let bpm = first
        .and_then(|(_, b)| segment_of(b))
        .and_then(|s| s["bpm"].as_f64())
        .or_else(|| map["tempo"]["bpm"].as_f64())
        .ok_or("The beat map has no tempo")?;

    let mut reply = route(cli, project, Command::TempoFollow { clip: clip.to_string(), pins, bpm, bar, keep_time }, None)?;

    // The map's segments that the clip plays, on the session's bars.
    let on = reply["bar"].as_i64().unwrap_or(1);
    let first_bar = reply["first_beat_seconds"]
        .as_f64()
        .and_then(|t| beats.iter().find(|b| (b.0 - t).abs() < 1e-9))
        .map_or(1, |b| b.1);
    let last_bar = downbeats.last().map_or(first_bar, |d| d.1);
    let mut placed = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        let from = seg["bar"].as_i64().unwrap_or(1);
        let to = segments.get(i + 1).and_then(|s| s["bar"].as_i64()).unwrap_or(last_bar + 1);
        if to <= first_bar {
            continue;
        }
        placed.push(json!({
            "bar": on + from.max(first_bar) - first_bar,
            "file_bar": from.max(first_bar),
            "bpm": seg["bpm"],
            "steady": seg["steady"],
            "drift_ms": seg["drift_ms"],
        }));
    }
    let odd: Vec<Json> = map["odd_bars"]
        .as_array()
        .map(|a| a.iter().filter_map(|o| o.get("bar").and_then(Json::as_i64).or_else(|| o.as_i64())).filter(|b| *b >= first_bar).map(|b| json!(on + b - first_bar)).collect())
        .unwrap_or_default();
    let out = reply.as_object_mut().ok_or("The host replied with no object")?;
    out.insert("by".into(), json!(by.name()));
    out.insert("segments".into(), Json::Array(placed));
    if !odd.is_empty() {
        out.insert("odd_bars".into(), Json::Array(odd));
    }
    out.insert(
        "beat_map".into(),
        json!({"file": file, "analyzer": map["analyzer"], "beats_from": map["model"].get("beats_from").cloned().unwrap_or(Json::Null)}),
    );
    Ok(reply)
}
