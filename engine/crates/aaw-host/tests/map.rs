//! `daw map`: the song as a grid of tracks by bars.

mod common;

use aaw_host::session::Session;
use common::{edit, write_song};
use serde_json::{json, Value as Json};
use std::fmt::Write;

fn map(s: &Session, per: Option<Json>, from: Option<Json>, to: Option<Json>, tracks: &[&str], lanes: bool) -> Json {
    let tracks: Vec<String> = tracks.iter().map(|t| t.to_string()).collect();
    s.map(per.as_ref(), from.as_ref(), to.as_ref(), &tracks, lanes).unwrap()
}

/// The map's lines as one text.
fn lines(m: &Json) -> String {
    m["map"].as_array().unwrap().iter().map(|l| format!("{}\n", l.as_str().unwrap())).collect()
}

fn text(s: &Session) -> String {
    lines(&map(s, None, None, None, &[], false))
}

/// The cells of the row named `name` in the map's text.
fn row(text: &str, name: &str) -> String {
    let line = text
        .lines()
        .map(str::trim_start)
        .find(|l| l.starts_with(&format!("{name} ")))
        .unwrap_or_else(|| panic!("no row {name} in\n{text}"));
    line[name.len()..].trim_start().to_string()
}

fn song(yaml: &str) -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    write_song(dir.path());
    std::fs::write(dir.path().join("song.yaml"), yaml).unwrap();
    let s = Session::open(&dir.path().join("song.yaml"), false).unwrap();
    (dir, s)
}

const PHRASE: &str = "[{pitch: 48, duration: 2}, {pitch: 55, at: 4, duration: 2}, {pitch: 53, at: 8, duration: 2}, {pitch: 51, at: 12, duration: 2}]";

#[test]
fn three_duplicates_on_one_beat_show_as_stacked() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let mut s = Session::open(&path, true).unwrap();
    // The drums' second clip plays from bar 5 to bar 8; each copy is put on
    // bar 9, as a duplicate was before copies were laid in a row.
    edit(&mut s, json!({"op": "set", "path": "session.length_beats", "value": 48})).unwrap();
    for _ in 0..3 {
        edit(&mut s, json!({"op": "clip.duplicate", "clip": "tracks.drums.clips.1", "at": 32})).unwrap();
    }
    let m = map(&s, None, None, None, &["drums"], false);
    let t = lines(&m);
    assert_eq!(row(&t, "drums"), "AAAAAAAA####");
    // Each letter's clips by their handles, in the order they play.
    let clips: Vec<&str> = m["clips"]["A"].as_str().unwrap().split(' ').collect();
    assert_eq!(clips.len(), 5);
    assert!(clips.iter().all(|c| c.starts_with('@')));
    assert!(t.contains("A  pattern beat, 4 beats, 8 hits; drums bars 1 ×4, 5 ×4, 9 ×4, 9 ×4, 9 ×4"));
}

#[test]
fn copies_of_a_phrase_share_a_letter_and_a_changed_copy_does_not() {
    let yaml = format!(
        r#"
session: {{tempo: 120, length_beats: 64}}
tracks:
- id: bass
  type: midi
  clips:
  - {{id: a, at: 0, length_beats: 16, notes: {PHRASE}}}
  - {{id: b, at: 16, length_beats: 16, notes: {PHRASE}}}
  - {{id: c, at: 32, length_beats: 16, notes: [{{pitch: 48, duration: 2}}, {{pitch: 55, at: 4, duration: 2}}, {{pitch: 53, at: 8, duration: 2}}, {{pitch: 50, at: 12, duration: 2}}]}}
  - {{id: d, at: 48, length_beats: 16, notes: [{{pitch: 48, duration: 2}}, {{pitch: 55, at: 4, duration: 2}}, {{pitch: 53, at: 8, duration: 2}}, {{pitch: 51, at: 12, duration: 2, velocity: 90}}]}}
"#
    );
    let (_d, s) = song(&yaml);
    let t = text(&s);
    assert_eq!(row(&t, "bass"), "AAAAAAAABBBBCCCC");
    assert!(t.contains("A  notes a, 16 beats, 4 notes C3–G3; bass bars 1, 5"));
    assert!(t.contains("bass: no instrument; 0 dB; C3–G3; 1 notes/bar"));
}

#[test]
fn a_held_chord_holds_and_sections_mutes_and_lanes_have_rows() {
    let yaml = r#"
session: {tempo: 120, length_beats: 32}
tracks:
- id: keys
  type: midi
  mute: true
  clips:
  - {id: chord, at: 0, length_beats: 16, notes: [{pitch: 60, duration: 12}, {pitch: 64, duration: 12}, {pitch: 67, duration: 12}, {pitch: 62, at: 14, duration: 2}]}
  automation:
  - param: gain_db
    points: [{at: 0, value: -24}, {at: 8, value: 0}, {at: 20, value: 0, curve: hold}, {at: 28, value: -6}]
- id: lead
  type: midi
  solo: true
  clips: [{id: hook, at: 24, length_beats: 8, notes: [{pitch: 72, at: 1, duration: 1}]}]
sections: [{id: intro, at: 0, length_beats: 16}, {id: chorus, at: 16, length_beats: 4}, {id: outro, at: 20, length_beats: 12}]
"#;
    let (_d, s) = song(yaml);
    let m = map(&s, None, None, None, &[], true);
    let t = &lines(&m);
    assert_eq!(row(t, "bar"), "1   5");
    assert_eq!(row(t, "sections"), "int-cou-", "{t}");
    assert_eq!(row(t, "keys (muted)"), "A::A....");
    assert_eq!(row(t, "lead (solo)"), "......B:");
    // A ramp moves the first two bars and a held value jumps in the last.
    assert_eq!(row(t, "gain_db"), "~~-----~");
    // By the beat, the chord holds after its first beat and the hook starts on its second.
    let m = map(&s, Some(json!("beat")), Some(json!(24)), None, &["lead"], false);
    assert_eq!(row(&lines(&m), "lead (solo)"), ":A::::::");
}

#[test]
fn audio_clips_play_throughout_and_overlap_as_stacked() {
    // tone.wav is half a second: a beat at 120.
    let yaml = r#"
session: {tempo: 120, length_beats: 16}
samples:
  tone: {path: tone.wav}
tracks:
- id: vox
  pads: {}
  audio:
  - {sample: tone, at: 0}
  - {sample: tone, at: 8}
  - {sample: tone, at: 17/2, source_end_seconds: 0.25}
  - {sample: tone, at: 12, source_end_seconds: 0.25}
"#;
    let (_d, s) = song(yaml);
    let t = text(&s);
    assert_eq!(row(&t, "vox"), "A.#B", "{t}");
    assert!(t.contains("A  audio tone 0–0.5 s, 1 beat; vox bars 1, 3"), "{t}");
    assert!(t.contains("B  audio tone 0–0.25 s, 0.5 beats; vox bars 3.1.3, 4"), "{t}");
    assert!(t.contains("vox: audio; 0 dB\n") || t.ends_with("vox: audio; 0 dB\n"), "{t}");
    let m = map(&s, Some(json!("beat")), None, None, &[], false);
    assert_eq!(row(&lines(&m), "vox"), "A.......#...B...");
}

#[test]
fn ranges_cells_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, false).unwrap();
    let m = map(&s, Some(json!(2)), Some(json!(8)), Some(json!(32)), &[], false);
    assert_eq!((m["from"].clone(), m["to"].clone(), m["per_beats"].clone()), (json!(8.0), json!(32.0), json!(8.0)));
    let t = &lines(&m);
    assert_eq!(row(t, "drums"), "AAA");
    assert_eq!(row(t, "bar"), "3");
    // Clips are named by their paths without a host.
    assert_eq!(m["clips"]["A"], json!("tracks.drums.clips.0 tracks.drums.clips.1"));
    let e = s.map(None, None, None, &["nope".into()], false).unwrap_err();
    assert_eq!(e, "No track nope; the tracks are drums, bass");
    let e = s.map(Some(&json!("loud")), None, None, &[], false).unwrap_err();
    assert!(e.starts_with("--per takes a number of bars, bar or beat"), "{e}");
    let e = s.map(None, Some(&json!(40)), Some(&json!(8)), &[], false).unwrap_err();
    assert_eq!(e, "--to 8 is not after --from 40");
}

#[test]
fn audio_past_the_end_is_shown_and_said() {
    // Only an audio clip may run past the song's end; it must start before it.
    let yaml = r#"
session: {tempo: 120, length_beats: 8}
samples: {tone: {path: tone.wav}}
tracks:
- id: vox
  pads: {}
  audio: [{sample: tone, at: 15/2}]
"#;
    let (_d, s) = song(yaml);
    let t = text(&s);
    assert_eq!(row(&t, "vox"), ".AA");
    assert!(t.contains("The song ends where bar 3 begins; nothing after it is heard."), "{t}");
}

#[test]
fn eight_tracks_of_sixty_four_bars_map_in_under_two_thousand_characters() {
    let mut yaml = String::from(
        "session: {tempo: 120, length_beats: 256}\nsamples: {hit: {path: hit.wav}}\n\
         patterns:\n  beat: {length_beats: 4, steps: {h: x...x...x...x...}}\n  fill: {length_beats: 4, steps: {h: x.x.x.x.xxxxxxxx}}\n\
         sections: [{id: intro, at: 0, length_beats: 32}, {id: verse, at: 32, length_beats: 64}, {id: chorus, at: 96, length_beats: 64}, {id: bridge, at: 160, length_beats: 32}, {id: outro, at: 192, length_beats: 64}]\ntracks:\n",
    );
    for name in ["kick", "snare", "hats"] {
        let _ = write!(
            yaml,
            "- id: {name}\n  pads: {{h: {{sample: hit}}}}\n  clips: [{{pattern: beat, at: 32, repeats: 15}}, {{pattern: fill, at: 92}}, {{pattern: beat, at: 96, repeats: 24}}, {{pattern: beat, at: 192, repeats: 16}}]\n"
        );
    }
    for (k, name) in ["bass", "keys", "pad", "lead", "arp"].iter().enumerate() {
        let _ = writeln!(yaml, "- id: {name}\n  type: midi\n  clips:");
        for c in 0..16 {
            if (c + k) % 5 == 0 {
                continue;
            }
            let last = if c % 4 == 3 { 51 + k } else { 50 + k };
            let _ = writeln!(
                yaml,
                "  - {{id: {name}{c}, at: {}, length_beats: 16, notes: [{{pitch: {}, duration: 2}}, {{pitch: {}, at: 4, duration: 2}}, {{pitch: {}, at: 8, duration: 4}}, {{pitch: {last}, at: 12, duration: 2}}]}}",
                c * 16,
                40 + 7 * k,
                43 + 7 * k,
                47 + 7 * k
            );
        }
    }
    let (_d, s) = song(&yaml);
    let t = text(&s);
    for name in ["kick", "snare", "hats", "bass", "keys", "pad", "lead", "arp"] {
        assert_eq!(row(&t, name).chars().count(), 64, "{t}");
    }
    assert!(t.chars().count() < 2000, "{} characters:\n{t}", t.chars().count());
}
