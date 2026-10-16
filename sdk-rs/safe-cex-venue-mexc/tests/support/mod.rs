use serde::{Serialize, Serializer, ser};
use serde_json::{Map, Value, json};

pub fn encode(value: &impl Serialize) -> Value {
    value.serialize(Numbers).unwrap()
}
pub fn numeric(n: f64) -> Value {
    if n.is_nan() {
        json!({"$number":"7ff8000000000000"})
    } else {
        json!({"$number":format!("{:016x}",n.to_bits())})
    }
}
pub fn decode_number(v: &Value) -> f64 {
    if let Some(n) = v.as_f64() {
        return n;
    }
    let s = v["$number"].as_str().unwrap();
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}
struct Numbers;
struct Compound {
    value: Value,
    key: Option<String>,
    number: bool,
    variant: Option<&'static str>,
}
impl Compound {
    fn new(value: Value) -> Self {
        Self {
            value,
            key: None,
            number: false,
            variant: None,
        }
    }
    fn finish(self) -> Result<Value, serde_json::Error> {
        Ok(if let Some(key) = self.variant {
            json!({key:self.value})
        } else {
            self.value
        })
    }
    fn field<T: Serialize + ?Sized>(&mut self, key: &str, v: &T) -> Result<(), serde_json::Error> {
        if self.number {
            let value = serde_json::to_value(v)?;
            self.value = numeric(value.as_str().unwrap().parse().unwrap_or(f64::INFINITY));
        } else {
            self.value
                .as_object_mut()
                .unwrap()
                .insert(key.into(), v.serialize(Numbers)?);
        }
        Ok(())
    }
    fn element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), serde_json::Error> {
        self.value
            .as_array_mut()
            .unwrap()
            .push(v.serialize(Numbers)?);
        Ok(())
    }
}
impl Serializer for Numbers {
    type Ok = Value;
    type Error = serde_json::Error;
    type SerializeSeq = Compound;
    type SerializeTuple = Compound;
    type SerializeTupleStruct = Compound;
    type SerializeTupleVariant = Compound;
    type SerializeMap = Compound;
    type SerializeStruct = Compound;
    type SerializeStructVariant = Compound;
    fn serialize_bool(self, v: bool) -> Result<Value, Self::Error> {
        Ok(json!(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_i16(self, v: i16) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_i32(self, v: i32) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_i64(self, v: i64) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u8(self, v: u8) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u16(self, v: u16) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u32(self, v: u32) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u64(self, v: u64) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_f32(self, v: f32) -> Result<Value, Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_f64(self, v: f64) -> Result<Value, Self::Error> {
        Ok(numeric(v))
    }
    fn serialize_char(self, v: char) -> Result<Value, Self::Error> {
        Ok(json!(v.to_string()))
    }
    fn serialize_str(self, v: &str) -> Result<Value, Self::Error> {
        Ok(json!(v))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Value, Self::Error> {
        v.serialize(self)
    }
    fn serialize_none(self) -> Result<Value, Self::Error> {
        Ok(Value::Null)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Value, Self::Error> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<Value, Self::Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, Self::Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        v: &'static str,
    ) -> Result<Value, Self::Error> {
        Ok(json!(v))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<Value, Self::Error> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        k: &'static str,
        v: &T,
    ) -> Result<Value, Self::Error> {
        Ok(json!({k:v.serialize(self)?}))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound, Self::Error> {
        Ok(Compound::new(json!([])))
    }
    fn serialize_tuple(self, n: usize) -> Result<Compound, Self::Error> {
        self.serialize_seq(Some(n))
    }
    fn serialize_tuple_struct(self, _: &'static str, n: usize) -> Result<Compound, Self::Error> {
        self.serialize_tuple(n)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        k: &'static str,
        n: usize,
    ) -> Result<Compound, Self::Error> {
        let mut c = self.serialize_tuple(n)?;
        c.variant = Some(k);
        Ok(c)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound, Self::Error> {
        Ok(Compound::new(Value::Object(Map::new())))
    }
    fn serialize_struct(self, name: &'static str, n: usize) -> Result<Compound, Self::Error> {
        let mut c = self.serialize_map(Some(n))?;
        c.number = name == "$serde_json::private::Number";
        Ok(c)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        k: &'static str,
        n: usize,
    ) -> Result<Compound, Self::Error> {
        let mut c = self.serialize_map(Some(n))?;
        c.variant = Some(k);
        Ok(c)
    }
}
impl ser::SerializeSeq for Compound {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<Value, Self::Error> {
        self.finish()
    }
}
impl ser::SerializeTuple for Compound {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<Value, Self::Error> {
        self.finish()
    }
}
impl ser::SerializeTupleStruct for Compound {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<Value, Self::Error> {
        self.finish()
    }
}
impl ser::SerializeTupleVariant for Compound {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<Value, Self::Error> {
        self.finish()
    }
}
impl ser::SerializeStruct for Compound {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        self.field(k, v)
    }
    fn end(self) -> Result<Value, Self::Error> {
        self.finish()
    }
}
impl ser::SerializeStructVariant for Compound {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        self.field(k, v)
    }
    fn end(self) -> Result<Value, Self::Error> {
        self.finish()
    }
}
impl ser::SerializeMap for Compound {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, k: &T) -> Result<(), Self::Error> {
        self.key = Some(serde_json::to_value(k)?.as_str().unwrap().into());
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        let k = self.key.take().unwrap();
        self.field(&k, v)
    }
    fn end(self) -> Result<Value, Self::Error> {
        self.finish()
    }
}
