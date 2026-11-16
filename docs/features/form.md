# The form of a song — implemented November 16, 2026

Item 3 of the reverse-engineer track (D182, D186): which bars of a finished
song are the same music, its sections labelled A, B, A′ and named by what they
hold, each section's key and where the key changes, with the meter and the bars
checked. `daw samples form` does it, from the song's audio and, when `daw
separate` has taken the song apart, its voice and drums. Everything is measured
on the Mac with numpy; no model is installed.

## Command

```sh
uv run daw samples form projects/NAME/samples/ID_song.wav
uv run daw samples form SONG.wav --picture /tmp/form.png
```

`FILE` is a file or a library sample, as for `daw samples beats`, whose map it
reads with its corrections. With the parts `daw separate PROJECT FILE --split
stems` made of it, it finds them by their recipe in the project's `separated`
folder, the one beside the file's `samples` folder, and lists them in `parts`.

## How the sections are found

1. **Bars.** Each bar of the beat map from bar 1 is read beat by beat: its
   pitch classes, the tones' twelve and the bass's twelve apart, from the
   chromagram `daw samples chords` reads, and its level in 32 bands. A bar of
   another length is read at the usual bar's number of steps.
2. **Bar against bar.** Every pair of bars is compared on their chords (the
   cosine of their pitch classes, step by step) and their sound (how far apart
   their levels are, in dB). Each comparison is made a rank among all the
   song's pairs, so a song that plays one figure throughout is told apart by
   its sound and a song of one sound by its chords; their mean is the bar
   matrix the picture draws.
3. **Where sections start.** At a bar line where what follows is most unlike
   what came before, over four bars and over eight (Foote's checkerboard along
   the matrix's diagonal), at least four bars from another start and two from
   the song's ends; and, with the voice part, where the voice comes in after
   two bars or more of rest. `starts_by` says which, and `change` how unlike.
4. **Which are the same.** Each section is compared with each before it, bar
   by bar from their starts or from their ends, on chords and sound apart.
   Both at least alike as the same music (0.75 and 0.72 as ranks) is that
   section again, its letter. Chords alike and sound less so (0.5) is it
   varied, A′. Chords alike only moved some semitones is it in another key,
   A′ too: the bars match moved and the sections' whole pitch-class profiles
   match best moved by the same semitones, or the profiles alone match moved
   (0.75, and 0.4 over unmoved) with the sound the same. A section that is an
   earlier one again with its start a bar sooner or later is moved there.
   Sections are not slid further: a song that plays one two-bar figure
   throughout would then match every section with every other.
5. **Names.** With the voice part: the sections before the voice first sings
   are the intro and those after it last sings the outro; of the letters that
   come back and are sung, the loudest is the chorus, or the letter it always
   leads into, since a pre-chorus is often the louder; the earliest other
   letter that comes back and is sung, before the chorus, the verse; a letter
   always just before the chorus the pre-chorus, always just after it the
   post-chorus; music sung once after a chorus a bridge; and the rest of a run
   of sung sections that holds the verse, between choruses, the verse's too.
   A section with no voice is `instrumental`, a quiet one without drums
   `break`, and music louder than the chorus that comes back unsung a `drop`.
   Without the parts, the names come from how loud each section is and how
   often it comes: the intro and outro are music that never comes back at the
   ends, and a last section falling 9 dB fades out. `named_by` says from what.
   A part whose loud bars are 30 dB under the song's holds only what leaked
   into it and is in `silent_parts`.
6. **Keys.** Each section's key is the key its pitch classes are most like,
   with its confidence; `key_sure` is `daw samples chords`' test, that the
   sound is plainly in a key, which a section of a full mix often is not, and
   its likeliest key is still given. `key_changes` are a section that is an
   earlier one moved, with the semitones, and a key held plainly from a
   section to the end over eight bars or more. A key that comes and goes with
   the chords is not called a change.

## Checks

- **The meter.** `meter` and `odd_bars` as the map has them. `beat.divides_in`
  is 2 or 3, from where the onsets between beats fall, at the first third of
  a beat or at its half (`thirds_share`); a swung eighth falls at the second
  third and is not counted. `heard_as` is the meter that makes, 12/8 for four
  beats that each divide in three, 6/8 for two.
- **The pulse.** `beat_changes_length` lists runs of eight beats or more whose
  length is 1.3 times the map's usual one or under, where it may count
  another pulse, and the bars with it.
- **The bars, by the drums.** With the drums part, its kicks and snares are
  heard as `daw transcribe --drums` hears them and placed on the map's bars by
  eighths. The song's `cycle` is the beats they fall on in most bars: each
  bar is turned to where it best matches a count of all bars and the count
  made again from the bars so turned, twice, so bars a beat away do not muddle
  it. `bars_off` are runs of two bars or more that match it better turned a
  whole number of beats, as if each started earlier or later.
- `warnings` say what to correct: a meter the beat's division disagrees
  with, a doubled beat that divides in three, a pulse that changes, a form
  read mostly on such bars, and bars the drums place elsewhere.

## Report

`path`, `beat_map`, `tempo_bpm`, `meter`, `first_downbeat_seconds`,
`bar_count`, `key`, `key_confidence`, `key_sure`, `parts`, `silent_parts`
when any, `named_by`, `form` (the labels in order), `sections` (each with
`label`, `name`, `bars`, `count`, `seconds`, `until_seconds`, `key`,
`key_confidence`, `key_sure`, `level_db`, `starts_by`, `change`, `sung` and
`drums` with the parts, and `like` with `section`, `how`, `semitones`,
`chords` and `sound` for a letter heard before), `key_changes` (`section`,
`bar`, `seconds`, `semitones`, `from`, `to`, `heard_as`), `checks` and
`warnings`; `picture` with `--picture`. `daw describe beats.form` is the
contract.

## What was checked

On the day it was built, with nobody listening, against the forms the records
are known to have:

- **A generated song** of a drums-only intro, a verse, a louder chorus, the
  verse and chorus again and the chorus a tone up: its sections within a bar,
  the verses one letter and the choruses another, the last chorus the chorus
  moved +2, one key change there; with a voice part and a drums part beside
  it as `daw separate` leaves them, the intro, verse and chorus named by the
  voice. Hats on the thirds of the beat read as 12/8. A hand-made map a beat
  late for four bars had those bars found by the drums.
- **A rock record with a famous key change**, separated with
  `--split stems`: intro, verse, pre-chorus and chorus twice, a solo and
  bridge, the pre-chorus, and the last choruses heard as the chorus moved +3
  semitones at bar 99, G major to B♭ major, as the record does.
- **A dance-pop record** with its last choruses a tone up: intro, chorus hook,
  verse, pre-chorus and chorus twice, a bridge, and the key change +2 at bar
  95. Its second chorus matched its first only a bar later, which moved its
  start there.
- **A rock record** whose second verse repeats the first: verse, pre-chorus
  and chorus twice, the same letters, the voice coming in at bar 4.
- **A pop record built on one bass figure,** whose sections differ more in
  sound than in chords: intro, verse in two halves, a pre-chorus, a chorus and
  a post-chorus, each time the same letters. Its pre-chorus is louder than its
  chorus; the chorus was named as what it always leads into.
- **A solo piano in 3/4:** the map's 3/4 held, the tune's halves came out
  A B A B A C, and its silent voice and drums parts were left out of the
  names.
- **A slow 6/8 anthem:** the map was wrong. The model counted eighths in one
  section, quarters in another and dotted quarters in the rest, and the map
  doubled its 65 BPM to 134 in 2/4. The form said so: its beats divide in
  three, so 6/8; the doubling is wrong; the pulse changes over two runs of
  bars; and its sections are as unreliable as those bars. The map itself is a
  Later line.
- **A live-played rock edit:** the drums' cycle kick on one and three, snare
  on two and four, and no bars off.

A song with its parts takes 10 to 35 s, most of it the drums check; one
without, 3 to 15 s.

Nobody has heard a section played against its label.

## Limits

- The names are guesses from where the voice and drums are and how loud a
  section is; the words, a later command, will say which section is the
  chorus more surely, by its lines repeating.
- Sections start on bar lines the map has; a map whose bars are wrong gives
  sections as wrong, which the checks warn of. A boundary can be a bar off,
  and a long verse can come out in two or more sections of their own letters.
- A two-beat shift of the bars is invisible to the drums check in a backbeat
  that repeats every two beats; the map's odd bars still list it.
- A section's key from a full mix is often not sure; a key change shows most
  surely as music that comes back moved.
- The meter check names the beat's division, not a time signature the map
  should take; a beat map that counts different pulses in different places is
  not corrected here.
