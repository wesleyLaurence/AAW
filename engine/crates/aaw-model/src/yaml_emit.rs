//! The canonical `song.yaml` writer: a port of PyYAML's `SafeDumper` with the
//! project's flow-style rule, `sort_keys=False`, `allow_unicode=True` and
//! `width=110`, the bytes every `song.yaml` has been written in.

use crate::pyfmt::float_repr;
use crate::value::Value;
use crate::yaml_load::{resolve_scalar, BOOL, FLOAT, INT, NULL, STR};

#[derive(Clone, Debug)]
enum Event {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    MappingStart { flow: bool },
    MappingEnd,
    SequenceStart { flow: bool },
    SequenceEnd,
    Scalar {
        tag: &'static str,
        value: String,
        implicit: (bool, bool),
    },
}

/// Which mappings the project writes in flow style.
/// It looks only at the keys, so it applies to any mapping with those keys.
fn flow_mapping(keys: &[&str]) -> bool {
    let has = |k: &str| keys.contains(&k);
    (has("pad") && has("at"))
        || has("pattern")
        || (has("id") && has("length_beats"))
        || has("type")
        || has("shape")
        || (has("to") && keys.iter().all(|k| ["to", "gain_db", "pre_fader"].contains(k)))
        || (has("at") && has("value"))
}

fn scalar(tag: &'static str, value: String) -> Event {
    // The serializer's implicit flags: plain if the text resolves back to its tag,
    // quoted if the default (string) tag is its tag.
    let implicit = (resolve_scalar(&value, true) == tag, tag == STR);
    Event::Scalar {
        tag,
        value,
        implicit,
    }
}

fn represent(v: &Value, events: &mut Vec<Event>) {
    match v {
        Value::None => events.push(scalar(NULL, "null".into())),
        Value::Bool(b) => events.push(scalar(BOOL, if *b { "true" } else { "false" }.into())),
        Value::Int(n) => events.push(scalar(INT, n.to_string())),
        Value::Float(f) => {
            let text = if f.is_nan() {
                ".nan".to_string()
            } else if f.is_infinite() {
                if *f > 0.0 { ".inf" } else { "-.inf" }.to_string()
            } else {
                let r = float_repr(*f).to_lowercase();
                if !r.contains('.') && r.contains('e') {
                    r.replacen('e', ".0e", 1)
                } else {
                    r
                }
            };
            events.push(scalar(FLOAT, text));
        }
        Value::Str(s) => events.push(scalar(STR, s.clone())),
        Value::List(items) => {
            events.push(Event::SequenceStart { flow: false });
            for item in items {
                represent(item, events);
            }
            events.push(Event::SequenceEnd);
        }
        Value::Dict(d) => {
            let keys: Vec<&str> = d.keys().filter_map(|k| k.as_str()).collect();
            events.push(Event::MappingStart {
                flow: flow_mapping(&keys),
            });
            for (k, v) in d {
                represent(&k.0, events);
                represent(v, events);
            }
            events.push(Event::MappingEnd);
        }
        Value::Bytes(_) | Value::Other(_) => unreachable!("dumps hold only JSON values"),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Style {
    Plain,
    Single,
    Double,
}

#[derive(Clone, Debug)]
struct Analysis {
    scalar: Vec<char>,
    empty: bool,
    multiline: bool,
    allow_flow_plain: bool,
    allow_block_plain: bool,
    allow_single_quoted: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    StreamStart,
    FirstDocumentStart,
    DocumentStart,
    DocumentEnd,
    DocumentRoot,
    Nothing,
    FirstFlowSequenceItem,
    FlowSequenceItem,
    FirstFlowMappingKey,
    FlowMappingKey,
    FlowMappingSimpleValue,
    FlowMappingValue,
    FirstBlockSequenceItem,
    BlockSequenceItem,
    FirstBlockMappingKey,
    BlockMappingKey,
    BlockMappingSimpleValue,
    BlockMappingValue,
}

fn tag_handle(tag: &str) -> String {
    format!("!!{}", tag.trim_start_matches("tag:yaml.org,2002:"))
}

fn is_break(c: char) -> bool {
    matches!(c, '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn is_space_or_break(c: char) -> bool {
    matches!(c, '\0' | ' ' | '\t' | '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

struct Emitter {
    out: String,
    events: Vec<Event>,
    pos: usize,
    states: Vec<State>,
    state: State,
    indents: Vec<Option<usize>>,
    indent: Option<usize>,
    flow_level: usize,
    root_context: bool,
    simple_key_context: bool,
    mapping_context: bool,
    column: usize,
    whitespace: bool,
    indention: bool,
    open_ended: bool,
    best_indent: usize,
    best_width: usize,
    analysis: Option<Analysis>,
    style: Option<Style>,
}

impl Emitter {
    fn event(&self) -> &Event {
        &self.events[self.pos]
    }

    fn next_event(&self) -> Option<&Event> {
        self.events.get(self.pos + 1)
    }

    fn run(&mut self) {
        while self.pos < self.events.len() {
            self.dispatch();
            self.pos += 1;
        }
    }

    fn pop_state(&mut self) {
        self.state = self.states.pop().expect("state stack");
    }

    fn increase_indent(&mut self, flow: bool, indentless: bool) {
        self.indents.push(self.indent);
        match self.indent {
            None => self.indent = Some(if flow { self.best_indent } else { 0 }),
            Some(i) if !indentless => self.indent = Some(i + self.best_indent),
            _ => {}
        }
    }

    fn dispatch(&mut self) {
        match self.state {
            State::StreamStart => {
                assert!(matches!(self.event(), Event::StreamStart));
                self.state = State::FirstDocumentStart;
            }
            State::FirstDocumentStart | State::DocumentStart => {
                match self.event() {
                    Event::DocumentStart => {
                        let first = self.state == State::FirstDocumentStart;
                        let implicit = first && !self.check_empty_document();
                        if !implicit {
                            self.write_indent();
                            self.write_indicator("---", true, false, false);
                        }
                        self.state = State::DocumentRoot;
                    }
                    Event::StreamEnd => {
                        if self.open_ended {
                            self.write_indicator("...", true, false, false);
                            self.write_indent();
                        }
                        self.state = State::Nothing;
                    }
                    e => panic!("expected DocumentStart, got {e:?}"),
                }
            }
            State::DocumentEnd => {
                assert!(matches!(self.event(), Event::DocumentEnd));
                self.write_indent();
                self.state = State::DocumentStart;
            }
            State::DocumentRoot => {
                self.states.push(State::DocumentEnd);
                self.expect_node(true, false, false);
            }
            State::Nothing => panic!("expected nothing"),
            State::FirstFlowSequenceItem | State::FlowSequenceItem => {
                if matches!(self.event(), Event::SequenceEnd) {
                    self.indent = self.indents.pop().expect("indent");
                    self.flow_level -= 1;
                    self.write_indicator("]", false, false, false);
                    self.pop_state();
                } else {
                    if self.state == State::FlowSequenceItem {
                        self.write_indicator(",", false, false, false);
                    }
                    if self.column > self.best_width {
                        self.write_indent();
                    }
                    self.states.push(State::FlowSequenceItem);
                    self.expect_node(false, false, false);
                }
            }
            State::FirstFlowMappingKey | State::FlowMappingKey => {
                if matches!(self.event(), Event::MappingEnd) {
                    self.indent = self.indents.pop().expect("indent");
                    self.flow_level -= 1;
                    self.write_indicator("}", false, false, false);
                    self.pop_state();
                } else {
                    if self.state == State::FlowMappingKey {
                        self.write_indicator(",", false, false, false);
                    }
                    if self.column > self.best_width {
                        self.write_indent();
                    }
                    if self.check_simple_key() {
                        self.states.push(State::FlowMappingSimpleValue);
                        self.expect_node(false, true, true);
                    } else {
                        self.write_indicator("?", true, false, false);
                        self.states.push(State::FlowMappingValue);
                        self.expect_node(false, true, false);
                    }
                }
            }
            State::FlowMappingSimpleValue => {
                self.write_indicator(":", false, false, false);
                self.states.push(State::FlowMappingKey);
                self.expect_node(false, true, false);
            }
            State::FlowMappingValue => {
                if self.column > self.best_width {
                    self.write_indent();
                }
                self.write_indicator(":", true, false, false);
                self.states.push(State::FlowMappingKey);
                self.expect_node(false, true, false);
            }
            State::FirstBlockSequenceItem | State::BlockSequenceItem => {
                if self.state == State::BlockSequenceItem
                    && matches!(self.event(), Event::SequenceEnd)
                {
                    self.indent = self.indents.pop().expect("indent");
                    self.pop_state();
                } else {
                    self.write_indent();
                    self.write_indicator("-", true, false, true);
                    self.states.push(State::BlockSequenceItem);
                    self.expect_node(false, false, false);
                }
            }
            State::FirstBlockMappingKey | State::BlockMappingKey => {
                if self.state == State::BlockMappingKey && matches!(self.event(), Event::MappingEnd)
                {
                    self.indent = self.indents.pop().expect("indent");
                    self.pop_state();
                } else {
                    self.write_indent();
                    if self.check_simple_key() {
                        self.states.push(State::BlockMappingSimpleValue);
                        self.expect_node(false, true, true);
                    } else {
                        self.write_indicator("?", true, false, true);
                        self.states.push(State::BlockMappingValue);
                        self.expect_node(false, true, false);
                    }
                }
            }
            State::BlockMappingSimpleValue => {
                self.write_indicator(":", false, false, false);
                self.states.push(State::BlockMappingKey);
                self.expect_node(false, true, false);
            }
            State::BlockMappingValue => {
                self.write_indent();
                self.write_indicator(":", true, false, true);
                self.states.push(State::BlockMappingKey);
                self.expect_node(false, true, false);
            }
        }
    }

    fn expect_node(&mut self, root: bool, mapping: bool, simple_key: bool) {
        self.root_context = root;
        self.mapping_context = mapping;
        self.simple_key_context = simple_key;
        match self.event().clone() {
            Event::Scalar { .. } => {
                self.process_tag();
                // expect_scalar
                self.increase_indent(true, false);
                self.process_scalar();
                self.indent = self.indents.pop().expect("indent");
                self.pop_state();
            }
            Event::SequenceStart { flow } => {
                if self.flow_level > 0 || flow || self.check_empty_sequence() {
                    self.write_indicator("[", true, true, false);
                    self.flow_level += 1;
                    self.increase_indent(true, false);
                    self.state = State::FirstFlowSequenceItem;
                } else {
                    let indentless = self.mapping_context && !self.indention;
                    self.increase_indent(false, indentless);
                    self.state = State::FirstBlockSequenceItem;
                }
            }
            Event::MappingStart { flow } => {
                if self.flow_level > 0 || flow || self.check_empty_mapping() {
                    self.write_indicator("{", true, true, false);
                    self.flow_level += 1;
                    self.increase_indent(true, false);
                    self.state = State::FirstFlowMappingKey;
                } else {
                    self.increase_indent(false, false);
                    self.state = State::FirstBlockMappingKey;
                }
            }
            e => panic!("expected a node, got {e:?}"),
        }
    }

    fn check_empty_sequence(&self) -> bool {
        matches!(self.event(), Event::SequenceStart { .. })
            && matches!(self.next_event(), Some(Event::SequenceEnd))
    }

    fn check_empty_mapping(&self) -> bool {
        matches!(self.event(), Event::MappingStart { .. })
            && matches!(self.next_event(), Some(Event::MappingEnd))
    }

    /// PyYAML's check needs an untagged scalar; the serializer tags every scalar.
    fn check_empty_document(&self) -> bool {
        false
    }

    fn check_simple_key(&mut self) -> bool {
        // The prepared tag counts toward the limit even when it is not written.
        if let Event::Scalar { value, tag, .. } = self.event().clone() {
            if self.analysis.is_none() {
                self.analysis = Some(analyze_scalar(&value));
            }
            let a = self.analysis.as_ref().expect("analysis");
            let length = a.scalar.len() + tag_handle(tag).len();
            return length < 128 && !a.empty && !a.multiline;
        }
        "!!map".len() < 128 && (self.check_empty_sequence() || self.check_empty_mapping())
    }

    fn process_tag(&mut self) {
        let Event::Scalar { tag, implicit, .. } = self.event().clone() else {
            return;
        };
        if self.style.is_none() {
            self.style = Some(self.choose_scalar_style());
        }
        let style = self.style.expect("style");
        if (style == Style::Plain && implicit.0) || (style != Style::Plain && implicit.1) {
            return;
        }
        self.write_indicator(&tag_handle(tag), true, false, false);
    }

    fn choose_scalar_style(&mut self) -> Style {
        let Event::Scalar { value, implicit, .. } = self.event().clone() else {
            unreachable!()
        };
        if self.analysis.is_none() {
            self.analysis = Some(analyze_scalar(&value));
        }
        let a = self.analysis.as_ref().expect("analysis");
        if implicit.0
            && !(self.simple_key_context && (a.empty || a.multiline))
            && ((self.flow_level > 0 && a.allow_flow_plain)
                || (self.flow_level == 0 && a.allow_block_plain))
        {
            return Style::Plain;
        }
        if a.allow_single_quoted && !(self.simple_key_context && a.multiline) {
            return Style::Single;
        }
        Style::Double
    }

    fn process_scalar(&mut self) {
        let Event::Scalar { value, .. } = self.event().clone() else {
            unreachable!()
        };
        if self.analysis.is_none() {
            self.analysis = Some(analyze_scalar(&value));
        }
        if self.style.is_none() {
            self.style = Some(self.choose_scalar_style());
        }
        let split = !self.simple_key_context;
        let text = self.analysis.take().expect("analysis").scalar;
        match self.style.take().expect("style") {
            Style::Double => self.write_double_quoted(&text, split),
            Style::Single => self.write_single_quoted(&text, split),
            Style::Plain => self.write_plain(&text, split),
        }
    }

    fn write(&mut self, data: &[char]) {
        self.column += data.len();
        self.out.extend(data.iter());
    }

    fn write_indicator(&mut self, indicator: &str, need_whitespace: bool, whitespace: bool, indention: bool) {
        let data = if self.whitespace || !need_whitespace {
            indicator.to_string()
        } else {
            format!(" {indicator}")
        };
        self.whitespace = whitespace;
        self.indention = self.indention && indention;
        self.column += data.chars().count();
        self.open_ended = false;
        self.out.push_str(&data);
    }

    fn write_indent(&mut self) {
        let indent = self.indent.unwrap_or(0);
        if !self.indention || self.column > indent || (self.column == indent && !self.whitespace) {
            self.write_line_break(None);
        }
        if self.column < indent {
            self.whitespace = true;
            self.out.push_str(&" ".repeat(indent - self.column));
            self.column = indent;
        }
    }

    fn write_line_break(&mut self, data: Option<char>) {
        self.whitespace = true;
        self.indention = true;
        self.column = 0;
        self.out.push(data.unwrap_or('\n'));
    }

    fn write_single_quoted(&mut self, text: &[char], split: bool) {
        self.write_indicator("'", true, false, false);
        let mut spaces = false;
        let mut breaks = false;
        let (mut start, mut end) = (0, 0);
        while end <= text.len() {
            let ch = text.get(end).copied();
            if spaces {
                if ch != Some(' ') {
                    if start + 1 == end
                        && self.column > self.best_width
                        && split
                        && start != 0
                        && end != text.len()
                    {
                        self.write_indent();
                    } else {
                        self.write(&text[start..end]);
                    }
                    start = end;
                }
            } else if breaks {
                if ch.is_none_or(|c| !is_break(c)) {
                    if text[start] == '\n' {
                        self.write_line_break(None);
                    }
                    for &br in &text[start..end] {
                        self.write_line_break(Some(br));
                    }
                    self.write_indent();
                    start = end;
                }
            } else if (ch.is_none_or(|c| c == ' ' || is_break(c) || c == '\'')) && start < end {
                self.write(&text[start..end]);
                start = end;
            }
            if ch == Some('\'') {
                self.column += 2;
                self.out.push_str("''");
                start = end + 1;
            }
            if let Some(c) = ch {
                spaces = c == ' ';
                breaks = is_break(c);
            }
            end += 1;
        }
        self.write_indicator("'", false, false, false);
    }

    fn write_double_quoted(&mut self, text: &[char], split: bool) {
        self.write_indicator("\"", true, false, false);
        let (mut start, mut end) = (0, 0);
        while end <= text.len() {
            let ch = text.get(end).copied();
            let escape = match ch {
                None => true,
                Some(c) => {
                    matches!(c, '"' | '\\' | '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{feff}')
                        || !((' '..='~').contains(&c)
                            || ('\u{a0}'..='\u{d7ff}').contains(&c)
                            || ('\u{e000}'..='\u{fffd}').contains(&c))
                }
            };
            if escape {
                if start < end {
                    self.write(&text[start..end]);
                    start = end;
                }
                if let Some(c) = ch {
                    let data = match c {
                        '\0' => "\\0".to_string(),
                        '\u{07}' => "\\a".to_string(),
                        '\u{08}' => "\\b".to_string(),
                        '\t' => "\\t".to_string(),
                        '\n' => "\\n".to_string(),
                        '\u{0b}' => "\\v".to_string(),
                        '\u{0c}' => "\\f".to_string(),
                        '\r' => "\\r".to_string(),
                        '\u{1b}' => "\\e".to_string(),
                        '"' => "\\\"".to_string(),
                        '\\' => "\\\\".to_string(),
                        '\u{85}' => "\\N".to_string(),
                        '\u{a0}' => "\\_".to_string(),
                        '\u{2028}' => "\\L".to_string(),
                        '\u{2029}' => "\\P".to_string(),
                        c if (c as u32) <= 0xff => format!("\\x{:02X}", c as u32),
                        c if (c as u32) <= 0xffff => format!("\\u{:04X}", c as u32),
                        c => format!("\\U{:08X}", c as u32),
                    };
                    self.column += data.len();
                    self.out.push_str(&data);
                    start = end + 1;
                }
            }
            if 0 < end
                && end + 1 < text.len()
                && (ch == Some(' ') || start >= end)
                && self.column as i64 + end as i64 - start as i64 > self.best_width as i64
                && split
            {
                let mut data: Vec<char> = if start < end {
                    text[start..end].to_vec()
                } else {
                    Vec::new()
                };
                data.push('\\');
                if start < end {
                    start = end;
                }
                self.write(&data);
                self.write_indent();
                self.whitespace = false;
                self.indention = false;
                if text[start] == ' ' {
                    self.write(&['\\']);
                }
            }
            end += 1;
        }
        self.write_indicator("\"", false, false, false);
    }

    fn write_plain(&mut self, text: &[char], split: bool) {
        if self.root_context {
            self.open_ended = true;
        }
        if text.is_empty() {
            return;
        }
        if !self.whitespace {
            self.write(&[' ']);
        }
        self.whitespace = false;
        self.indention = false;
        let mut spaces = false;
        let mut breaks = false;
        let (mut start, mut end) = (0, 0);
        while end <= text.len() {
            let ch = text.get(end).copied();
            if spaces {
                if ch != Some(' ') {
                    if start + 1 == end && self.column > self.best_width && split {
                        self.write_indent();
                        self.whitespace = false;
                        self.indention = false;
                    } else {
                        self.write(&text[start..end]);
                    }
                    start = end;
                }
            } else if breaks {
                if ch.is_none_or(|c| !is_break(c)) {
                    if text[start] == '\n' {
                        self.write_line_break(None);
                    }
                    for &br in &text[start..end] {
                        self.write_line_break(Some(br));
                    }
                    self.write_indent();
                    self.whitespace = false;
                    self.indention = false;
                    start = end;
                }
            } else if ch.is_none_or(|c| c == ' ' || is_break(c)) {
                self.write(&text[start..end]);
                start = end;
            }
            if let Some(c) = ch {
                spaces = c == ' ';
                breaks = is_break(c);
            }
            end += 1;
        }
    }
}

fn analyze_scalar(scalar: &str) -> Analysis {
    let text: Vec<char> = scalar.chars().collect();
    if text.is_empty() {
        return Analysis {
            scalar: text,
            empty: true,
            multiline: false,
            allow_flow_plain: false,
            allow_block_plain: true,
            allow_single_quoted: true,
        };
    }
    let mut block_indicators = false;
    let mut flow_indicators = false;
    let mut line_breaks = false;
    let mut special_characters = false;
    let mut leading_space = false;
    let mut leading_break = false;
    let mut trailing_space = false;
    let mut trailing_break = false;
    let mut break_space = false;
    let mut space_break = false;
    if scalar.starts_with("---") || scalar.starts_with("...") {
        block_indicators = true;
        flow_indicators = true;
    }
    let mut preceded_by_whitespace = true;
    let mut followed_by_whitespace = text.len() == 1 || is_space_or_break(text[1]);
    let mut previous_space = false;
    let mut previous_break = false;
    let n = text.len();
    for (index, &ch) in text.iter().enumerate() {
        if index == 0 {
            if "#,[]{}&*!|>'\"%@`".contains(ch) {
                flow_indicators = true;
                block_indicators = true;
            }
            if ch == '?' || ch == ':' {
                flow_indicators = true;
                if followed_by_whitespace {
                    block_indicators = true;
                }
            }
            if ch == '-' && followed_by_whitespace {
                flow_indicators = true;
                block_indicators = true;
            }
        } else {
            if ",?[]{}".contains(ch) {
                flow_indicators = true;
            }
            if ch == ':' {
                flow_indicators = true;
                if followed_by_whitespace {
                    block_indicators = true;
                }
            }
            if ch == '#' && preceded_by_whitespace {
                flow_indicators = true;
                block_indicators = true;
            }
        }
        if is_break(ch) {
            line_breaks = true;
        }
        if !(ch == '\n' || (' '..='~').contains(&ch)) {
            let unicode = (ch == '\u{85}'
                || ('\u{a0}'..='\u{d7ff}').contains(&ch)
                || ('\u{e000}'..='\u{fffd}').contains(&ch)
                || ('\u{10000}'..'\u{10ffff}').contains(&ch))
                && ch != '\u{feff}';
            if !unicode {
                special_characters = true;
            }
        }
        if ch == ' ' {
            if index == 0 {
                leading_space = true;
            }
            if index == n - 1 {
                trailing_space = true;
            }
            if previous_break {
                break_space = true;
            }
            previous_space = true;
            previous_break = false;
        } else if is_break(ch) {
            if index == 0 {
                leading_break = true;
            }
            if index == n - 1 {
                trailing_break = true;
            }
            if previous_space {
                space_break = true;
            }
            previous_space = false;
            previous_break = true;
        } else {
            previous_space = false;
            previous_break = false;
        }
        preceded_by_whitespace = is_space_or_break(ch);
        followed_by_whitespace = index + 2 >= n || is_space_or_break(text[index + 2]);
    }
    let mut allow_flow_plain = true;
    let mut allow_block_plain = true;
    let mut allow_single_quoted = true;
    if leading_space || leading_break || trailing_space || trailing_break {
        allow_flow_plain = false;
        allow_block_plain = false;
    }
    if break_space {
        allow_flow_plain = false;
        allow_block_plain = false;
        allow_single_quoted = false;
    }
    if space_break || special_characters {
        allow_flow_plain = false;
        allow_block_plain = false;
        allow_single_quoted = false;
    }
    if line_breaks {
        allow_flow_plain = false;
        allow_block_plain = false;
    }
    if flow_indicators {
        allow_flow_plain = false;
    }
    if block_indicators {
        allow_block_plain = false;
    }
    Analysis {
        scalar: text,
        empty: false,
        multiline: line_breaks,
        allow_flow_plain,
        allow_block_plain,
        allow_single_quoted,
    }
}

/// `yaml.dump(value, Dumper=ProjectDumper, sort_keys=False, allow_unicode=True,
/// width=110)`.
pub fn dump(value: &Value) -> String {
    let mut events = vec![Event::StreamStart, Event::DocumentStart];
    represent(value, &mut events);
    events.extend([Event::DocumentEnd, Event::StreamEnd]);
    let mut emitter = Emitter {
        out: String::new(),
        events,
        pos: 0,
        states: Vec::new(),
        state: State::StreamStart,
        indents: Vec::new(),
        indent: None,
        flow_level: 0,
        root_context: false,
        simple_key_context: false,
        mapping_context: false,
        column: 0,
        whitespace: true,
        indention: true,
        open_ended: false,
        best_indent: 2,
        best_width: 110,
        analysis: None,
        style: None,
    };
    emitter.run();
    emitter.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml_load::load;

    #[test]
    fn writes_block_and_flow_like_pyyaml() {
        let v = load(
            "schema_version: 1\nsession: {tempo: 120.0}\ntracks:\n- id: a\n  pads: {}\n  clips:\n  - {pattern: p, at: '1/3'}\n",
        )
        .unwrap();
        assert_eq!(
            dump(&v),
            "schema_version: 1\nsession:\n  tempo: 120.0\ntracks:\n- id: a\n  pads: {}\n  clips:\n  - {pattern: p, at: 1/3}\n"
        );
    }

    #[test]
    fn quotes_strings_that_would_resolve_otherwise() {
        let v = load("[a, '16', 'yes', '', ' x', 'a: b', '#c', 'é', \"t\\tb\", '1e5']").unwrap();
        assert_eq!(
            dump(&v),
            "- a\n- '16'\n- 'yes'\n- ''\n- ' x'\n- 'a: b'\n- '#c'\n- é\n- \"t\\tb\"\n- 1e5\n"
        );
    }
}
