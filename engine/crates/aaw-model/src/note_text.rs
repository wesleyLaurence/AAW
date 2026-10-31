//! Note text: pitched music written a token a cell on a grid, as the drum
//! rows are, rows as voices, and the same read back from a clip's notes.
//! `daw describe midi.text` is the notation; `note.write` and `notes` with
//! a grid use it.

use crate::rules;
use crate::schema::Note;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// A note the text writes, in beats from where the text starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub pitch: i64,
    pub at: BigRational,
    pub duration: BigRational,
    pub velocity: i64,
}

/// What a text wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    /// In row order, then cell order.
    pub notes: Vec<Written>,
    /// Beats the text covers from its start: the longest row, rests
    /// counted, or the last note's end where a length runs past it.
    pub span: BigRational,
}

/// A clip's notes as text.
#[derive(Clone, Debug, PartialEq)]
pub struct Printed {
    /// A row a voice, from the highest down, bar lines where the bar is
    /// whole cells.
    pub text: String,
    /// The notes whose start or length the grid could not hold, by index,
    /// each written at its nearest cell.
    pub off_grid: Vec<usize>,
}

fn int(n: usize) -> BigRational {
    BigRational::from_integer(BigInt::from(n))
}

/// How many cells a bar is, when it is whole cells.
fn cells_per_bar(grid: &BigRational, bar: &BigRational) -> Option<usize> {
    let per = bar / grid;
    per.is_integer().then(|| per.to_integer().to_usize()).flatten().filter(|n| *n > 0)
}

/// `text` at `at` beats from a clip's start, each cell `grid` beats, bars of
/// `bar` beats.
pub fn parse(text: &str, grid: &BigRational, bar: &BigRational, at: &BigRational) -> Result<Parsed, String> {
    if !grid.is_positive() {
        return Err("The grid is beats, more than 0: 1/4 is a sixteenth note".into());
    }
    let per = cells_per_bar(grid, bar);
    let rows: Vec<&str> = text.lines().collect();
    let many = rows.len() > 1;
    let mut notes = Vec::new();
    let mut span = BigRational::zero();
    for (r, row) in rows.iter().enumerate() {
        let row = without_comment(row);
        let spaced = row.replace('[', " [ ").replace(']', " ] ").replace('|', " | ");
        let tokens: Vec<&str> = spaced.split_whitespace().collect();
        let place = |cell: usize| -> String {
            let row = if many { format!("row {}, ", r + 1) } else { String::new() };
            match per {
                Some(per) => format!("{row}bar {}, cell {}", cell / per + 1, cell % per + 1),
                None => format!("{row}cell {}", cell + 1),
            }
        };
        let mut cell = 0usize;
        let mut bar_start = 0usize;
        // The notes the last sounding token made, which `-` holds.
        let mut held: Vec<usize> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let tok = tokens[i];
            i += 1;
            match tok {
                "|" => {
                    let Some(per) = per else {
                        return Err(format!(
                            "{}: bar lines need a bar of whole cells, and a bar of {} beats is not whole cells of {}",
                            place(cell),
                            crate::fraction_str(bar),
                            crate::fraction_str(grid)
                        ));
                    };
                    if cell == bar_start {
                        continue;
                    }
                    if !(at / bar).is_integer() {
                        return Err(format!(
                            "at {} is not a bar's start, and the text has bar lines: they count whole bars from the clip's start",
                            crate::fraction_str(at)
                        ));
                    }
                    let cells = cell - bar_start;
                    if cells != per {
                        return Err(format!(
                            "{}bar {} has {cells} cells; a bar of {} beats on a {} grid has {per}",
                            if many { format!("row {}, ", r + 1) } else { String::new() },
                            bar_start / per + 1,
                            crate::fraction_str(bar),
                            crate::fraction_str(grid)
                        ));
                    }
                    bar_start = cell;
                }
                "[" => {
                    let Some(close) = tokens[i..].iter().position(|t| *t == "]") else {
                        return Err(format!("{}: a split [ ] is not closed", place(cell)));
                    };
                    let inside = &tokens[i..i + close];
                    i += close + 1;
                    if inside.is_empty() {
                        return Err(format!("{}: a split [ ] holds a token or more", place(cell)));
                    }
                    if let Some(bad) = inside.iter().find(|t| matches!(**t, "[" | "|")) {
                        return Err(format!("{}: `{bad}` cannot be inside a split", place(cell)));
                    }
                    let unit = grid / int(inside.len());
                    for (j, sub) in inside.iter().enumerate() {
                        let start = at + int(cell) * grid + int(j) * &unit;
                        token(sub, &start, &unit, &mut notes, &mut held).map_err(|e| format!("{}: {e}", place(cell)))?;
                    }
                    cell += 1;
                }
                "]" => return Err(format!("{}: a `]` closes no split", place(cell))),
                _ => {
                    let start = at + int(cell) * grid;
                    token(tok, &start, grid, &mut notes, &mut held).map_err(|e| format!("{}: {e}", place(cell)))?;
                    cell += 1;
                }
            }
        }
        if let Some(per) = per {
            // A row's last bar is checked when a bar line was written and
            // it runs over; one short is a row stopping short.
            let cells = cell - bar_start;
            if bar_start > 0 && cells > per {
                return Err(format!(
                    "{}bar {} has {cells} cells; a bar of {} beats on a {} grid has {per}",
                    if many { format!("row {}, ", r + 1) } else { String::new() },
                    bar_start / per + 1,
                    crate::fraction_str(bar),
                    crate::fraction_str(grid)
                ));
            }
        }
        let row_span = int(cell) * grid;
        if row_span > span {
            span = row_span;
        }
    }
    for n in &notes {
        let end = &n.at - at + &n.duration;
        if end > span {
            span = end;
        }
    }
    if notes.is_empty() && span.is_zero() {
        return Err("The text has no cells: a pitch such as c2, . for a rest, - to hold, [ ] to split a cell".into());
    }
    Ok(Parsed { notes, span })
}

/// A row up to its comment: a `#` where a token starts, so that the sharp
/// of `a#4` is not one.
fn without_comment(row: &str) -> &str {
    let mut at_start = true;
    for (i, c) in row.char_indices() {
        if c == '#' && at_start {
            return &row[..i];
        }
        at_start = c.is_whitespace();
    }
    row
}

/// One token at `start`, a cell of `unit` beats: a rest, a hold, a pitch or
/// a chord with lengths and a velocity.
fn token(tok: &str, start: &BigRational, unit: &BigRational, notes: &mut Vec<Written>, held: &mut Vec<usize>) -> Result<(), String> {
    match tok {
        "." => {
            held.clear();
            Ok(())
        }
        "-" => {
            if held.is_empty() {
                return Err("`-` holds nothing: no note sounds before it".into());
            }
            for i in held.iter() {
                notes[*i].duration += unit;
            }
            Ok(())
        }
        _ => {
            let (head, velocity) = match tok.rsplit_once('@') {
                Some((head, v)) => {
                    let velocity: i64 = v.parse().ok().filter(|v| (1..=127).contains(v)).ok_or_else(|| format!("velocity after @ is 1 to 127, not `{v}`"))?;
                    (head, velocity)
                }
                None => (tok, 100),
            };
            if head.contains('@') {
                return Err(format!("`{tok}`: velocity goes once, at the end of the token"));
            }
            let parts: Vec<&str> = head.split(',').collect();
            if parts.iter().any(|p| p.is_empty()) {
                return Err(format!("`{tok}`: a chord is pitches joined by commas, c2,eb3,g3"));
            }
            let mut pitches = Vec::new();
            for part in &parts {
                let (name, cells) = match part.split_once(':') {
                    Some((name, n)) => {
                        let n: usize = n.parse().ok().filter(|n| *n >= 1).ok_or_else(|| format!("`{part}`: a length after : is cells, 1 or more"))?;
                        (name, Some(n))
                    }
                    None => (*part, None),
                };
                pitches.push((pitch(name)?, cells));
            }
            // A length on the last pitch is the chord's, for the pitches
            // without one of their own.
            let last = pitches.last().and_then(|(_, n)| *n);
            held.clear();
            for (p, cells) in pitches {
                let cells = cells.or(last).unwrap_or(1);
                held.push(notes.len());
                notes.push(Written {
                    pitch: p,
                    at: start.clone(),
                    duration: int(cells) * unit,
                    velocity,
                });
            }
            Ok(())
        }
    }
}

/// A pitch token: a name with its octave, or a MIDI number.
fn pitch(name: &str) -> Result<i64, String> {
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        return name
            .parse::<i64>()
            .ok()
            .filter(|n| (0..=127).contains(n))
            .ok_or_else(|| format!("`{name}` is outside MIDI notes 0 to 127"));
    }
    rules::midi(name).map_err(|_| {
        // A capital A to G followed by a quality, Am7 or Cmaj7, or by no
        // octave at all.
        let rest = name.get(1..).unwrap_or("");
        let symbol = name.starts_with(|c: char| ('A'..='G').contains(&c))
            && (rest.chars().any(|c| c.is_ascii_alphabetic() && c != 'b') || !rest.chars().any(|c| c.is_ascii_digit()));
        if symbol {
            format!("`{name}` reads as a chord symbol, which is not written yet: write its notes, a2,c3,e3,g3")
        } else {
            format!("`{name}` is not a pitch: a name with its octave, c2 or F#3, or a MIDI number")
        }
    })
}

/// A note at its nearest cell on `grid`, with its length in cells; halfway
/// goes later, a length under half a cell is one cell.
fn fit(n: &Note, grid: &BigRational) -> (usize, usize, bool) {
    let half = BigRational::new(1.into(), 2.into());
    let at = n.at_exact();
    let duration = n.duration_exact();
    let cell = (&at / grid + &half).floor();
    let cells = (&duration / grid + &half).floor().max(BigRational::one());
    let on = at == &cell * grid && duration == &cells * grid;
    (cell.to_integer().to_usize().unwrap_or(0), cells.to_integer().to_usize().unwrap_or(1), on)
}

struct Token {
    cell: usize,
    cells: usize,
    velocity: i64,
    pitches: Vec<i64>,
}

/// `notes` as text on `grid`, a row a voice: each note goes in the first
/// row whose last note has ended by its start, the notes taken by start
/// then from the highest down, so a held chord takes one row and a melody
/// over it the next. Rows print from the highest first. A note before the
/// clip is left out and counted off the grid.
pub fn print(notes: &[Note], grid: &BigRational, bar: &BigRational) -> Printed {
    let per = cells_per_bar(grid, bar);
    let mut off_grid = Vec::new();
    let mut tokens: Vec<Token> = Vec::new();
    let mut order: Vec<(usize, usize, i64, i64)> = Vec::new();
    for (i, n) in notes.iter().enumerate() {
        if n.at_exact().is_negative() {
            off_grid.push(i);
            continue;
        }
        let (cell, cells, on) = fit(n, grid);
        if !on {
            off_grid.push(i);
        }
        order.push((cell, cells, n.velocity, n.pitch));
    }
    // Chords: one start, one length, one velocity.
    order.sort_by(|a, b| (a.0, a.1, a.2, a.3).cmp(&(b.0, b.1, b.2, b.3)));
    for (cell, cells, velocity, pitch) in order {
        match tokens.last_mut() {
            Some(t) if (t.cell, t.cells, t.velocity) == (cell, cells, velocity) => t.pitches.push(pitch),
            _ => tokens.push(Token { cell, cells, velocity, pitches: vec![pitch] }),
        }
    }
    // By start, then the highest first.
    tokens.sort_by(|a, b| (a.cell, b.pitches.iter().max()).cmp(&(b.cell, a.pitches.iter().max())));
    let mut rows: Vec<(usize, Vec<Token>)> = Vec::new();
    for t in tokens {
        let end = t.cell + t.cells;
        match rows.iter_mut().find(|(busy, _)| *busy <= t.cell) {
            Some((busy, row)) => {
                *busy = end;
                row.push(t);
            }
            None => rows.push((end, vec![t])),
        }
    }
    rows.sort_by_key(|(_, row)| std::cmp::Reverse(row.iter().flat_map(|t| t.pitches.iter().copied()).max()));
    let lines: Vec<String> = rows.iter().map(|(_, row)| line(row, per)).collect();
    Printed {
        text: lines.join("\n"),
        off_grid,
    }
}

/// One row: holds as `-` within a bar, `:N` across a bar line, `.` where
/// nothing starts, a bar line every bar, stopping at the last token.
fn line(row: &[Token], per: Option<usize>) -> String {
    let crosses = |t: &Token| per.is_some_and(|p| t.cell / p != (t.cell + t.cells - 1) / p);
    let end = row.iter().map(|t| if crosses(t) { t.cell + 1 } else { t.cell + t.cells }).max().unwrap_or(0);
    let mut cells: Vec<String> = vec![".".to_string(); end];
    for t in row {
        let mut word = t.pitches.iter().map(|p| rules::note_name(*p).unwrap_or_default().to_ascii_lowercase()).collect::<Vec<_>>().join(",");
        if crosses(t) {
            word.push_str(&format!(":{}", t.cells));
        } else {
            for c in t.cell + 1..t.cell + t.cells {
                cells[c] = "-".to_string();
            }
        }
        if t.velocity != 100 {
            word.push_str(&format!("@{}", t.velocity));
        }
        cells[t.cell] = word;
    }
    let mut out = String::new();
    for (c, word) in cells.iter().enumerate() {
        if c > 0 {
            out.push(' ');
            if per.is_some_and(|p| c % p == 0) {
                out.push_str("| ");
            }
        }
        out.push_str(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Beat;

    fn r(text: &str) -> BigRational {
        crate::beat(&Beat::Str(text.into())).unwrap()
    }

    fn parsed(text: &str, grid: &str, bar: &str, at: &str) -> Parsed {
        parse(text, &r(grid), &r(bar), &r(at)).unwrap()
    }

    fn refused(text: &str, grid: &str, bar: &str, at: &str) -> String {
        parse(text, &r(grid), &r(bar), &r(at)).unwrap_err()
    }

    fn brief(p: &Parsed) -> Vec<(i64, String, String, i64)> {
        p.notes.iter().map(|n| (n.pitch, crate::fraction_str(&n.at), crate::fraction_str(&n.duration), n.velocity)).collect()
    }

    fn note(pitch: i64, at: &str, duration: &str, velocity: i64) -> Note {
        Note {
            id: String::new(),
            pitch,
            at: Beat::Str(at.into()),
            duration: Beat::Str(duration.into()),
            velocity,
        }
    }

    fn printed(notes: &[Note], grid: &str, bar: &str) -> Printed {
        print(notes, &r(grid), &r(bar))
    }

    #[test]
    fn every_kind_of_token_is_a_cell() {
        // A pitch by name in either case or by number, a rest, a hold, a
        // split, a chord, a length, a velocity; a comment to the row's end.
        let p = parsed("c2 . G2 - 36 [bb1 c2] c2,eb3,g3:2 . c2:3@70 # a bass, a#1 not", "1/4", "4", "0");
        assert_eq!(
            brief(&p),
            vec![
                (36, "0".into(), "1/4".into(), 100),
                (43, "1/2".into(), "1/2".into(), 100),
                (36, "1".into(), "1/4".into(), 100),
                (34, "5/4".into(), "1/8".into(), 100),
                (36, "11/8".into(), "1/8".into(), 100),
                (36, "3/2".into(), "1/2".into(), 100),
                (51, "3/2".into(), "1/2".into(), 100),
                (55, "3/2".into(), "1/2".into(), 100),
                (36, "2".into(), "3/4".into(), 70),
            ]
        );
        assert_eq!(p.span, r("11/4"));
    }

    #[test]
    fn a_hold_lengthens_a_chord_across_a_bar_line_and_a_length_is_one_pitchs_or_the_chords() {
        let p = parsed("c2,g2 - - - | - . . .", "1", "4", "0");
        assert_eq!(brief(&p), vec![(36, "0".into(), "5".into(), 100), (43, "0".into(), "5".into(), 100)]);
        let p = parsed("c2:8,e4 . . . | . . . .", "1", "4", "0");
        assert_eq!(brief(&p), vec![(36, "0".into(), "8".into(), 100), (64, "0".into(), "1".into(), 100)]);
        let p = parsed("c2:2,g2:8", "1", "4", "0");
        assert_eq!(brief(&p), vec![(36, "0".into(), "2".into(), 100), (43, "0".into(), "8".into(), 100)]);
        // A hold inside a split is the split's share; after one, a cell.
        let p = parsed("[c2 -] - .", "1", "4", "0");
        assert_eq!(brief(&p), vec![(36, "0".into(), "2".into(), 100)]);
    }

    #[test]
    fn rows_are_voices_from_one_start_and_may_stop_short() {
        let p = parsed("e4 . g4 a4\nc2,g2:4", "1", "4", "4");
        assert_eq!(
            brief(&p),
            vec![
                (64, "4".into(), "1".into(), 100),
                (67, "6".into(), "1".into(), 100),
                (69, "7".into(), "1".into(), 100),
                (36, "4".into(), "4".into(), 100),
                (43, "4".into(), "4".into(), 100),
            ]
        );
        assert_eq!(p.span, r("4"));
        // The span is the longest row, rests counted, or a length past it.
        assert_eq!(parsed("c2\n. . . . . .", "1/2", "4", "0").span, r("3"));
        assert_eq!(parsed("c2:8 . .", "1/2", "4", "6").span, r("4"));
    }

    #[test]
    fn bar_lines_are_checked_in_any_meter_and_a_miscount_is_refused_where_it_is() {
        parsed("c2 . . . | c2 . . .", "1", "4", "0");
        parsed("c2 . . | c2 . .", "1", "3", "0");
        parsed("c2 . . . . . . . . . . . . . | c2", "1/4", "7/2", "0");
        parsed("| c2 . . . | c2 . . . ||", "1", "4", "0");
        assert_eq!(refused("c2 . . . | c2 . . | c2", "1", "4", "0"), "bar 2 has 3 cells; a bar of 4 beats on a 1 grid has 4");
        assert_eq!(refused("c2 . . . | c2 . . . . | c2", "1", "4", "0"), "bar 2 has 5 cells; a bar of 4 beats on a 1 grid has 4");
        assert_eq!(refused("c2 . . . | c2 . . . .", "1", "4", "0"), "bar 2 has 5 cells; a bar of 4 beats on a 1 grid has 4");
        assert_eq!(refused("c2 . . .\nc2 . . | c2", "1", "4", "0"), "row 2, bar 1 has 3 cells; a bar of 4 beats on a 1 grid has 4");
        assert_eq!(refused("c2 . . . | c2", "1", "4", "2"), "at 2 is not a bar's start, and the text has bar lines: they count whole bars from the clip's start");
        assert_eq!(refused("c2 . . | c2", "1", "7/2", "0"), "cell 4: bar lines need a bar of whole cells, and a bar of 7/2 beats is not whole cells of 1");
        // Without bar lines, a row is any length and at is any beat.
        assert_eq!(parsed("c2 . . . . . c2", "1", "4", "2").span, r("7"));
    }

    #[test]
    fn each_refusal_names_its_place_and_what_is_wanted() {
        assert_eq!(refused("c2 . . . | c2 . h2", "1", "4", "0"), "bar 2, cell 3: `h2` is not a pitch: a name with its octave, c2 or F#3, or a MIDI number");
        assert_eq!(refused("c2 . . . | c2 . h2\nc2", "1", "4", "0"), "row 1, bar 2, cell 3: `h2` is not a pitch: a name with its octave, c2 or F#3, or a MIDI number");
        for symbol in ["Am7", "Cmaj7", "Abm", "G"] {
            assert_eq!(refused(symbol, "1", "4", "0"), format!("bar 1, cell 1: `{symbol}` reads as a chord symbol, which is not written yet: write its notes, a2,c3,e3,g3"));
        }
        assert_eq!(refused("128", "1", "4", "0"), "bar 1, cell 1: `128` is outside MIDI notes 0 to 127");
        assert_eq!(refused("- c2", "1", "4", "0"), "bar 1, cell 1: `-` holds nothing: no note sounds before it");
        assert_eq!(refused("c2 . -", "1", "4", "0"), "bar 1, cell 3: `-` holds nothing: no note sounds before it");
        assert_eq!(refused("c2@0", "1", "4", "0"), "bar 1, cell 1: velocity after @ is 1 to 127, not `0`");
        assert_eq!(refused("c2@70,g2@80", "1", "4", "0"), "bar 1, cell 1: `c2@70,g2@80`: velocity goes once, at the end of the token");
        assert_eq!(refused("c2:0", "1", "4", "0"), "bar 1, cell 1: `c2:0`: a length after : is cells, 1 or more");
        assert_eq!(refused("c2,,g2", "1", "4", "0"), "bar 1, cell 1: `c2,,g2`: a chord is pitches joined by commas, c2,eb3,g3");
        assert_eq!(refused("[c2 d2", "1", "4", "0"), "bar 1, cell 1: a split [ ] is not closed");
        assert_eq!(refused("[]", "1", "4", "0"), "bar 1, cell 1: a split [ ] holds a token or more");
        assert_eq!(refused("[c2 [d2]]", "1", "4", "0"), "bar 1, cell 1: `[` cannot be inside a split");
        assert_eq!(refused("c2 ]", "1", "4", "0"), "bar 1, cell 2: a `]` closes no split");
        assert_eq!(refused("# nothing", "1", "4", "0"), "The text has no cells: a pitch such as c2, . for a rest, - to hold, [ ] to split a cell");
        assert_eq!(refused("c2", "0", "4", "0"), "The grid is beats, more than 0: 1/4 is a sixteenth note");
    }

    #[test]
    fn the_notation_prints_back_as_written() {
        for (text, grid) in [
            ("c2 . g2 . | c2 . a#1", "1"),
            ("c2,d#3,g3 - - - | c2@70 . . g1,g2", "1"),
            ("c2 . . . | c2 . . . | . . c2", "1"),
            ("e4 . g4 a4 . e4 . g4 | a#4 a4 . g4 . e4 . d4\nc2,g2:16", "1/2"),
        ] {
            let p = parsed(text, grid, "4", "0");
            let notes: Vec<Note> = p
                .notes
                .iter()
                .map(|w| note(w.pitch, &crate::fraction_str(&w.at), &crate::fraction_str(&w.duration), w.velocity))
                .collect();
            let out = printed(&notes, grid, "4");
            assert_eq!(out.text, text);
            assert_eq!(out.off_grid, Vec::<usize>::new());
        }
    }

    #[test]
    fn a_held_dyad_under_a_melody_prints_as_two_rows_melody_first() {
        let notes = vec![
            note(36, "0", "8", 100),
            note(43, "0", "8", 100),
            note(64, "0", "1", 100),
            note(67, "2", "1", 100),
            note(69, "3", "1", 100),
            note(64, "5", "1", 100),
            note(62, "7", "1", 100),
        ];
        assert_eq!(printed(&notes, "1", "4").text, "e4 . g4 a4 | . e4 . d4\nc2,g2:8");
        // Without bar lines, a length that stays is a row of holds.
        assert_eq!(printed(&notes, "1", "7/2").text, "e4 . g4 a4 . e4 . d4\nc2,g2 - - - - - - -");
    }

    #[test]
    fn notes_off_the_grid_are_written_at_their_nearest_cell_and_counted() {
        let notes = vec![note(60, "0", "1", 96), note(62, "1.975", "0.5", 100), note(64, "2.025", "1/3", 100), note(65, "7/3", "1/3", 100), note(50, "-1", "1", 100)];
        let out = printed(&notes, "1/4", "4");
        // The d4 a little ahead of beat 2 and the e4 a little behind land
        // on one cell with different lengths: the higher takes the row.
        assert_eq!(out.text, "c4@96 - - - . . . . e4 f4\n. . . . . . . . d4 -");
        assert_eq!(out.off_grid, vec![1, 2, 3, 4]);
        let out = printed(&notes[2..4], "1/3", "4");
        assert_eq!(out.text, ". . . . . . e4 f4");
        assert_eq!(out.off_grid, vec![0]);
        assert_eq!(printed(&[], "1/4", "4"), Printed { text: String::new(), off_grid: vec![] });
    }
}
