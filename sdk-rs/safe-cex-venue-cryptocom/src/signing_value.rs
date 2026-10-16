//! Ordinary JSON signing data with primitive values and shared object/array identities.
use safe_cex_types::object_map::JsObjectMap;
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
enum Node {
    Scalar(Value),
    Array(Arc<Mutex<Vec<SigningValue>>>),
    Object(Arc<Mutex<JsObjectMap<SigningValue>>>),
}

/// Cloning captures a JS value: primitives copy, while objects and arrays retain identity.
/// Operations expose owned values, never a lock guard. Cycles are outside the signing domain.
#[derive(Debug, Clone)]
pub struct SigningValue(Node);

impl SigningValue {
    pub fn from_value(value: Value) -> Self {
        Self(match value {
            Value::Array(values) => Node::Array(Arc::new(Mutex::new(
                values.into_iter().map(Self::from_value).collect(),
            ))),
            Value::Object(values) => Node::Object(Arc::new(Mutex::new(
                values
                    .into_iter()
                    .map(|(key, value)| (key, Self::from_value(value)))
                    .collect(),
            ))),
            scalar => Node::Scalar(scalar),
        })
    }

    /// In-memory tree for the synchronous serializer, not the returned request's params.
    pub fn to_value(&self) -> Value {
        match &self.0 {
            Node::Scalar(value) => value.clone(),
            Node::Array(values) => {
                let children = values.lock().unwrap().clone();
                Value::Array(children.iter().map(Self::to_value).collect())
            }
            Node::Object(values) => {
                let children = values.lock().unwrap().clone();
                Value::Object(
                    children
                        .iter()
                        .map(|(key, value)| (key.clone(), value.to_value()))
                        .collect(),
                )
            }
        }
    }

    // The same traversal as paramsToString over retained handles. Visit in source
    // order and propagate errors before reading later siblings, without a full tree copy.
    pub(crate) fn params_to_string(
        &self,
        level: usize,
    ) -> Result<String, safe_cex_num::JsCoercionError> {
        if level >= 3 {
            return self.coerce_string();
        }
        match &self.0 {
            Node::Scalar(value) => safe_cex_num::js_string(Some(value)),
            Node::Array(values) => {
                let children = values.lock().unwrap().clone();
                let mut output = String::new();
                for child in children {
                    output.push_str(&child.params_to_string(level + 1)?);
                }
                Ok(output)
            }
            Node::Object(values) => {
                let children = values.lock().unwrap().clone();
                let mut entries: Vec<_> = children.iter().collect();
                entries
                    .sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
                let mut output = String::new();
                for (key, child) in entries {
                    output.push_str(key);
                    output.push_str(&child.params_to_string(level + 1)?);
                }
                Ok(output)
            }
        }
    }

    fn coerce_string(&self) -> Result<String, safe_cex_num::JsCoercionError> {
        match &self.0 {
            Node::Scalar(value) => safe_cex_num::js_string(Some(value)),
            Node::Array(values) => {
                let children = values.lock().unwrap().clone();
                let mut output = String::new();
                for (index, child) in children.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    if !matches!(&child.0, Node::Scalar(Value::Null)) {
                        output.push_str(&child.coerce_string()?);
                    }
                }
                Ok(output)
            }
            Node::Object(values) => {
                let has_to_string = values.lock().unwrap().contains_key("toString");
                let mut shape = serde_json::Map::new();
                if has_to_string {
                    shape.insert("toString".into(), Value::Null);
                }
                safe_cex_num::js_string(Some(&Value::Object(shape)))
            }
        }
    }

    pub fn same_reference(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Node::Array(a), Node::Array(b)) => Arc::ptr_eq(a, b),
            (Node::Object(a), Node::Object(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    pub fn property(&self, key: &str) -> Option<Self> {
        match &self.0 {
            Node::Object(values) => values.lock().unwrap().get(key).cloned(),
            _ => None,
        }
    }

    pub fn element(&self, index: usize) -> Option<Self> {
        match &self.0 {
            Node::Array(values) => values.lock().unwrap().get(index).cloned(),
            _ => None,
        }
    }

    // The process-wide edit lock makes cycle-check + insertion atomic even if a host
    // invokes separate handles on separate threads. Readers only take individual node locks.
    pub fn set_property(&self, key: String, value: Self) -> bool {
        let _edit = EDIT.lock().unwrap();
        let Node::Object(values) = &self.0 else {
            return false;
        };
        if value.contains(self, &mut Vec::new()) {
            return false;
        }
        values.lock().unwrap().insert(key, value);
        true
    }

    /// Set an existing dense slot or append at len; sparse arrays are outside this JSON domain.
    pub fn set_element(&self, index: usize, value: Self) -> bool {
        let _edit = EDIT.lock().unwrap();
        let Node::Array(values) = &self.0 else {
            return false;
        };
        if value.contains(self, &mut Vec::new()) {
            return false;
        }
        let mut values = values.lock().unwrap();
        if index > values.len() {
            return false;
        }
        if index == values.len() {
            values.push(value);
        } else {
            values[index] = value;
        }
        true
    }

    pub fn remove_property(&self, key: &str) -> Option<Self> {
        match &self.0 {
            Node::Object(values) => values.lock().unwrap().remove(key),
            _ => None,
        }
    }

    pub fn remove_element(&self, index: usize) -> Option<Self> {
        let Node::Array(values) = &self.0 else {
            return None;
        };
        let mut values = values.lock().unwrap();
        (index < values.len()).then(|| values.remove(index))
    }

    fn contains(&self, target: &Self, visited: &mut Vec<Self>) -> bool {
        if self.same_reference(target) {
            return true;
        }
        if visited.iter().any(|value| self.same_reference(value)) {
            return false;
        }
        let children: Vec<_> = match &self.0 {
            Node::Scalar(_) => return false,
            Node::Array(values) => values.lock().unwrap().clone(),
            Node::Object(values) => values.lock().unwrap().values().cloned().collect(),
        };
        visited.push(self.clone());
        children.iter().any(|value| value.contains(target, visited))
    }
}
static EDIT: Mutex<()> = Mutex::new(());
