//! Native SCXF UI record codec. Revision one uses Rust strings as UTF-8 bytes.
//! Serde writes typed native records directly; no intermediate JSON tree.
use serde::{
    Serialize,
    ser::{
        self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
        SerializeTupleStruct, SerializeTupleVariant,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum Section {
    SocketHealth = 26,
    ActiveChasers = 27,
    ActiveTwaps = 28,
    OrdersRisk = 29,
    DisplayedPositions = 30,
    DisplayedMargin = 31,
    BalanceInfo = 32,
    ManagedOrders = 33,
}

pub fn encode<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    let mut encoder = Encoder { bytes: vec![0; 4] };
    value.serialize(&mut encoder).expect("native frame record");
    let size = (encoder.bytes.len() - 4) as u32;
    encoder.bytes[..4].copy_from_slice(&size.to_le_bytes());
    while !encoder.bytes.len().is_multiple_of(8) {
        encoder.bytes.push(0);
    }
    encoder.bytes
}
pub fn empty(time: f64) -> Vec<u8> {
    let mut out = b"SCXF\x01\0\0\0".to_vec();
    out.extend_from_slice(&time.to_le_bytes());
    out
}
pub fn append(frame: &mut Vec<u8>, section: Section, payload: &[u8]) {
    assert!(frame.len() >= 16 && &frame[..4] == b"SCXF");
    let count = u16::from_le_bytes([frame[6], frame[7]])
        .checked_add(1)
        .expect("section count");
    frame[6..8].copy_from_slice(&count.to_le_bytes());
    frame.extend_from_slice(&(section as u16).to_le_bytes());
    frame.extend_from_slice(&1u16.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
}
struct Encoder {
    bytes: Vec<u8>,
}
impl Encoder {
    fn text(&mut self, s: &str) {
        self.bytes
            .extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.bytes.extend_from_slice(s.as_bytes());
    }
    fn compound(&mut self, tag: u8) -> Compound<'_> {
        self.bytes.push(tag);
        let count_at = self.bytes.len();
        self.bytes.extend_from_slice(&0u32.to_le_bytes());
        Compound {
            encoder: self,
            count_at,
            count: 0,
        }
    }
}
struct Compound<'a> {
    encoder: &'a mut Encoder,
    count_at: usize,
    count: u32,
}
impl Compound<'_> {
    fn item<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), serde_json::Error> {
        v.serialize(&mut *self.encoder)?;
        self.count += 1;
        Ok(())
    }
    fn finish(self) -> Result<(), serde_json::Error> {
        self.encoder.bytes[self.count_at..self.count_at + 4]
            .copy_from_slice(&self.count.to_le_bytes());
        Ok(())
    }
}
impl<'a> ser::Serializer for &'a mut Encoder {
    type Ok = ();
    type Error = serde_json::Error;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;
    fn serialize_bool(self, v: bool) -> Result<(), Self::Error> {
        self.bytes.push(if v { 3 } else { 2 });
        Ok(())
    }
    fn serialize_f64(self, v: f64) -> Result<(), Self::Error> {
        self.bytes.push(4);
        self.bytes.extend_from_slice(&v.to_le_bytes());
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<(), Self::Error> {
        self.serialize_f64(f64::from(v))
    }
    fn serialize_i8(self, v: i8) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_i16(self, v: i16) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_i32(self, v: i32) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_i64(self, v: i64) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u8(self, v: u8) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u16(self, v: u16) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u32(self, v: u32) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_u64(self, v: u64) -> Result<(), Self::Error> {
        self.serialize_f64(v as f64)
    }
    fn serialize_str(self, v: &str) -> Result<(), Self::Error> {
        self.bytes.push(5);
        self.text(v);
        Ok(())
    }
    fn serialize_char(self, v: char) -> Result<(), Self::Error> {
        self.serialize_str(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Self::Error> {
        let mut seq = self.serialize_seq(Some(v.len()))?;
        for b in v {
            SerializeSeq::serialize_element(&mut seq, b)?;
        }
        SerializeSeq::end(seq)
    }
    fn serialize_none(self) -> Result<(), Self::Error> {
        self.serialize_unit()
    }
    fn serialize_unit(self) -> Result<(), Self::Error> {
        self.bytes.push(1);
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), Self::Error> {
        v.serialize(self)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Self::Error> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        v: &'static str,
    ) -> Result<(), Self::Error> {
        self.serialize_str(v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        let mut map = self.serialize_map(Some(1))?;
        SerializeMap::serialize_entry(&mut map, name, v)?;
        SerializeMap::end(map)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(self.compound(6))
    }
    fn serialize_tuple(self, n: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(n))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        n: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(n))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(ser::Error::custom("tuple variants are not frame records"))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(self.compound(7))
    }
    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(None)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(ser::Error::custom("struct variants are not frame records"))
    }
}
impl SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        self.encoder.text(key);
        self.item(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        self.encoder.text(key);
        self.item(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        let start = self.encoder.bytes.len();
        key.serialize(&mut *self.encoder)?;
        if self.encoder.bytes[start] != 5 {
            return Err(ser::Error::custom("frame record keys must be strings"));
        }
        self.encoder.bytes.remove(start);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.item(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_record_bytes_and_revision() {
        assert_eq!(
            encode("é"),
            [7, 0, 0, 0, 5, 2, 0, 0, 0, 0xc3, 0xa9, 0, 0, 0, 0, 0]
        );
        let mut f = empty(42.);
        append(&mut f, Section::BalanceInfo, &encode(&-0.0));
        assert_eq!(&f[18..20], &1u16.to_le_bytes());
        assert_eq!(&f[29..37], &(-0.0f64).to_le_bytes());
    }
}
