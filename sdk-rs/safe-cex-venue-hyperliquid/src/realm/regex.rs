//! RegExp literals the ported bodies use. The source syntax subset (classes,
//! `\d`, `\s`, groups, anchors, `i`) is shared by JS and the `regex` crate.
//! Matching runs on UTF-8; inputs with lone surrogates match their U+FFFD form.

pub(crate) struct Pattern {
    pub source: String,
    pub flags: String,
    re: regex::Regex,
}

impl Pattern {
    /// Parse a `/source/flags` literal.
    pub(crate) fn literal(text: &str) -> Result<Self, String> {
        let end = text
            .rfind('/')
            .filter(|&i| i > 0 && text.starts_with('/'))
            .ok_or_else(|| format!("Invalid regular expression literal: {text}"))?;
        let source = &text[1..end];
        let flags = &text[end + 1..];
        Self::new(source, flags)
    }
    pub(crate) fn new(source: &str, flags: &str) -> Result<Self, String> {
        let mut prefix = String::new();
        for flag in flags.chars() {
            match flag {
                'i' => prefix.push_str("(?i)"),
                'g' | 'u' | 'y' => {}
                other => return Err(format!("Invalid regular expression flags: {other}")),
            }
        }
        let re = regex::Regex::new(&format!("{prefix}{source}"))
            .map_err(|e| format!("Invalid regular expression: /{source}/: {e}"))?;
        Ok(Self {
            source: source.into(),
            flags: flags.into(),
            re,
        })
    }
    pub(crate) fn test(&self, units: &[u16]) -> bool {
        self.re.is_match(&String::from_utf16_lossy(units))
    }
    /// `exec` without lastIndex: the whole match then each group (None when unmatched).
    pub(crate) fn exec(&self, units: &[u16]) -> Option<(usize, Vec<Option<Vec<u16>>>)> {
        let text = String::from_utf16_lossy(units);
        let captures = self.re.captures(&text)?;
        let start = text[..captures.get(0)?.start()].encode_utf16().count();
        let groups = captures
            .iter()
            .map(|m| m.map(|m| m.as_str().encode_utf16().collect()))
            .collect();
        Some((start, groups))
    }
    /// `String.prototype.split(regex)`: separators removed, captured groups spliced in.
    pub(crate) fn split(&self, units: &[u16]) -> Vec<Option<Vec<u16>>> {
        let text = String::from_utf16_lossy(units);
        if text.is_empty() {
            return if self.re.is_match("") {
                vec![]
            } else {
                vec![Some(vec![])]
            };
        }
        let mut out = vec![];
        let mut last = 0;
        for captures in self.re.captures_iter(&text) {
            let whole = captures.get(0).expect("group 0");
            // An empty match at the very start or end does not split.
            if whole.end() == 0 || (whole.start() == whole.end() && whole.start() >= text.len()) {
                continue;
            }
            if whole.start() == whole.end() && whole.start() == last && last != 0 {
                continue;
            }
            out.push(Some(text[last..whole.start()].encode_utf16().collect()));
            for group in captures.iter().skip(1) {
                out.push(group.map(|m| m.as_str().encode_utf16().collect()));
            }
            last = whole.end();
        }
        out.push(Some(text[last..].encode_utf16().collect()));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
    fn texts(v: Vec<Option<Vec<u16>>>) -> Vec<Option<String>> {
        v.into_iter()
            .map(|p| p.map(|p| String::from_utf16_lossy(&p)))
            .collect()
    }
    #[test]
    fn split_with_capturing_group_matches_js() {
        // "15m".split(/(\d+)/) is ["", "15", "m"] in V8.
        let p = Pattern::literal(r"/(\d+)/").unwrap();
        assert_eq!(
            texts(p.split(&u("15m"))),
            vec![Some("".into()), Some("15".into()), Some("m".into())]
        );
        assert_eq!(texts(p.split(&u(""))), vec![Some("".into())]);
    }
    #[test]
    fn case_insensitive_suffix_and_exec_groups() {
        let p = Pattern::literal("/-PERP$/i").unwrap();
        assert!(p.test(&u("BTC-perp")) && !p.test(&u("BTC-PERPX")));
        let p = Pattern::literal(r#"/"type"\s*:\s*"([^\"]+)"/"#).unwrap();
        let (start, groups) = p.exec(&u(r#"{"a":1,"type" : "order"}"#)).unwrap();
        assert_eq!(start, 7);
        assert_eq!(
            String::from_utf16_lossy(groups[1].as_ref().unwrap()),
            "order"
        );
    }
}
