//! Own-dictionary writes with an explicit report of declined prototype effects.
//!
//! The dictionary retains literal keys and never mutates a prototype or global
//! referent. This is a CHOSEN security refusal, not reference parity.
use safe_cex_num::{ProtoMember, TableHit, js_table_lookup};
use safe_cex_types::object_map::JsObjectMap;

/// What the same operation on a stock JavaScript ordinary object would target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "a declined inherited write must be disclosed by the caller"]
pub enum ReferenceWrite {
    OwnProperty,
    ShadowInherited(ProtoMember),
    /// Whole-object assignment invokes the inherited setter, creating no own key.
    ReplacePrototype,
    /// A nested write targets the inherited object/function itself, not a setter.
    MutateInheritedReferent(ProtoMember),
}

/// Safe local write and reference mechanism are intentionally separate facts.
#[derive(Debug, Clone, PartialEq)]
#[must_use = "prototype effects are declined and must be disclosed by the caller"]
pub struct DictionaryWrite<T> {
    pub reference: ReferenceWrite,
    pub previous: Option<T>,
}

impl<T> DictionaryWrite<T> {
    pub fn declined_prototype_effect(&self) -> bool {
        matches!(
            self.reference,
            ReferenceWrite::ReplacePrototype | ReferenceWrite::MutateInheritedReferent(_)
        )
    }
}

fn inherited(key: &str) -> Option<ProtoMember> {
    match js_table_lookup::<&str, ()>(&[], key) {
        TableHit::Inherited(member) => Some(member),
        _ => None,
    }
}

/// Assign an object-valued entry. Own `__proto__` data properties shadow the
/// setter too; only a missing own property would replace the source prototype.
/// Rust deliberately creates an own entry instead, retaining the supplied row.
pub fn assign_object<T>(
    dictionary: &mut JsObjectMap<T>,
    key: String,
    value: T,
) -> DictionaryWrite<T> {
    let reference = if dictionary.contains_key(&key) {
        ReferenceWrite::OwnProperty
    } else {
        match inherited(&key) {
            Some(ProtoMember::Prototype) => ReferenceWrite::ReplacePrototype,
            Some(member) => ReferenceWrite::ShadowInherited(member),
            None => ReferenceWrite::OwnProperty,
        }
    };
    DictionaryWrite {
        reference,
        previous: dictionary.insert(key, value),
    }
}

/// Mutate an own entry, initializing a missing one locally. In source code an
/// inherited truthy referent suppresses initialization and receives the nested
/// mutation. Rust creates an own entry and reports that declined effect.
/// This API models object-valued rows; scalar/falsy JS entries require a distinct
/// caller branch and must not be passed off as this operation.
pub fn mutate_object<T>(
    dictionary: &mut JsObjectMap<T>,
    key: String,
    initialize: impl FnOnce() -> T,
    mutate: impl FnOnce(&mut T),
) -> ReferenceWrite {
    if let Some(value) = dictionary.get_mut(&key) {
        mutate(value);
        return ReferenceWrite::OwnProperty;
    }
    let reference = inherited(&key).map_or(
        ReferenceWrite::OwnProperty,
        ReferenceWrite::MutateInheritedReferent,
    );
    let mut value = initialize();
    mutate(&mut value);
    dictionary.insert(key, value);
    reference
}
