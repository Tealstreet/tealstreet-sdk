//! Linked identity survives optional-field omission and snapshot readmission.
use serde::{Deserialize, Deserializer, de::IgnoredAny};
use std::collections::{BTreeMap, HashSet};

const MAX_IDS: usize = 10_000;
#[derive(Default)]
pub(crate) struct Linkage {
    ids: HashSet<String>,
    exhausted: bool,
}
fn id(value: &str) -> Result<&str, String> {
    if !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control) {
        Ok(value)
    } else {
        Err("drb invalid linked order identity".into())
    }
}
fn present<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some) // Absent is optional; present null is not a value.
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Children {
    Ids(Vec<String>),
    Marker(BTreeMap<String, IgnoredAny>),
}
#[derive(Deserialize)]
struct Fields {
    order_id: String,
    #[serde(default, deserialize_with = "present")]
    is_primary_otoco: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    is_secondary_oto: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    primary_order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    trigger_order_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    oco_ref: Option<String>,
    #[serde(default, deserialize_with = "present")]
    oto_order_ids: Option<Children>,
}
impl Linkage {
    pub(crate) fn insert(&mut self, id: String) {
        if self.ids.contains(&id) {
            return;
        }
        if self.ids.len() == MAX_IDS {
            // Never evict a live protection marker to make an edit look safe.
            self.exhausted = true;
        } else {
            self.ids.insert(id);
        }
    }
    pub(crate) fn blocks(&self, id: &str) -> bool {
        self.exhausted || self.ids.contains(id)
    }
    pub(crate) fn merge(&mut self, other: Self) {
        self.exhausted |= other.exhausted;
        for id in other.ids {
            self.insert(id);
        }
    }
    /// Stage this alongside decoded rows; publish only after the whole event
    /// passes validation. Absent optional linkage never revokes earlier proof.
    pub(crate) fn observe<'de, D: Deserializer<'de>>(&mut self, row: D) -> Result<(), String> {
        let row = Fields::deserialize(row).map_err(|_| "drb invalid linked order fields")?;
        let mut linked = row.is_primary_otoco == Some(true) || row.is_secondary_oto == Some(true);
        for parent in [row.primary_order_id, row.trigger_order_id]
            .into_iter()
            .flatten()
        {
            self.insert(id(&parent)?.to_owned());
            linked = true;
        }
        if let Some(group) = row.oco_ref {
            linked |= !group.is_empty(); // A group reference is not a native order ID.
        }
        if let Some(children) = row.oto_order_ids {
            // OpenAPI defines an array of native IDs; AsyncAPI defines an opaque
            // object. Recognize the stream marker without guessing its map shape.
            match children {
                Children::Ids(children) => {
                    if children.len() > MAX_IDS {
                        return Err("drb linked order limit".into());
                    }
                    linked |= !children.is_empty();
                    for child in children {
                        self.insert(id(&child)?.to_owned());
                    }
                }
                Children::Marker(children) => {
                    linked |= !children.is_empty();
                }
            }
        }
        if linked {
            self.insert(id(&row.order_id)?.to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_exhaustion_never_evicts_protection_or_allows_unproven_edits() {
        let mut links = Linkage::default();
        for i in 0..MAX_IDS {
            links.insert(format!("synthetic-{i}"));
        }
        assert!(links.blocks("synthetic-0"));
        assert!(!links.blocks("ordinary"));
        links.insert("one-more".into());
        assert_eq!(links.ids.len(), MAX_IDS);
        assert!(links.blocks("synthetic-0"));
        assert!(links.blocks("ordinary"));
    }
}
