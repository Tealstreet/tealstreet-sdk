// ts: ApiType (binancepm.api.ts)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiType {
    Papi,
    Fapi,
}
#[derive(serde::Deserialize)]
#[serde(remote = "ApiType", rename_all = "lowercase")]
enum ApiTypeWire {
    Papi,
    Fapi,
}
impl<'de> serde::Deserialize<'de> for ApiType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        ApiTypeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(text))
    }
}

// ts: BinanceOrderBookApplyResult
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BinanceOrderBookApplyResult {
    Applied,
    Ignored,
    Gap,
}
#[derive(serde::Deserialize)]
#[serde(remote = "BinanceOrderBookApplyResult", rename_all = "lowercase")]
enum BinanceOrderBookApplyResultWire {
    Applied,
    Ignored,
    Gap,
}
impl<'de> serde::Deserialize<'de> for BinanceOrderBookApplyResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        BinanceOrderBookApplyResultWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )
    }
}
