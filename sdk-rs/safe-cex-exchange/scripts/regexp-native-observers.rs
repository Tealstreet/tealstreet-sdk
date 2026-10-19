use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub struct Audit {
    calls: Rc<Cell<usize>>,
    error: Js,
    callback_args: Rc<RefCell<Vec<Value>>>,
}
impl Audit {
    pub fn new() -> Self {
        Self {
            calls: Rc::new(Cell::new(0)),
            error: Js::object(vec![("original", Js::Bool(true))]),
            callback_args: Rc::new(RefCell::new(Vec::new())),
        }
    }
    pub fn input(&self, row: &Value) -> Js {
        match row["kind"].as_str().unwrap() {
            "units" => Js::utf16(&units(&row["units"])),
            "number" => Js::Number(row["number"].as_str().unwrap().parse().unwrap()),
            "undefined" => Js::Undefined,
            "stringBeforeNumber" | "stringBeforeThrow" => {
                let output = Js::utf16(&units(&row["units"]));
                let calls = self.calls.clone();
                let string = Js::function("toString", move |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(output.clone())
                });
                let throw = row["kind"] == "stringBeforeThrow";
                let error = self.error.clone();
                let calls = self.calls.clone();
                let number = Js::function("valueOf", move |_, _| {
                    calls.set(calls.get() + 1);
                    if throw {
                        Err(Thrown::new(error.clone()))
                    } else {
                        Ok(Js::Number(7.0))
                    }
                });
                Js::object(vec![("toString", string), ("valueOf", number)])
            }
            "objectString" | "objectNumber" => {
                let string = row["kind"] == "objectString";
                let result = if string {
                    Js::utf16(&units(&row["units"]))
                } else {
                    Js::Number(row["number"].as_str().unwrap().parse().unwrap())
                };
                let calls = self.calls.clone();
                let method = if string { "toString" } else { "valueOf" };
                Js::object(vec![(
                    method,
                    Js::function(method, move |_, _| {
                        calls.set(calls.get() + 1);
                        Ok(result.clone())
                    }),
                )])
            }
            "callback" => {
                let output = Js::utf16(&units(&row["units"]));
                let calls = self.calls.clone();
                let args = self.callback_args.clone();
                Js::function("replacement", move |_, values| {
                    calls.set(calls.get() + 1);
                    args.borrow_mut()
                        .push(Value::Array(values.iter().map(project).collect()));
                    Ok(output.clone())
                })
            }
            "throw" => {
                let make = |name| {
                    let calls = self.calls.clone();
                    let error = self.error.clone();
                    Js::function(name, move |_, _| {
                        calls.set(calls.get() + 1);
                        Err(Thrown::new(error.clone()))
                    })
                };
                Js::object(vec![
                    ("toString", make("toString")),
                    ("valueOf", make("valueOf")),
                ])
            }
            other => panic!("unreviewed fixture {other}"),
        }
    }
    pub fn finish(
        &self,
        result: Result<Js, Thrown>,
        regexp: Option<&Js>,
        previous: Option<&Js>,
        replace: bool,
    ) -> Value {
        match result {
            Err(error) => {
                json!({"ok": false, "sameError": error.value.same_reference(&self.error), "calls": self.calls.get()})
            }
            Ok(value) => {
                let mut result =
                    json!({"ok": true, "value": project(&value), "calls": self.calls.get()});
                if let Some(regexp) = regexp {
                    let last = regexp.get("lastIndex").unwrap();
                    result["lastIndex"] = previous.map_or_else(
                        || project(&last),
                        |previous| json!({"same": last.same_reference(previous)}),
                    );
                }
                if replace {
                    result["callbackArgs"] = json!(*self.callback_args.borrow());
                }
                result
            }
        }
    }
}
pub fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
        .collect()
}
pub fn project(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind": "undefined"}),
        Js::Null => json!({"kind": "null"}),
        Js::String(units) => json!({"kind": "string", "units": units.as_ref()}),
        Js::Number(number) => {
            json!({"kind": "number", "bits": format!("{:016x}", number.to_bits())})
        }
        Js::Array(_) => {
            let length = value.get("length").unwrap().as_f64().unwrap() as usize;
            json!({"kind": "array", "items": (0..length).map(|index| project(&value.get(&index.to_string()).unwrap())).collect::<Vec<_>>()})
        }
        _ => json!({"kind": "object"}),
    }
}
