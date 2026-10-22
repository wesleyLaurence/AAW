# Favorites — implemented October 21, 2026

Status: implemented (D141). The first of the person's notes of October 19,
2026 still open after MIDI effects (D123): "a section of the browser anything
is put in, a sample, a patch, a rack, an effect kind or a device, kept with
the person's library and listed first."

## What

Favorites is the first section of the browser. Anything the browser lists
goes in it: a sample, a Synth patch, an effect rack, an instrument rack, or a
device kind (Sampler, Synth, an audio effect kind, a MIDI effect kind). Each
is put there from its row's menu, Add to Favorites, and taken out with
Remove from Favorites; the agent does the same with `daw favorite add|remove`
and reads them with `daw favorite list`. A favorite is kept with the person's
library, in one file for every project, and listed in the order it was added.

## Why

A person who has found the kick they like, the pad they reach for and the
glue they put on every drum bus should not search for them again in every
song. Ableton's Collections do this with colors; here one list is enough,
and it is a text file the agent reads, so what the person keeps at hand is
also a hint about their taste.

## Design

**The file.** `~/Music/AAW/library/favorites.yaml` (or `library/favorites.yaml`
under `AAW_WORKSPACE`), beside `patches/`, `racks/` and `instruments/`:

```yaml
favorites:
- {kind: sample, path: /Users/me/Samples/Drums/kick 01.wav}
- {kind: patch, name: Soft Pad}
- {kind: rack, name: Drum Glue}
- {kind: instrument_rack, name: Warm Bass}
- {kind: device, type: compressor}
```

A sample is its file's full path, as the index has it; a patch, a rack and an
instrument rack are their names, matched by slug, so `soft-pad` and `Soft
Pad` are one favorite; a device is its kind. The list is the order added,
and nothing is in it twice. There is no file until the first favorite.

**Adding checks the library.** `daw favorite add KIND WHAT` refuses a patch,
a rack or an instrument rack the library does not have, naming the command
that lists them; a device kind that is none, listing the kinds; a sample
whose file is not there. The name written is the library's, so a patch added
as `soft pad` is the file's `Soft Pad`. One already there stays where it is
and the reply says `added: false`. `daw favorite remove KIND WHAT` takes one
out and leaves the patch, the rack or the file as it is; one not there is
refused. `KIND` is `sample`, `patch`, `rack`, `instrument-rack` or `device`.

**Listing says what is still there.** `daw favorite list` prints each
favorite with its `kind`, its `name` as the library has it now, its `path`,
`slug` or `type`, and `missing`: a patch, a rack or an instrument rack whose
file is gone, a sample whose file is gone, or a device kind the engine no
longer has. A missing favorite stays in the file until it is removed, since
a patch saved again under its name comes back.

**In the browser.** Favorites is the first section in the column, and the
one the browser opens on when it was the last shown. Its rows are grouped
under the headings of the other sections, Samples, Instruments, MIDI Effects
and Audio Effects, each drawn as its own section draws it, with the same +,
double-click, drag and audition: a sample plays when clicked and drags to
the timeline or a header as an audio clip or a Sampler; a patch, a rack and
an instrument rack drag by their type, so the device panel, the headers and
the agent panel take them as they take the originals. A favorite sample the
index no longer has, because its folder was removed from the library, is
drawn from its path, named by its file, with nothing measured; one whose file
is gone, and a patch, a rack or an instrument rack the library no longer
has, is gray with why, and offers only Remove from Favorites. The search
field narrows the favorites by name. The arrow keys do not walk the
favorites' samples; the Samples section has them.

**Add to Favorites** is in the menu of every row of every section: a
sample's, a device kind's, a patch's, a rack's and an instrument rack's,
reading Remove from Favorites when the row is one. The change is written to
the file at once and the section lists it; the window coming to the front
reads the file again, so a favorite added with `daw favorite add` in a
terminal shows.

**The index's rows.** The browser asks the index for the favorite samples'
rows by their paths, `daw samples search --path P` repeated, which the
Python search gained for it; a path the index does not have is left out of
the reply and the browser draws that favorite from its path.

**What it is not.** No colors, no several lists, no order but the one
added, no favorite of a clip, a section or a project: one list of what is
reached for. A favorite is not a copy: it names the thing where it is.

## Verification

- Rust: the file read and written in order without doubles, a bad file
  saying what is wrong, each kind of favorite added, checked against the
  library, refused with the way to find it, listed with what the library
  has now and missing when the rack's file is gone, removed by name or
  path, and the refusals (`engine/crates/aaw-host/tests/favorites.rs` and
  the module's own tests).
- Python: `samples search --path` keeping only the files named and
  leaving out one the index lacks.
- Swift: each kind's group, matching by path, kind or slug, the stand-in
  row for a sample the index lacks, and Favorites first among the sections.
- A scripted run of the app with a favorites file of a patch, a device and
  a sample, Favorites clicked, in a picture.

Nothing by hand: no row's menu opened, no favorite dragged, nothing heard.
