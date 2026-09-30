# Code review: parameter automation (PR #4), 2026-09-29

Scope: the merged automation PR, `5dd2e6c^1..5dd2e6c`: `automation.py` (new),
`effects.py`, `engine.py`, `model.py`, `cli.py`, docs and tests. The reviewer
confirmed findings 1–4 with probe scripts on generated audio, and finding 1 also on
local renders.

Line numbers in each heading are from the reviewed commit `5dd2e6c`. All 14
findings are now fixed. Of the other notes, the earliest-report crash is fixed in PR
#11, the local test runner has been repaired, and formatting needs no change. A
final pass over the fixes found one more latent issue, fixed in PR #12 (see Final
pass).

## Status

| # | Finding | Kind | Status |
|---|---|---|---|
| 1 | Project hash includes defaults; old renders fail verification | correctness | Fixed in PR #5 |
| 2 | Filter/EQ coefficient jumps burst | correctness | Fixed in PR #6 |
| 3 | Envelope evaluation uses gigabytes on long sessions | performance | Fixed in PR #7 |
| 4 | Filter update grid offset by upstream latency | correctness | Fixed in PR #6 |
| 5 | "makeup_db is static" in describe text and docs | docs | Fixed in PR #6 |
| 6 | "master gain is static" in mvp.md | docs | Fixed in PR #6 |
| 7 | Fade description backwards in describe text | docs | Fixed in PR #6 |
| 8 | `wav_atomic` uses soundfile private internals | robustness | Fixed in PR #8 |
| 9 | `Chain.run` never resets `self.n` | latent bug | Fixed in PR #9 |
| 10 | `bounds()` treats `gt`/`lt` as inclusive | latent bug | Fixed in PR #10 |
| 11 | `ids` shadowed in automation validation | latent bug | Fixed in PR #10 |
| 12 | Scalar-or-column branching repeated | cleanup | Fixed in PR #7 |
| 13 | Envelope lookup and latency offset duplicated | cleanup | Fixed in PR #9 (started in PR #6) |
| 14 | One-point lanes take the full automated path | cleanup/perf | Fixed in PR #7 |

PR #5 is commit `79c2122`, PR #6 is commit `ece667d`, PR #7 is commit `ea0c135`
(merged as `abb4cd2`), PR #8 is commit `5a68cb3` (merged as `41bfd37`), PR #9 is
commit `8720c26` (merged as `78042c4`), PR #10 is commit `d9ea91b` (merged as
`3e0f256`), PR #11 is commit `9038d1a` (merged as `cd3ed0a`), and PR #12 is commit
`491e18d` (merged as `58c8918`). Every finding and note is closed.

| Note | Status |
|---|---|
| Test runner points at the old `code/DAW` path | Fixed locally (`.venv` rebuilt); no repo change |
| Earliest render reports raise `KeyError: 'target'` | Fixed in PR #11 |
| Formatting | Checked; no change warranted |
| Final pass: legacy fingerprints break at the next schema change | Fixed in PR #12 |

## Fixed

### 1. Project hash includes defaults (`model.py:690`)

**Problem.** `project_hash` hashed `model_dump()` including default values. The
automation fields (`automation: []` on tracks, returns and master; effect
`id: None`) therefore changed every project's hash. Nothing detected this;
`engine_version` stayed 0.1.0.

**Impact.** `perception.context()` recomputes the hash of `song.snapshot.yaml` and
compares it with the report, so `daw listen` and `daw compare` failed with "Render
snapshot hash mismatch" on older renders. Saved `apply --expect` SHAs also went
stale. The problem predates PR #4: the effects and sends PRs had already broken
renders made before them. Only 24 of 50 local renders verified.

**Fix (PR #5).**
- `project_hash` hashes the saved form (`exclude_none`, `exclude_defaults`), the
  same as `save()` writes.
- `hash_matches()` also accepts the full-dump forms earlier engines wrote, one per
  schema change, listed in `model.LEGACY_FIELDS`.
- `docs/perception.md` and decision D30 record the rule.
- 49 of 50 local renders now verify. The remaining one is a very early report with
  no `target` field (see Other notes).
- Every project's `project_sha256` changed once, so saved `apply --expect` SHAs
  need a fresh `daw inspect`. Render IDs changed too.

**Tests.** `test_project_hash_fingerprints_the_saved_form` and
`test_reports_from_earlier_engines_still_verify`. Both fail on the old code.

### 2. Filter/EQ coefficient jumps burst (`effects.py:219`)

**Problem.** `AutomatedSos` carried transposed direct-form biquad state (`zi`)
across abrupt coefficient changes.

**Impact.** A 24 dB lowpass hold step from 18 kHz to 300 Hz, on noise peaking at
1.51, peaked at about 9.2. Jumps between 20 kHz and 10 Hz reached about 118, and an
EQ freq/q jump reached 392. The result was a loud pop, or a render aborted with
"Unsafe PCM export". The review said short ramps stayed clean. They did not: see
the table below.

**Fix (PR #6).**
- Each automated section is a trapezoidal state-variable filter (Simper's "Linear
  Trap SVF" formulas: Butterworth sections, RBJ-equivalent bells and shelves).
- Between updates a section runs as its equivalent direct form II biquad through
  `lfilter`.
- On a coefficient change, the direct form's history `(w[n-1], w[n-2])` is
  converted through `T` / `T⁻¹` so the SVF integrator state carries over.
  `sosfilt` was replaced by `lfilter` because its per-call overhead was 4× higher.
- Decision D31 records the choice.

Worst peak over 12 noise seeds, as a multiple of the input peak:

| Case | Before | After |
|---|---|---|
| lowpass 48 dB, 20 kHz → 10 Hz jump | 219 | 0.97 |
| lowpass 48 dB, 20 kHz → 10 Hz, 1/64-beat ramp | 11.9 | 0.97 |
| lowpass 48 dB, 18 kHz → 300 Hz jump | 6.9 | 0.90 |
| highpass 48 dB, 20 kHz → 10 Hz jump | 103 | 2.82 |
| highpass 48 dB, 20 kHz → 10 Hz, 1/64-beat ramp | 4.2 | 1.46 |
| highpass 48 dB, 18 kHz → 300 Hz jump | 5.7 | 1.72 |
| EQ bell freq + q jumps | 16.3 | 2.61 |
| upward jumps (10 Hz → 20 kHz) | ≈1 | ≈1 |

The remaining 2.8× case is a highpass dropping far in one step. It gives a low
thump that decays over tens of milliseconds, as an analog filter would; the docs
and `daw describe automation` say so. Constant lanes still match static filters
within 1e-9, for all filter modes and slopes and all EQ shapes.

**Cost trade-off.** Continuous sweep, per minute of audio, on an x86 Mac:

| Filter | Before | After |
|---|---|---|
| 12 dB filter | 1.3 s | 1.5 s |
| one EQ band | 1.5 s | 1.5 s |
| 48 dB filter | 1.4 s | 3.8 s |

Chaining each section's FIR numerator into the next section's `lfilter` might save
another 20%. This was not attempted.

**Tests.** `test_filter_cutoff_jump_does_not_burst` (six cases),
`test_eq_frequency_and_q_jumps_do_not_burst`.

### 3. Envelope evaluation memory (`automation.py:66`)

**Problem.** `Automation.value` evaluated each lane with
`Envelope.at(np.arange(total))`, which created about 10 full-length int64/float64
temporaries.

**Impact.** One 2-point gain lane on a 15-minute, 48 kHz session peaked at 3.2 GB,
and every gain, pan and send lane repeated that, so long sessions could run out of
memory.

**Fix (PR #7).**
- `Envelope.span(start, count)` fills each segment in place in one float64 output
  array. Hold segments are assigned. Linear segments are transformed in place with
  the same arithmetic as `at()`, so both give bitwise-equal values.
- Only segments that overlap the span are visited, so per-block reads of lanes
  with many points stay cheap.
- `Automation.value` and `Device.param` use `span`. `AutomatedSos` still uses `at`
  for its sparse 64-frame ticks.
- Decision D32 records the change together with finding 14.

The same lane, 15 minutes at 48 kHz, on an Apple M2:

| | Peak | Time |
|---|---|---|
| Before | 3.20 GB | 2.87 s |
| After | 0.35 GB | 0.11 s |

0.35 GB is the result array itself. In a render, `amplitude()` on a moving gain
lane adds two short-lived arrays of the same size, so the peak per lane is about
1 GB.

**Tests.** `test_span_matches_values_at_each_frame` (three lanes, four spans) and
`test_lane_values_take_one_array_over_the_timeline`, which bounds the peak at 1.1×
the result with `tracemalloc`. The memory test fails on the old code.

### 4. Filter update grid offset by upstream latency (`effects.py:210`)

**Problem.** `AutomatedSos` aligned its 64-frame control grid to chain-input frames
rather than timeline frames. After upstream latency the grid was offset by that
latency, which contradicted the docs.

**Impact.** A cutoff step on beat 1 (frame 24000) after a 3 ms limiter changed the
output at frame 24048.

**Fix (PR #6).** Ticks are computed on timeline frames
(`start = self.frame - self.upstream`), and envelopes are evaluated at those ticks
directly. The step now lands on frame 24000.

**Test.** `test_filter_updates_align_to_the_timeline_after_latency`.

### 5–7. Docs contradicting the automation PR

- **5** (`cli.py:134`, `docs/effects.md:75`): "makeup_db is static". It now says
  `threshold_db` and `makeup_db` can be automated.
- **6** (`docs/mvp.md:100`): "master gain is static". It now says a master
  `gain_db` lane replaces `session.master_gain_db`.
- **7** (`cli.py:157`): said a linear-in-dB fade "drops quickly at the end". It now
  says the fade is already at −48 dB halfway and most of it is inaudible, which
  agrees with `docs/automation.md`.

PR #6 also updated the `timing` and `clicks` describe texts and
`docs/automation.md` for fixes 2 and 4.

### 8. `wav_atomic` uses soundfile private internals (`engine.py:250`)

**Problem.** `wav_atomic` suppressed the timestamped PEAK chunk of float stems with
`sf._snd.sf_command(f._file, SFC_SET_ADD_PEAK_CHUNK, sf._ffi.NULL, 0)` and ignored
the return value.

**Impact.** A soundfile release that renamed those internals would fail every
render. If libsndfile had ignored the command, the timestamp would have come back
silently and stem hashes would have varied between renders.

**Fix (PR #8).**
- Float stems are written by `float_wav()` in plain Python: a fixed header, then
  the samples as little-endian float32 in 65536-frame blocks. No libsndfile call
  touches them.
- The header copies what libsndfile wrote with PEAK off, including a zeroed
  `PAD ` chunk where PEAK would go. Stem bytes and hashes are unchanged from
  earlier renders.
- The PCM_24 mix still goes through `sf.write`. It has no PEAK chunk, and
  libsndfile's float-to-integer conversion is not worth reproducing.
- Checked byte-identical to the old writer for noise, large, subnormal, negative
  zero, empty, 1–3 channel, Fortran-ordered, sliced, float32 and exactly
  block-sized inputs. A 5-minute stereo stem writes in 0.16 s rather than 0.25 s.

**Test.** `test_float_stem_bytes_match_earlier_renders` pins the SHA-256 the old
writer produced for audio spanning several write blocks and reads the file back.
It and `test_float_stems_have_no_timestamped_peak_chunk` both fail when a plain
libsndfile float write is substituted.

### 9. `Chain.run` never resets the timeline counter (`effects.py:493`)

**Problem.** `run` reset each device's `window` and `position` but not the
chain's frame counter `self.n`.

**Impact.** Latent: every caller builds a fresh chain per render. A second `run()`
on the same chain would have read automation at frames shifted by the first run's
length plus latency. Devices would also have kept their filter, detector and
delay/reverb tail state, so resetting `self.n` alone would not have been enough.

**Fix (PR #9).**
- A chain runs one timeline. `run` raises `RuntimeError` once the chain has
  processed frames, and the docstring says so.
- `run` no longer resets `position`, which only half-reset the chain.

**Test.** `test_chain_runs_one_timeline`, which fails on the old code.

### 10. `bounds()` treats exclusive constraints as inclusive (`model.py:311`)

**Problem.** `bounds()` read `gt` as if it were `ge` and `lt` as if it were `le`.

**Impact.** Latent: no automatable field has an exclusive limit. If one became
automatable (for example `velocity_scale`, `gt=0`), a lane value equal to the limit
would pass validation while the static field rejects it. A log-domain lane at 0
would give `log(0) = -inf`.

**Fix (PR #10).**
- `bounds()` returns a `Range` (`low`, `high`, `low_open`, `high_open`), and
  `value in range` uses `>`/`<` at open ends. `Target` carries it in place of
  `low`/`high`.
- When a field has several limits on one side, the tighter one wins. At equal
  values the exclusive one wins, whatever order pydantic lists them in (it gives
  `Gt` before `Ge`).
- Messages for closed ranges are unchanged (`outside -1 to 1`). An open end is
  labelled: `outside 0 (exclusive) to 2`.
- `docs/automation.md` now says lane values have the limits the static parameter
  accepts.

**Tests.**
- `test_lane_limits_match_the_static_field` checks that a value is in range
  exactly when pydantic accepts it for the static field. It checks each limit and
  the next float on either side. It covers every automatable field,
  `Clip.velocity_scale`, and a test model with `gt`/`lt` limits and with `ge`/`gt`
  at the same value.
- `test_exclusive_limits_are_named_in_errors` checks the message format.
- With the old logic swapped in, the three exclusive cases and the message test
  fail. The 15 inclusive cases pass either way.

### 11. `ids` shadowed in automation validation (`model.py:569`)

**Problem.** In `references()`, `ids` held the track IDs used for sidechain
checks, then was reassigned to each chain's effect IDs. `t` was reassigned from a
track to a lane `Target`.

**Impact.** Latent. A later check using `ids` as track IDs would silently have
compared against the last chain's effect IDs.

**Fix (PR #10).** The names are now `track_ids` (next to `return_ids`),
`effect_ids` and `lane_target`. Behavior is unchanged, so no new test was added.
The existing validation tests pass.

### 12. Scalar-or-column branching repeated (`engine.py:366`)

**Problem.** The same `np.ndim` special case appeared four times: `amplitude`
added a column, `engine.py` stripped it again with `master[:, 0]`, and
`Device.fraction` and `DelayDevice.process` each branched.

**Fix (PR #7).**
- Automation values are a scalar or a 1-D array.
- A new `automation.per_frame()` shapes either one to scale stereo frames
  (`np.reshape(v, (-1, 1))`), and the use sites call it.
- The delay broadcasts feedback to the block length once and slices it per chunk.
- No `np.ndim` checks remain.

**Tests.** The existing render, chain and block-invariance tests. Static paths
give the same output as before.

### 13. Envelope lookup and latency offset duplicated (`effects.py:199`)

**Problem.** `Device.param` and `AutomatedSos.process` each subtracted
`self.upstream` to map chain input frames to timeline frames. The automated
filter's `values()` closure also had its own static fallback (`np.full`) alongside
`param`'s `getattr`. PR #6 removed `AutomatedSos.values` but left the rest.

**Fix (PR #9).**
- **Offset.** `Chain.process` sets each device's `frame` to the timeline frame
  (`self.n - d.upstream`). Devices no longer see chain input frames or
  `upstream`, and the reverb's per-partition start is a timeline frame.
- **Lookup.** `Device.param` is the one place that returns envelope values or the
  static value.
  - The automated filter takes the span from the update at or before `x[0]` and
    keeps every `CONTROL`-th value. A static band parameter is broadcast.
  - `svf_params` now takes `values(name)`.
- **Static values.** `spec_value()` resolves a static value by name, including
  `bands.N.FIELD`, the counterpart of `with_values()`.
- **Docs.** D29 now says the chain applies the offset.
- **Result.** Output is bit-identical to the previous code. The check covered four
  chains with automated devices after latency (a filter after a reverb, a partly
  automated EQ, a compressor, and delay plus reverb after a limiter) at block sizes
  127, 4096 and 5 s.
- **Cost.** A 4-band EQ with 12 moving lanes costs 1–2% more (3.76 s → 3.81–3.84 s
  per minute), because each lane is evaluated over the block rather than at its
  64-frame ticks. A 48 dB sweep and a one-lane EQ are unchanged.
- `Envelope.at` is no longer called in the package. It stays as the pointwise
  definition that `test_span_matches_values_at_each_frame` checks `span` against.

**Test.** `test_reverb_mix_after_latency_lands_on_the_timeline` guards the reverb's
lookup. With the chain's `- d.upstream` removed, it and the two existing latency
tests fail.

### 14. One-point lanes take the full automated path (`effects.py:447`)

**Problem.** A constant lane still built full-timeline arrays for gain, pan and
send, and sent filters and EQs through `AutomatedSos`.

**Impact.** The arrays wasted memory (see 3), and filter and EQ output matched the
static path only within 1e-9.

**Fix (PR #7).**
- `Envelope.constant` is the lane's value when all its points share one value,
  for any number of points, and `None` otherwise. It keeps the value as written
  rather than `exp(log(v))`.
- `Automation.value` returns that scalar for channel and send lanes.
- `device()` folds constant lanes into the spec with `with_values()`, including
  `bands.N.FIELD`. Only moving lanes reach the device, and `AutomatedSos` is used
  only when a filter or EQ lane moves.
- `Device.automated` lists every lane, so reports still show constant lanes.
- Projects whose lanes are all constant render bit-identically to their static
  form. A re-render of a song with a one-point filter or EQ lane can differ from
  its earlier render by up to about 1e-9.

**Tests.**
- `test_lanes_sharing_one_value_are_constant`.
- The constant filter and EQ tests now require exact equality, which fails on the
  old code. They still check the state-variable path against static filters and
  all three EQ shapes within 1e-9 by running `AutomatedSos` directly.
- `test_constant_lanes_on_other_effects_match_static_values` covers compressor,
  delay and reverb.
- The constant gain lane test compares mix and stem hashes.
- The chain report test checks that a constant lane is still listed.
- `test_filter_updates_align_to_the_timeline_after_latency` now compares against a
  lane that steps after the tone ends, because a constant reference no longer
  takes the state-variable path.

The gain, compressor, delay and reverb equality tests also pass on the old code:
per-frame values were already bitwise equal to the scalar path there.

## Other notes

### Test runner (fixed locally)

**Problem.** `uv run pytest` failed because the `.venv` scripts still pointed at
the old `code/DAW` path. This affected `pytest`, `py.test`, the `activate` scripts
and several dependency scripts, but not `daw`.

**Fix.** Local only, since `.venv` is not in the repository. `.venv` was deleted
and rebuilt with `uv sync --extra dev`. Plain `uv sync` would leave out pytest,
which is in the `dev` extra. `uv run pytest -q` works again, as AGENTS.md says.

**Test counts.**

| Commit or branch | Passed |
|---|---|
| `b3d9463` | 159 |
| `ea0c135` | 165 |
| `claude/wav-peak-chunk` | 166 |
| `claude/chain-timeline-frames` | 168 |
| `claude/automation-validation-bounds` | 187 |
| `claude/legacy-render-reports` | 188 |
| `claude/legacy-hash-guard` | 189 |

### Earliest render reports (fixed in PR #11)

**Problem.** The earliest render reports have no `target` key and no `render_id`.

**Impact.** One local render was affected.
- `perception.context()` raised `KeyError: 'target'` after the hash check, so
  `daw listen` and `daw compare` printed `{"error": "'target'"}`.
- Passing such a `report.json` directly failed earlier, in `resolve()`, with
  "Expected a render report or render pointer JSON", because a report was
  recognized by `render_id`.

**Fix (PR #11).**
- A missing `target` means a full render. Those reports predate previews. The
  existing timeline check still rejects a preview with a missing `target`,
  because its length differs from the session.
- `source.render_id` is `null` for such reports.
- `resolve()` recognizes a report by `audio_sha256`, which every report has.
- `docs/perception.md` describes this next to the note on older reports without
  stem hashes.
- The affected local render (30 s, five sections, 10 stems) now analyzes both
  from its folder and from its `report.json`. Its stems are reported as
  `hash_verified_against_render: false`.
- All 50 local renders now pass `analyze()`, compared with 49 of 50 after PR #5.

**Test.** `test_reports_from_before_previews_are_full_renders`:
- It cuts a full render's report down to the earliest key set and checks that it
  analyzes through its `report.json` path.
- It checks that the mix, timeline, sections and musical context match the
  original analysis.
- It checks that a preview cut down the same way is rejected.
- It fails on the old code.

### Formatting (no change)

**Finding.** The note said the repo is formatted with Black < 24. That is only
roughly true:
- Black < 24 would reformat `cli.py`, `library.py` and `perception.py`.
- Black 26 would reformat `analysis.py`, `cli.py` and `perception.py`.
- `perception.py` mixes the two styles. Its parenthesized long strings are Black
  24+ style, and its unparenthesized conditional expressions are Black < 24 style,
  so each version rewrites part of it.
- There is no formatter configuration in `pyproject.toml` and no CI.

**Decision.** No change. A repo-wide reformat would churn unrelated code and still
has no canonical target. Check only the lines a change touches, with either
version (`uv run --with 'black<24' black --diff FILE`). PR #11's changed lines are
clean under both versions.

## Final pass

Scope: the fixes, `5dd2e6c..cd3ed0a` (PRs #5–#11), on `master` at `cd3ed0a`.
Line numbers in this section are from `cd3ed0a`.

### Checks

- `uv run pytest -q`: 188 passed.
- `Envelope.span` equals `at()` bitwise on about 15,000 random spans: lanes of one
  to six points, same-position jumps, holds, both domains, starts before zero and
  empty spans.
- A chain of a look-ahead limiter, an automated filter and a three-band EQ with
  two automated bands gives identical output for random blocks of 1–3000 frames
  and for one block. This held for 12 chains with random filter modes, slopes and
  cutoff lanes.
- 300 random cutoff jumps on noise, both modes and all slopes: the worst peak was
  2.0× the input peak (12 dB highpass, 17.4 kHz → 28 Hz), within the documented
  2.8×.
- Every test this doc names exists. `Envelope.at` and `np.ndim` are no longer
  used in the package, and only `Chain` reads `upstream`.
- The lines PRs #5–#11 changed are clean under Black < 24 and current Black.

### Legacy fingerprints break at the next schema change (`model.py:749`)

**Problem.** `hash_matches` rebuilds the earlier forms from today's full dump,
less the fields in `LEGACY_FIELDS`. Its first step assumes that today's full dump
is what the PR #4 engine hashed. That holds only until a field is added.

**Impact.** Latent: no field has been added since PR #4.
- With `Track.width: float = 1.0` added in a scratch copy, the saved-form hash
  was unchanged, but a full-dump fingerprint no longer verified. Every render from
  before PR #5 would fail `daw listen` and `daw compare` again.
- `test_reports_from_earlier_engines_still_verify` still passed, because it
  derives its forms from the current dump, as the code does.

**Fix (PR #12).**
- A comment at `LEGACY_FIELDS` says a new field needs a new first entry, and that
  keys match by name anywhere in the dump. D30 says the same.
- `test_fingerprints_of_earlier_engines_still_verify` pins the fingerprints that
  the engines at `5dd2e6c`, `695ca77`, `74c074a` and `f71aa1d` wrote. Each is
  for a project using every model of that engine's schema, and was computed with
  that engine. The models forbid extra fields, so each engine also confirmed that
  its project fits its schema.

**Test.** The new test fails with an unlisted field added to `Track` or to
`Reverb`, and passes once the field is listed. It also fails when a new field
with a common name is listed (`Master.gain_db` as `{"gain_db": 0}`), because
matching by name then strips the tracks' `gain_db` too.

### Docs

Uneven wrapping that these PRs left in `docs/automation.md` is reflowed. The
wording is unchanged.
