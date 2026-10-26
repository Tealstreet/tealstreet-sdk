//! Lodash 4.18.1 compounders; see LODASH-LICENSE and the pinned live oracle.
//! Explicit regex classes are deliberately not Unicode categories.
#[path = "lodash_deburr.rs"]
mod deburr;
use deburr::deburr_letter;

fn lower(c: char) -> bool {
    matches!(c, 'a'..='z' | '\u{df}'..='\u{f6}' | '\u{f8}'..='\u{ff}')
}
fn upper(c: char) -> bool {
    matches!(c, 'A'..='Z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{de}')
}
fn dingbat(c: char) -> bool {
    matches!(c, '\u{2700}'..='\u{27bf}')
}
fn regional(c: char) -> bool {
    matches!(c, '\u{1f1e6}'..='\u{1f1ff}')
}
fn astral(c: char) -> bool {
    c as u32 > 0xffff
}
fn combo(c: char) -> bool {
    matches!(c, '\u{300}'..='\u{36f}' | '\u{fe20}'..='\u{fe2f}' | '\u{20d0}'..='\u{20ff}')
}
fn brk(c: char) -> bool {
    matches!(c, '\0'..='/' | ':'..='@' | '['..='`' | '{'..='\u{bf}' | '\u{d7}' | '\u{f7}' | '\u{2000}'..='\u{206f}' | '\u{180e}')
        || crate::js_whitespace(c)
}
fn misc(c: char) -> bool {
    !astral(c) && !brk(c) && !c.is_ascii_digit() && !dingbat(c) && !lower(c) && !upper(c)
}
fn misc_lower(c: char) -> bool {
    lower(c) || misc(c)
}
fn misc_upper(c: char) -> bool {
    upper(c) || misc(c)
}
fn run(s: &[char], i: usize, predicate: fn(char) -> bool) -> usize {
    i + s[i..].iter().take_while(|&&c| predicate(c)).count()
}
fn at(s: &[char], i: usize, predicate: fn(char) -> bool) -> bool {
    s.get(i).is_some_and(|&c| predicate(c))
}

fn emoji_base(s: &[char], i: usize, joined: bool) -> Option<usize> {
    let c = *s.get(i)?;
    if (joined && !astral(c)) || dingbat(c) {
        return Some(i + 1);
    }
    if regional(c) && at(s, i + 1, regional) {
        return Some(i + 2);
    }
    astral(c).then_some(i + 1)
}
fn emoji(s: &[char], i: usize) -> Option<usize> {
    let mut end = emoji_base(s, i, false)?;
    loop {
        if matches!(s.get(end), Some('\u{fe0e}' | '\u{fe0f}')) {
            end += 1;
        }
        if s.get(end)
            .is_some_and(|&c| combo(c) || matches!(c, '\u{1f3fb}'..='\u{1f3ff}'))
        {
            end += 1;
        }
        if s.get(end) == Some(&'\u{200d}')
            && let Some(next) = emoji_base(s, end + 1, true)
        {
            end = next;
            continue;
        }
        return Some(end);
    }
}
fn ordinal(s: &[char], i: usize, uppercase: bool) -> Option<usize> {
    let digits = run(s, i, |c| c.is_ascii_digit());
    if digits == i {
        return None;
    }
    let suffix = match s[digits - 1] {
        '1' => "st",
        '2' => "nd",
        '3' => "rd",
        _ => "th",
    };
    let suffix = if uppercase {
        suffix.to_ascii_uppercase()
    } else {
        suffix.to_owned()
    };
    if !s[digits..].starts_with(&suffix.chars().collect::<Vec<_>>()) {
        return None;
    }
    let end = digits + 2;
    let boundary = s
        .get(end)
        .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != '_');
    let next_case = s.get(end).is_some_and(|c| {
        if uppercase {
            c.is_ascii_lowercase()
        } else {
            c.is_ascii_uppercase()
        }
    });
    (boundary || next_case || s.get(end) == Some(&'_')).then_some(end)
}

fn word_end(s: &[char], i: usize) -> Option<usize> {
    // Preserve ordered alternatives and greedy backtracking from reUnicodeWord.
    let start = i + usize::from(at(s, i, upper));
    let end = run(s, start, lower);
    if end > start && (end == s.len() || at(s, end, brk) || at(s, end, upper)) {
        return Some(end);
    }
    let end = run(s, i, misc_upper);
    for e in (i + 1..=end).rev() {
        if e == s.len() || at(s, e, brk) || (at(s, e, upper) && at(s, e + 1, misc_lower)) {
            return Some(e);
        }
    }
    let end = run(s, start, misc_lower);
    if end > start {
        return Some(end);
    }
    let end = run(s, i, upper);
    if end > i {
        return Some(end);
    }
    if let Some(end) = ordinal(s, i, true).or_else(|| ordinal(s, i, false)) {
        return Some(end);
    }
    let end = run(s, i, |c| c.is_ascii_digit());
    if end > i {
        return Some(end);
    }
    emoji(s, i)
}

fn compound(value: &str, camel: bool) -> String {
    let mut prepared = String::new();
    for c in value.chars() {
        if combo(c) {
            continue;
        }
        if let Some(mapped) = deburr_letter(c) {
            prepared.push_str(mapped);
        } else {
            prepared.push(c);
        }
    }
    let chars: Vec<char> = prepared
        .chars()
        .filter(|c| !matches!(c, '\'' | '\u{2019}'))
        .collect();
    let mut result = String::new();
    let mut i = 0;
    let mut count = 0;
    while i < chars.len() {
        if let Some(end) = word_end(&chars, i) {
            let word = chars[i..end].iter().collect::<String>().to_lowercase();
            if count > 0 && camel {
                // Lodash upperFirst uppercases the first Unicode symbol, including a ZWJ sequence.
                let units: Vec<char> = word.chars().collect();
                let fitz = |c| matches!(c, '\u{1f3fb}'..='\u{1f3ff}');
                let first = if at(&units, 0, fitz) && at(&units, 1, fitz) {
                    1
                } else {
                    emoji(&units, 0).unwrap_or(1)
                };
                result.push_str(&units[..first].iter().collect::<String>().to_uppercase());
                result.extend(&units[first..]);
            } else {
                if count > 0 {
                    result.push('_');
                }
                result.push_str(&word);
            }
            count += 1;
            i = end;
        } else {
            i += 1;
        }
    }
    result
}

/// Lodash 4.18.1 snakeCase over Unicode strings, without observable memoization.
pub fn lodash_snake_case(value: &str) -> String {
    compound(value, false)
}
/// Lodash 4.18.1 camelCase over Unicode strings, without observable memoization.
pub fn lodash_camel_case(value: &str) -> String {
    compound(value, true)
}
