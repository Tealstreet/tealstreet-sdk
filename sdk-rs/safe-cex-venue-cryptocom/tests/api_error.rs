use safe_cex_venue_cryptocom::api_error::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
};
#[derive(Debug)]
enum N {
    U,
    Null,
    Bool(bool),
    Num(f64),
    Text(Vec<u16>),
    Big(String),
    Symbol,
    Function,
    Array,
    Custom {
        tag: String,
        text: String,
        throws: bool,
    },
    Record,
}
type V = Rc<N>;
#[derive(Debug)]
struct E(&'static str);
struct Host {
    spec: Value,
    expected: Value,
    body: V,
    bodies: Vec<V>,
    original: Rc<E>,
    type_error: Rc<E>,
    events: RefCell<Vec<String>>,
    reads: RefCell<BTreeMap<String, usize>>,
    wire: RefCell<Option<V>>,
    response_reads: Cell<usize>,
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn val(v: &Value) -> V {
    Rc::new(if v["kind"] == "undefined" {
        N::U
    } else if v["kind"] == "number" {
        N::Num(match v["value"].as_str().unwrap() {
            "NaN" => f64::NAN,
            v => v.parse().unwrap(),
        })
    } else if v["kind"] == "string" {
        N::Text(units(&v["units"]))
    } else if v["kind"] == "bigint" {
        N::Big(v["value"].as_str().unwrap().into())
    } else if v["kind"] == "symbol" {
        N::Symbol
    } else if v["kind"] == "function" {
        N::Function
    } else if v["kind"] == "object" {
        N::Custom {
            tag: v["tag"].as_str().unwrap().into(),
            text: v["text"].as_str().unwrap().into(),
            throws: v["throws"].as_bool().unwrap_or(false),
        }
    } else if v["kind"] == "text-observed" {
        N::Text(units(&v["units"]))
    } else {
        match v {
            Value::Null => N::Null,
            Value::String(s) => N::Text(s.encode_utf16().collect()),
            Value::Bool(v) => N::Bool(*v),
            Value::Number(v) => N::Num(v.as_f64().unwrap()),
            Value::Array(_) => N::Array,
            _ => panic!("bounded input"),
        }
    })
}
impl Host {
    fn event(&self, name: &str) -> Result<(), Rc<E>> {
        self.events.borrow_mut().push(name.into());
        if self.spec["fail"].as_str() == Some(name) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn flag(&self, key: &str) -> bool {
        self.spec[key].as_bool().unwrap_or(false)
    }
    fn new(spec: Value, expected: Value) -> Self {
        let body = if spec["record"].as_bool().unwrap_or(false) {
            Rc::new(N::Record)
        } else {
            spec.get("body").map(val).unwrap_or_else(|| Rc::new(N::U))
        };
        let bodies = spec["bodySequence"]
            .as_array()
            .map(|v| v.iter().map(val).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![body.clone()]);
        let body = if spec.get("bodySequence").is_some() {
            bodies[1.min(bodies.len() - 1)].clone()
        } else {
            body
        };
        Self {
            bodies,
            spec,
            expected,
            body,
            original: Rc::new(E("original-error")),
            type_error: Rc::new(E("type-error")),
            events: RefCell::new(vec![]),
            reads: RefCell::new(BTreeMap::new()),
            wire: RefCell::new(None),
            response_reads: Cell::new(0),
        }
    }
    fn sequence(&self, key: &str, spec_key: &str) -> V {
        let mut reads = self.reads.borrow_mut();
        let n = reads.entry(key.into()).or_default();
        let vals = self.spec[spec_key].as_array();
        let result = vals
            .map(|v| val(&v[(*n).min(v.len() - 1)]))
            .unwrap_or_else(|| Rc::new(N::U));
        *n += 1;
        result
    }
    fn coerce(&self, value: V, hint: &str, symbol_allowed: bool) -> Result<Vec<u16>, Rc<E>> {
        match value.as_ref() {
            N::U => Ok("undefined".encode_utf16().collect()),
            N::Null => Ok("null".encode_utf16().collect()),
            N::Bool(v) => Ok(v.to_string().encode_utf16().collect()),
            N::Text(v) => Ok(v.clone()),
            N::Big(v) => Ok(v.encode_utf16().collect()),
            N::Num(v) => Ok(if v.is_nan() {
                "NaN".into()
            } else if *v == f64::INFINITY {
                "Infinity".into()
            } else if *v == f64::NEG_INFINITY {
                "-Infinity".into()
            } else if *v == 0.0 {
                "0".into()
            } else {
                v.to_string()
            }
            .encode_utf16()
            .collect()),
            N::Symbol => {
                if symbol_allowed {
                    Ok("Symbol(x)".encode_utf16().collect())
                } else {
                    Err(self.type_error.clone())
                }
            }
            N::Custom { tag, text, throws } => {
                if tag == "wire" {
                    assert!(
                        Rc::ptr_eq(&value, self.wire.borrow().as_ref().unwrap()),
                        "CRYPTOCOM-APIERROR-NATIVE-STRINGIFY-IDENTITY"
                    );
                }
                self.event(&format!("coerce:{tag}:{hint}"))?;
                if *throws {
                    Err(self.original.clone())
                } else {
                    Ok(text.encode_utf16().collect())
                }
            }
            N::Array => Ok(vec![]),
            N::Record => Ok("[object Object]".encode_utf16().collect()),
            N::Function => panic!("function String outside selected floor"),
        }
    }
}
impl ApiErrorHost for Host {
    type Value = V;
    type Error = Rc<E>;
    fn undefined_value(&self) -> V {
        Rc::new(N::U)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v.as_ref(), N::U | N::Null)
    }
    fn undefined(&self, v: &V) -> bool {
        matches!(v.as_ref(), N::U)
    }
    fn truthy(&self, v: &V) -> bool {
        match v.as_ref() {
            N::U | N::Null => false,
            N::Bool(v) => *v,
            N::Num(v) => *v != 0.0 && !v.is_nan(),
            N::Text(v) => !v.is_empty(),
            N::Big(v) => v != "0",
            _ => true,
        }
    }
    fn zero_number(&self, v: &V) -> bool {
        matches!(v.as_ref(),N::Num(v)if *v==0.0)
    }
    fn primitive_string(&self, v: &V) -> bool {
        matches!(v.as_ref(), N::Text(_))
    }
    fn get(&self, v: &V, key: &'static str) -> Result<V, Rc<E>> {
        match (v.as_ref(), key) {
            (N::Custom { tag, .. }, "response") if tag == "error" => {
                self.event("get:error:response")?;
                let i = self.response_reads.get();
                self.response_reads.set(i + 1);
                Ok(if self.flag("responseNull") {
                    Rc::new(N::Null)
                } else if self.flag("responseUndefined") {
                    Rc::new(N::U)
                } else {
                    Rc::new(N::Custom {
                        tag: format!("response{i}"),
                        text: "".into(),
                        throws: false,
                    })
                })
            }
            (N::Custom { tag, .. }, "status") if tag.starts_with("response") => {
                self.event("get:response:status")?;
                Ok(self.spec["statuses"]
                    .as_array()
                    .map(|v| val(&v[0]))
                    .unwrap_or_else(|| Rc::new(N::U)))
            }
            (N::Custom { tag, .. }, "data") if tag.starts_with("response") => {
                self.event("get:response:data")?;
                let index = tag
                    .strip_prefix("response")
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                Ok(self.bodies[index.min(self.bodies.len() - 1)].clone())
            }
            (N::Record, "code") => {
                self.event("get:body:code")?;
                Ok(self.sequence("code", "codes"))
            }
            (N::Record, "message") => {
                self.event("get:body:message")?;
                Ok(self.sequence("message", "messages"))
            }
            (N::Custom { .. }, "code" | "message") => Ok(Rc::new(N::U)),
            _ => panic!("get {key} {v:?}"),
        }
    }
    fn is_record(&self, v: &V) -> Result<bool, Rc<E>> {
        assert!(
            Rc::ptr_eq(v, &self.body),
            "CRYPTOCOM-APIERROR-NATIVE-BODY-IDENTITY"
        );
        self.event("record")?;
        if matches!(v.as_ref(), N::Record | N::Custom { .. } | N::Array) {
            self.event("array")?;
            Ok(!matches!(v.as_ref(), N::Array))
        } else {
            Ok(false)
        }
    }
    fn stringify_response_body(&self, v: V) -> Result<V, Rc<E>> {
        assert!(
            Rc::ptr_eq(&v, &self.body),
            "CRYPTOCOM-APIERROR-NATIVE-HELPER-IDENTITY"
        );
        self.event("helper")?;
        if matches!(v.as_ref(), N::Text(_)) {
            return Ok(v);
        }
        let json = self.event("json");
        if json.is_err() || self.flag("jsonThrow") {
            self.event("string")?;
            if self.flag("stringThrow") {
                return Err(self.original.clone());
            }
            return self.coerce(v, "string", true).map(|v| Rc::new(N::Text(v)));
        }
        let source = &self.expected["dependencyReturns"]["json"];
        let result = if source["kind"] == "string" {
            Rc::new(N::Text(units(&source["units"])))
        } else {
            val(source)
        };
        self.wire.replace(Some(result.clone()));
        Ok(result)
    }
    fn template(&self, v: V) -> Result<Vec<u16>, Rc<E>> {
        self.coerce(v, "string", false)
    }
    fn at_least_500(&self, v: &V) -> Result<bool, Rc<E>> {
        let units = self.coerce(v.clone(), "number", false)?;
        let s = String::from_utf16_lossy(&units);
        Ok(s.trim().parse::<f64>().unwrap_or(f64::NAN) >= 500.0)
    }
    fn string_value(&self, units: Vec<u16>) -> V {
        Rc::new(N::Text(units))
    }
}
#[test]
fn api_error_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/api_error_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let specs: Vec<Value> = serde_json::from_str(include_str!("api_error_fixtures.json")).unwrap();
    for (spec, e) in specs.into_iter().zip(source["rows"].as_array().unwrap()) {
        let name = spec["name"].as_str().unwrap().to_owned();
        let h = Host::new(spec, e.clone());
        let error = if h.flag("errorUndefined") {
            Rc::new(N::U)
        } else if h.flag("errorNull") {
            Rc::new(N::Null)
        } else {
            Rc::new(N::Custom {
                tag: "error".into(),
                text: "".into(),
                throws: false,
            })
        };
        let actual = format_cryptocom_api_error(&h, error);
        match actual {
            Ok(v) => {
                assert!(
                    e["thrown"].is_null(),
                    "CRYPTOCOM-APIERROR-NATIVE-ERROR {name}"
                );
                match (v.as_ref(), e["result"]["kind"].as_str().unwrap()) {
                    (N::U, "undefined") => {}
                    (N::Text(v), "string") => assert_eq!(
                        *v,
                        units(&e["result"]["units"]),
                        "CRYPTOCOM-APIERROR-NATIVE-STRING {name}"
                    ),
                    _ => panic!("CRYPTOCOM-APIERROR-NATIVE-VALUE {name}"),
                }
            }
            Err(err) => {
                let original = if e["thrown"] == "original-error" {
                    &h.original
                } else {
                    &h.type_error
                };
                assert!(
                    Rc::ptr_eq(&err, original),
                    "CRYPTOCOM-APIERROR-NATIVE-ERROR-IDENTITY {name}"
                );
                assert_eq!(
                    err.0,
                    e["thrown"].as_str().unwrap(),
                    "CRYPTOCOM-APIERROR-NATIVE-ERROR-KIND {name}"
                );
            }
        }
        assert_eq!(
            *h.events.borrow(),
            e["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "CRYPTOCOM-APIERROR-NATIVE-EFFECTS {name}"
        );
    }
    assert_eq!(
        source["rows"].as_array().unwrap().len(),
        59,
        "independent formatter floor"
    );
    assert_eq!(
        serde_json::from_str::<Vec<Value>>(include_str!("api_error_fixtures.json"))
            .unwrap()
            .len(),
        59,
        "independent fixture floor"
    );
    println!(
        "CRYPTOCOM API error floor59 stops index58 status-coercion-throws; no JSON reencoding"
    );
}
