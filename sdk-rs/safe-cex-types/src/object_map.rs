//! Ordered own data properties. Prototype mutation is deliberately a separate operation.
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// String-keyed dictionary with OrdinaryOwnPropertyKeys enumeration after every insertion.
/// Integer indices ascend; all other names retain insertion order, including replacements.
#[derive(Debug, Clone, PartialEq)]
pub struct JsObjectMap<T>(IndexMap<String, T>);

impl<T> Default for JsObjectMap<T> {
    fn default() -> Self {
        Self(IndexMap::new())
    }
}
impl<T> JsObjectMap<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, key: String, value: T) -> Option<T> {
        let previous = self.0.insert(key, value);
        self.0.sort_by(|a, _, b, _| {
            let rank = |key: &str| {
                crate::js_coercion::js_array_index(key).map_or((1, 0), |index| (0, index))
            };
            rank(a).cmp(&rank(b))
        });
        previous
    }
    /// Removing an own key preserves the order of all remaining properties.
    pub fn remove(&mut self, key: &str) -> Option<T> {
        self.0.shift_remove(key)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut T> {
        self.0.get_mut(key)
    }
}
// No DerefMut: callers cannot bypass insertion ordering with IndexMap::entry/insert.
impl<T> std::ops::Deref for JsObjectMap<T> {
    type Target = IndexMap<String, T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> FromIterator<(String, T)> for JsObjectMap<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(iter: I) -> Self {
        let mut result = Self::new();
        result.extend(iter);
        result
    }
}
impl<T> Extend<(String, T)> for JsObjectMap<T> {
    fn extend<I: IntoIterator<Item = (String, T)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}
impl<T> IntoIterator for JsObjectMap<T> {
    type Item = (String, T);
    type IntoIter = indexmap::map::IntoIter<String, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl<'a, T> IntoIterator for &'a JsObjectMap<T> {
    type Item = (&'a String, &'a T);
    type IntoIter = indexmap::map::Iter<'a, String, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
impl<T: Serialize> Serialize for JsObjectMap<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
// Module scope keeps this generic decoder visitor visible to compiler inventory.
struct ObjectVisitor<T>(std::marker::PhantomData<T>);
impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
    type Value = JsObjectMap<T>;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an object")
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut result = JsObjectMap::new();
        while let Some((key, value)) = map.next_entry()? {
            result.insert(key, value);
        }
        Ok(result)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for JsObjectMap<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
    }
}
