//! Fingerprints the earlier Python engines wrote must still verify.
//!
//! The projects and hashes are those of tests/test_daw.py: each project uses
//! every model of one schema, and each hash is what the engine of that schema
//! recorded in its render reports.

const PROJECTS: [(&str, &str, &str, &str); 4] = [
    (
        "automation",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "patterns": {"beat": {"length_beats": 4, "steps": {"k": "x...x...x...x..."}, "events": [{"at": "1/3", "pad": "k"}]}}, "tracks": [{"id": "kick", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat"}]}, {"id": "bass", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat", "at": 4}], "effects": [{"type": "filter", "mode": "lowpass", "cutoff_hz": 800, "id": "tone"}, {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 200, "gain_db": 3}]}, {"type": "compressor", "threshold_db": -20, "sidechain": "kick"}], "sends": [{"to": "space"}], "automation": [{"param": "effects.tone.cutoff_hz", "points": [{"at": 0, "value": 800}, {"at": 8, "value": 4000}]}]}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}], "master": {"effects": [{"type": "limiter"}], "automation": [{"param": "gain_db", "points": [{"at": 0, "value": -6}]}]}, "returns": [{"id": "space", "effects": [{"type": "delay", "time_beats": "1/2"}, {"type": "reverb"}], "automation": [{"param": "gain_db", "points": [{"at": 4, "value": -6, "curve": "hold"}]}]}]}"#,
        "c0c7d463215a7839767cef6302532441594a6f9432486e4061406bc51fbdc509",
        "1a5157a445ee3336a3293b3822f8dcc148c512113281ac875902507d2a12e410",
    ),
    (
        "sends and returns",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "patterns": {"beat": {"length_beats": 4, "steps": {"k": "x...x...x...x..."}, "events": [{"at": "1/3", "pad": "k"}]}}, "tracks": [{"id": "kick", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat"}]}, {"id": "bass", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat", "at": 4}], "effects": [{"type": "filter", "mode": "lowpass", "cutoff_hz": 800}, {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 200, "gain_db": 3}]}, {"type": "compressor", "threshold_db": -20, "sidechain": "kick"}], "sends": [{"to": "space"}]}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}], "master": {"effects": [{"type": "limiter"}]}, "returns": [{"id": "space", "effects": [{"type": "delay", "time_beats": "1/2"}, {"type": "reverb"}]}]}"#,
        "2079b7e7da4f90bbbfcd9b93414f91f2ede9df70a7bfec57401b4c8d32f06a30",
        "1d12d51adff73da1d2f5352535304f3dd094a9f8aca8f438f28596f8b0147c79",
    ),
    (
        "effects",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "patterns": {"beat": {"length_beats": 4, "steps": {"k": "x...x...x...x..."}, "events": [{"at": "1/3", "pad": "k"}]}}, "tracks": [{"id": "kick", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat"}]}, {"id": "bass", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat", "at": 4}], "effects": [{"type": "filter", "mode": "lowpass", "cutoff_hz": 800}, {"type": "eq", "bands": [{"shape": "bell", "freq_hz": 200, "gain_db": 3}]}, {"type": "compressor", "threshold_db": -20, "sidechain": "kick"}]}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}], "master": {"effects": [{"type": "limiter"}]}}"#,
        "0caffe96828674076f68c7060c86e5cc553cd6fab7d196c58bea5a6a7ae2b159",
        "9261151f211b46edfd0d6d16a1b3ace98232e8596dd26266d5e2bb291cb08c4a",
    ),
    (
        "first schema",
        r#"{"session": {"tempo": 120, "length_beats": 8}, "samples": {"hit": {"path": "hit.wav", "root_note": "C3"}}, "patterns": {"beat": {"length_beats": 4, "steps": {"k": "x...x...x...x..."}, "events": [{"at": "1/3", "pad": "k"}]}}, "tracks": [{"id": "kick", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat"}]}, {"id": "bass", "pads": {"k": {"sample": "hit"}}, "clips": [{"pattern": "beat", "at": 4}]}], "sections": [{"id": "drop", "at": 4, "length_beats": 4}]}"#,
        "1b90bf9dcc3079a74cd794764f03445e7aa76eb30ea55b62cad15efa479a26dc",
        "d654717bcca8296240b5d2d6261a1dabe0bc801d15c0bb71e1dcc581d94beb4c",
    ),
];

#[test]
fn earlier_fingerprints_still_verify() {
    for (name, json, earlier, current) in PROJECTS {
        // JSON is YAML, and the Python engine read these documents as YAML.
        let project = aaw_model::parse(json).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(aaw_model::project_hash(&project), current, "{name}");
        assert_ne!(current, earlier, "{name}");
        assert!(aaw_model::hash_matches(&project, earlier), "{name}");
    }
}

#[test]
fn saved_form_round_trips() {
    for (name, json, _, current) in PROJECTS {
        let project = aaw_model::parse(json).unwrap();
        let text = aaw_model::to_yaml(&project);
        let again = aaw_model::parse(&text).unwrap();
        assert_eq!(aaw_model::to_yaml(&again), text, "{name}");
        assert_eq!(aaw_model::project_hash(&again), current, "{name}");
    }
}
