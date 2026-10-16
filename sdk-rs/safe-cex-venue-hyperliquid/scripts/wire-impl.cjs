// Emit explicit declarations: the parity extractor intentionally does not expand macros.
module.exports = (kind, name, wire) => `
impl Serialize for ${name} {
 fn serialize<S: serde::Serializer>(&self, serializer:S)->Result<S::Ok,S::Error> { ${wire}::serialize(self,serializer) }
}
impl<'de> Deserialize<'de> for ${name} {
 fn deserialize<D: serde::Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
 ${kind === 'numeric_union' ? `let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
 input.decode_with(${wire}::deserialize).map_err(serde::de::Error::custom)` : kind === 'literal' ? `let text = String::deserialize(deserializer)?;
 ${wire}::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&text))` : `struct OnlyMap;
 impl<'de> serde::de::Visitor<'de> for OnlyMap {
  type Value=${name};
  fn expecting(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str("an object record") }
  fn visit_map<A:serde::de::MapAccess<'de>>(self,map:A)->Result<Self::Value,A::Error> {
   ${wire}::deserialize(serde::de::value::MapAccessDeserializer::new(map))
  }
 }
 let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
 input.decode_with(|decoder| serde::Deserializer::deserialize_map(decoder, OnlyMap))
  .map_err(serde::de::Error::custom)`}
 }
}
`;
