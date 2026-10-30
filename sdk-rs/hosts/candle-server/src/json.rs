//! `JSON.stringify` output: JS number spelling (`100`, `1e+21`, `-0` as `0`,
//! non-finite as `null`) and insertion-ordered keys, so responses match the
//! TypeScript server byte for byte.

pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(&'static str, Json)>),
}

impl Json {
    pub fn str(value: impl Into<String>) -> Self {
        Self::Str(value.into())
    }

    pub fn opt_str(value: Option<&str>) -> Option<Self> {
        value.map(Self::str)
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Num(value) => write_number(*value, out),
            Self::Str(value) => write_string(value, out),
            Self::Arr(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Self::Obj(fields) => {
                out.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// An object builder that drops absent (`undefined`) fields, as `JSON.stringify` does.
#[derive(Default)]
pub struct Object(Vec<(&'static str, Json)>);

impl Object {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(mut self, key: &'static str, value: Json) -> Self {
        self.0.push((key, value));
        self
    }

    pub fn maybe(mut self, key: &'static str, value: Option<Json>) -> Self {
        if let Some(value) = value {
            self.0.push((key, value));
        }
        self
    }

    pub fn build(self) -> Json {
        Json::Obj(self.0)
    }
}

pub fn write_number(value: f64, out: &mut String) {
    if !value.is_finite() {
        out.push_str("null");
    } else if value == 0.0 {
        out.push('0');
    } else {
        out.push_str(ryu_js::Buffer::new().format_finite(value));
    }
}

fn write_string(value: &str, out: &mut String) {
    // serde_json escapes exactly what JSON.stringify does for valid UTF-8.
    out.push_str(&serde_json::to_string(value).expect("a string serializes"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_use_the_javascript_spelling() {
        let cases = [
            (100.0, "100"),
            (-0.0, "0"),
            (0.1, "0.1"),
            (1e21, "1e+21"),
            (1.5e-7, "1.5e-7"),
            (123456789012.5, "123456789012.5"),
            // Golden spellings from node's `JSON.stringify`.
            (-42.0, "-42"),
            (1e-7, "1e-7"),
            (1e-6, "0.000001"),
            (0.0000015, "0.0000015"),
            (0.00001234, "0.00001234"),
            (9007199254740991.0, "9007199254740991"),
            (9007199254740992.0, "9007199254740992"),
            (1e16, "10000000000000000"),
            (12345678901234567890.0, "12345678901234567000"),
            (1e20, "100000000000000000000"),
            (1.5e21, "1.5e+21"),
            (-1e21, "-1e+21"),
            (1e300, "1e+300"),
            (5e-324, "5e-324"),
            (f64::NAN, "null"),
            (f64::INFINITY, "null"),
        ];
        for (value, expected) in cases {
            assert_eq!(Json::Num(value).render(), expected, "{value}");
        }
    }

    #[test]
    fn objects_keep_insertion_order_and_drop_absent_fields() {
        let json = Object::new()
            .field("b", Json::Num(1.0))
            .maybe("skipped", None)
            .field("a", Json::str("x\"\n\u{1}é"))
            .build();
        assert_eq!(json.render(), r#"{"b":1,"a":"x\"\n\u0001é"}"#);
    }
}
