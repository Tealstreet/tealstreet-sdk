// Copyright 2011 the V8 project authors. All rights reserved.
// Adapted from Node v24.20.0 deps/v8/src/date/dateparser{,-inl}.{h,cc}.
// BSD-style license: V8-LICENSE in this directory.

/// ECMAScript TimeClip, including invalid dates and normalization of -0 to +0.
pub fn js_time_clip(value: f64) -> f64 {
    if value.abs() <= 8.64e15 {
        value.trunc() + 0.0
    } else {
        f64::NAN
    }
}

/// ECMAScript Date Number construction followed by Date.prototype.toISOString.
/// Reuses TimeClip, then renders the UTC calendar including extended years.
pub fn js_date_to_iso_string(
    value: f64,
) -> Result<String, safe_cex_types::js_coercion::JsCoercionError> {
    let milliseconds = js_time_clip(value);
    if !milliseconds.is_finite() {
        return Err(safe_cex_types::js_coercion::JsCoercionError {
            name: "RangeError",
            message: "Invalid time value",
        });
    }
    let milliseconds = milliseconds.trunc() as i64;
    let days = milliseconds.div_euclid(86_400_000);
    let time = milliseconds.rem_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_offset = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_offset + 2) / 5 + 1;
    let month = month_offset + if month_offset < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{year:+07}")
    };
    Ok(format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        time / 3_600_000,
        time / 60_000 % 60,
        time / 1000 % 60,
        time % 1000
    ))
}

/// V8 Date.parse acceptance with explicit host local-wall-time to UTC conversion.
/// The adapter must implement the host timezone, including V8-compatible DST disambiguation.
/// CHOSEN reference: Node V8 grammar and its 32-bit signed Smi timezone bound.
/// Use `|ms| ms` for an explicitly UTC host; coercion to String happens upstream.
pub fn js_date_parse(input: &str, local_to_utc: impl FnOnce(f64) -> f64) -> f64 {
    js_date_parse_utf16(&input.encode_utf16().collect::<Vec<_>>(), local_to_utc)
}

/// Date.parse over JS UTF-16 code units, retaining NUL and unpaired surrogates.
pub fn js_date_parse_utf16(input: &[u16], local_to_utc: impl FnOnce(f64) -> f64) -> f64 {
    let Some((date, offset)) = Parser::new(input).parse() else {
        return f64::NAN;
    };
    let utc = if let Some(offset) = offset {
        date - f64::from(offset) * 1000.0
    } else if date.abs() <= 8.64e15 + 30.0 * 86400000.0 {
        local_to_utc(date)
    } else {
        return f64::NAN;
    };
    js_time_clip(utc)
}

#[derive(Clone, Copy, PartialEq)]
enum Word {
    Invalid,
    Month,
    Zone,
    Separator,
    AmPm,
}
#[derive(Clone, Copy, PartialEq)]
enum Token {
    Number(i32, usize),
    Symbol(u8),
    Keyword(Word, i32, usize),
    White,
    Unknown,
    End,
    Invalid,
}
impl Token {
    fn number(self) -> Option<i32> {
        if let Self::Number(n, _) = self {
            Some(n)
        } else {
            None
        }
    }
    fn fixed(self, size: usize) -> bool {
        matches!(self, Self::Number(_, len) if len == size)
    }
    fn symbol(self, c: u8) -> bool {
        self == Self::Symbol(c)
    }
    fn sign(self) -> Option<i32> {
        match self {
            Self::Symbol(b'+') => Some(1),
            Self::Symbol(b'-') => Some(-1),
            _ => None,
        }
    }
    fn z(self) -> bool {
        self == Self::Keyword(Word::Zone, 0, 1)
    }
    fn millis(self) -> i32 {
        let Self::Number(n, len) = self else {
            return 0;
        };
        if len < 3 {
            n * 10_i32.pow((3 - len) as u32)
        } else {
            n / 10_i32.pow((len.min(9) - 3) as u32)
        }
    }
}
fn whitespace(c: u16) -> bool {
    char::from_u32(u32::from(c)).is_some_and(crate::js_whitespace)
}
fn word_character(c: u16) -> bool {
    c >= 65 && (!whitespace(c) || matches!(c, 0x2028 | 0x2029))
}
fn keyword(prefix: &[u16], len: usize) -> Token {
    for (i, month) in [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ]
    .iter()
    .enumerate()
    {
        if prefix.iter().copied().eq(month.encode_utf16()) {
            return Token::Keyword(Word::Month, i as i32 + 1, len);
        }
    }
    let names = [
        ("am", Word::AmPm, 0),
        ("pm", Word::AmPm, 12),
        ("ut", Word::Zone, 0),
        ("utc", Word::Zone, 0),
        ("z", Word::Zone, 0),
        ("gmt", Word::Zone, 0),
        ("cdt", Word::Zone, -5),
        ("cst", Word::Zone, -6),
        ("edt", Word::Zone, -4),
        ("est", Word::Zone, -5),
        ("mdt", Word::Zone, -6),
        ("mst", Word::Zone, -7),
        ("pdt", Word::Zone, -7),
        ("pst", Word::Zone, -8),
        ("t", Word::Separator, 0),
    ];
    if len <= 3 {
        for (name, kind, value) in names {
            if prefix.iter().copied().eq(name.encode_utf16()) {
                return Token::Keyword(kind, value, len);
            }
        }
    }
    Token::Keyword(Word::Invalid, 0, len)
}
fn tokenize(input: &[u16]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() && input[i] != 0 {
        let c = input[i];
        let start = i;
        if (48..=57).contains(&c) {
            while i < input.len() && input[i] == 48 {
                i += 1;
            }
            let mut n = 0;
            let mut digits = 0;
            while i < input.len() && (48..=57).contains(&input[i]) {
                if digits < 9 {
                    n = n * 10 + i32::from(input[i] - 48);
                }
                digits += 1;
                i += 1;
            }
            out.push(Token::Number(n, i - start));
        } else if b":-+.)".iter().any(|x| u16::from(*x) == c) {
            out.push(Token::Symbol(c as u8));
            i += 1;
        } else if word_character(c) {
            let mut prefix = Vec::new();
            while i < input.len() && word_character(input[i]) {
                if prefix.len() < 3 {
                    prefix.push(if (65..=90).contains(&input[i]) {
                        input[i] + 32
                    } else {
                        input[i]
                    });
                }
                i += 1;
            }
            out.push(keyword(&prefix, i - start));
        } else if whitespace(c) {
            out.push(Token::White);
            i += 1;
        } else if c == 40 {
            let mut balance = 0;
            while i < input.len() && input[i] != 0 {
                match input[i] {
                    40 => balance += 1,
                    41 => balance -= 1,
                    _ => {}
                }
                i += 1;
                if balance == 0 {
                    break;
                }
            }
            out.push(Token::Unknown);
        } else {
            out.push(Token::Unknown);
            i += 1;
        }
    }
    out.push(Token::End);
    out
}
#[derive(Default)]
struct Time {
    values: Vec<i32>,
    hour_offset: Option<i32>,
}
impl Time {
    fn add(&mut self, n: i32) -> bool {
        if self.values.len() == 4 {
            false
        } else {
            self.values.push(n);
            true
        }
    }
    fn final_value(&mut self, n: i32) {
        if self.add(n) {
            self.values.resize(4, 0);
        }
    }
    fn expects(&self, n: i32) -> bool {
        match self.values.len() {
            1 | 2 => (0..=59).contains(&n),
            3 => (0..=999).contains(&n),
            _ => false,
        }
    }
    fn millis(mut self) -> Option<i64> {
        self.values.resize(4, 0);
        let [mut h, m, s, ms]: [i32; 4] = self.values.try_into().ok()?;
        if let Some(offset) = self.hour_offset {
            if !(0..=12).contains(&h) {
                return None;
            }
            h = h % 12 + offset;
        }
        if !((0..=23).contains(&h)
            && (0..=59).contains(&m)
            && (0..=59).contains(&s)
            && (0..=999).contains(&ms)
            || h == 24 && m == 0 && s == 0 && ms == 0)
        {
            return None;
        }
        Some(i64::from(h) * 3600000 + i64::from(m) * 60000 + i64::from(s) * 1000 + i64::from(ms))
    }
}
#[derive(Default)]
struct Zone {
    sign: Option<i32>,
    hour: Option<i32>,
    minute: Option<i32>,
}
impl Zone {
    fn set(&mut self, n: i32) {
        self.sign = Some(if n < 0 { -1 } else { 1 });
        self.hour = Some(n.abs());
        self.minute = Some(0);
    }
    fn expects(&self, n: i32) -> bool {
        self.hour.is_some() && self.minute.is_none() && (0..=59).contains(&n)
    }
    fn utc(&self) -> bool {
        self.hour == Some(0) && self.minute == Some(0)
    }
    fn seconds(&self) -> Option<Option<i32>> {
        let Some(sign) = self.sign else {
            return Some(None);
        };
        let seconds = (self.hour.unwrap_or(0) as u32)
            .wrapping_mul(3600)
            .wrapping_add((self.minute.unwrap_or(0) as u32).wrapping_mul(60));
        if seconds > i32::MAX as u32 {
            None
        } else {
            Some(Some(seconds as i32 * sign))
        }
    }
}
struct Parser {
    tokens: Vec<Token>,
    index: usize,
    days: Vec<i32>,
    named_month: Option<i32>,
    iso: bool,
    time: Time,
    zone: Zone,
}
impl Parser {
    fn new(input: &[u16]) -> Self {
        Self {
            tokens: tokenize(input),
            index: 0,
            days: Vec::new(),
            named_month: None,
            iso: false,
            time: Time::default(),
            zone: Zone::default(),
        }
    }
    fn peek(&self) -> Token {
        self.tokens.get(self.index).copied().unwrap_or(Token::End)
    }
    fn next(&mut self) -> Token {
        let t = self.peek();
        if t != Token::End {
            self.index += 1;
        }
        t
    }
    fn skip(&mut self, c: u8) -> bool {
        if self.peek().symbol(c) {
            self.next();
            true
        } else {
            false
        }
    }
    fn add_day(&mut self, n: i32) -> bool {
        if self.days.len() == 3 {
            false
        } else {
            self.days.push(n);
            true
        }
    }
    fn iso_parse(&mut self) -> Token {
        if let Some(sign) = self.peek().sign() {
            let sign_token = self.next();
            if !self.peek().fixed(6) {
                return sign_token;
            }
            let year = self.next().number().unwrap_or(0);
            if sign < 0 && year == 0 {
                return sign_token;
            }
            self.add_day(sign * year);
        } else if self.peek().fixed(4) {
            let n = self.next().number().unwrap_or(0);
            self.add_day(n);
        } else {
            return self.next();
        }
        if self.skip(b'-') {
            if !self.peek().fixed(2) || !self.peek().number().is_some_and(|n| (1..=12).contains(&n))
            {
                return self.next();
            }
            let n = self.next().number().unwrap_or(0);
            self.add_day(n);
            if self.skip(b'-') {
                if !self.peek().fixed(2)
                    || !self.peek().number().is_some_and(|n| (1..=31).contains(&n))
                {
                    return self.next();
                }
                let n = self.next().number().unwrap_or(0);
                self.add_day(n);
            }
        }
        if !matches!(self.peek(), Token::Keyword(Word::Separator, _, _)) {
            if self.peek() != Token::End {
                return self.next();
            }
        } else {
            self.next();
            if !self.peek().fixed(2) || !self.peek().number().is_some_and(|n| (0..=24).contains(&n))
            {
                return Token::Invalid;
            }
            let h = self.next().number().unwrap_or(0);
            self.time.add(h);
            if !self.skip(b':')
                || !self.peek().fixed(2)
                || !self
                    .peek()
                    .number()
                    .is_some_and(|n| (0..=59).contains(&n) && (h != 24 || n == 0))
            {
                return Token::Invalid;
            }
            let n = self.next().number().unwrap_or(0);
            self.time.add(n);
            if self.skip(b':') {
                if !self.peek().fixed(2)
                    || !self
                        .peek()
                        .number()
                        .is_some_and(|n| (0..=59).contains(&n) && (h != 24 || n == 0))
                {
                    return Token::Invalid;
                }
                let n = self.next().number().unwrap_or(0);
                self.time.add(n);
                if self.skip(b'.') {
                    if !self.peek().number().is_some_and(|n| h != 24 || n == 0) {
                        return Token::Invalid;
                    }
                    let n = self.next().millis();
                    self.time.add(n);
                }
            }
            if self.peek().z() {
                self.next();
                self.zone.set(0);
            } else if let Some(sign) = self.peek().sign() {
                self.next();
                self.zone.sign = Some(sign);
                if self.peek().fixed(4) {
                    let hm = self.next().number().unwrap_or(0);
                    let h = hm / 100;
                    let m = hm % 100;
                    if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
                        return Token::Invalid;
                    }
                    self.zone.hour = Some(h);
                    self.zone.minute = Some(m);
                } else {
                    if !self.peek().fixed(2)
                        || !self.peek().number().is_some_and(|n| (0..=23).contains(&n))
                    {
                        return Token::Invalid;
                    }
                    self.zone.hour = self.next().number();
                    if !self.skip(b':')
                        || !self.peek().fixed(2)
                        || !self.peek().number().is_some_and(|n| (0..=59).contains(&n))
                    {
                        return Token::Invalid;
                    }
                    self.zone.minute = self.next().number();
                }
            }
            if self.peek() != Token::End {
                return Token::Invalid;
            }
        }
        if self.zone.hour.is_none() && self.time.values.is_empty() {
            self.zone.set(0);
        }
        self.iso = true;
        Token::End
    }
    fn parse(mut self) -> Option<(f64, Option<i32>)> {
        let mut token = self.iso_parse();
        if token == Token::Invalid {
            return None;
        }
        let mut read_number = !self.days.is_empty();
        while token != Token::End {
            if let Token::Number(n, _) = token {
                read_number = true;
                if self.skip(b':') {
                    if self.skip(b':') {
                        if !self.time.values.is_empty() {
                            return None;
                        }
                        self.time.add(n);
                        self.time.add(0);
                    } else {
                        if !self.time.add(n) {
                            return None;
                        }
                        if self.peek().symbol(b'.') {
                            self.next();
                        }
                    }
                } else if self.skip(b'.') && self.time.expects(n) {
                    self.time.add(n);
                    self.peek().number()?;
                    let ms = self.next().millis();
                    self.time.final_value(ms);
                } else if self.zone.expects(n) {
                    self.zone.minute = Some(n);
                } else if self.time.expects(n) {
                    self.time.final_value(n);
                    let peek = self.peek();
                    if peek != Token::End
                        && peek != Token::White
                        && !peek.z()
                        && peek.sign().is_none()
                    {
                        return None;
                    }
                } else {
                    if !self.add_day(n) {
                        return None;
                    }
                    self.skip(b'-');
                }
            } else if let Token::Keyword(kind, value, _) = token {
                match kind {
                    Word::AmPm if !self.time.values.is_empty() => {
                        self.time.hour_offset = Some(value)
                    }
                    Word::Month => {
                        self.named_month = Some(value);
                        self.skip(b'-');
                    }
                    Word::Zone if read_number => self.zone.set(value),
                    _ => {
                        if read_number || self.peek().number().is_some() {
                            return None;
                        }
                    }
                }
            } else if token.sign().is_some() && (self.zone.utc() || !self.time.values.is_empty()) {
                self.zone.sign = token.sign();
                let (n, len) = if let Token::Number(n, len) = self.peek() {
                    self.next();
                    (n, len)
                } else {
                    (0, 0)
                };
                read_number = true;
                if self.peek().symbol(b':') {
                    self.zone.hour = Some(n);
                    self.zone.minute = None;
                } else if len == 1 || len == 2 {
                    self.zone.hour = Some(n);
                    self.zone.minute = Some(0);
                } else if len == 3 || len == 4 {
                    self.zone.hour = Some(n / 100);
                    self.zone.minute = Some(n % 100);
                } else {
                    return None;
                }
            } else if (token.sign().is_some() || token.symbol(b')')) && read_number {
                return None;
            }
            token = self.next();
        }
        if self.days.is_empty() {
            return None;
        }
        self.days.resize(3, 1);
        let (a, b, c) = (self.days[0], self.days[1], self.days[2]);
        let (mut y, m, d) = if let Some(m) = self.named_month {
            if !(1..=31).contains(&a) {
                (a, m, b)
            } else {
                (b, m, a)
            }
        } else if self.iso || !(1..=31).contains(&a) {
            (a, b, c)
        } else {
            (c, a, b)
        };
        if !self.iso {
            if (0..=49).contains(&y) {
                y += 2000;
            } else if (50..=99).contains(&y) {
                y += 1900;
            }
        }
        if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        let days = days_from_civil(i64::from(y), i64::from(m), i64::from(d));
        Some((
            days as f64 * 86400000.0 + self.time.millis()? as f64,
            self.zone.seconds()?,
        ))
    }
}
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * month + 2) / 5 + day - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}
