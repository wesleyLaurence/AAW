# The equalizer's output gain — implemented November 26, 2026

The `eq` effect takes an `output_db`, a gain after its bands. It runs from −24
to 24 dB and is 0 unless given, so a cut or a boost is evened out in the same
device, as EQ Eight's Gain knob does.

```yaml
effects:
  - type: eq
    id: tone
    output_db: -2
    bands:
      - {shape: low_shelf, freq_hz: 120, gain_db: 3}
```

It came from the person's note of November 26, 2026: "The EQ should have a
gain on it so you dont need a separate utility". Before it, a boost that made
a track louder was turned back down by a `utility` after the equalizer: two
devices for one move.

## What it does

- **The field** is `output_db`, the name the saturation and the overdrive give
  the same gain after their curve; `gain_db` stays the band's. The file leaves
  it out at 0, written before the bands when it is set.
- **The sound:** after the bands, each frame is multiplied by the gain. At
  0 dB the multiplication is skipped, so an equalizer without the field
  renders exactly as before. A live edit glides to the new gain, and the
  bands' filters keep their state.
- **Automation** moves it as `effects.REF.output_db`, a linear lane from −24
  to 24. `daw describe automation` lists an equalizer's lane targets as
  `output_db`, `bands.N.freq_hz`, `bands.N.gain_db` and `bands.N.q`, and so
  does `daw lane set --help`.
- **The agent** sets it with `daw set PROJECT OWNER.effects.REF.output_db DB`
  or `daw effect add --type eq --output-db DB`; `daw describe effects` says
  what it does.
- **Inside a Synth's patch** an `eq` takes it too, under
  `instrument.effects.REF.output_db`.
- **In the app** the equalizer's panel, now 440 points wide, has an Output
  field labelled Out, with its knob bar and its lane mark, at the right of
  the row under the curve. The curve has the gain added, so what is drawn
  is what plays. The bands' points stay at their own gains, where a drag
  sets them. A Synth's patch lists Output after its bands.

## Decisions

- **The eq's own fields and its bands' fields are separate tables.**
  `rules::effect_params("eq")` is now the equalizer's own `output_db`, as every
  other kind's are its own, and `rules::band_params()` lists the bands'
  fields. Keeping `output_db` in the band table was rejected: it would have
  accepted `bands.N.output_db` and listed it for every band.
- **The gain sits on the chain's unit, not in a new device.** Wrapping the
  equalizer's filters in a new `Device` variant was rejected, because it
  would have changed the filters' matching on take-over. `Unit` holds an
  optional `Trim`, set for every equalizer, applied after the device and
  before the spectrum tap, so the app's spectrum is the gained output. It is
  always present on an equalizer, so 0 to −6 is a glide, not a new signature
  that would reset the filters.
- **The points are not moved by the gain.** Drawing each point at its band's
  gain plus the output was rejected: a drag reads the band's gain straight
  from the point's height.
