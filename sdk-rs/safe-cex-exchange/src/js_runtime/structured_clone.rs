//! HTML structured clone for native worker graphs, without JSON materialization.
use super::value::{Js, Prop, Thrown, error_object, utf16};
use std::{cell::RefCell, rc::Rc};

pub fn structured_clone(value: &Js) -> Result<Js, Thrown> {
    copy(value, &mut Vec::new())
}

fn hidden(target: &Js, name: &str, value: Js) {
    let Js::Object(object) = target else {
        unreachable!()
    };
    object.props.borrow_mut().insert(
        utf16(name),
        Prop::DataDescriptor {
            value,
            writable: true,
            enumerable: false,
            configurable: true,
        },
    );
}

fn copy(value: &Js, seen: &mut Vec<(Js, Js)>) -> Result<Js, Thrown> {
    match value {
        Js::Number(value) if value.is_nan() => return Ok(Js::Number(f64::NAN)),
        Js::Undefined | Js::Null | Js::Bool(_) | Js::Number(_) | Js::BigInt(_) | Js::String(_) => {
            return Ok(value.clone());
        }
        Js::SymbolIterator | Js::Builtin(_) | Js::Function(_) | Js::Promise(_) => {
            return Err(Thrown::error("DataCloneError", "Value could not be cloned"));
        }
        _ => {}
    }
    if let Some((_, output)) = seen.iter().find(|(old, _)| old.strict_equals(value)) {
        return Ok(output.clone());
    }
    let output = match value {
        Js::Array(array) => {
            let output = Js::array(vec![]);
            output.set("length", Js::Number(array.len() as f64))?;
            output
        }
        Js::Object(object) if object.class == "Object" => Js::object(vec![]),
        Js::Object(object) if object.class == "Error" => {
            let name = value.get("name")?;
            let selected = [
                "Error",
                "EvalError",
                "RangeError",
                "ReferenceError",
                "SyntaxError",
                "TypeError",
                "URIError",
            ]
            .into_iter()
            .find(|candidate| name.strict_equals(&Js::str(candidate)))
            .unwrap_or("Error");
            let output = error_object(selected, "");
            output.delete(&utf16("message"))?;
            output
        }
        Js::Map(_) => Js::Map(Rc::new(RefCell::new(Default::default()))),
        Js::Set(_) => Js::Set(Rc::new(RefCell::new(Default::default()))),
        _ => {
            return Err(Thrown::error(
                "DataCloneError",
                "Native object kind could not be cloned",
            ));
        }
    };
    seen.push((value.clone(), output.clone()));
    match value {
        Js::Map(entries) => {
            let entries: Vec<_> = entries.borrow().iter().cloned().collect();
            let Js::Map(target) = &output else {
                unreachable!()
            };
            for (key, child) in entries {
                let key = copy(&key, seen)?;
                let child = copy(&child, seen)?;
                target.borrow_mut().push((key, child));
            }
        }
        Js::Set(entries) => {
            let entries: Vec<_> = entries.borrow().iter().cloned().collect();
            let Js::Set(target) = &output else {
                unreachable!()
            };
            for child in entries {
                target.borrow_mut().push(copy(&child, seen)?);
            }
        }
        Js::Object(object) if object.class == "Error" => {
            let descriptor = value.own_property_descriptor(&utf16("message"))?;
            if descriptor.has_own("value") {
                let message = descriptor.get("value")?;
                let message = if message.is_object() {
                    message.ordinary_to_primitive(["toString", "valueOf"])?
                } else {
                    message
                };
                hidden(&output, "message", Js::String(message.to_js_string()?));
            }
            let stack = value.get("stack")?;
            hidden(
                &output,
                "stack",
                if matches!(stack, Js::String(_)) {
                    stack
                } else {
                    Js::Undefined
                },
            );
            let descriptor = value.own_property_descriptor(&utf16("cause"))?;
            if descriptor.has_own("value") {
                hidden(&output, "cause", copy(&descriptor.get("value")?, seen)?);
            }
        }
        _ => {
            let mut keys = Vec::new();
            for key in value.own_keys() {
                let descriptor = value.own_property_descriptor(&key)?;
                if !descriptor.is_undefined() && descriptor.get("enumerable")?.truthy() {
                    keys.push(key);
                }
            }
            for key in keys {
                if value.own_property_descriptor(&key)?.is_undefined() {
                    continue;
                }
                let child = value.get_key(&key)?;
                output.define_key(&key, copy(&child, seen)?)?;
            }
        }
    }
    Ok(output)
}
