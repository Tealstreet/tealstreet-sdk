use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TwapAmendment {
    pub id: String,
    pub update: TwapUpdate,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TwapUpdate {
    pub interval_ms: Option<f64>,
    pub remaining_slices: Option<u32>,
}

impl TwapAmendment {
    pub fn parse(data: &Value) -> Result<Self, &'static str> {
        let fields = data.as_object().ok_or("Invalid TWAP amendment")?;
        let update = data["update"].as_object().ok_or("Invalid TWAP amendment")?;
        if fields
            .keys()
            .any(|key| !matches!(key.as_str(), "id" | "update"))
            || update
                .keys()
                .any(|key| !matches!(key.as_str(), "intervalMs" | "remainingSlices"))
        {
            return Err("unsupported amend field: TWAP identity and quantity are immutable");
        }
        if update.is_empty()
            || update
                .values()
                .any(|value| !value.as_f64().is_some_and(f64::is_finite))
        {
            return Err("Invalid TWAP amendment");
        }
        let parsed: Self =
            serde_json::from_value(data.clone()).map_err(|_| "Invalid TWAP amendment")?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty()
            || self.id.len() > 256
            || (self.update.interval_ms.is_none() && self.update.remaining_slices.is_none())
            || self
                .update
                .interval_ms
                .is_some_and(|value| !(value > 0.0 && value <= 9_007_199_254_740_991.0))
            || self
                .update
                .remaining_slices
                .is_some_and(|value| !(1..=crate::MAX_ORDERS_COUNT).contains(&value))
        {
            return Err("Invalid TWAP amendment");
        }
        Ok(())
    }
}
