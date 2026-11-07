//! Presence at a model boundary, before JavaScript wire serialization.
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
    ser::SerializeStruct,
};
use std::fmt;

/// The native serializer/decoder use these hints; ordinary JSON still omits
/// absent/undefined object properties and refuses null for a non-nullable T.
pub const MODEL_RECORD: &str = "$safe_cex::ModelRecord";
pub const INFO_RECORD: &str = "$safe_cex::InfoRecord";
pub const MODEL_FIELD: &str = "$safe_cex::ModelField";
pub const UNDEFINED: &str = "$safe_cex::Undefined";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelField<T> {
    #[default]
    Absent,
    Undefined,
    Value(T),
}
impl<T> ts_rs::IsOption for ModelField<T> {
    type Inner = T;
}
impl<T> ModelField<T> {
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
    pub fn is_undefined(&self) -> bool {
        matches!(self, Self::Undefined)
    }
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Absent | Self::Undefined => None,
        }
    }
    /// Reading a missing property and assigning it in an object literal creates
    /// an own undefined property. Cloning a record itself keeps Absent intact.
    pub fn into_property(self) -> Self {
        match self {
            Self::Absent => Self::Undefined,
            other => other,
        }
    }
    pub fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }
    pub fn into_value(self) -> Option<T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Absent | Self::Undefined => None,
        }
    }
    pub fn as_ref(&self) -> Option<&T> {
        self.value()
    }
    pub fn as_field_ref(&self) -> ModelField<&T> {
        match self {
            Self::Value(value) => ModelField::Value(value),
            Self::Absent => ModelField::Absent,
            Self::Undefined => ModelField::Undefined,
        }
    }
}
impl<T> ModelField<T> {
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Value(_))
    }
    pub fn is_none(&self) -> bool {
        !self.is_some()
    }
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Option<U> {
        self.into_value().map(f)
    }
    pub fn and_then<U>(self, f: impl FnOnce(T) -> Option<U>) -> Option<U> {
        self.into_value().and_then(f)
    }
    pub fn unwrap(self) -> T {
        self.into_value().unwrap()
    }
    pub fn expect(self, message: &str) -> T {
        self.into_value().expect(message)
    }
    pub fn unwrap_or(self, default: T) -> T {
        self.into_value().unwrap_or(default)
    }
    pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
        self.into_value().unwrap_or_else(f)
    }
    pub fn unwrap_or_default(self) -> T
    where
        T: Default,
    {
        self.into_value().unwrap_or_default()
    }
    pub fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool {
        self.into_value().is_some_and(f)
    }
    pub fn filter(self, f: impl FnOnce(&T) -> bool) -> Self {
        match self {
            Self::Value(value) if f(&value) => Self::Value(value),
            Self::Value(_) => Self::Absent,
            other => other,
        }
    }
    pub fn ok_or_else<E>(self, f: impl FnOnce() -> E) -> Result<T, E> {
        self.into_value().ok_or_else(f)
    }
    pub fn get_or_insert_with(&mut self, f: impl FnOnce() -> T) -> &mut T {
        if !self.is_some() {
            *self = Self::Value(f());
        }
        self.as_mut().expect("just inserted")
    }
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
    pub fn or(self, other: impl Into<Self>) -> Self {
        if self.is_some() { self } else { other.into() }
    }
    pub fn or_else<V: Into<Self>>(self, f: impl FnOnce() -> V) -> Self {
        if self.is_some() { self } else { f().into() }
    }
    pub fn as_deref(&self) -> Option<&T::Target>
    where
        T: std::ops::Deref,
    {
        self.value().map(std::ops::Deref::deref)
    }
}
impl<T: PartialEq> PartialEq<Option<T>> for ModelField<T> {
    fn eq(&self, other: &Option<T>) -> bool {
        self.value() == other.as_ref()
    }
}
impl<T: Serialize> ModelField<T> {
    /// Call only from a MODEL_RECORD serializer. A skipped field records an own
    /// undefined property in the native carrier; JSON's skip is an omission.
    pub fn serialize_field<S: SerializeStruct>(
        &self,
        state: &mut S,
        name: &'static str,
    ) -> Result<(), S::Error> {
        match self {
            Self::Absent => Ok(()),
            Self::Undefined => state.skip_field(name),
            Self::Value(value) => state.serialize_field(name, value),
        }
    }
}
impl<T> From<T> for ModelField<T> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}
impl<T> From<Option<T>> for ModelField<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Value(value),
            None => Self::Absent,
        }
    }
}
impl<T: PartialEq> PartialEq<T> for ModelField<T> {
    fn eq(&self, other: &T) -> bool {
        matches!(self, Self::Value(value) if value == other)
    }
}
impl<T: Serialize> Serialize for ModelField<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Value(value) => value.serialize(serializer),
            Self::Absent | Self::Undefined => serializer.serialize_unit_struct(UNDEFINED),
        }
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ModelField<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for FieldVisitor<T> {
            type Value = ModelField<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a model field or native undefined")
            }
            fn visit_newtype_struct<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                T::deserialize(deserializer).map(ModelField::Value)
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(ModelField::Undefined)
            }
        }
        deserializer.deserialize_newtype_struct(MODEL_FIELD, FieldVisitor(std::marker::PhantomData))
    }
}
impl ModelField<f64> {
    /// Number(undefined) is NaN; storage retains the distinct presence state.
    pub fn to_number(&self) -> f64 {
        self.value().copied().unwrap_or(f64::NAN)
    }
}

pub fn deserialize_info<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ModelField<crate::retained_json::RetainedJsonInput>, D::Error> {
    ModelField::deserialize(deserializer)
}

impl PartialEq<ModelField<f64>> for f64 {
    fn eq(&self, other: &ModelField<f64>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::OrderType>> for crate::OrderType {
    fn eq(&self, other: &ModelField<crate::OrderType>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::OrderSide>> for crate::OrderSide {
    fn eq(&self, other: &ModelField<crate::OrderSide>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::OrderStatus>> for crate::OrderStatus {
    fn eq(&self, other: &ModelField<crate::OrderStatus>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::OrderTimeInForce>> for crate::OrderTimeInForce {
    fn eq(&self, other: &ModelField<crate::OrderTimeInForce>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::OrderStopTriggerType>> for crate::OrderStopTriggerType {
    fn eq(&self, other: &ModelField<crate::OrderStopTriggerType>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::PositionSide>> for crate::PositionSide {
    fn eq(&self, other: &ModelField<crate::PositionSide>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::PositionMode>> for crate::PositionMode {
    fn eq(&self, other: &ModelField<crate::PositionMode>) -> bool {
        other == self
    }
}
impl PartialEq<ModelField<crate::MarginType>> for crate::MarginType {
    fn eq(&self, other: &ModelField<crate::MarginType>) -> bool {
        other == self
    }
}
