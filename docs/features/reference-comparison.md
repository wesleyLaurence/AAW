# Comparing with a reference — proposed October 4, 2026

Status: proposed, not built. Backlog: Next, Comparing with a reference. What
it is for is in [concept.md](../concept.md#library-and-perception): tracks the
person points to as "this is what good sounds like", analyzed once and compared
with every mix.

## What

A song the person names as a reference is analyzed once and kept in the
workspace's library. A render is then compared with it section by section:
loudness, low end, brightness, width, how much louder the chorus is than the
verse. The report is observations an agent can act on: "the sub is 5 dB heavier
than the reference in the drop", "the verse and the drop are within 1 LU of each
other; the reference's drop is 4 LU louder".

```sh
daw reference add ~/Music/somebody/track.m4a --name club-ref
daw reference sections club-ref                  # the sections it found
daw compare projects/demo/renders/latest.json --reference club-ref
```

## Why

An agent does not hear its mix. `daw listen` measures it, but numbers alone
do not say whether a mix is right: −18 LUFS with 61% of its power in the low band
is a mistake in one genre and the goal in another. A reference turns the
person's taste into targets the agent can measure against and work toward,
section by section, without guessing what "good" means.

On October 4, 2026, an agent's first trial song had its tracks 27 LU apart. The
agent found this from `daw listen`, but had nothing to tell it how far apart they
should be or what the low end should weigh.

## Design

**Adding a reference.** `daw reference add FILE --name NAME` decodes the file
(`.m4a` and `.mp3` as import does), makes its beat map (`daw samples beats`),
and analyzes it as `daw listen` analyzes a standalone file, also per section. It
keeps the result in `library/references/NAME/` in the workspace: the decoded
audio's hash and the original's path, not a copy of the audio. A reference is the
person's own file and never leaves the Mac.

**Sections of a reference.** They come from the beat map's phrases, with each
phrase's relative energy, labelled `low`, `mid` and `high` by loudness.
`daw reference sections NAME '[{"id": "verse", "at": 32}, ...]'` names them by
hand, in the reference's own beats, for when the phrases are not where a person
would put them.

**Comparing.** `daw compare RENDER --reference NAME` matches the song's sections
to the reference's by ID when both have the same IDs, otherwise by role: the
song's loudest section against the reference's loudest, and so on. With no
sections on either side it compares the whole files. For each pair it reports,
after matching loudness as `compare` does now:

- the difference in each band's share (`band_fraction`), sub to air;
- crest factor, as a measure of how dynamic or squashed the mix is;
- stereo correlation and side energy;
- the integrated loudness and true peak of the whole file, against the
  reference's;
- the contour: how much louder or quieter each section is than the song's
  average, against the reference's.

It ends with `observations`, a few lines of the largest differences in words,
measured and not judged: which way and by how much, not "fix this".

**What it does not do.** It does not compare tracks, since a reference has no
stems. Stem separation of a reference is a separate Later line. It does not
compare keys, notes or arrangement. It is not a judge of quality: a mix can
differ from its reference on purpose.

## Done when

- A generated reference and a mix made 6 dB heavier in the sub report that, by
  section, with the band and the amount.
- Sections are matched by ID, and otherwise by loudness, and a song with no
  sections is compared as a whole.
- A three-minute `.m4a` reference is added in under ten seconds and compared in
  under five, measured on this Mac.
- Nothing is written outside the workspace and the render's analysis folder.
  `daw describe` says what each field means.

## Open questions

- The bands compared: `daw listen`'s seven until
  [a finer spectrum](spectrum-detail.md) is built, then third octaves and the
  tilt.
- Whether a reference should keep a copy of its decoded audio, so that it
  survives the original being moved, at the cost of space.
- Several references for one song, averaged or reported side by side.
- A reference attached to a project in `SONG.md`, so that every `compare` uses
  it without being told.
- A clip of the reference that the person can play next to the mix, at matched
  loudness, which is what a person mixing to a reference does.
