// ts: ApiType (binancepm.api.ts)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiType {
    Papi,
    Fapi,
}
impl<'de> serde::Deserialize<'de> for ApiType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum Wire {
            Papi,
            Fapi,
        }
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        let value =
            <Wire as serde::Deserialize>::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(text))?;
        Ok(match value {
            Wire::Papi => Self::Papi,
            Wire::Fapi => Self::Fapi,
        })
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
impl<'de> serde::Deserialize<'de> for BinanceOrderBookApplyResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum Wire {
            Applied,
            Ignored,
            Gap,
        }
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        let value =
            <Wire as serde::Deserialize>::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(text))?;
        Ok(match value {
            Wire::Applied => Self::Applied,
            Wire::Ignored => Self::Ignored,
            Wire::Gap => Self::Gap,
        })
    }
}
