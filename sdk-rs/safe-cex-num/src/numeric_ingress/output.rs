//! Capture serialization events without JSON's non-finite-to-null projection.
use super::{JsNumberDeserializer, JsNumberInput, Node};
use serde::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};
use serde_json::Error;

pub(super) fn capture<T: Serialize + ?Sized>(value: &T) -> Result<JsNumberInput, Error> {
    value.serialize(Capture)
}
struct Capture;
struct Sequence {
    values: Vec<Node>,
    variant: Option<String>,
}
struct Object {
    values: Vec<(Vec<u16>, Node)>,
    key: Option<Vec<u16>>,
    name: Option<&'static str>,
    variant: Option<String>,
}

impl ser::Serializer for Capture {
    type Ok = JsNumberInput;
    type Error = Error;
    type SerializeSeq = Sequence;
    type SerializeTuple = Sequence;
    type SerializeTupleStruct = Sequence;
    type SerializeTupleVariant = Sequence;
    type SerializeMap = Object;
    type SerializeStruct = Object;
    type SerializeStructVariant = Object;
    fn is_human_readable(&self) -> bool {
        false
    }
    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value as f64)))
    }
    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Float(value)))
    }
    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Bool(v)))
    }
    fn serialize_char(self, v: char) -> Result<Self::Ok, Error> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Text(v.encode_utf16().collect())))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Array(
            v.iter().map(|v| Node::Float(f64::from(*v))).collect(),
        )))
    }
    fn serialize_none(self) -> Result<Self::Ok, Error> {
        self.serialize_unit()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Self::Ok, Error> {
        capture(v)
    }
    fn serialize_unit(self) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Null))
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Error> {
        if name == safe_cex_types::model_field::UNDEFINED {
            Ok(JsNumberInput(Node::Undefined))
        } else {
            self.serialize_unit()
        }
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        v: &'static str,
    ) -> Result<Self::Ok, Error> {
        self.serialize_str(v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Error> {
        let captured = capture(v)?;
        if name == safe_cex_types::retained_json::RETAINED_NATIVE {
            safe_cex_types::retained_json::deserialize_native_payload(JsNumberDeserializer(
                captured.0,
            ))
            .map(JsNumberInput)
        } else {
            Ok(captured)
        }
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Error> {
        Ok(JsNumberInput(Node::Object(vec![(
            variant.encode_utf16().collect(),
            capture(v)?.0,
        )])))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Sequence, Error> {
        Ok(Sequence {
            values: Vec::new(),
            variant: None,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Sequence, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Sequence, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Sequence, Error> {
        Ok(Sequence {
            values: Vec::new(),
            variant: Some(variant.to_owned()),
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Object, Error> {
        Ok(Object {
            values: Vec::new(),
            key: None,
            name: None,
            variant: None,
        })
    }
    fn serialize_struct(self, name: &'static str, _: usize) -> Result<Object, Error> {
        Ok(Object {
            values: Vec::new(),
            key: None,
            name: Some(name),
            variant: None,
        })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Object, Error> {
        Ok(Object {
            values: Vec::new(),
            key: None,
            name: None,
            variant: Some(variant.to_owned()),
        })
    }
}
impl Sequence {
    fn finish(self) -> JsNumberInput {
        let array = JsNumberInput(Node::Array(self.values));
        match self.variant {
            Some(name) => {
                JsNumberInput(Node::Object(vec![(name.encode_utf16().collect(), array.0)]))
            }
            None => array,
        }
    }
}
impl SerializeSeq for Sequence {
    type Ok = JsNumberInput;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(capture(v)?.0);
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Error> {
        Ok(self.finish())
    }
}
impl SerializeTuple for Sequence {
    type Ok = JsNumberInput;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(capture(v)?.0);
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Error> {
        Ok(self.finish())
    }
}
impl SerializeTupleStruct for Sequence {
    type Ok = JsNumberInput;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(capture(v)?.0);
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Error> {
        Ok(self.finish())
    }
}
impl SerializeTupleVariant for Sequence {
    type Ok = JsNumberInput;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(capture(v)?.0);
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Error> {
        Ok(self.finish())
    }
}
impl Object {
    fn finish(self) -> Result<JsNumberInput, Error> {
        if self.key.is_some() {
            return Err(ser::Error::custom("serialized key without value"));
        }
        if matches!(
            self.name,
            Some("$serde_json::private::Number" | "$serde_json::private::RawValue")
        ) {
            let mut fields = self.values.into_iter();
            let (_, Node::Text(text)) = fields
                .next()
                .ok_or_else(|| <Error as ser::Error>::custom("empty private JSON value"))?
            else {
                return Err(ser::Error::custom("invalid private JSON value"));
            };
            if fields.next().is_some() {
                return Err(ser::Error::custom("extra private JSON value fields"));
            }
            return serde_json::from_str::<JsNumberInput>(
                &String::from_utf16(&text).map_err(<Error as ser::Error>::custom)?,
            );
        }
        let object = JsNumberInput(Node::Object(self.values));
        Ok(match self.variant {
            Some(name) => JsNumberInput(Node::Object(vec![(
                name.encode_utf16().collect(),
                object.0,
            )])),
            None => object,
        })
    }
}
impl SerializeMap for Object {
    type Ok = JsNumberInput;
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        if self.key.is_some() {
            return Err(ser::Error::custom("serialized key without value"));
        }
        self.key = Some(match capture(key)?.0 {
            Node::Text(v) => v,
            Node::Float(v) if v.is_finite() => crate::js_number_string(v).encode_utf16().collect(),
            Node::Bool(v) => v.to_string().encode_utf16().collect(),
            _ => return Err(ser::Error::custom("invalid JSON object key")),
        });
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let key = self
            .key
            .take()
            .ok_or_else(|| <Error as ser::Error>::custom("serialized value without key"))?;
        self.values.push((key, capture(value)?.0));
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Error> {
        self.finish()
    }
}
impl SerializeStruct for Object {
    type Ok = JsNumberInput;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.values
            .push((key.encode_utf16().collect(), capture(value)?.0));
        Ok(())
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), Error> {
        if matches!(
            self.name,
            Some(
                safe_cex_types::model_field::MODEL_RECORD
                    | safe_cex_types::model_field::INFO_RECORD
            )
        ) {
            self.values
                .push((key.encode_utf16().collect(), Node::Undefined));
        }
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Error> {
        self.finish()
    }
}
impl SerializeStructVariant for Object {
    type Ok = JsNumberInput;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.values
            .push((key.encode_utf16().collect(), capture(value)?.0));
        Ok(())
    }
    fn end(self) -> Result<Self::Ok, Error> {
        self.finish()
    }
}
