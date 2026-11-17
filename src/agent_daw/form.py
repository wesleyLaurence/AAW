"""The form of a song: which bars are the same music. `daw samples form`.

1. Each bar of the beat map is described twice, beat by beat: by its pitch
   classes, the tones and the bass apart, and by its level in 32 bands.
2. Every bar is compared with every other on each, and each comparison is
   turned into a rank among all the song's pairs, so a song that plays one
   chord throughout is told apart by its sound and a song of one sound by its
   chords. Their mean is the bar matrix.
3. Sections start where what follows a bar line is most unlike what came
   before it, over four bars and over eight (Foote's checkerboard), at least
   MIN_BARS apart, and where the voice comes in after a rest, when the song's
   parts from `daw separate` are beside it.
4. Each section is compared with the sections before it, bar by bar from their
   starts or their ends: one whose chords and sound both match an earlier one
   is that one again, its letter; one whose chords match and sound does not is
   it varied, A′; one whose chords match only when moved some semitones is it
   in another key, which is a key change. Any other is a new letter.
5. Names say what a section holds: with the parts, where the voice sings and
   the drums play; and without them, how loud it is and how often it comes.
   With the words `daw transcribe --words` kept beside the voice part, the
   chorus is the music that comes back with its words: a verse comes back
   with new ones. The words also start sections where the hook's lines begin
   and end where the music does not change near them, and the sections that
   sing the hook are one part, as a rap's hooks over the verses' beat.
6. Each section's key is the key its pitch classes are most like.
7. Checks: how each beat divides, in two or in three; whether the map's beat
   keeps one length; and with the drums part, whether the kick and the snare
   play each bar as the song's other bars, or as if it started a beat away.

Every value is an estimate from decoded audio; nothing here listens.
"""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np

from . import beats as beatmap
from . import harmony

MIN_BARS = 4  # a section is at least this long, but at the song's ends
EDGE_BARS = 2  # and at its ends at least this
NOVELTY_FLOOR = 0.2  # a bar line less unlike than this, of the song's most, starts no section
SAME_CHORDS = 0.75  # aligned bars' chords at least this, as ranks, are the same chords
SAME_SOUND = 0.72  # and their sound at least this is the same sound
VARIED_SOUND = 0.5  # chords the same and sound at least this is a variation
MOVED_CHORDS = 0.5  # chords moved some semitones this alike are moved
MOVED_GAIN = 0.2  # and this much more alike than unmoved
SUNG_DB = -20.0  # a bar the voice part is this near its loudest bars sings
PLAYED_DB = -20.0  # and the drums part plays
ABSENT_DB = -30.0  # a part whose loud bars are this far under the song's is silent
SUNG_SHARE = 0.5  # a section sings when this share of its bars do
REST_BARS = 2  # the voice coming in after this many bars' rest starts a section
FADE_DB = 9.0  # a last section falling this far over its bars fades out
KEPT_BARS = 8  # a key held to the end this long is a key change
SAME_WORDS = 0.5  # a letter whose sections' words are this alike comes back with its words
HOOK_COUNT = 3  # the hook is a line sung at least this often, the most often of all
HOOK_NEAR = 2  # and the lines that come back sung within this many bars of it
HOOKED = 0.5  # a section this much of whose bars sing the hook's lines holds the hook
LEAST_WORDS = 4  # a section of fewer words is not compared by them
EVEN_SPREAD = 1.3  # a beat this much longer in one place than another counts another pulse
NEAR = 0.06  # of a beat: an onset this near a place is on it
OFF_SHARE = 0.6  # the drums play a bar as if moved when this much better so
PROFILE_MOVED = 0.75  # whole sections' pitch classes this alike moved, and
PROFILE_GAIN = 0.4  # this much more than unmoved, with the same sound, are moved
LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


# --- the bars -------------------------------------------------------------------


def bar_spans(report) -> list[dict]:
    """The map's bars from bar 1: each one's first beat index, beats and seconds."""
    rows = report["beats"]
    times = [b["seconds"] for b in rows]
    period = 60 / report["tempo"]["bpm"]
    ends = [*times[1:], min(times[-1] + period, report.get("duration") or times[-1] + period)]
    bars = []
    for i, b in enumerate(rows):
        if b["bar"] < 1:
            continue
        if b["beat"] == 1 or not bars:
            bars.append({"bar": b["bar"], "first": i, "beats": 0, "seconds": times[i]})
        bars[-1]["beats"] += 1
        bars[-1]["until"] = ends[i]
    return bars


def per_beat(mono, sr, report):
    """Each beat's pitch classes, the tones' 12 and the bass's 12, and its level in
    32 bands in dB."""
    rows = report["beats"]
    times = np.array([b["seconds"] for b in rows])
    period = 60 / report["tempo"]["bpm"]
    ends = np.append(times, min(times[-1] + period, len(mono) / sr))
    tones, bass, fps = harmony.chroma(mono, sr)
    pitch = np.hstack([beatmap._per_beat(tones, fps, ends), beatmap._per_beat(bass, fps, ends)])
    power, _ = beatmap.band_power(mono, sr)
    level = beatmap._per_beat(power, sr / beatmap.HOP, ends)
    floor = max(float(power.max()), 1e-30) * 1e-9
    return pitch, 10 * np.log10(level + floor)


def _unit(rows):
    centered = rows - rows.mean(axis=-1, keepdims=True)
    norm = np.linalg.norm(centered, axis=-1, keepdims=True)
    return np.where(norm > 1e-9, centered / np.maximum(norm, 1e-9), 0.0)


def bar_features(bars, pitch, level, places):
    """Each bar as `places` steps through its beats, so a bar of another length is
    compared step for step: its pitch classes made unit, and its levels."""
    tones, sound = [], []
    for b in bars:
        index = b["first"] + np.floor(np.arange(places) * b["beats"] / places).astype(int)
        p = pitch[index]
        tones.append(np.hstack([_unit(p[:, :12]), _unit(p[:, 12:])]) / np.sqrt(2))
        sound.append(level[index])
    return np.array(tones), np.array(sound)


def moved(tones, shift):
    """Bars' pitch classes moved up `shift` semitones."""
    return np.concatenate(
        [np.roll(tones[..., :12], shift, axis=-1), np.roll(tones[..., 12:], shift, axis=-1)], axis=-1
    )


def chords_alike(tones, shift=0):
    """Bar by bar, how alike their pitch classes are, step by step (cosine, -1 to
    1), the second bar's moved up `shift` semitones."""
    other = moved(tones, shift) if shift else tones
    return np.einsum("ipk,jpk->ij", tones, other) / tones.shape[1]


def sound_apart(sound):
    """Bar by bar, how far apart their levels are, step by step and band by band (dB)."""
    n = len(sound)
    out = np.zeros((n, n))
    for i in range(n):
        out[i] = np.sqrt(np.mean((sound - sound[i]) ** 2, axis=(1, 2)))
    return out


def ranks(m, reference=None):
    """`m`'s values as their rank among `reference`'s off-diagonal ones (`m`'s own
    unless given), 0 to 1; a bar against itself 1."""
    ref = m if reference is None else reference
    off = ~np.eye(len(ref), dtype=bool)
    order = np.sort(ref[off])
    out = np.clip(np.searchsorted(order, m, side="left") / max(len(order) - 1, 1), 0, 1)
    if reference is None:
        out[~off] = 1.0
    return out


# --- sections ---------------------------------------------------------------------


def novelty(s, half):
    """Foote's checkerboard along the diagonal at each bar line: how unlike the
    `half` bars after it are to the `half` before, against how alike each side is."""
    n = len(s)
    g = np.exp(-0.5 * ((np.arange(2 * half) - half + 0.5) / (half * 0.6)) ** 2)
    sign = np.ones((2 * half, 2 * half))
    sign[:half, half:] = -1
    sign[half:, :half] = -1
    kernel = np.outer(g, g) * sign
    padded = np.pad(s, half, mode="edge")
    out = np.array([float((padded[i : i + 2 * half, i : i + 2 * half] * kernel).sum()) for i in range(n + 1)])
    out = np.maximum(out, 0)
    return out / max(out.max(), 1e-12)


def boundaries(curve, required=(), least=MIN_BARS, edge=EDGE_BARS, floor=NOVELTY_FLOOR, after=()):
    """The bar indexes sections start at, the song's end last: the bar lines in
    `required`, then the strongest peaks of `curve`, then the bar lines in `after`,
    each at least `least` bars from another and `edge` from the song's ends."""
    n = len(curve) - 1
    chosen = [0, n]

    def fits(i):
        return all(abs(i - c) >= (edge if c in (0, n) else least) for c in chosen)

    for i in sorted(required, key=lambda i: -curve[i]):
        if 0 < i < n and fits(i):
            chosen.append(int(i))
    for i in np.argsort(-curve[1:n]) + 1:
        if curve[i] < floor:
            break
        if fits(i):
            chosen.append(int(i))
    for i in after:
        if 0 < i < n and fits(i):
            chosen.append(int(i))
    return sorted(chosen)


def aligned(m, a, b):
    """How alike two runs of bars are, bar by bar from their starts or from their
    ends, whichever is the more: the mean of `m` along the diagonal. They are not
    slid against each other: a song that plays one two-bar figure throughout
    would then match every section with every other."""
    (a0, a1), (b0, b1) = a, b
    k = min(a1 - a0, b1 - b0)
    start = float(np.mean([m[a0 + i, b0 + i] for i in range(k)]))
    end = float(np.mean([m[a1 - k + i, b1 - k + i] for i in range(k)]))
    return max(start, end)


def profile_moved(a, b) -> tuple[int, float, float]:
    """The semitones that move pitch-class profile `b` most like `a`, how alike
    they are so (correlation) and how alike unmoved."""
    r = [float(np.corrcoef(a, np.roll(b, k))[0, 1]) for k in range(12)]
    k = int(np.argmax(r))
    return k, r[k], r[0]


def a_bar_off(chords, sound, span, other):
    """Whether a section is an earlier one again when its start moves a bar: the
    bar (-1 or 1) its bars match the other's at, with how alike, or None. Only
    one bar, and only a match as close as the same music: a song that plays one
    two-bar figure throughout matches itself two bars off."""
    (a0, a1), (b0, b1) = span, other
    k = min(a1 - a0, b1 - b0) - 1
    best = None
    for d in (-1, 1):
        pairs = [(a0 + i, b0 + i + d) for i in range(k + 1) if a0 <= a0 + i < a1 and b0 <= b0 + i + d < b1]
        if len(pairs) < max(2, k):
            continue
        c = float(np.mean([chords[x, y] for x, y in pairs]))
        s = float(np.mean([sound[x, y] for x, y in pairs]))
        if c >= SAME_CHORDS and s >= SAME_SOUND and (best is None or c + s > best[1] + best[2]):
            best = (d, c, s)
    return best


def labelled(spans, chords, sound, chords_moved, profiles) -> list[dict]:
    """Each section's letter: an earlier section's when it is that one again,
    varied or in another key, otherwise the next letter. A key is moved when the
    bars match moved and the two sections' pitch classes, taken whole, match
    best moved by the same semitones. A section that is an earlier one again
    with its start a bar off starts there instead; `spans` is changed so."""
    out = []
    for i in range(len(spans)):
        span = spans[i]
        best = None
        for j in range(i):
            c0 = aligned(chords, span, spans[j])
            s0 = aligned(sound, span, spans[j])
            found = None
            if c0 >= SAME_CHORDS and s0 >= SAME_SOUND:
                found = ("same", 0, c0, s0)
            elif c0 >= SAME_CHORDS and s0 >= VARIED_SOUND:
                found = ("varied", 0, c0, s0)
            elif s0 >= VARIED_SOUND:
                k, ck = max(((k, aligned(m, span, spans[j])) for k, m in chords_moved.items()), key=lambda x: x[1])
                pk, pr, p0 = profile_moved(profiles[i], profiles[j])
                bars_say = ck >= MOVED_CHORDS and ck >= c0 + MOVED_GAIN and pk == k and pr >= p0 + MOVED_GAIN
                whole_says = pk and pr >= PROFILE_MOVED and pr >= p0 + PROFILE_GAIN and s0 >= SAME_SOUND
                if bars_say or whole_says:
                    found = ("moved", pk, max(ck, c0), s0)
            if found and (best is None or found[2] + found[3] > best[1][2] + best[1][3]):
                best = (j, found)
        if (best is None or best[1][0] != "same") and i > 0:
            for j in range(i):
                off = a_bar_off(chords, sound, span, spans[j])
                start = span[0] - off[0] if off else None
                if off and start - spans[i - 1][0] >= EDGE_BARS and span[1] - start >= EDGE_BARS:
                    spans[i - 1] = (spans[i - 1][0], start)
                    spans[i] = span = (start, span[1])
                    best = (j, ("same", 0, off[1], off[2]))
                    break
        if best is None:
            letter = LETTERS[len({o["letter"] for o in out}) % len(LETTERS)]
            out.append({"letter": letter, "like": None})
            continue
        j, (how, k, c, s) = best
        out.append(
            {
                "letter": out[j]["letter"],
                "like": {
                    "section": j,
                    "how": how,
                    "semitones": (k if k <= 6 else k - 12) if how == "moved" else 0,
                    "chords": round(c, 2),
                    "sound": round(s, 2),
                },
            }
        )
    for o in out:
        how = (o["like"] or {}).get("how")
        o["label"] = o["letter"] + ("′" if how in ("varied", "moved") else "")
    return out


# --- what plays -------------------------------------------------------------------


def parts_of(path: Path) -> dict:
    """The parts `daw separate` made of this file, by the recipe in the project's
    `separated` folder beside it: part name to file."""
    from .library import digest

    sha = None
    best, made = {}, ""
    for project in (path.parent.parent, path.parent):
        folder = project / "separated"
        if not folder.is_dir():
            continue
        for recipe in folder.glob("*.separated.json"):
            try:
                record = json.loads(recipe.read_text())
            except (OSError, ValueError):
                continue
            source = record.get("source", {})
            if (project / source.get("path", "")).resolve() != path.resolve():
                sha = sha or digest(path)
                if source.get("sha256") != sha:
                    continue
            parts = {
                name: project / info["path"]
                for name, info in record.get("parts", {}).items()
                if (project / info.get("path", "")).is_file()
            }
            key = (len(parts), record.get("made", ""))
            if parts and key > (len(best), made):
                best, made = parts, record.get("made", "")
    return best


def bar_levels(mono, sr, bars) -> np.ndarray:
    """Each bar's level in dB (mean power)."""
    out = []
    for b in bars:
        x = mono[int(b["seconds"] * sr) : max(int(b["until"] * sr), int(b["seconds"] * sr) + 1)]
        out.append(10 * np.log10(float(np.mean(np.square(x, dtype=np.float64))) + 1e-12))
    return np.array(out)


def against_loudest(levels):
    """Levels against the loud bars', the 95th percentile."""
    return levels - float(np.percentile(levels, 95))


def entries(sung) -> list[int]:
    """The bars the voice comes in at after REST_BARS bars or more of rest."""
    out, rest = [], REST_BARS
    for i, s in enumerate(sung):
        if s and rest >= REST_BARS:
            out.append(i)
        rest = 0 if s else rest + 1
    return out


def same_words(sections, letters, bars, heard) -> dict:
    """For each letter of two sections or more, how alike their words are, 0 to 1:
    the mean over its pairs of sections of the share of the shorter's words found
    in the same order in the longer, so a chorus cut short is still its chorus."""
    from .words import tokens

    said = []
    for a, b in sections:
        start, end = bars[a]["seconds"], bars[b - 1]["until"]
        said.append(tokens(" ".join(w["word"] for w in heard if start <= w["start"] < end)))
    by_letter = {}
    for i, l in enumerate(letters):
        if len(said[i]) >= LEAST_WORDS:
            by_letter.setdefault(l["letter"], []).append(i)
    out = {}
    for letter, idx in by_letter.items():
        pairs = [(i, j) for k, i in enumerate(idx) for j in idx[k + 1 :]]
        if pairs:
            out[letter] = round(
                float(np.mean([found_in(said[i], said[j]) for i, j in pairs])), 2
            )
    return out


def hooked_bars(heard, bars) -> np.ndarray:
    """Which bars sing the hook, from the bar each of its lines starts in to the bar
    it ends in. The hook is the line sung most often, HOOK_COUNT times or more,
    with the lines that come back sung around it: those more than half of whose
    times are within HOOK_NEAR bars of that line's. Another line that comes back
    elsewhere, a pre-chorus's, is not the hook."""
    from .words import groups_of, lines, text, tokens

    said = lines(heard)
    rows = [{"text": text(line), "seconds": line[0]["start"]} for line in said]
    starts = np.array([b["seconds"] for b in bars])

    def bar(seconds):
        return max(int(np.searchsorted(starts, seconds, side="right")) - 1, 0)

    groups = sorted(groups_of(rows), key=lambda g: (-len(g), -len(tokens(rows[g[0]]["text"]))))
    out = np.zeros(len(bars), dtype=bool)
    if not groups or len(groups[0]) < HOOK_COUNT:
        return out
    near = {bar(said[i][0]["start"]) for i in groups[0]}
    hook = [groups[0]]
    for g in groups[1:]:
        close = sum(1 for i in g if any(abs(bar(said[i][0]["start"]) - b) <= HOOK_NEAR for b in near))
        if 2 * close > len(g):
            hook.append(g)
    for g in hook:
        for i in g:
            a, b = bar(said[i][0]["start"]), bar(said[i][-1]["end"] - 0.05)
            out[a : max(a, b) + 1] = True
    return out


def hook_runs(hooked) -> list[tuple[int, int]]:
    """The runs of MIN_BARS bars or more that sing the hook, a bar's rest within one
    bridged: each as its first bar's index and the index after its last."""
    runs, i, n = [], 0, len(hooked)
    while i < n:
        if not hooked[i]:
            i += 1
            continue
        j = i
        while j < n and (hooked[j] or (j + 1 < n and hooked[j + 1])):
            j += 1
        if j - i >= MIN_BARS:
            runs.append((i, j))
        i = j
    return runs


def joined_by_hook(letters, spans, hooked, chords, sound) -> list[dict]:
    """Sections that sing the hook are one part, whatever letters the music gave
    them, and the others keep theirs. That tells apart a rap verse and its hook
    over one beat, and makes its hooks one. A section whose letter's first section
    is now another is like that first section again: the same or, its music not
    matching, varied (′)."""
    sings = [i for i, (a, b) in enumerate(spans) if float(np.mean(hooked[a:b])) >= HOOKED]
    if len(sings) < 2:
        return letters
    for i in sings:
        letters[i] = {"letter": "hook", "like": None}
    first = {}
    for i, l in enumerate(letters):
        name = l["letter"]
        if name not in first:
            first[name] = i
            l["like"] = None
            continue
        j = first[name]
        if l["like"] is not None and l["like"]["section"] == j:
            continue
        c, s = aligned(chords, spans[i], spans[j]), aligned(sound, spans[i], spans[j])
        how = "same" if c >= SAME_CHORDS and s >= SAME_SOUND else "varied"
        l["like"] = {"section": j, "how": how, "semitones": 0, "chords": round(c, 2), "sound": round(s, 2)}
    # Letters again in the order they first come.
    order = {}
    for l in letters:
        order.setdefault(l["letter"], LETTERS[len(order) % len(LETTERS)])
    for l in letters:
        l["letter"] = order[l["letter"]]
        how = (l["like"] or {}).get("how")
        l["label"] = l["letter"] + ("′" if how in ("varied", "moved") else "")
    return letters


def found_in(a, b) -> float:
    """The share of the shorter list's words found, in order, in the longer."""
    from difflib import SequenceMatcher

    matched = sum(m.size for m in SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks())
    return matched / max(1, min(len(a), len(b)))


def named(sections, letters, level, sung=None, played=None, worded=None) -> list[str]:
    """What each section is, by what it holds: with the parts, where the voice sings
    and the drums play; without them, by how loud each is and how often it comes.
    `worded` is same_words(): the music that comes back with its words is the
    chorus, and music that comes back with new words is not."""
    n = len(sections)
    names = [None] * n
    by_letter = {}
    for i, l in enumerate(letters):
        by_letter.setdefault(l["letter"], []).append(i)
    loud = np.array([float(np.mean(level[a:b])) for a, b in sections])
    sings = None if sung is None else np.array([float(np.mean(sung[a:b])) >= SUNG_SHARE for a, b in sections])
    drums = None if played is None else np.array([float(np.mean(played[a:b])) >= SUNG_SHARE for a, b in sections])
    quiet_line = float(np.median(loud)) - 3

    def alone(i):
        return len(by_letter[letters[i]["letter"]]) == 1

    def opens(i):
        # Before the voice first sings; without the parts, music that never comes back.
        return not sings[i] if sings is not None else alone(i)

    def closes(i):
        if sings is not None and not sings[i]:
            return True
        return alone(i) and loud[i] < quiet_line

    i = 0
    while i < n - 1 and opens(i):
        names[i] = "intro"
        i += 1
    a, b = sections[n - 1]
    fades = b - a >= 2 and level[b - 1] <= level[a] - FADE_DB
    j = n - 1
    while j > i and (closes(j) or (j == n - 1 and fades)):
        names[j] = "outro"
        j -= 1
    repeated = [l for l, idx in by_letter.items() if len(idx) >= 2 and any(names[i] is None for i in idx)]

    def mean_loud(l):
        return float(np.mean([loud[i] for i in by_letter[l] if names[i] is None]))

    def sung_letter(l):
        return sings is None or np.mean([sings[i] for i in by_letter[l]]) >= 0.5

    vocal = sorted((l for l in repeated if sung_letter(l)), key=mean_loud, reverse=True)
    worded = worded or {}
    same = [l for l in vocal if worded.get(l, 0) >= SAME_WORDS]
    # With the words: of the music that comes back with its words, the loudest;
    # without them, of the sung music that comes back, the loudest.
    candidates = same or vocal
    chorus = candidates[0] if candidates else None
    # A loud section always followed by the same sung one leads into it: that one
    # is the chorus, as a pre-chorus is often the louder.
    if chorus is not None:
        after = {letters[i + 1]["letter"] if i + 1 < n else None for i in by_letter[chorus]}
        if len(after) == 1 and (following := after.pop()) in candidates and following != chorus:
            chorus = following
    instrumental = [l for l in repeated if not sung_letter(l)]
    drop = max(instrumental, key=mean_loud, default=None)
    if drop is not None and (chorus is None or mean_loud(drop) <= mean_loud(chorus) or sings is None):
        drop = None
    first = {l: min((i for i in idx if names[i] is None), default=n) for l, idx in by_letter.items()}
    verse = None
    if chorus is not None:
        # A verse comes back with new words.
        others = [l for l in vocal if l != chorus and worded.get(l, 0) < SAME_WORDS] or [
            l for l in vocal if l != chorus
        ]
        earlier = [l for l in others if first[l] < first[chorus]]
        verse = min(earlier or others, key=lambda l: first[l], default=None)
    pre = post = None
    if chorus is not None:
        for l in vocal:
            if l in (chorus, verse):
                continue
            if pre is None and all(i + 1 < n and letters[i + 1]["letter"] == chorus for i in by_letter[l]):
                pre = l
            elif post is None and all(i > 0 and letters[i - 1]["letter"] == chorus for i in by_letter[l]):
                post = l
    roles = {chorus: "chorus", verse: "verse", pre: "pre-chorus", post: "post-chorus", drop: "drop"}
    roles.pop(None, None)
    seen_chorus = False
    for i in range(n):
        letter = letters[i]["letter"]
        if letter == chorus:
            seen_chorus = True
        if names[i] is not None:
            continue
        if letter in roles:
            names[i] = roles[letter]
        elif drums is not None and not drums[i] and loud[i] < quiet_line:
            names[i] = "break"
        elif sings is not None and not sings[i]:
            names[i] = "instrumental"
        elif alone(i) and seen_chorus:
            names[i] = "bridge"
    # The rest of a run of sung sections that holds a verse, between the choruses,
    # is the verse's too.
    i = 0
    while i < n:
        j = i
        while j < n and names[j] in (None, "verse"):
            j += 1
        if "verse" in names[i:j]:
            for k in range(i, j):
                names[k] = "verse"
        i = max(j, i + 1)
    # A section of a letter some other section of which is the verse is too.
    verses = {letters[i]["letter"] for i in range(n) if names[i] == "verse"}
    names = [name or ("verse" if letters[i]["letter"] in verses else "section") for i, name in enumerate(names)]
    return names


# --- keys -------------------------------------------------------------------------


def key_of(profile) -> dict:
    """The key a pitch-class profile is most like, with its confidence, and whether
    the sound is plainly in it (`daw samples chords`' test); a section of a
    full mix often is not, and its likeliest key is still said."""
    found = harmony.estimate(profile)
    if found["tonal"]:
        return {"name": found["name"], "confidence": found["confidence"], "relative": found["relative"], "sure": True}
    unit = np.asarray(profile, dtype=float)
    if unit.sum() <= 0 or unit.std() <= 1e-9:
        return {"name": None, "confidence": None, "relative": None, "sure": False}
    unit = unit / unit.sum()
    r = np.array([np.corrcoef(unit, k)[0, 1] for k in harmony.KEYS])
    odds = np.exp(harmony.SHARPNESS * (r - r.max()))
    odds /= odds.sum()
    best = int(np.argmax(r))
    tonic, mode = harmony.KEY_NAMES[best]
    return {
        "name": harmony.key_name(tonic, mode),
        "confidence": round(float(odds[best]), 3),
        "relative": harmony.key_name(*harmony.relative(tonic, mode)),
        "sure": False,
    }


def key_changes(sections, keys, letters, song_key) -> list[dict]:
    """Where the key moves: a section that is an earlier one moved some semitones,
    and the key from a section on to the song's end, when every section after it
    plainly in a key is in that one or its relative, over KEPT_BARS or more. A
    section of a full mix is seldom plainly in a key, and its likeliest key moves
    with its chords, so a key that comes and goes is not called a change."""
    out = []
    current = song_key

    def same(a, b):
        return a["name"] == b["name"] or a["relative"] == b["name"]

    for i, (sec, key, l) in enumerate(zip(sections, keys, letters)):
        like = l["like"] or {}
        if like.get("how") == "moved":
            out.append(
                {
                    "section": i,
                    "bar": sec["bars"][0],
                    "seconds": sec["seconds"],
                    "semitones": like["semitones"],
                    "from": current,
                    "to": key["name"],
                    "heard_as": f"section {like['section'] + 1} again, {like['semitones']:+d} semitones",
                }
            )
            current = key["name"] or current
            continue
        if not key["sure"] or current is None or key["name"] in (current, None) or key["relative"] == current:
            continue
        rest = [k for k, s in zip(keys[i:], sections[i:]) if k["sure"] and s["name"] != "outro"]
        held = sum(s["count"] for s in sections[i:] if s["name"] != "outro")
        if held >= KEPT_BARS and all(same(k, key) for k in rest):
            out.append(
                {
                    "section": i,
                    "bar": sec["bars"][0],
                    "seconds": sec["seconds"],
                    "semitones": None,
                    "from": current,
                    "to": key["name"],
                    "heard_as": "the key from here to the end",
                }
            )
            current = key["name"]
    return out


# --- checks -----------------------------------------------------------------------


def division(mono, sr, report) -> dict:
    """How the beats divide, from the onsets between them: their weight near the
    first third of a beat against near its half. A swung eighth sits at the
    second third, so that one is not counted."""
    from . import swing

    times, strength = swing.onsets(mono, sr)
    beat_times = np.array([b["seconds"] for b in report["beats"]])
    if len(times) < 16 or len(beat_times) < 4:
        return {"divides_in": None, "why": "too few onsets"}
    index = np.searchsorted(beat_times, times, side="right") - 1
    inside = (index >= 0) & (index < len(beat_times) - 1)
    index, times, w = index[inside], times[inside], np.sqrt(strength[inside])
    place = (times - beat_times[index]) / (beat_times[index + 1] - beat_times[index])
    third = float(w[np.abs(place - 1 / 3) <= NEAR].sum())
    half = float(w[np.abs(place - 1 / 2) <= NEAR].sum())
    if third + half <= 0:
        return {"divides_in": None, "why": "no onsets between the beats"}
    share = third / (third + half)
    return {
        "divides_in": 3 if share >= 0.5 else 2,
        "thirds_share": round(share, 2),
    }


def meter_name(per_bar: int, divides_in: int | None) -> str:
    if divides_in == 3:
        return {2: "6/8", 3: "9/8", 4: "12/8"}.get(per_bar, f"{per_bar * 3}/8")
    return f"{per_bar}/4"


def pulse(report) -> list[dict]:
    """Where the map's beat is EVEN_SPREAD times longer or shorter than its usual
    one for eight beats or more: a place it may count another pulse."""
    rows = report["beats"]
    times = np.array([b["seconds"] for b in rows])
    if len(times) < 16:
        return []
    gaps = np.diff(times)
    local = np.array([np.median(gaps[max(0, i - 4) : i + 4]) for i in range(len(gaps))])
    usual = float(np.median(gaps))
    ratio = local / usual
    other = (ratio > EVEN_SPREAD) | (ratio < 1 / EVEN_SPREAD)
    out, i = [], 0
    while i < len(other):
        if not other[i]:
            i += 1
            continue
        j = i
        while j < len(other) and other[j]:
            j += 1
        if j - i >= 8:
            out.append(
                {
                    "from_bar": rows[i]["bar"],
                    "to_bar": rows[j - 1]["bar"],
                    "seconds": rows[i]["seconds"],
                    "until_seconds": rows[min(j, len(rows) - 1)]["seconds"],
                    "beat_ms": round(float(np.median(gaps[i:j])) * 1000),
                    "usual_ms": round(usual * 1000),
                    "times_the_usual": round(float(np.median(ratio[i:j])), 2),
                }
            )
        i = j
    return out


def drums_check(drum_mono, sr, report, bars) -> dict:
    """Whether the kick and snare play each bar as the song's other bars do: the
    song's cycle, where in the bar each falls most, and the bars that match it
    better moved a whole number of beats."""
    from . import drums, hits

    found = drums.heard(drum_mono, sr)
    grid = hits.Grid(report)
    per_bar = int(report["beats_per_bar"])
    cells = per_bar * 2  # eighths
    by_bar = {}
    count = {"kick": np.zeros(cells), "snare": np.zeros(cells)}
    length = {b["bar"]: b["beats"] for b in bars}
    for m in found["moments"]:
        _, index = grid.place(m["seconds"])
        bar, beat = grid.bar_beat(index)
        if length.get(bar) != per_bar:
            continue
        cell = int(round((beat - 1) * 2)) % cells
        for d in ("kick", "snare"):
            if d in m["drums"]:
                count[d][cell] += 1
                by_bar.setdefault(bar, []).append((d, cell))
    if count["kick"].sum() < 8 or count["snare"].sum() < 8:
        return {"used": False, "why": "the drums part has too few kicks and snares"}
    def score(template, played, r):
        return sum(template[d][(c + 2 * r) % cells] for d, c in played) / len(played)

    # The song's cycle: what most bars play. Bars a beat away muddle a plain count,
    # so each bar is turned to where it best matches the count and the count made
    # again from the bars so turned, twice.
    template = {d: count[d] / count[d].sum() for d in count}
    for _ in range(2):
        turned = {d: np.zeros(cells) for d in count}
        for played in by_bar.values():
            best = max(range(per_bar), key=lambda r: score(template, played, r))
            for d, c in played:
                turned[d][(c + 2 * best) % cells] += 1
        template = {d: turned[d] / max(turned[d].sum(), 1) for d in turned}
    cycle = {d: [float(c / 2 + 1) for c in np.flatnonzero(template[d] >= 0.5 * template[d].max())] for d in template}
    off = {}
    for bar, played in by_bar.items():
        if len(played) < 3:
            continue
        scores = [score(template, played, r) for r in range(per_bar)]
        best = int(np.argmax(scores))
        if best and scores[best] - scores[0] >= OFF_SHARE * scores[best]:
            off[bar] = best
    runs = []
    for bar in sorted(off):
        r = off[bar]
        if runs and runs[-1]["beats"] == r and bar - runs[-1]["bars"][1] <= 2:
            runs[-1]["bars"][1] = bar
        else:
            runs.append({"bars": [bar, bar], "beats": r})
    runs = [r for r in runs if r["bars"][1] > r["bars"][0]]
    for r in runs:
        shift = r["beats"] if r["beats"] <= per_bar // 2 else r["beats"] - per_bar
        r["beats"] = shift
        r["said"] = (
            f"bars {r['bars'][0]}–{r['bars'][1]}: the kick and snare play as if each bar started "
            f"{abs(shift)} beat{'s' if abs(shift) != 1 else ''} {'later' if shift > 0 else 'earlier'}"
        )
    return {"used": True, "cycle": cycle, "bars_off": runs}


# --- the report -------------------------------------------------------------------


def measure(path: Path, report: dict, parts: dict | None = None) -> dict:
    """The form of the song at `path` on its beat map `report`, with the parts
    `daw separate` made of it when there are any."""
    mono, sr = beatmap.read(path)
    bars = bar_spans(report)
    if len(bars) < 2 * MIN_BARS:
        raise ValueError(f"{len(bars)} bars is too short for a form: {path}")
    per_bar = int(report["beats_per_bar"])
    pitch, level = per_beat(mono, sr, report)
    tones, sound = bar_features(bars, pitch, level, per_bar)
    chords_raw = chords_alike(tones)
    chords = ranks(chords_raw)
    sounds = ranks(-sound_apart(sound))
    s = (chords + sounds) / 2
    curve = (novelty(s, 4) + novelty(s, 8)) / 2
    loud = bar_levels(mono, sr, bars)

    parts = parts or {}
    sung = played = None
    heard_parts, absent = {}, []
    mix_top = float(np.percentile(loud, 95))
    for name in ("vocals", "drums"):
        if name in parts:
            audio = beatmap.read(parts[name])
            levels = bar_levels(*audio, bars)
            # A part whose loud bars are far under the song's holds only what
            # leaked into it: the song has no voice, or no drums.
            if float(np.percentile(levels, 95)) < mix_top + ABSENT_DB:
                absent.append(name)
                continue
            heard_parts[name] = (audio, levels)
    if "vocals" in heard_parts:
        sung = against_loudest(heard_parts["vocals"][1]) >= SUNG_DB
    if "drums" in heard_parts:
        played = against_loudest(heard_parts["drums"][1]) >= PLAYED_DB
    required = entries(sung) if sung is not None else []
    heard_words = hooked = None
    edges = []
    if "vocals" in heard_parts:
        from . import words

        heard_words = words.kept(Path(parts["vocals"]))
        if heard_words is not None:
            hooked = hooked_bars(heard_words["words"], bars)
            edges = [i for run in hook_runs(hooked) for i in run]
    # The hook's edges start sections where the music does not change near them,
    # as over a rap's one beat; where it does, the music's bar line stands.
    cut = boundaries(curve, required, after=edges)
    spans = list(zip(cut[:-1], cut[1:]))
    chords_moved = {k: ranks(chords_alike(tones, k), chords_raw) for k in range(1, 12)}
    profiles = [
        pitch[bars[a]["first"] : bars[b - 1]["first"] + bars[b - 1]["beats"], :12].sum(axis=0) for a, b in spans
    ]
    letters = labelled(spans, chords, sounds, chords_moved, profiles)
    if hooked is not None:
        letters = joined_by_hook(letters, spans, hooked, chords, sounds)
    # A section moved a bar by its match holds other bars now.
    profiles = [
        pitch[bars[a]["first"] : bars[b - 1]["first"] + bars[b - 1]["beats"], :12].sum(axis=0) for a, b in spans
    ]
    worded = None
    if heard_words is not None:
        worded = same_words(spans, letters, bars, heard_words["words"])
    names = named(spans, letters, loud, sung, played, worded)

    whole = key_of(pitch[:, :12].sum(axis=0))
    song_key = whole["name"]
    sections, keys = [], []
    for (a, b), l, name, profile in zip(spans, letters, names, profiles):
        first, last = bars[a], bars[b - 1]
        key = key_of(profile)
        keys.append(key)
        sec = {
            "label": l["label"],
            "name": name,
            "bars": [first["bar"], last["bar"]],
            "count": b - a,
            "seconds": round(first["seconds"], 3),
            "until_seconds": round(last["until"], 3),
            "key": key["name"],
            "key_confidence": key["confidence"],
            "key_sure": key["sure"],
            "level_db": round(float(np.mean(loud[a:b])), 1),
            "starts_by": starts_by(a, sung, hooked, required, edges),
            "change": round(float(curve[a]), 2),
        }
        if sung is not None:
            sec["sung"] = round(float(np.mean(sung[a:b])), 2)
        if played is not None:
            sec["drums"] = round(float(np.mean(played[a:b])), 2)
        if worded is not None:
            said = [w["word"] for w in heard_words["words"] if sec["seconds"] <= w["start"] < sec["until_seconds"]]
            sec["words"] = " ".join(x.strip() for x in said[:8]) + (" …" if len(said) > 8 else "")
            if l["letter"] in worded:
                sec["same_words"] = worded[l["letter"]]
            sec["hook"] = round(float(np.mean(hooked[a:b])), 2)
        if l["like"]:
            sec["like"] = {**l["like"], "section": l["like"]["section"] + 1}
        sections.append(sec)

    changes = key_changes(sections, keys, letters, song_key)
    for c in changes:
        c["section"] += 1
    divided = division(mono, sr, report)
    checks = {
        "meter": report["meter"],
        "meter_set_by": report.get("meter_set_by"),
        "beats_per_bar": per_bar,
        "odd_bars": report.get("odd_bars", []),
        "beat": divided,
        "heard_as": meter_name(per_bar, divided.get("divides_in")),
        "beat_changes_length": pulse(report),
    }
    if "drums" in heard_parts:
        checks["drums"] = drums_check(*heard_parts["drums"][0], report, bars)
    elif "drums" in absent:
        checks["drums"] = {"used": False, "why": "the drums part is silent: the song has no drums"}
    else:
        checks["drums"] = {"used": False, "why": "no drums part beside the song; daw separate --split stems makes one"}
    warnings = []
    if divided.get("divides_in") == 3 and checks["heard_as"] != report["meter"]:
        warnings.append(
            f"The map counts {report['meter']}, and its beats divide in three: the song may be in "
            f"{checks['heard_as']}."
        )
    model = report.get("model") or {}
    if model.get("counted") == "doubled" and divided.get("divides_in") == 3:
        warnings.append(
            "The map doubled the model's beat, which divides in three, not two. "
            f"daw samples beats --bpm {model.get('heard_bpm')} counts it as heard."
        )
    for p in checks["beat_changes_length"]:
        warnings.append(
            f"From bar {p['from_bar']} to {p['to_bar']} the map's beat is {p['times_the_usual']} times its usual "
            "length: it may count another pulse there."
        )
    moving = sum(p["to_bar"] - p["from_bar"] + 1 for p in checks["beat_changes_length"])
    if moving >= len(bars) / 4:
        warnings.append(
            f"{moving} of the {len(bars)} bars are counted at another pulse: the sections are read on those bars "
            "and are no better than they are."
        )
    for r in checks["drums"].get("bars_off", []):
        warnings.append(f"The drums check the bars: {r['said']}; daw samples beats --downbeat moves them.")
    return {
        "path": str(path),
        "beat_map": report.get("map"),
        "tempo_bpm": report["tempo"]["bpm"],
        "meter": report["meter"],
        "first_downbeat_seconds": report["first_downbeat_seconds"],
        "bar_count": len(bars),
        "key": song_key,
        "key_confidence": whole["confidence"],
        "key_sure": whole["sure"],
        "parts": {name: str(p) for name, p in parts.items()},
        **({"silent_parts": absent} if absent else {}),
        "named_by": named_by(sung is not None, worded is not None, "vocals" in absent, path),
        **({"words_file": heard_words["file"]} if heard_words is not None else {}),
        "form": " ".join(s["label"] for s in sections),
        "sections": sections,
        "key_changes": changes,
        "checks": checks,
        **({"warnings": warnings} if warnings else {}),
        "_matrix": s,
        "_bars": bars,
    }


def starts_by(a, sung, hooked, required, edges) -> str:
    if a == 0:
        return "the song's start"
    if a in required:
        return "the voice coming in"
    if a in edges:
        return "the hook coming in" if hooked[a] else "the hook ending"
    return "a change"


def named_by(parts: bool, worded: bool, voiceless: bool, path: Path) -> str:
    if worded:
        return "the words that come back, the voice and drums parts, level and repetition"
    if parts:
        return (
            "the voice and drums parts, level and repetition; "
            f"daw transcribe {path.name} --words names the chorus by the words that come back"
        )
    if voiceless:
        return "level and repetition; the song has no voice"
    return "level and repetition; with the parts from daw separate --split stems, the voice and drums too"


def picture(found: dict, output: Path) -> str:
    """The bar matrix, light where two bars are alike, with the sections marked
    and labelled along both sides."""
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.figure import Figure

    s, bars = found["_matrix"], found["_bars"]
    n = len(s)
    fig = Figure(figsize=(12, 11), layout="constrained")
    FigureCanvasAgg(fig)
    ax = fig.subplots()
    ax.imshow(s, cmap="magma", vmin=0, vmax=1, interpolation="nearest")
    index = {b["bar"]: i for i, b in enumerate(bars)}
    for sec in found["sections"]:
        a = index[sec["bars"][0]] - 0.5
        b = index[sec["bars"][1]] + 0.5
        ax.axhline(a, color="cyan", lw=0.6)
        ax.axvline(a, color="cyan", lw=0.6)
        text = f"{sec['label']} {sec['name']}"
        ax.text((a + b) / 2, -1, text, ha="center", va="bottom", fontsize=7, rotation=90)
        ax.text(n + 0.5, (a + b) / 2, f"{text}  {sec['bars'][0]}–{sec['bars'][1]}", va="center", fontsize=7)
    ticks = list(range(0, n, 8))
    ax.set_xticks(ticks, [bars[i]["bar"] for i in ticks], fontsize=7)
    ax.set_yticks(
        ticks,
        [f"{bars[i]['bar']}  {int(bars[i]['seconds'] // 60)}:{bars[i]['seconds'] % 60:04.1f}" for i in ticks],
        fontsize=7,
    )
    ax.set_xlim(-0.5, n - 0.5)
    ax.set_xlabel(
        "Bars against bars: light where their chords and sound are alike; a light line off the diagonal "
        "is music that comes back",
        fontsize=9,
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output, dpi=100)
    return str(output.resolve())
