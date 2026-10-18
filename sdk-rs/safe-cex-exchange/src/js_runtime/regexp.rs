//! The host RegExp: a backtracking matcher over UTF-16 for the non-unicode
//! syntax subset the ported bodies use, plus exec/test/toString/@@replace.
//! Unsupported syntax or flags throw a SyntaxError rather than misbehaving.
use super::{
    ops::to_primitive,
    value::{Js, Str, Thrown, arg},
};
use std::rc::Rc;

fn to_string(value: &Js) -> Result<Str, Thrown> {
    to_primitive(value)?.to_js_string()
}

/// ToLength, clamped to the addressable index range.
fn to_length(value: &Js) -> Result<usize, Thrown> {
    let n = value.to_number()?;
    Ok(if n.is_nan() || n <= 0.0 {
        0
    } else if n.is_infinite() {
        usize::MAX
    } else {
        n.trunc().min(usize::MAX as f64) as usize
    })
}

enum ClassItem {
    Range(u16, u16),
    Digit(bool),
    Space(bool),
    Word(bool),
}

enum Node {
    Char(u16),
    Any,
    Class(Vec<ClassItem>, bool),
    Start,
    End,
    WordBoundary(bool),
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    Group(Box<Node>, Option<usize>),
    Repeat {
        node: Box<Node>,
        min: usize,
        max: usize,
        greedy: bool,
    },
}

fn is_line_terminator(c: u16) -> bool {
    matches!(c, 0x0A | 0x0D | 0x2028 | 0x2029)
}

pub(super) fn is_space(c: u16) -> bool {
    matches!(
        c,
        0x09..=0x0D | 0x20 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F
            | 0x3000 | 0xFEFF
    )
}

fn is_word(c: u16) -> bool {
    matches!(c, 0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A | 0x5F)
}

/// Non-unicode Canonicalize: single-unit toUpperCase that never maps non-ASCII to ASCII.
fn canonicalize(c: u16) -> u16 {
    let Some(ch) = char::from_u32(u32::from(c)) else {
        return c;
    };
    let mut upper = ch.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) if (u as u32) <= 0xFFFF && !(c >= 128 && (u as u32) < 128) => u as u16,
        _ => c,
    }
}

fn lowercase(c: u16) -> u16 {
    char::from_u32(u32::from(c))
        .and_then(|ch| {
            let mut lower = ch.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) if (l as u32) <= 0xFFFF => Some(l as u16),
                _ => None,
            }
        })
        .unwrap_or(c)
}

struct Parser<'p> {
    src: &'p [u16],
    pos: usize,
    groups: usize,
    icase: bool,
}

type ParseResult<T> = Result<T, &'static str>;

impl Parser<'_> {
    fn peek(&self) -> Option<u16> {
        self.src.get(self.pos).copied()
    }
    fn peek_is(&self, c: char) -> bool {
        self.peek() == Some(c as u16)
    }
    fn eat(&mut self, c: char) -> bool {
        let hit = self.peek_is(c);
        if hit {
            self.pos += 1;
        }
        hit
    }
    fn next(&mut self) -> Option<u16> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }
    fn literal(&self, c: u16) -> Node {
        Node::Char(if self.icase { canonicalize(c) } else { c })
    }
    fn alternation(&mut self) -> ParseResult<Node> {
        let mut alternatives = vec![self.sequence()?];
        while self.eat('|') {
            alternatives.push(self.sequence()?);
        }
        Ok(if alternatives.len() == 1 {
            alternatives.remove(0)
        } else {
            Node::Alt(alternatives)
        })
    }
    fn sequence(&mut self) -> ParseResult<Node> {
        let mut items = vec![];
        while let Some(c) = self.peek() {
            if c == u16::from(b'|') || c == u16::from(b')') {
                break;
            }
            let atom = self.atom()?;
            items.push(self.quantified(atom)?);
        }
        Ok(Node::Seq(items))
    }
    fn decimal(&mut self) -> Option<usize> {
        let start = self.pos;
        while self.peek().is_some_and(|c| (0x30..=0x39).contains(&c)) {
            self.pos += 1;
        }
        if start == self.pos {
            return None;
        }
        Some(
            String::from_utf16_lossy(&self.src[start..self.pos])
                .parse()
                .unwrap_or(usize::MAX),
        )
    }
    /// `{n}`, `{n,}` or `{n,m}`; anything else leaves `{` as a literal (Annex B).
    fn braces(&mut self) -> Option<(usize, usize)> {
        let start = self.pos;
        self.pos += 1;
        let bounds = (|| {
            let min = self.decimal()?;
            let max = if self.eat(',') {
                self.decimal().unwrap_or(usize::MAX)
            } else {
                min
            };
            self.eat('}').then_some((min, max))
        })();
        if bounds.is_none() {
            self.pos = start;
        }
        bounds
    }
    fn quantified(&mut self, atom: Node) -> ParseResult<Node> {
        let simple = match self.peek().map(ascii) {
            Some('*') => Some((0, usize::MAX)),
            Some('+') => Some((1, usize::MAX)),
            Some('?') => Some((0, 1)),
            Some('{') => None,
            _ => return Ok(atom),
        };
        let (min, max) = match simple {
            Some(bounds) => {
                self.pos += 1;
                bounds
            }
            None => match self.braces() {
                Some(bounds) => bounds,
                None => return Ok(atom),
            },
        };
        if matches!(atom, Node::Start | Node::End | Node::WordBoundary(_)) {
            return Err("Nothing to repeat");
        }
        if min > max {
            return Err("numbers out of order in {} quantifier");
        }
        let greedy = !self.eat('?');
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
        })
    }
    fn atom(&mut self) -> ParseResult<Node> {
        let c = self.next().ok_or("Unexpected end of pattern")?;
        Ok(match ascii(c) {
            '(' => {
                let index = if self.eat('?') {
                    if !self.eat(':') {
                        return Err(
                            "Invalid group (lookaround and named groups are not supported by the host RegExp)",
                        );
                    }
                    None
                } else {
                    self.groups += 1;
                    Some(self.groups)
                };
                let inner = self.alternation()?;
                if !self.eat(')') {
                    return Err("Unterminated group");
                }
                Node::Group(Box::new(inner), index)
            }
            '[' => self.class()?,
            '.' => Node::Any,
            '^' => Node::Start,
            '$' => Node::End,
            '*' | '+' | '?' => return Err("Nothing to repeat"),
            '{' => {
                self.pos -= 1;
                if self.braces().is_some() {
                    return Err("Nothing to repeat");
                }
                self.pos += 1;
                self.literal(c)
            }
            '\\' => self.escape()?,
            _ => self.literal(c),
        })
    }
    fn escape(&mut self) -> ParseResult<Node> {
        let c = self.next().ok_or("\\ at end of pattern")?;
        Ok(match ascii(c) {
            '\u{FFFD}' => self.literal(c),
            'd' | 'D' | 's' | 'S' | 'w' | 'W' => Node::Class(vec![set_escape(c)], false),
            'b' => Node::WordBoundary(false),
            'B' => Node::WordBoundary(true),
            '1'..='9' => return Err("Backreferences are not supported by the host RegExp"),
            _ => {
                let unit = self.char_escape(c)?;
                self.literal(unit)
            }
        })
    }
    fn hex(&mut self, digits: usize) -> Option<u16> {
        let text = self.src.get(self.pos..self.pos + digits)?;
        let value = u16::from_str_radix(&String::from_utf16(text).ok()?, 16).ok()?;
        self.pos += digits;
        Some(value)
    }
    fn char_escape(&mut self, c: u16) -> ParseResult<u16> {
        Ok(match ascii(c) {
            't' => 0x09,
            'n' => 0x0A,
            'v' => 0x0B,
            'f' => 0x0C,
            'r' => 0x0D,
            '0' if !self.peek().is_some_and(|d| (0x30..=0x39).contains(&d)) => 0,
            '0' => return Err("Octal escapes are not supported by the host RegExp"),
            'x' => self.hex(2).unwrap_or(c),
            'u' => self.hex(4).unwrap_or(c),
            'c' => match self.peek() {
                Some(l) if ascii(l).is_ascii_alphabetic() => {
                    self.pos += 1;
                    l % 32
                }
                // Annex B: a bare `\c` is a literal backslash; `c` is reparsed.
                _ => {
                    self.pos -= 1;
                    u16::from(b'\\')
                }
            },
            _ => c,
        })
    }
    fn class_atom(&mut self) -> ParseResult<Result<u16, ClassItem>> {
        let c = self.next().ok_or("Unterminated character class")?;
        if c != u16::from(b'\\') {
            return Ok(Ok(c));
        }
        let e = self.next().ok_or("\\ at end of pattern")?;
        Ok(match ascii(e) {
            'd' | 'D' | 's' | 'S' | 'w' | 'W' => Err(set_escape(e)),
            'b' => Ok(0x08),
            '-' => Ok(e),
            _ => Ok(self.char_escape(e)?),
        })
    }
    fn class(&mut self) -> ParseResult<Node> {
        let negated = self.eat('^');
        let mut items = vec![];
        loop {
            if self.eat(']') {
                break;
            }
            let low = self.class_atom()?;
            let range = self.peek_is('-')
                && self
                    .src
                    .get(self.pos + 1)
                    .is_some_and(|c| *c != u16::from(b']'));
            if !range {
                items.push(class_item(low));
                continue;
            }
            self.pos += 1;
            let high = self.class_atom()?;
            match (low, high) {
                (Ok(low), Ok(high)) => {
                    if low > high {
                        return Err("Range out of order in character class");
                    }
                    items.push(ClassItem::Range(low, high));
                }
                (low, high) => {
                    items.push(class_item(low));
                    items.push(ClassItem::Range(0x2D, 0x2D));
                    items.push(class_item(high));
                }
            }
        }
        Ok(Node::Class(items, negated))
    }
}

fn set_escape(c: u16) -> ClassItem {
    let negated = (c as u8).is_ascii_uppercase();
    match (c as u8).to_ascii_lowercase() {
        b'd' => ClassItem::Digit(negated),
        b's' => ClassItem::Space(negated),
        _ => ClassItem::Word(negated),
    }
}

fn class_item(atom: Result<u16, ClassItem>) -> ClassItem {
    atom.map_or_else(|set| set, |c| ClassItem::Range(c, c))
}

/// ASCII view of a pattern unit; non-ASCII never aliases a syntax character.
fn ascii(c: u16) -> char {
    if c < 0x80 {
        c as u8 as char
    } else {
        '\u{FFFD}'
    }
}

const MAX_DEPTH: usize = 20_000;

struct Matcher<'a> {
    input: &'a [u16],
    icase: bool,
    multiline: bool,
    dot_all: bool,
    captures: Vec<Option<(usize, usize)>>,
    depth: usize,
    overflow: bool,
}

type Continuation<'k> = &'k mut dyn FnMut(&mut Matcher<'_>, usize) -> bool;

impl Matcher<'_> {
    fn class_hit(&self, items: &[ClassItem], c: u16) -> bool {
        let hit = |c: u16| {
            items.iter().any(|item| match item {
                ClassItem::Range(low, high) => (*low..=*high).contains(&c),
                ClassItem::Digit(neg) => (0x30..=0x39).contains(&c) != *neg,
                ClassItem::Space(neg) => is_space(c) != *neg,
                ClassItem::Word(neg) => is_word(c) != *neg,
            })
        };
        let lower = lowercase(c);
        hit(c)
            || (self.icase
                && (hit(canonicalize(c)) || (canonicalize(lower) == canonicalize(c) && hit(lower))))
    }
    fn unit(&self, pos: usize) -> u16 {
        let c = self.input[pos];
        if self.icase { canonicalize(c) } else { c }
    }
    fn node(&mut self, node: &Node, pos: usize, k: Continuation<'_>) -> bool {
        if self.depth >= MAX_DEPTH {
            self.overflow = true;
            return false;
        }
        self.depth += 1;
        let len = self.input.len();
        let matched = match node {
            Node::Char(c) => pos < len && self.unit(pos) == *c && k(self, pos + 1),
            Node::Any => {
                pos < len
                    && (self.dot_all || !is_line_terminator(self.input[pos]))
                    && k(self, pos + 1)
            }
            Node::Class(items, negated) => {
                pos < len && self.class_hit(items, self.input[pos]) != *negated && k(self, pos + 1)
            }
            Node::Start => {
                (pos == 0 || (self.multiline && is_line_terminator(self.input[pos - 1])))
                    && k(self, pos)
            }
            Node::End => {
                (pos == len || (self.multiline && is_line_terminator(self.input[pos])))
                    && k(self, pos)
            }
            Node::WordBoundary(negated) => {
                let before = pos > 0 && is_word(self.input[pos - 1]);
                let after = pos < len && is_word(self.input[pos]);
                ((before != after) != *negated) && k(self, pos)
            }
            Node::Seq(items) => self.sequence(items, pos, k),
            Node::Alt(alternatives) => {
                let mut hit = false;
                for alternative in alternatives {
                    if self.node(alternative, pos, k) {
                        hit = true;
                        break;
                    }
                }
                hit
            }
            Node::Group(inner, None) => self.node(inner, pos, k),
            Node::Group(inner, Some(index)) => {
                let index = *index;
                self.node(inner, pos, &mut |m: &mut Matcher<'_>, end| {
                    let previous = m.captures[index];
                    m.captures[index] = Some((pos, end));
                    if k(m, end) {
                        return true;
                    }
                    m.captures[index] = previous;
                    false
                })
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => self.repeat(node, (*min, *max, *greedy), 0, pos, k),
        };
        self.depth -= 1;
        matched
    }
    fn sequence(&mut self, items: &[Node], pos: usize, k: Continuation<'_>) -> bool {
        match items.split_first() {
            None => k(self, pos),
            Some((first, rest)) => self.node(first, pos, &mut |m: &mut Matcher<'_>, next| {
                m.sequence(rest, next, k)
            }),
        }
    }
    fn repeat(
        &mut self,
        node: &Node,
        bounds: (usize, usize, bool),
        count: usize,
        pos: usize,
        k: Continuation<'_>,
    ) -> bool {
        let (min, max, greedy) = bounds;
        if count < min {
            return self.node(node, pos, &mut |m: &mut Matcher<'_>, next| {
                m.repeat(node, bounds, count + 1, next, k)
            });
        }
        if count >= max {
            return k(self, pos);
        }
        if !greedy && k(self, pos) {
            return true;
        }
        if self.node(node, pos, &mut |m: &mut Matcher<'_>, next| {
            next != pos && m.repeat(node, bounds, count + 1, next, k)
        }) {
            return true;
        }
        greedy && k(self, pos)
    }
}

struct Program {
    root: Node,
    groups: usize,
    source: Vec<u16>,
    flags: String,
    global: bool,
    sticky: bool,
    icase: bool,
    multiline: bool,
    dot_all: bool,
}

/// One successful match: overall span plus each capture group's span.
struct Match {
    start: usize,
    end: usize,
    captures: Vec<Option<(usize, usize)>>,
}

impl Program {
    fn compile(pattern: &[u16], flags: &[u16]) -> Result<Self, Thrown> {
        let display = || {
            format!(
                "/{}/{}",
                String::from_utf16_lossy(pattern),
                String::from_utf16_lossy(flags)
            )
        };
        let mut seen = String::new();
        for unit in flags {
            let flag = char::from_u32(u32::from(*unit)).unwrap_or('\u{FFFD}');
            if !"gimsy".contains(flag) || seen.contains(flag) {
                return Err(Thrown::error(
                    "SyntaxError",
                    &format!(
                        "Invalid regular expression flags '{}' (host RegExp supports g, i, m, s, y)",
                        String::from_utf16_lossy(flags)
                    ),
                ));
            }
            seen.push(flag);
        }
        let icase = seen.contains('i');
        let mut parser = Parser {
            src: pattern,
            pos: 0,
            groups: 0,
            icase,
        };
        let root = parser
            .alternation()
            .and_then(|root| {
                if parser.pos < pattern.len() {
                    Err("Unmatched ')'")
                } else {
                    Ok(root)
                }
            })
            .map_err(|reason| {
                Thrown::error(
                    "SyntaxError",
                    &format!("Invalid regular expression: {}: {reason}", display()),
                )
            })?;
        let flags: String = "gimsy".chars().filter(|f| seen.contains(*f)).collect();
        Ok(Self {
            root,
            groups: parser.groups,
            source: if pattern.is_empty() {
                "(?:)".encode_utf16().collect()
            } else {
                pattern.to_vec()
            },
            global: flags.contains('g'),
            sticky: flags.contains('y'),
            multiline: flags.contains('m'),
            dot_all: flags.contains('s'),
            flags,
            icase,
        })
    }
    fn match_at(&self, input: &[u16], start: usize) -> Result<Option<Match>, Thrown> {
        let mut matcher = Matcher {
            input,
            icase: self.icase,
            multiline: self.multiline,
            dot_all: self.dot_all,
            captures: vec![None; self.groups + 1],
            depth: 0,
            overflow: false,
        };
        let mut end = None;
        matcher.node(&self.root, start, &mut |_: &mut Matcher<'_>, at| {
            end = Some(at);
            true
        });
        if matcher.overflow {
            return Err(Thrown::error(
                "RangeError",
                "Maximum call stack size exceeded",
            ));
        }
        Ok(end.map(|end| Match {
            start,
            end,
            captures: matcher.captures,
        }))
    }
    /// RegExpBuiltinExec: lastIndex is read, honored and written for g/y only.
    fn exec(&self, this: &Js, input: &[u16]) -> Result<Option<Match>, Thrown> {
        let last_index = to_length(&this.get("lastIndex")?)?;
        let tracks = self.global || self.sticky;
        let mut start = if tracks { last_index } else { 0 };
        loop {
            if start > input.len() {
                if tracks {
                    this.set("lastIndex", Js::Number(0.0))?;
                }
                return Ok(None);
            }
            if let Some(found) = self.match_at(input, start)? {
                if tracks {
                    this.set("lastIndex", Js::Number(found.end as f64))?;
                }
                return Ok(Some(found));
            }
            if self.sticky {
                this.set("lastIndex", Js::Number(0.0))?;
                return Ok(None);
            }
            start += 1;
        }
    }
}

/// GetSubstitution for a string replacement template.
fn substitute(template: &[u16], input: &[u16], found: &Match) -> Vec<u16> {
    let mut out = vec![];
    let mut i = 0;
    let dollar = u16::from(b'$');
    let digit = |c: Option<&u16>| {
        c.filter(|c| (0x30..=0x39).contains(*c))
            .map(|c| usize::from(c - 0x30))
    };
    while i < template.len() {
        let c = template[i];
        let next = template.get(i + 1).copied();
        if c != dollar || next.is_none() {
            out.push(c);
            i += 1;
            continue;
        }
        match next.map(ascii) {
            Some('$') => {
                out.push(dollar);
                i += 2;
            }
            Some('&') => {
                out.extend_from_slice(&input[found.start..found.end]);
                i += 2;
            }
            Some('`') => {
                out.extend_from_slice(&input[..found.start]);
                i += 2;
            }
            Some('\'') => {
                out.extend_from_slice(&input[found.end..]);
                i += 2;
            }
            _ => {
                let groups = found.captures.len() - 1;
                let one = digit(template.get(i + 1));
                let two = one.and_then(|a| digit(template.get(i + 2)).map(|b| a * 10 + b));
                let (index, width) = match (one, two) {
                    (_, Some(n)) if (1..=groups).contains(&n) => (n, 3),
                    (Some(n), _) if (1..=groups).contains(&n) => (n, 2),
                    _ => {
                        out.push(c);
                        i += 1;
                        continue;
                    }
                };
                if let Some((s, e)) = found.captures[index] {
                    out.extend_from_slice(&input[s..e]);
                }
                i += width;
            }
        }
    }
    out
}

fn capture_values(input: &[u16], found: &Match) -> Vec<Js> {
    found.captures[1..]
        .iter()
        .map(|c| c.map_or(Js::Undefined, |(s, e)| Js::utf16(&input[s..e])))
        .collect()
}

/// RegExp.prototype[@@replace].
fn replace(program: &Program, this: &Js, input: &[u16], replacement: &Js) -> Result<Js, Thrown> {
    let template = if replacement.is_callable() {
        None
    } else {
        Some(to_string(replacement)?)
    };
    let mut matches = vec![];
    if program.global {
        this.set("lastIndex", Js::Number(0.0))?;
        while let Some(found) = program.exec(this, input)? {
            if found.start == found.end {
                let next = to_length(&this.get("lastIndex")?)? + 1;
                this.set("lastIndex", Js::Number(next as f64))?;
            }
            matches.push(found);
        }
    } else if let Some(found) = program.exec(this, input)? {
        matches.push(found);
    }
    let mut out = vec![];
    let mut cursor = 0;
    for found in matches {
        let piece = match &template {
            Some(template) => substitute(template, input, &found),
            None => {
                let mut args = vec![Js::utf16(&input[found.start..found.end])];
                args.extend(capture_values(input, &found));
                args.push(Js::Number(found.start as f64));
                args.push(Js::utf16(input));
                to_string(&replacement.call(Js::Undefined, args)?)?.to_vec()
            }
        };
        if found.start >= cursor {
            out.extend_from_slice(&input[cursor..found.start]);
            out.extend(piece);
            cursor = found.end;
        }
    }
    out.extend_from_slice(&input[cursor.min(input.len())..]);
    Ok(Js::utf16(&out))
}

fn exec_array(input: &[u16], found: &Match) -> Js {
    let mut items = vec![Js::utf16(&input[found.start..found.end])];
    items.extend(capture_values(input, found));
    Js::array(items)
}

/// RegExp.prototype[@@match].
fn match_all_or_first(program: &Program, this: &Js, input: &[u16]) -> Result<Js, Thrown> {
    if !program.global {
        return Ok(program
            .exec(this, input)?
            .map_or(Js::Null, |found| exec_array(input, &found)));
    }
    this.set("lastIndex", Js::Number(0.0))?;
    let mut matched = vec![];
    while let Some(found) = program.exec(this, input)? {
        if found.start == found.end {
            let next = to_length(&this.get("lastIndex")?)? + 1;
            this.set("lastIndex", Js::Number(next as f64))?;
        }
        matched.push(Js::utf16(&input[found.start..found.end]));
    }
    Ok(if matched.is_empty() {
        Js::Null
    } else {
        Js::array(matched)
    })
}

/// RegExp.prototype[@@search]: lastIndex is saved and restored around exec.
fn search(program: &Program, this: &Js, input: &[u16]) -> Result<Js, Thrown> {
    let previous = this.get("lastIndex")?;
    this.set("lastIndex", Js::Number(0.0))?;
    let found = program.exec(this, input)?;
    this.set("lastIndex", previous)?;
    Ok(Js::Number(found.map_or(-1.0, |f| f.start as f64)))
}

/// Split uses ToUint32, including modulo reduction before any integer cast.
pub(super) fn split_limit(limit: &Js) -> Result<usize, Thrown> {
    if limit.is_undefined() {
        return Ok(u32::MAX as usize);
    }
    let number = limit.to_number()?;
    Ok(if number.is_finite() {
        number.trunc().rem_euclid(4_294_967_296.0) as usize
    } else {
        0
    })
}

/// RegExp.prototype[@@split]: a sticky match at each position, captures
/// spliced into the result, honouring the ToUint32 limit.
fn split(program: &Program, input: &[u16], limit: &Js) -> Result<Js, Thrown> {
    let limit = split_limit(limit)?;
    let mut out = vec![];
    if limit == 0 {
        return Ok(Js::array(out));
    }
    if input.is_empty() {
        if program.match_at(input, 0)?.is_none() {
            out.push(Js::utf16(input));
        }
        return Ok(Js::array(out));
    }
    let (mut p, mut q) = (0, 0);
    while q < input.len() {
        let Some(found) = program.match_at(input, q)? else {
            q += 1;
            continue;
        };
        let end = found.end.min(input.len());
        if end == p {
            q += 1;
            continue;
        }
        out.push(Js::utf16(&input[p..q]));
        if out.len() == limit {
            return Ok(Js::array(out));
        }
        p = end;
        for capture in capture_values(input, &found) {
            out.push(capture);
            if out.len() == limit {
                return Ok(Js::array(out));
            }
        }
        q = p;
    }
    out.push(Js::utf16(&input[p..]));
    Ok(Js::array(out))
}

/// A RegExp instance: own `lastIndex`, with exec/test/toString/@@replace/@@match/
/// @@search/@@split and the
/// flag accessors as data on its prototype. The exec result array carries no
/// `index`/`input` (js_runtime arrays hold no named properties).
pub fn regexp(pattern: &[u16], flags: &[u16]) -> Result<Js, Thrown> {
    let program = Rc::new(Program::compile(pattern, flags)?);
    let flag = |f: char| Js::Bool(program.flags.contains(f));
    let exec_program = program.clone();
    let exec = Js::function("exec", move |this, args| {
        let input = to_string(&arg(&args, 0))?;
        Ok(exec_program
            .exec(&this, &input)?
            .map_or(Js::Null, |found| exec_array(&input, &found)))
    });
    let test_program = program.clone();
    let test = Js::function("test", move |this, args| {
        let input = to_string(&arg(&args, 0))?;
        Ok(Js::Bool(test_program.exec(&this, &input)?.is_some()))
    });
    let display = program.clone();
    let to_string_fn = Js::function("toString", move |_, _| {
        let mut out: Vec<u16> = vec![u16::from(b'/')];
        out.extend_from_slice(&display.source);
        out.push(u16::from(b'/'));
        out.extend(display.flags.encode_utf16());
        Ok(Js::utf16(&out))
    });
    let replacer = program.clone();
    let replace_fn = Js::function("[Symbol.replace]", move |this, args| {
        let input = to_string(&arg(&args, 0))?;
        replace(&replacer, &this, &input, &arg(&args, 1))
    });
    let matcher = program.clone();
    let match_fn = Js::function("[Symbol.match]", move |this, args| {
        let input = to_string(&arg(&args, 0))?;
        match_all_or_first(&matcher, &this, &input)
    });
    let searcher = program.clone();
    let search_fn = Js::function("[Symbol.search]", move |this, args| {
        let input = to_string(&arg(&args, 0))?;
        search(&searcher, &this, &input)
    });
    let splitter = program.clone();
    let split_fn = Js::function("[Symbol.split]", move |_, args| {
        let input = to_string(&arg(&args, 0))?;
        split(&splitter, &input, &arg(&args, 1))
    });
    let prototype = Js::object(vec![
        ("exec", exec),
        ("test", test),
        ("toString", to_string_fn),
        ("@@replace", replace_fn),
        ("@@match", match_fn),
        ("@@search", search_fn),
        ("@@split", split_fn),
        ("source", Js::utf16(&program.source)),
        ("flags", Js::str(&program.flags)),
        ("global", flag('g')),
        ("ignoreCase", flag('i')),
        ("multiline", flag('m')),
        ("dotAll", flag('s')),
        ("unicode", Js::Bool(false)),
        ("sticky", flag('y')),
        ("hasIndices", Js::Bool(false)),
    ]);
    let instance = Js::object_with_class("RegExp", vec![("lastIndex", Js::Number(0.0))]);
    if let Js::Object(o) = &instance {
        *o.proto.borrow_mut() = Some(prototype);
    }
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn re(pattern: &str, flags: &str) -> Js {
        regexp(&units(pattern), &units(flags)).unwrap()
    }
    fn test(pattern: &str, flags: &str, input: &str) -> bool {
        re(pattern, flags)
            .call_method("test", vec![Js::str(input)])
            .unwrap()
            .truthy()
    }
    fn replace_all(pattern: &str, input: &str, with: &str) -> String {
        re(pattern, "g")
            .call_method("@@replace", vec![Js::str(input), Js::str(with)])
            .unwrap()
            .to_display()
    }

    #[test]
    fn source_patterns_match_as_in_v8() {
        assert!(test(r"^\d+$", "", "1700000000000"));
        assert!(!test(r"^\d+$", "", "2024-01-01"));
        let auth = r"invalid auth:\s*couldn'?t find account";
        assert!(test(auth, "i", "Invalid Auth:  couldnt find account 7"));
        assert!(!test(auth, "i", "invalid auth: wrong key"));
        assert_eq!(replace_all("[-_/]", "eth-usd_x/y", ""), "ethusdxy");
        assert_eq!(replace_all(r"[_\s-]", "a_b c-d", ""), "abcd");
        assert!(test(
            "0x[a-fA-F0-9]{40}",
            "",
            &format!("0x{}", "aB".repeat(20))
        ));
        assert!(!test("0x[a-fA-F0-9]{40}", "", "0x1234"));
    }

    #[test]
    fn exec_returns_captures_and_tracks_last_index() {
        let found = re(r"(\d+)", "")
            .call_method("exec", vec![Js::str("1h30m")])
            .unwrap();
        assert_eq!(found.to_display(), "1,1");
        let global = re(r"\d", "g");
        let first = global.call_method("exec", vec![Js::str("a1b2")]).unwrap();
        assert_eq!(first.to_display(), "1");
        assert_eq!(global.get("lastIndex").unwrap().to_number().unwrap(), 2.0);
        global.call_method("exec", vec![Js::str("a1b2")]).unwrap();
        let done = global.call_method("exec", vec![Js::str("a1b2")]).unwrap();
        assert!(matches!(done, Js::Null));
        assert_eq!(global.get("lastIndex").unwrap().to_number().unwrap(), 0.0);
    }

    #[test]
    fn flags_and_text_are_canonical() {
        let r = re("a/b", "yig");
        assert_eq!(r.get("flags").unwrap().to_display(), "giy");
        assert_eq!(
            r.call_method("toString", vec![]).unwrap().to_display(),
            "/a/b/giy"
        );
        assert_eq!(re("", "").get("source").unwrap().to_display(), "(?:)");
        assert!(
            r.own_keys()
                .iter()
                .all(|k| String::from_utf16_lossy(k) == "lastIndex")
        );
    }

    #[test]
    fn annex_b_and_case_folding() {
        assert!(test(r"\c1", "", r"\c1"));
        assert!(test(r"\cJ", "", "\n"));
        assert!(test("{", "", "{"));
        assert!(!test("[a-z]", "i", "\u{212A}"));
        assert!(test("[a-z]", "i", "K"));
    }

    #[test]
    fn unsupported_syntax_throws() {
        assert!(regexp(&units(r"(a)\1"), &[]).is_err());
        assert!(regexp(&units("a"), &units("u")).is_err());
        assert!(regexp(&units("a"), &units("gg")).is_err());
        assert!(regexp(&units("*"), &[]).is_err());
        assert!(regexp(&units("{2}"), &[]).is_err());
        assert!(regexp(&units("^*"), &[]).is_err());
        assert!(regexp(&units("(?=a)"), &[]).is_err());
    }
}
