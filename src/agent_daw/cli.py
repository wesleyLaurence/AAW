"""The Python part of `daw`: the sample library, references, perception, `check`, `timeline`, `joins`, `export`, `separate`, `transcribe` and `key`.

Every other command belongs to the Rust `daw` and is passed on to it, so `uv run
daw` and the Rust binary are one command. Results are JSON on stdout; errors go
to stderr as JSON with a nonzero exit status.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

COMMANDS = ("samples", "reference", "listen", "compare", "check", "timeline", "joins", "export", "separate", "transcribe", "key")


def engine() -> Path:
    """The Rust `daw`: AAW_DAW, or the release build of this checkout."""
    override = os.environ.get("AAW_DAW")
    checkout = Path(__file__).resolve().parents[2]
    binary = Path(override) if override else checkout / "engine/target/release/daw"
    if not binary.is_file():
        raise RuntimeError(
            f"The Rust daw is not at {binary}; run `cargo build --release` in "
            f"{checkout / 'engine'} or set AAW_DAW"
        )
    return binary


class BadProject(argparse.ArgumentTypeError, ValueError):
    """A PROJECT that names no song: argparse shows the reason as it is."""


def song_file(path) -> Path:
    """The song file a PROJECT names: the file itself, song.yaml in a folder, or
    the song a project's `.aaw` names beside itself.

    A path that is not there is a folder unless it is written as a YAML file, and
    a `.aaw` that is gone names its folder's song.yaml, as the Rust `daw` takes
    them. A `.aaw` that does not read is refused with what is wrong.
    """
    path = Path(path)
    if path.suffix.lower() == ".aaw":
        if path.is_file():
            return path.parent / project_file(path)["song"]
        return path.parent / "song.yaml"
    written_as_file = path.suffix.lower() in (".yaml", ".yml")
    if path.is_dir() or not (written_as_file or path.exists()):
        return path / "song.yaml"
    return path


PROJECT_FORMAT = 1


def project_file(path: Path) -> dict:
    """What a project's `.aaw` says: its `format`, `id` and `song`, the song's file
    name in the same folder, checked as the Rust `daw` checks them."""
    instead = f"open {path.with_name('song.yaml')} instead"

    def refuse(why):
        return BadProject(f"{path} {why}; {instead}")

    try:
        text = path.read_text()
    except OSError as e:
        raise refuse(f"does not read: {e}") from None
    fields = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, sep, value = line.partition(":")
        if not sep or not key.strip() or " " in key.strip():
            raise refuse("is not an AAW project file")
        fields[key.strip()] = value.strip().strip("'\"")
    if "format" not in fields:
        raise refuse("is not an AAW project file: no format")
    if not fields["format"].isdigit():
        raise refuse("has a format that is not a whole number")
    if int(fields["format"]) > PROJECT_FORMAT:
        raise refuse(
            f"is format {fields['format']}, written by a newer AAW than this one, which reads {PROJECT_FORMAT}"
        )
    if not fields.get("id"):
        raise refuse("has no id")
    song = fields.get("song")
    if song is None:
        raise refuse("names no song")
    if not song or song in (".", "..") or "/" in song or "\\" in song:
        raise refuse("names a song that is not a file in its folder")
    return {"format": int(fields["format"]), "id": fields["id"], "song": song}


def run_engine(*args):
    """A Rust `daw` command's result, or its error as ValueError."""
    out = subprocess.run(
        [str(engine()), *map(str, args)], capture_output=True, text=True
    )
    if out.returncode:
        try:
            message = json.loads(out.stderr)["error"]
        except (ValueError, KeyError, TypeError):
            message = out.stderr.strip() or f"daw {args[0]} failed"
        raise ValueError(message)
    return json.loads(out.stdout)


def parser():
    from . import descriptors, library
    from .separation import SPLITS

    p = argparse.ArgumentParser(
        prog="daw",
        description="The sample library, references, perception, check, timeline, joins and export. Run daw --help for every command.",
    )
    sub = p.add_subparsers(dest="command", required=True)
    samples = sub.add_parser("samples")
    samples.add_argument("--db", type=Path, default=library.default_db())
    ss = samples.add_subparsers(dest="action", required=True)
    scan = ss.add_parser("scan")
    scan.add_argument("directory", type=Path)
    folders = ss.add_parser("folders", help="List, add, refresh or remove shared sample folders")
    folders.add_argument("operation", choices=["list", "add", "remove", "refresh"], nargs="?", default="list")
    folders.add_argument("directory", type=Path, nargs="?")
    search = ss.add_parser("search")
    search.add_argument("query", nargs="?", default="")
    search.add_argument("--folder", type=Path, action="append", help="Search within this folder; repeat for multiple sources")
    search.add_argument(
        "--path",
        type=Path,
        action="append",
        help="Only the file at this path, as the index has it; repeat for several, such as the favorites",
    )
    search.add_argument(
        "--category",
        help="The category in a sample's name, or the one a sample named for none measures as",
    )
    search.add_argument("--type", dest="kind", choices=["one-shot", "loop", "unknown"])
    search.add_argument("--key")
    search.add_argument("--bpm", type=int)
    search.add_argument("--limit", type=int, default=20)
    search.add_argument(
        "--sort",
        choices=["name", *library.SORTS],
        default="name",
        help="By name, or by length or a measurement with the most first: the "
        "longest, brightest (centroid), most low end (low), slowest attack, longest "
        "decay, noisiest, loudest, hardest hit (punch)",
    )
    search.add_argument("--reverse", action="store_true", help="Turn the order around")
    measured = search.add_argument_group(
        "measured filters", "match only samples analyzed with daw samples analyze"
    )
    pitch = measured.add_mutually_exclusive_group()
    pitch.add_argument("--pitched", action="store_true", default=None)
    pitch.add_argument("--unpitched", dest="pitched", action="store_false")
    measured.add_argument("--note-range", help="Measured root range, e.g. C1-B1")
    measured.add_argument("--measured-type", choices=library.MEASURED_KINDS)
    measured.add_argument("--measured-bpm", type=float, help="Within ±1 BPM")
    measured.add_argument(
        "--measured-key",
        metavar="KEY",
        help="In this key or its relative, such as A minor, Am or F#; nearest first with --key-within",
    )
    measured.add_argument(
        "--key-within",
        type=int,
        default=0,
        metavar="SEMITONES",
        help="Also the samples a transpose of up to this many semitones puts in --measured-key (0–6)",
    )
    measured.add_argument(
        "--measure",
        type=int,
        default=0,
        metavar="N",
        help="First measure N of the matches that are not measured, and keep what was measured",
    )
    sound = search.add_argument_group(
        "sound",
        "what a measured sample is like (daw describe samples). A word keeps the samples "
        "it is true of among their category: "
        + "; ".join(
            " ".join(f"--{word}" for word in names) + f" ({field})"
            for field, names in descriptors.WORDS.items()
        )
        + ". A number keeps those from --min-FIELD to --max-FIELD, FIELD being duration, "
        + ", ".join(
            short if short == field else f"{short} ({field})"
            for short, field in library.SORTS.items()
            if short != "duration"
        )
        + ": --max-decay 300",
    )
    for word in library.WORDS:
        sound.add_argument(
            f"--{word}", dest="words", action="append_const", const=word, help=argparse.SUPPRESS
        )
    for short in library.SORTS:
        for end in ("min", "max"):
            sound.add_argument(
                f"--{end}-{short}", type=float, metavar="X", help=argparse.SUPPRESS
            )
    like = ss.add_parser(
        "like",
        help="The measured samples nearest a chosen one in sound (daw describe samples)",
    )
    like.add_argument("sample", help="A sample's ID or a file, one in a project too")
    like.add_argument("--limit", type=int, default=10)
    like.add_argument(
        "--category",
        help="Compare with this category's samples, or any for every sample; its own unless given",
    )
    like.add_argument("--folder", type=Path, action="append", help="Compare within this folder; repeat for several")
    like.add_argument(
        "--measure",
        type=int,
        default=0,
        metavar="N",
        help="First measure N of the samples to compare with that are not measured",
    )
    analyze = ss.add_parser(
        "analyze",
        help="Measure pitch, onsets, tempo, loop/one-shot and what the sound is like "
        "from audio (daw describe samples)",
    )
    target = analyze.add_mutually_exclusive_group(required=True)
    target.add_argument("sample", nargs="?")
    target.add_argument(
        "--all",
        action="store_true",
        help="Every indexed sample not yet measured, several at a time; says how far "
        "it is on stderr, and a run that is stopped goes on from there",
    )
    analyze.add_argument("--refresh", action="store_true", help="Ignore the cache")
    beats = ss.add_parser(
        "beats",
        help="Beat and downbeat map of a whole song: tempo, every beat, bars and phrases",
    )
    beats.add_argument("sample")
    beats.add_argument("--near", help="List the beats around a time, as seconds or m:ss")
    beats.add_argument(
        "--window", type=float, default=3.0, help="Seconds either side of --near"
    )
    beats.add_argument("--all", action="store_true", help="List every beat")
    beats.add_argument(
        "--bpm", type=float, help="The song's tempo, to settle half or double time"
    )
    beats.add_argument(
        "--downbeat",
        action="append",
        help="A time whose nearest beat is a downbeat; give it again for a bar the map has wrong later on",
    )
    beats.add_argument(
        "--meter",
        help="The song's time signature, such as 3/4; its beats make a bar (4/4 unless given)",
    )
    beats.add_argument(
        "--refresh", action="store_true", help="Measure again, without corrections"
    )
    beats.add_argument(
        "--click", type=Path, help="Write part of the song with a click on each beat"
    )
    beats.add_argument(
        "--seconds", type=float, default=20, help="Length of the --click audition"
    )
    chords = ss.add_parser(
        "chords",
        help="A whole song's key and its chords by bar and beat, on its beat map",
    )
    chords.add_argument("sample")
    chords.add_argument("--near", help="Only the chords around a time, as seconds or m:ss")
    chords.add_argument(
        "--window", type=float, default=10.0, help="Seconds either side of --near"
    )
    form = ss.add_parser(
        "form",
        help="A whole song's sections on its beat map: which bars are the same music, labelled A, B, A′ "
        "and named, each one's key, the key changes, and checks of the meter and the bars",
        description="daw samples form FILE compares every bar of a song with every other on its chords and "
        "sound and gives its sections. With the parts daw separate --split stems made of the song beside it, "
        "the voice and drums name the sections and the drums check the bars; with the words daw transcribe "
        "--words kept beside the voice, the hook's sections are found and the chorus named by its words "
        "(daw describe beats.form).",
    )
    form.add_argument("sample")
    form.add_argument(
        "--picture",
        type=Path,
        help="Draw the bar matrix, light where two bars are alike, with the sections marked, as PNG",
    )
    sound = ss.add_parser(
        "sound",
        help="How a song and each part daw separate made of it sound: tone as an equalizer, width by "
        "section, pumping with the kick, reverb and delay tails, and the marks of compression",
        description="daw samples sound FILE reads a song and, with the parts daw separate --split stems "
        "made of it beside it, each part, for what a producer sets when mixing it: each one's tone against "
        "flat as the bands of an eq effect, its width whole and by the form's sections, its level dipping "
        "with the kick as a sidechain ducks it, the decay of its tails as a reverb's and their repeats as a "
        "delay's, and its crest and level range. With --against, a part made in a session is compared: the "
        "eq that turns it into the song's part and how far apart their dynamics are (daw describe beats.sound).",
    )
    sound.add_argument("sample")
    sound.add_argument(
        "--part",
        action="append",
        metavar="NAME",
        help="Only this part, mix for the song itself; repeat for more",
    )
    sound.add_argument(
        "--against",
        type=Path,
        metavar="FILE",
        help="A part made in a session, such as a render's stem: the eq that turns it into each part read, "
        "and their dynamics' differences",
    )
    sound.add_argument(
        "--picture",
        type=Path,
        help="Draw each part's tone with its eq, its pumping and its tails, as PNG",
    )
    hits = ss.add_parser(
        "hits",
        help="A song's or a stem's hits in one band, each with its time, bar and beat, "
        "level and the spacing to the next, on its beat map",
    )
    hits.add_argument("sample")
    hits.add_argument(
        "--band",
        default="all",
        help="kick (45-90 Hz), snare (1500-4000), hats (7000-16000), all (the default), "
        "or LOW-HIGH in Hz",
    )
    hits.add_argument("--near", help="Only the hits around a time, as seconds or m:ss")
    hits.add_argument(
        "--window", type=float, default=5.0, help="Seconds either side of --near"
    )
    hits.add_argument(
        "--min-level",
        type=float,
        default=-20.0,
        help="List the hits at most this many dB under the band's loudest (-20)",
    )
    hits.add_argument(
        "--min-rise",
        type=float,
        default=6.0,
        help="How many dB a peak rises over the 100 ms before it to be a hit (6)",
    )
    hits.add_argument(
        "--picture",
        type=Path,
        help="Draw the band's level with the hits marked, a row of 8 bars at a time, as PNG",
    )
    inspect = ss.add_parser("inspect")
    inspect.add_argument("sample")
    aud = ss.add_parser("audition")
    aud.add_argument("sample")
    aud.add_argument("--output", type=Path, required=True)
    aud.add_argument("--seconds", type=float, default=8)
    imp = ss.add_parser(
        "import",
        help="Copy a file into the project and add it to the song; .m4a and .mp3 are decoded to WAV",
    )
    imp.add_argument("sample")
    imp.add_argument("--project", type=song_file, required=True)
    imp.add_argument("--id", help="The sample's ID in the song")
    imp.add_argument(
        "--copy-only",
        action="store_true",
        help="Copy the file into the project and print its entry; the song is not changed",
    )
    imp.add_argument(
        "--root-note",
        help="Note with octave, e.g. C2, or auto to use the measured pitch",
    )
    check = sub.add_parser(
        "check",
        help="inspect, plus root notes against measured pitch and warnings, "
        "each with a code (daw describe check)",
    )
    check.add_argument("project", type=song_file)
    listen = sub.add_parser(
        "listen", help="Measure a saved render or WAV; write analysis JSON and images"
    )
    listen.add_argument("source", type=Path)
    listen.add_argument("--no-images", action="store_true")
    listen.add_argument(
        "--section",
        metavar="ID",
        help="One section of the mix and of each stem, with its third octaves, "
        "resonances, hits and what each compressor, limiter and clipper took off, in place of "
        "the report (daw describe listen)",
    )
    listen.add_argument(
        "--overlap",
        nargs=2,
        metavar=("A", "B"),
        help="Two of a render's stems, by band and by section, in place of the report "
        "(daw describe listen)",
    )
    listen.add_argument(
        "--write-translation",
        action="store_true",
        help="Write mono.wav and small-speaker.wav beside the analysis: the mix as "
        "translation measures it, for a person to hear (daw describe listen)",
    )
    compare = sub.add_parser(
        "compare",
        help="Compare two renders: actual and loudness-matched deltas; or one "
        "render with a reference, section by section (daw describe reference)",
    )
    compare.add_argument("before", type=Path, help="A render, or with --reference the mix")
    compare.add_argument("after", type=Path, nargs="?")
    compare.add_argument(
        "--reference", metavar="NAME", help="A song saved with daw reference add, in place of AFTER"
    )
    compare.add_argument("--no-images", action="store_true")
    reference = sub.add_parser(
        "reference",
        help="Songs kept as what good sounds like, for daw compare --reference",
    )
    rs = reference.add_subparsers(dest="action", required=True)
    add = rs.add_parser(
        "add", help="Analyze a song, .m4a and .mp3 too, and keep its measurements under NAME"
    )
    add.add_argument("file", type=Path)
    add.add_argument("--name", help="Letters, digits and -; the file's name unless given")
    add.add_argument("--bpm", type=float, help="The song's tempo, to settle half or double time")
    add.add_argument(
        "--downbeat", action="append", help="A time whose nearest beat is a downbeat; again for each"
    )
    add.add_argument("--meter", help="The song's time signature, such as 3/4; 4/4 unless given")
    add.add_argument("--replace", action="store_true", help="Write over a reference of this name")
    rs.add_parser("list", help="The saved references")
    show = rs.add_parser("show", help="A reference's measurements, whole and by section")
    show.add_argument("name")
    named = rs.add_parser(
        "sections",
        help="A reference's sections; with JSON, names them by hand",
    )
    named.add_argument("name")
    named.add_argument(
        "sections",
        nargs="?",
        help='Such as \'[{"id": "verse", "at": 32}, {"id": "drop", "seconds": "1:02"}]\': '
        "at in the reference's beats from its first downbeat, or seconds; each runs "
        "to the next",
    )
    named.add_argument(
        "--measured", action="store_true", help="Go back to the sections it measured"
    )
    remove = rs.add_parser("remove", help="Forget a reference; the song's file is left")
    remove.add_argument("name")
    key = sub.add_parser(
        "key",
        help="The key of what the song plays, from its notes and its samples, and with "
        "--sample the transpose that puts a sample in it",
    )
    key.add_argument("project", type=song_file)
    key.add_argument(
        "--sample",
        action="append",
        default=[],
        metavar="SAMPLE",
        help="One of the song's samples by its ID, a file, or a library ID, to fit to the song's key; repeat for several",
    )
    key.add_argument("--db", type=Path, default=None, help="The sample library's index")
    timeline = sub.add_parser(
        "timeline",
        help="The song in beats and seconds: where its sounds are, conversions, and its length",
    )
    timeline.add_argument("project", type=song_file)
    timeline.add_argument("--seconds", nargs="+", default=[], help="Times, as seconds or m:ss, to give in beats")
    timeline.add_argument("--beats", nargs="+", default=[], help="Beats to give in seconds")
    timeline.add_argument("--end-at", help="A beat for --pad to end on; gives the beat it starts on")
    timeline.add_argument("--pad", metavar="TRACK.PAD")
    timeline.add_argument(
        "--fit",
        action="store_true",
        help="Does nothing now: the length is the song's content, set after every edit",
    )
    timeline.add_argument("--tail", type=float, default=0, help="Kept with --fit; does nothing")
    timeline.add_argument(
        "--tempo-for", type=float, metavar="SECONDS", help="The tempo at which the song is this long"
    )
    joins = sub.add_parser(
        "joins",
        help="Check a render's joins between parts of a song, and its length",
    )
    joins.add_argument("source", type=Path, help="A render: its folder, report or pointer")
    joins.add_argument("--limit", type=float, help="The longest the file may be, seconds")
    joins.add_argument(
        "--seconds", type=float, default=4, help="Length of each join's excerpt"
    )
    joins.add_argument("--no-excerpts", action="store_true")
    export = sub.add_parser(
        "export",
        help="Write what is heard of the song, whole or a range, as a named WAV, AIFF, FLAC, AAC or MP3 file at a stated level",
        description="Write what is heard of the song, whole or a range of it, as a named file: "
        "muted and solo-muted tracks and deactivated clips are not in it. A name alone, or no "
        "--to, goes in the project's exports/ folder.",
    )
    export.add_argument("project", type=song_file)
    export.add_argument(
        "--to",
        type=Path,
        help="NAME.wav, .aiff, .flac, .m4a or .mp3; a name alone is put in exports/; "
        "exports/TITLE.wav when not given",
    )
    export.add_argument("--bits", type=int, choices=[16, 24, 32], help="For .wav, .aiff (24 unless given; 32 is float) and .flac (16 or 24)")
    export.add_argument("--sample-rate", type=int, help="Hz, such as 44100 or 48000: the song's unless given")
    export.add_argument("--bitrate", type=int, help="kb/s: 256 for .m4a, 320 for .mp3")
    where = export.add_argument_group(
        "range", "the whole song unless given; a place is beats from the start (66) or BAR:BEAT (17:1)"
    )
    where.add_argument("--start", metavar="PLACE", help="Where the file starts")
    where.add_argument("--end", metavar="PLACE", help="Where it stops: 36:1 ends after bar 35")
    where.add_argument("--bars", metavar="A-B", help="Whole bars from 1, both included: 2-35")
    where.add_argument("--section", metavar="ID", help="One section of the song")
    level = export.add_argument_group(
        "level", "one gain for the whole file, held under --ceiling; as rendered by default"
    ).add_mutually_exclusive_group()
    level.add_argument("--gain", type=float, help="A gain in dB")
    level.add_argument("--peak", type=float, help="Put the true peak at this dBTP")
    level.add_argument("--lufs", type=float, help="Reach this integrated loudness")
    level.add_argument("--match", metavar="SAMPLE", help="Be as loud as this sample of the song")
    export.add_argument(
        "--ceiling", type=float, default=-0.1, help="The highest sample peak a gain may reach, dBFS"
    )
    export.add_argument("--render", type=Path, help="A render to export instead of the latest")
    export.add_argument("--replace", action="store_true", help="Write over an existing file")
    separate = sub.add_parser(
        "separate",
        help="Take a song apart on this Mac into vocals and instrumental, or drums, bass, "
        "vocals and the rest, each a sample in the project; daw separate models lists the "
        "models and daw separate setup installs them (daw describe separate)",
        description="daw separate PROJECT SOURCE takes a song apart with an open-source model "
        "run on this Mac and writes each part into the project as a sample. SOURCE is a "
        "sample's ID, an audio clip as tracks.T.audio.N, or a file, imported first. "
        "daw separate models lists what can run and what is downloaded; daw separate setup "
        "makes the runtime and downloads the default models, or --models NAME.",
    )
    separate.add_argument("project", help="The project, or models, or setup")
    separate.add_argument("source", nargs="?", help="A sample ID, an audio clip, or a file")
    separate.add_argument(
        "--split",
        choices=list(SPLITS),
        help="vocals (the default): vocals and instrumental; stems: drums, bass, vocals, other; "
        "stems6: with guitar and piano",
    )
    separate.add_argument(
        "--quality", choices=["fast", "best"], default="best", help="Which model of the split; best unless given"
    )
    separate.add_argument("--model", help="A model by name, from daw separate models")
    separate.add_argument(
        "--id", dest="prefix", help="The parts' sample IDs are PREFIX-PART; the source's ID unless given"
    )
    separate.add_argument(
        "--tracks",
        action="store_true",
        help="Lay the parts on tracks of their own under the source, and silence the source clip",
    )
    install = separate.add_argument_group("setup", "what daw separate setup installs")
    install.add_argument("--models", nargs="+", metavar="NAME", help="These models; the defaults unless given")
    install.add_argument("--all", action="store_true", help="Every model in the manifest")
    install.add_argument("--runtime-only", action="store_true", help="The environment and no model")
    transcribe = sub.add_parser(
        "transcribe",
        help="The notes a part plays, heard from its audio by Basic Pitch on this Mac, each with "
        "its bar and beat on the song's beat map, velocity and confidence; as note text, a MIDI "
        "file or a note clip in a song (daw describe beats.transcribe); with --words the words a voice "
        "sings by bar, heard by Whisper (daw describe beats.words)",
        description="daw transcribe SOURCE hears the notes of a part, best a stem from daw separate, "
        "and places them on the song's beat map. SOURCE is a file or a library sample, or with "
        "--project one of the song's samples by ID or an audio clip as tracks.T.audio.N.",
    )
    transcribe.add_argument("source", help="A file or library sample; with --project a sample ID or tracks.T.audio.N")
    transcribe.add_argument(
        "--project",
        type=song_file,
        help="Add the notes to this song as a note clip: where the audio clip plays them, or with bar 1 at --at",
    )
    transcribe.add_argument("--track", help="The MIDI track the clip goes on; a new one named after the part unless given")
    transcribe.add_argument("--at", type=float, help="The song's beat bar 1 of the part lands on (0)")
    transcribe.add_argument(
        "--line", action="store_true", help="One note at a time, for a bass line or a lead: the surest of notes that sound together"
    )
    transcribe.add_argument("--range", dest="note_range", metavar="LOW-HIGH", help="Only notes from LOW to HIGH, such as E1-G3")
    transcribe.add_argument(
        "--min-confidence",
        type=float,
        default=0.0,
        help="Leave out notes the model is less sure of, 0 to 1; sure counts those of 0.5 and more (0: none left out)",
    )
    transcribe.add_argument(
        "--beats",
        metavar="FILE",
        help="Place the notes on this file's beat map, the whole song's; a part daw separate made is placed on its source's unless given",
    )
    transcribe.add_argument("--near", help="Only the notes around a time, as seconds or m:ss")
    transcribe.add_argument("--window", type=float, default=10.0, help="Seconds either side of --near")
    transcribe.add_argument("--midi", type=Path, help="Write the notes as a type 0 MIDI file, bar 1 at bar_one_beat")
    transcribe.add_argument("--text", action="store_true", help="The notes as note text, a row a voice")
    transcribe.add_argument("--grid", help="A cell's beats for --text: 1/4, the default, is a sixteenth note")
    transcribe.add_argument("--db", type=Path, default=None, help="The sample library's index")
    transcribe.add_argument(
        "--quantize", metavar="GRID", help="Move the written notes onto a grid in beats, 1/4 a sixteenth, as daw note quantize does"
    )
    transcribe.add_argument(
        "--drums",
        action="store_true",
        help="A drum part's kicks, snares and hats as C2, D2 and F#2, the grooves its bars share and its fills; "
        "in a song a Sampler of its own clearest hits, a looped clip a groove and a clip a fill",
    )
    transcribe.add_argument(
        "--as-played",
        action="store_true",
        help="With --drums, write one clip of every hit as played instead of grooves looped and fills",
    )
    transcribe.add_argument(
        "--words",
        action="store_true",
        help="The words a voice sings, heard by Whisper on this Mac, as lines with each word's bar and beat; "
        "the vocals part daw separate made of a song is read in its place, the doubtful stretches read again "
        "on short clips by the larger model, and the lines that come back listed (daw describe beats.words)",
    )
    transcribe.add_argument("--language", default="en", help="With --words, the language sung, a code such as en or es; '' lets Whisper tell (en)")
    transcribe.add_argument("--lyrics", type=Path, metavar="OUT.txt", help="With --words, write the lines as text, a line each with its time and bar")
    transcribe.add_argument(
        "--once", action="store_true", help="With --words, do not read the doubtful stretches again: one reading, faster"
    )
    transcribe.add_argument(
        "--again", action="store_true", help="With --words, hear the voice again rather than use the words kept beside it"
    )
    return p


def execute(a):
    if a.command in ("listen", "compare"):
        from . import perception

        if a.command == "listen":
            if a.overlap and a.section:
                raise ValueError("listen takes --overlap A B or --section ID, not both")
            if a.write_translation and (a.overlap or a.section):
                raise ValueError(
                    "--write-translation goes with the whole report, not --overlap or --section"
                )
            if a.overlap:
                return perception.overlap_of(a.source, *a.overlap, images=not a.no_images)
            if a.section:
                return perception.section_of(a.source, a.section, images=not a.no_images)
            return perception.printed(
                perception.listen(a.source, not a.no_images, a.write_translation)
            )
        if (a.after is None) == (a.reference is None):
            raise ValueError(
                "compare takes two renders, or one render and --reference NAME"
            )
        if a.reference is not None:
            from . import reference

            return reference.compare(a.before, a.reference)
        return perception.compare(a.before, a.after, images=not a.no_images)
    if a.command == "reference":
        return reference_command(a)
    if a.command == "key":
        return song_key(a)
    if a.command == "check":
        return check(a.project)
    if a.command == "timeline":
        from . import timeline

        return timeline.report(
            a.project, a.seconds, a.beats, a.end_at, a.pad, a.fit, a.tail, a.tempo_for
        )
    if a.command == "joins":
        from . import joins

        if not 0 < a.seconds <= 30 or (a.limit is not None and a.limit <= 0):
            raise ValueError("seconds must be >0 and <=30, and limit >0")
        return joins.check(a.source, a.limit, a.seconds, not a.no_excerpts)
    if a.command == "separate":
        from . import separation

        if a.project == "models":
            return separation.listing(a.split)
        if a.project == "setup":
            return separation.setup(a.models, a.all, a.runtime_only)
        if a.source is None:
            raise ValueError("separate takes PROJECT SOURCE, or models, or setup")
        if a.models or a.all or a.runtime_only:
            raise ValueError("--models, --all and --runtime-only go with daw separate setup")
        return separation.separate(
            song_file(a.project), a.source, a.split, a.quality, a.model, a.prefix, a.tracks
        )
    if a.command == "transcribe":
        return transcribe(a)
    if a.command == "export":
        from .export import export

        if a.bitrate is not None and not 64 <= a.bitrate <= 320:
            raise ValueError("bitrate must be 64–320 kb/s")
        if a.bars is not None or a.section is not None:
            if sum(v is not None for v in (a.bars, a.section, a.start or a.end)) > 1:
                raise ValueError("A range is --start and --end, --bars or --section, one of them")
        return export(
            a.project,
            a.to,
            start=a.start,
            end=a.end,
            bars=a.bars,
            section=a.section,
            sample_rate=a.sample_rate,
            bits=a.bits,
            bitrate=a.bitrate,
            gain=a.gain,
            peak=a.peak,
            lufs=a.lufs,
            match=a.match,
            ceiling=a.ceiling,
            render=a.render,
            replace=a.replace,
        )
    from . import library
    from .model import midi

    if a.action == "folders":
        if a.operation == "list":
            return library.folders(a.db)
        if a.operation == "refresh":
            return [library.scan(Path(f["path"]), a.db) if f["available"] else
                    {"root": f["path"], "error": "Folder unavailable"} for f in library.folders(a.db)]
        if a.directory is None:
            raise ValueError("folders add/remove requires a directory")
        if a.operation == "add":
            return library.scan(a.directory, a.db)
        return library.remove_folder(a.db, a.directory)
    if a.action == "scan":
        return library.scan(a.directory, a.db)
    if a.action == "search":
        if not 1 <= a.limit <= 1000:
            raise ValueError("limit must be 1–1000")
        note_range = None
        if a.note_range:
            low, sep, high = a.note_range.partition("-")
            if not sep:
                raise ValueError("note-range must look like C1-B1")
            note_range = (midi(low), midi(high))
            if note_range[0] > note_range[1]:
                raise ValueError("note-range low note exceeds high note")
        if a.measure < 0:
            raise ValueError("measure must be 0 or more")
        if not 0 <= a.key_within <= 6:
            raise ValueError("key-within must be 0–6 semitones")
        if a.key_within and a.measured_key is None:
            raise ValueError("--key-within goes with --measured-key")
        from .harmony import parse_key

        measured_key = None if a.measured_key is None else parse_key(a.measured_key)
        notes = []
        found = library.search(
            a.db,
            a.query,
            a.category,
            a.kind,
            a.key,
            a.bpm,
            a.limit,
            pitched=a.pitched,
            note_range=note_range,
            measured_kind=a.measured_type,
            measured_bpm=a.measured_bpm,
            roots=a.folder,
            paths=a.path,
            sort=a.sort,
            reverse=a.reverse,
            words=a.words or (),
            ranges={
                field: (getattr(a, f"min_{short}"), getattr(a, f"max_{short}"))
                for short, field in library.SORTS.items()
            },
            measure=a.measure,
            notes=notes,
            measured_key=measured_key,
            key_within=a.key_within,
        )
        for note in notes:
            print(json.dumps({"note": note}), file=sys.stderr)
        return found
    if a.action == "like":
        if not 1 <= a.limit <= 1000 or a.measure < 0:
            raise ValueError("limit must be 1–1000, and measure 0 or more")
        source = library.resolve(a.db, a.sample)
        return library.like(a.db, source, a.limit, a.category, a.folder, a.measure)
    if a.action == "analyze" and a.all:
        return library.analyze_all(a.db, a.refresh)
    source = library.resolve(a.db, a.sample)
    if a.action == "analyze":
        return library.analyze(a.db, source, a.refresh)
    if a.action == "beats":
        return beat_map(a, source)
    if a.action == "chords":
        return song_chords(a, source)
    if a.action == "hits":
        return song_hits(a, source)
    if a.action == "form":
        return song_form(a, source)
    if a.action == "sound":
        return song_sound(a, source)
    if a.action == "inspect":
        return library.inspect(source)
    if a.action == "audition":
        if not 0 < a.seconds <= 60:
            raise ValueError("seconds must be >0 and <=60")
        return library.audition(source, a.output, a.seconds)
    return import_sample(a, source)


def reference_command(a):
    """`daw reference`: add, list, show, sections and remove."""
    from . import beats, reference

    if a.action == "add":
        if a.bpm is not None and not beats.MIN_BPM <= a.bpm <= beats.MAX_BPM:
            raise ValueError(f"bpm must be {beats.MIN_BPM:g}–{beats.MAX_BPM:g}")
        downbeat = None if a.downbeat is None else [beats.seconds(t) for t in a.downbeat]
        return reference.add(a.file, a.name, a.bpm, downbeat, a.meter, a.replace)
    if a.action == "list":
        return reference.listing()
    if a.action == "show":
        return reference.shown(reference.load(a.name))
    if a.action == "remove":
        return reference.remove(a.name)
    if a.sections is not None and a.measured:
        raise ValueError("sections takes JSON or --measured, not both")
    try:
        spec = None if a.sections is None else json.loads(a.sections)
    except ValueError:
        raise ValueError(
            'Sections are a JSON list such as [{"id": "verse", "at": 32}]'
        ) from None
    return reference.sections(a.name, spec, a.measured)


def beat_map(a, source):
    """A song's beat map: its summary, and the beats asked for."""
    from . import beats, library

    if a.bpm is not None and not beats.MIN_BPM <= a.bpm <= beats.MAX_BPM:
        raise ValueError(f"bpm must be {beats.MIN_BPM:g}–{beats.MAX_BPM:g}")
    if a.window <= 0 or not 0 < a.seconds <= 60:
        raise ValueError("window must be >0, and seconds >0 and <=60")
    downbeat = None if a.downbeat is None else [beats.seconds(t) for t in a.downbeat]
    at = None if a.near is None else beats.seconds(a.near)
    if a.meter is not None:
        beats.bar_of(a.meter)
    report = library.beat_map(a.db, source, a.bpm, downbeat, a.refresh, a.meter)
    rows = report.pop("beats")
    if a.all:
        report["beats"] = rows
    if at is not None:
        report["near"] = beats.near(rows, at, a.window)
    if a.click:
        # Around the time asked about, or else from just before the first downbeat.
        start = report["first_downbeat_seconds"] - 0.5 if at is None else at - a.seconds / 2
        report["click"] = beats.click(source, rows, a.click, start, a.seconds)
    return report


def song_chords(a, source):
    """A song's key and its chords, a run of beats a chord, on its beat map."""
    from . import beats, harmony, library

    if a.window <= 0:
        raise ValueError("window must be >0")
    at = None if a.near is None else beats.seconds(a.near)
    report = library.beat_map(a.db, source)
    rows = report["beats"]
    mono, sr = beats.read(source)
    key = harmony.key_of(mono, sr)
    period = 60 / report["tempo"]["bpm"]
    times = [b["seconds"] for b in rows]
    times.append(min(times[-1] + period, len(mono) / sr))
    runs = harmony.chords(
        mono,
        sr,
        times,
        [b["beat"] == 1 for b in rows],
        harmony.parse_key(key["name"]) if key["tonal"] else None,
    )
    found = []
    for run in runs:
        first = rows[run["first_beat"]]
        end = times[run["first_beat"] + run["beats"]]
        found.append(
            {
                "bar": first["bar"],
                "beat": first["beat"],
                "seconds": first["seconds"],
                "until_seconds": round(end, 6),
                "beats": run["beats"],
                "chord": run["chord"],
                "fit": run["fit"],
            }
        )
    if at is not None:
        found = [c for c in found if c["until_seconds"] >= at - a.window and c["seconds"] <= at + a.window]
    key.pop("profile", None)
    return {
        "path": str(source),
        "key": key,
        "tempo_bpm": report["tempo"]["bpm"],
        "meter": report["meter"],
        "first_downbeat_seconds": report["first_downbeat_seconds"],
        "chords": found,
        "beat_map": report.get("map"),
        "source": "measured from decoded audio (mono sum) on the beat map; estimates, not ground truth",
    }


def song_form(a, source):
    """A song's sections on its beat map, with its parts when daw separate made them."""
    from . import form, library

    if a.picture is not None and a.picture.suffix.lower() != ".png":
        raise ValueError("picture is a .png file")
    report = library.beat_map(a.db, source)
    found = form.measure(source, report, form.parts_of(source))
    if a.picture is not None:
        found["picture"] = form.picture(found, a.picture)
    found.pop("_matrix")
    found.pop("_bars")
    found["source"] = "measured from decoded audio on the beat map; estimates, not ground truth"
    return found


def song_sound(a, source):
    """How a song and each of its parts sound, on its beat map and its form."""
    from . import form, library, sound

    if a.picture is not None and a.picture.suffix.lower() != ".png":
        raise ValueError("picture is a .png file")
    if a.against is not None and not a.against.is_file():
        raise ValueError(f"No file {a.against}")
    try:
        report = library.beat_map(a.db, source)
        problem = None
    except ValueError as error:
        report, problem = None, str(error)
    parts = form.parts_of(source)
    sections, sections_why = None, None
    if report is not None:
        try:
            found = form.measure(source, report, parts)
            sections = found["sections"]
        except ValueError as error:
            sections_why = str(error)
    out = {
        "path": str(source),
        "tempo_bpm": None if report is None else report["tempo"]["bpm"],
        **({"no_beat_map": problem} if problem else {}),
        "parts": {name: str(path) for name, path in parts.items()},
        "sections": None
        if sections is None
        else [
            {k: s[k] for k in ("label", "name", "bars", "seconds", "until_seconds") if k in s}
            for s in sections
        ],
        **({"no_sections": sections_why} if sections_why else {}),
    }
    if not parts:
        out["no_parts"] = (
            "Only the song is read: daw separate PROJECT FILE --split stems makes its parts, "
            "and each part's sound is read beside it"
        )
    out.update(sound.measure(source, report, parts, sections, a.part, a.against))
    if a.picture is not None:
        out["picture"] = sound.picture(out, a.picture)
    out["source"] = (
        "measured from decoded audio and the parts daw separate made; estimates, checked on renders "
        "with known settings, not ground truth"
    )
    return out


def song_hits(a, source):
    """A song's or a stem's hits in one band, on its beat map where it has one."""
    from . import beats, hits, library

    name, low, high = hits.band(a.band)
    if a.window <= 0:
        raise ValueError("window must be >0")
    if a.min_level > 0 or a.min_rise <= 0:
        raise ValueError("min-level must be 0 or under, and min-rise over 0")
    at = None if a.near is None else beats.seconds(a.near)
    if a.picture is not None and a.picture.suffix.lower() != ".png":
        raise ValueError("picture is a .png file")
    try:
        report = library.beat_map(a.db, source)
        problem = None
    except ValueError as error:
        # A loop or a hit too short for a beat map still has hits, in seconds.
        report, problem = None, str(error)
    found, drawn = hits.measure(source, low, high, report, a.min_rise, a.min_level)
    rows = found["hits"]
    if at is not None:
        rows = [r for r in rows if abs(r["seconds"] - at) <= a.window]
    out = {
        "path": str(source),
        "band": {"name": name, "low_hz": low, "high_hz": high},
        "tempo_bpm": None if report is None else report["tempo"]["bpm"],
        "first_downbeat_seconds": None if report is None else report["first_downbeat_seconds"],
        "beat_map": None if report is None else report.get("map"),
        **({"no_beat_map": problem} if problem else {}),
        "min_level_db": a.min_level,
        "min_rise_db": a.min_rise,
        "loudest_dbfs": found["loudest_dbfs"],
        "levels": found["levels"],
        "count": len(rows),
        "hits": rows,
    }
    if at is not None:
        out["near"] = {"seconds": at, "window_seconds": a.window, "of": found["count"]}
    if a.picture is not None:
        span = None if at is None else (at - a.window, at + a.window)
        mono, sr, db, step, _, grid = drawn
        out["picture"] = hits.picture((mono, sr, db, step, rows, grid), a.picture, name, span)
    out["source"] = "measured from decoded audio (mono sum) on the beat map; estimates, not ground truth"
    return out


def transcribe(a):
    """`daw transcribe`: a part's notes on the song's beat map."""
    from . import beats, library, transcription
    from .model import midi

    if not 0 <= a.min_confidence <= 1 or a.window <= 0:
        raise ValueError("min-confidence must be 0–1, and window >0")
    if a.track is not None and a.project is None:
        raise ValueError("--track goes with --project")
    if a.at is not None and (a.project is None or a.at < 0):
        raise ValueError("--at goes with --project, and is a beat of 0 or more")
    if a.as_played and not a.drums:
        raise ValueError("--as-played goes with --drums")
    worded = [f for f, v in (("--lyrics", a.lyrics), ("--once", a.once), ("--again", a.again)) if v]
    if worded and not a.words:
        raise ValueError(f"{', '.join(worded)} {'goes' if len(worded) == 1 else 'go'} with --words")
    if a.words:
        from . import words

        given = [
            flag
            for flag, value in (
                ("--drums", a.drums),
                ("--line", a.line),
                ("--range", a.note_range),
                ("--min-confidence", a.min_confidence),
                ("--quantize", a.quantize),
                ("--midi", a.midi),
                ("--text", a.text),
                ("--grid", a.grid),
                ("--track", a.track),
                ("--at", a.at is not None),
            )
            if value
        ]
        if given:
            raise ValueError(
                f"{', '.join(given)} {'is' if len(given) == 1 else 'are'} for notes; --words lists lines and their words, "
                "and --lyrics writes them as text"
            )
        return words.transcribe_words(
            a.source,
            song=a.project,
            beats_of=a.beats,
            near=None if a.near is None else beats.seconds(a.near),
            window=a.window,
            lyrics_out=a.lyrics,
            language=a.language,
            again=not a.once,
            fresh=a.again,
            db=getattr(a, "db", None) or library.default_db(),
        )
    if a.drums:
        from . import drums

        given = [
            flag
            for flag, value in (
                ("--line", a.line),
                ("--range", a.note_range),
                ("--min-confidence", a.min_confidence),
                ("--quantize", a.quantize),
            )
            if value
        ]
        if given:
            raise ValueError(
                f"{', '.join(given)} {'is' if len(given) == 1 else 'are'} for pitched parts; with --drums each hit is "
                "written on its sixteenth with its own timing, which daw note quantize moves after"
            )
        return drums.transcribe(
            a.source,
            song=a.project,
            track=a.track,
            at=a.at,
            beats_of=a.beats,
            near=None if a.near is None else beats.seconds(a.near),
            window=a.window,
            midi_out=a.midi,
            text=a.text,
            grid=a.grid,
            as_played=a.as_played,
            db=getattr(a, "db", None) or library.default_db(),
        )
    note_range = None
    if a.note_range:
        low, sep, high = a.note_range.partition("-")
        if not sep:
            raise ValueError("range must look like E1-G3")
        note_range = (midi(low), midi(high))
        if note_range[0] > note_range[1]:
            raise ValueError("range's low note is above its high note")
    return transcription.transcribe(
        a.source,
        song=a.project,
        track=a.track,
        at=a.at,
        line=a.line,
        note_range=note_range,
        min_confidence=a.min_confidence,
        beats_of=a.beats,
        near=None if a.near is None else beats.seconds(a.near),
        window=a.window,
        midi_out=a.midi,
        text=a.text,
        grid=a.grid,
        quantize=a.quantize,
        db=getattr(a, "db", None) or library.default_db(),
    )


def song_key(a):
    """`daw key`: the key of what the song plays, and what fits a sample to it."""
    from . import library, tonality
    from .model import load

    db = a.db or library.default_db()
    found = tonality.of_song(a.project)
    tempo = float(load(a.project, False)["session"]["tempo"])
    fits = []
    samples = load(a.project, False)["samples"]
    for value in a.sample:
        # One of the song's own samples by its ID, a file, or a library ID.
        own = samples.get(value)
        path = a.project.parent / own["path"] if own else library.resolve(db, value)
        fits.append(tonality.fit(value, found["key"], library.analyze(db, path), tempo))
    found["key"].pop("profile", None)
    if fits:
        found["samples"] = fits
    return found


def import_sample(a, source):
    """Copies a sample into the project and adds it to the song."""
    from . import library
    from .model import load, midi, project_hash

    if a.copy_only:
        # What adds the copy to the song, such as the app, names it then.
        if a.root_note not in (None, "auto"):
            midi(a.root_note)
        project = None
    elif not a.id:
        raise ValueError("--id is required unless --copy-only")
    else:
        project = load(a.project)
        if a.id in project["samples"]:
            raise ValueError(f"Sample ID already exists: {a.id}")
    root_note, measured, asset = a.root_note, None, None
    if root_note == "auto":
        audio = source
        if source.suffix.lower() in library.COMPRESSED:
            # Pitch is measured from the decoded copy.
            asset = library.import_asset(source, a.project.parent)
            audio = a.project.parent / asset["path"]
        measured = library.analyze(a.db, audio)["pitch"]
        if not measured["pitched"]:
            raise ValueError(
                "No reliable pitch measured (confidence "
                f"{measured['confidence']}); inspect the sample and pass --root-note"
            )
        root_note = measured["note"]
    asset = asset or library.import_asset(source, a.project.parent)
    asset["root_note"] = root_note
    if a.copy_only:
        result = dict(asset)
    else:
        # The song takes the sample as an edit, so a running host sees it and can
        # undo it, and an edit made since the read above is not written over.
        with tempfile.TemporaryDirectory() as scratch:
            patch = Path(scratch) / "patch.json"
            patch.write_text(json.dumps({"samples": {a.id: asset}}))
            label = f"Import sample {a.id}"
            sha = project_hash(project)
            run_engine("apply", a.project, patch, "--expect", sha, "--label", label)
        result = {"sample_id": a.id, **asset}
    if "source_sha256" in asset:
        info = library.probe(a.project.parent / asset["path"])
        result["decoded"] = {
            "sample_rate": info.samplerate,
            "channels": info.channels,
            "duration": info.duration,
        }
    if measured:
        result["measured_pitch"] = measured
        if abs(measured["cents"]) > 10:
            result["suggested_pad_transpose"] = round(-measured["cents"] / 100, 2)
    return result


def check(path):
    """`daw inspect`, each root note against its measured pitch, and warnings."""
    from .model import load, warnings

    project = load(path)
    result = run_engine("inspect", path)
    checked = root_notes(project, path.parent)
    seconds = checked.pop("seconds")
    result.update(checked)
    result["warnings"] += warnings(project, seconds)
    result["warnings"] += device_warnings(project, path)
    return result


def device_warnings(project, path):
    """A device of the person's own the song names that is not installed, or
    installed at another version than the song was written with: in a
    chain, among a MIDI track's MIDI effects, or as its instrument."""
    chains = [(f"tracks.{t['id']}.effects", t["effects"]) for t in project["tracks"]]
    chains += [(f"tracks.{t['id']}.midi_effects", t.get("midi_effects", [])) for t in project["tracks"]]
    chains += [(f"groups.{g['id']}.effects", g["effects"]) for g in project.get("groups", [])]
    chains += [(f"returns.{r['id']}.effects", r["effects"]) for r in project.get("returns", [])]
    chains.append(("master.effects", project["master"]["effects"]))
    named = [(f"{owner}.{e.get('id') or i}", e) for owner, effects in chains for i, e in enumerate(effects) if e.get("type") == "device"]
    for t in project["tracks"]:
        device = (t.get("instrument") or {}).get("device")
        if device:
            named.append((f"tracks.{t['id']}.instrument", device))
    if not named:
        return []
    installed = {d["slug"]: d for d in run_engine("plugin", "list", "--project", path)["devices"]}
    out = []
    for ref, e in named:
        found = installed.get(e["name"])
        if found is None:
            out.append(warning("device-missing", f"{ref}: {e['name']} is not installed; its sound passes through", ref))
        elif e.get("version") not in (None, found["version"]):
            out.append(warning("device-version", f"{ref}: {e['name']} is installed at {found['version']}; the song was written with {e['version']}", ref))
    return out


def warning(code, message, path):
    """A warning as `daw check` gives them; daw describe check lists the codes."""
    return {"code": code, "level": "warning", "message": message, "paths": [path]}


def root_notes(project, root):
    """Compare each declared root_note with the pitch measured from its asset.

    A sample the engine cannot read is a warning here and not a note to compare.
    Also gives each readable sample's length in seconds, for the checks of
    audio clips that play to the end of their file.
    """
    from .analysis import compare_root, measure_pitch, PITCH_SECONDS
    from .library import probe
    import soundfile as sf

    report, warnings, seconds = {}, [], {}
    for name, sample in project["samples"].items():
        path = root / sample["path"]
        try:
            info = probe(path)
        except ValueError as e:
            warnings.append(
                warning("sample-unreadable", f"{name}: {e}; a render will fail", f"samples.{name}")
            )
            continue
        seconds[name] = info.frames / info.samplerate
        if not sample["root_note"]:
            continue
        x, sr = sf.read(
            path,
            frames=min(info.frames, round((PITCH_SECONDS + 5) * info.samplerate)),
            always_2d=True,
            dtype="float64",
        )
        entry = compare_root(sample["root_note"], measure_pitch(x.mean(axis=1), sr))
        report[name] = entry
        if entry["status"] != "ok":
            warnings.append(
                warning(
                    "root-note-mismatch",
                    f"{name}: root_note {entry['declared']} but measured "
                    f"{entry['measured']} ({entry['status']})",
                    f"samples.{name}",
                )
            )
    return {"root_notes": report, "warnings": warnings, "seconds": seconds}


def main():
    argv = sys.argv[1:]
    if not argv or argv[0] not in COMMANDS:
        # Not Python's: the Rust daw takes this process's place.
        try:
            binary = str(engine())
            os.execv(binary, [binary, *argv])
        except (RuntimeError, OSError) as exc:
            command = argv[0] if argv else None
            print(json.dumps({"error": str(exc), "command": command}), file=sys.stderr)
            return 1
    args = parser().parse_args(argv)
    try:
        result = execute(args)
        print(json.dumps(result, indent=2, allow_nan=False), flush=True)
    except BrokenPipeError:
        # A reader that stopped, as `head` does: end quietly, with the status a
        # shell gives a closed pipe, and nothing left to flush at exit.
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        return 141
    except (ValueError, OSError, RuntimeError, KeyError, TypeError) as exc:
        print(json.dumps({"error": str(exc), "command": args.command}), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
