# The equalizer's output gain — proposed November 26, 2026

Status: proposed, Next item 3 of the backlog (D206).

## What

The `eq` effect takes an `output_db`: a gain after its bands, from −24 to 24
dB and 0 unless given, so a cut or a boost is evened out in the same device,
as EQ Eight's Gain knob does.

```yaml
effects:
  - type: eq
    id: tone
    output_db: -2
    bands:
      - {shape: low_shelf, freq_hz: 120, gain_db: 3}
```

## Why

The person's note of November 26, 2026: "The EQ should have a gain on it so
you dont need a separate utility". Today a boost that makes a track louder is
turned back down by a `utility` after the equalizer: two devices for one move.

## Design

- **`output_db`**, the name the saturation and the overdrive give the same
  gain after their curve; `gain_db` stays the band's. Left out of the file at
  0, so every song with an `eq` saves and renders as before, bit for bit.
- **Automated** as `effects.REF.output_db`, as the bands' fields are.
- **In the app** a field with a knob bar in the equalizer's panel, beside its
  curve, typed or dragged as the panel's other fields are, with its lane mark.
  The curve is drawn with the gain added, so what is drawn is what plays.
- **The agent** sets it with `daw set` and `daw effect add --output-db`; `daw
  describe effects` says it.
- **Inside a Synth's patch** an `eq` takes it too, as it takes the bands.

## Done when

A render with `output_db: -6` is the render without it 6 dB lower; a song
without the field saves and renders as before; the golden and the
legacy-fields tests hold; the panel shows the field in a scripted picture.
