use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChaserAmendment {
    pub id: String,
    pub update: ChaserUpdate,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChaserUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<f64>,
}

impl ChaserAmendment {
    pub fn parse(data: &Value) -> Result<Self, &'static str> {
        let fields = data.as_object().ok_or("Invalid chaser amendment")?;
        let update = data["update"]
            .as_object()
            .ok_or("Invalid chaser amendment")?;
        if fields
            .keys()
            .any(|key| !matches!(key.as_str(), "id" | "update"))
            || update
                .keys()
                .any(|key| !matches!(key.as_str(), "limitPrice" | "offset" | "maxAttempts"))
        {
            return Err("unsupported amend field: chaser identity and quantity are immutable");
        }
        if update.is_empty()
            || update
                .values()
                .any(|value| !value.as_f64().is_some_and(f64::is_finite))
        {
            return Err("Invalid chaser amendment");
        }
        let parsed: Self =
            serde_json::from_value(data.clone()).map_err(|_| "Invalid chaser amendment")?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty()
            || self.id.len() > 256
            || (self.update.limit_price.is_none()
                && self.update.offset.is_none()
                && self.update.max_attempts.is_none())
            || self
                .update
                .limit_price
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
            || self
                .update
                .offset
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || self
                .update
                .max_attempts
                .is_some_and(|value| !(0.0..f64::from(u32::MAX)).contains(&value))
        {
            return Err("Invalid chaser amendment");
        }
        Ok(())
    }
}
