//! Fingerprints the earlier engines wrote must still verify.
//!
//! The projects are those of tests/test_daw.py without their patterns, which
//! the song no longer holds (D191): each uses every model of one schema. Each
//! `earlier` hash is the form the engine of that schema recorded in its
//! render reports, and `before` the saved form an engine wrote before
//! patterns were removed, with every audio track's `pads: {}`, both as the
//! build of D190, whose fingerprints these tests held to those engines,
//! computed them.

const PROJECTS: [(&str, &str, &str, &str, &str); 4] = [
    (
        "automation",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "tracks": [{"id": "kick", "pads": {}}, {"id": "bass", "pads": {}, "effects": [{"type": "filter", "mode": "lowpass", "cutoff_hz": 800, "id": "tone"}, {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 200, "gain_db": 3}]}, {"type": "compressor", "threshold_db": -20, "sidechain": "kick"}], "sends": [{"to": "space"}], "automation": [{"param": "effects.tone.cutoff_hz", "points": [{"at": 0, "value": 800}, {"at": 8, "value": 4000}]}]}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}], "master": {"effects": [{"type": "limiter"}], "automation": [{"param": "gain_db", "points": [{"at": 0, "value": -6}]}]}, "returns": [{"id": "space", "effects": [{"type": "delay", "time_beats": "1/2"}, {"type": "reverb"}], "automation": [{"param": "gain_db", "points": [{"at": 4, "value": -6, "curve": "hold"}]}]}]}"#,
        "262f56b4ce4972502b69cd40dcb20b6b0395ca0352e80316309d2f8ddbff21ef",
        "ca7f2375ba36a4529bc1566c82291eccfc3d0a33492638c3bd50c59629cf0fca",
        "7574a39e2d7f52a7cc042cbd81db56d6242caf49243b16b8a86f6458c1a29f08",
    ),
    (
        "sends and returns",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "tracks": [{"id": "kick", "pads": {}}, {"id": "bass", "pads": {}, "effects": [{"type": "filter", "mode": "lowpass", "cutoff_hz": 800}, {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 200, "gain_db": 3}]}, {"type": "compressor", "threshold_db": -20, "sidechain": "kick"}], "sends": [{"to": "space"}]}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}], "master": {"effects": [{"type": "limiter"}]}, "returns": [{"id": "space", "effects": [{"type": "delay", "time_beats": "1/2"}, {"type": "reverb"}]}]}"#,
        "1b6e3be96763e67d8f101f5822ea94581059252607b3c1704075b4f9b4f049e5",
        "542fd9416300d7a4dce0775889c0eec624657a7468c294a154e29f4c02d04d7e",
        "b32ab59d1842421c4d00ef79bc8faf5261151f5a1f39812cc301a6348bf2f9a6",
    ),
    (
        "effects",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "tracks": [{"id": "kick", "pads": {}}, {"id": "bass", "pads": {}, "effects": [{"type": "filter", "mode": "lowpass", "cutoff_hz": 800}, {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 200, "gain_db": 3}]}, {"type": "compressor", "threshold_db": -20, "sidechain": "kick"}]}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}], "master": {"effects": [{"type": "limiter"}]}}"#,
        "004bf35773ebd9173e75c6592ceabebee458aee81f4a12e5b46b8d06ea34d63e",
        "218c62932f364405a226610186a0fc2fecd18699060313f8d29c952a5f342f8b",
        "31a0b8c61df4190fddb554323022bc89dec50c347374baa77266eb33de940ee2",
    ),
    (
        "first schema",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "tracks": [{"id": "kick", "pads": {}}, {"id": "bass", "pads": {}}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}]}"#,
        "9b1c56b18358acefe858469f15d64cb6f629bf1b3742048c74068d7edcf6bea2",
        "fb5103e1b08eb22f7bc1c0792020633a9b710ba53c7a8580428e571cb5df0707",
        "333d6e1c1c520dc27301f2050f48030748a6a357455056c4eb7c2198802507bc",
    ),
];

#[test]
fn earlier_fingerprints_still_verify() {
    for (name, json, earlier, before, current) in PROJECTS {
        // JSON is YAML, and the Python engine read these documents as YAML.
        let project = aaw_model::parse(json).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(aaw_model::project_hash(&project), current, "{name}");
        assert_ne!(current, earlier, "{name}");
        assert_ne!(current, before, "{name}");
        assert!(aaw_model::hash_matches(&project, earlier), "{name}");
        assert!(aaw_model::hash_matches(&project, before), "{name}");
    }
}

#[test]
fn saved_form_round_trips() {
    for (name, json, _, _, current) in PROJECTS {
        let project = aaw_model::parse(json).unwrap();
        let text = aaw_model::to_yaml(&project);
        assert!(!text.contains("pads"), "{name}: {text}");
        let again = aaw_model::parse(&text).unwrap();
        assert_eq!(aaw_model::to_yaml(&again), text, "{name}");
        assert_eq!(aaw_model::project_hash(&again), current, "{name}");
    }
}
