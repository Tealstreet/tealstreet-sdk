use super::{SizeRules, positive};
use safe_cex_engine::model::Market;
use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
use safe_cex_engine::{ErrorKind, Outcome, VenueError, num};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct EffectRow {
    pub inst_id: Option<String>,
    pub order_id: Option<String>,
    pub tpsl_id: Option<String>,
    pub algo_id: Option<String>,
    pub new_size: Option<String>,
    pub new_trigger_price: Option<String>,
    pub new_tp_trigger_price: Option<String>,
    pub new_sl_trigger_price: Option<String>,
    pub leverage: Option<String>,
}

impl EffectRow {
    pub fn target(&self) -> Option<&str> {
        self.order_id
            .as_deref()
            .or(self.tpsl_id.as_deref())
            .or(self.algo_id.as_deref())
    }
}

#[derive(Clone)]
pub(super) struct EffectRules {
    pub market: Market,
    pub sizes: SizeRules,
    pub maximum: f64,
}

#[derive(Clone)]
pub(super) struct EffectWrite {
    pub path: &'static str,
    pub body: String,
    pub effect: AppliedEffect,
    pub targets: Vec<Option<String>>,
    pub position_key: Option<String>,
    pub rules: Option<EffectRules>,
    pub remove_leg: bool,
}

impl EffectWrite {
    pub fn applied(&self, index: usize) -> ElementResult {
        ElementResult::Applied {
            effect: self.effect,
            target_order_id: self.targets.get(index).cloned().flatten(),
            position_key: self.position_key.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), VenueError> {
        self.validate_wire().map_err(|message| {
            VenueError::new(message)
                .kind(ErrorKind::InvalidInput)
                .outcome(Outcome::NotSent)
        })
    }

    fn validate_wire(&self) -> Result<(), String> {
        if self.effect == AppliedEffect::Cancel {
            let (limit, key) = match self.path {
                "/api/v1/trade/cancel-batch-orders" => (20, "orderId"),
                "/api/v1/trade/cancel-tpsl" => (10, "tpslId"),
                "/api/v1/trade/cancel-algo" => (1, "algoId"),
                _ => return Err("Unknown BloFin cancel route".into()),
            };
            let rows: Vec<EffectRow> = if limit == 1 {
                vec![serde_json::from_str(&self.body).map_err(|error| error.to_string())?]
            } else {
                serde_json::from_str(&self.body).map_err(|error| error.to_string())?
            };
            if rows.is_empty() || rows.len() > limit || rows.len() != self.targets.len() {
                return Err("BloFin cancellation exceeds its native batch count".into());
            }
            for (row, target) in rows.iter().zip(&self.targets) {
                let native_target = match key {
                    "orderId" => row.order_id.as_deref(),
                    "tpslId" => row.tpsl_id.as_deref(),
                    _ => row.algo_id.as_deref(),
                };
                if row.inst_id.as_deref().is_none_or(str::is_empty)
                    || target.as_deref().is_none_or(str::is_empty)
                    || native_target != target.as_deref()
                {
                    return Err("BloFin cancellation lacks its native identity".into());
                }
            }
            return Ok(());
        }
        let body: EffectRow =
            serde_json::from_str(&self.body).map_err(|error| error.to_string())?;
        if self.targets.len() != 1 {
            return Err("Invalid BloFin native effect shape".into());
        }
        if self.effect == AppliedEffect::Amend {
            let rules = self
                .rules
                .as_ref()
                .ok_or("Missing BloFin amendment rules")?;
            if body.inst_id.as_deref() != Some(rules.market.id.as_str()) {
                return Err("BloFin amendment instrument does not match its rules".into());
            }
            let target = body.target();
            if target.is_none_or(str::is_empty) || target != self.targets[0].as_deref() {
                return Err("BloFin amendment lacks its native identity".into());
            }
            if let Some(text) = body.new_size.as_deref() {
                let value = number(text)?;
                rules
                    .sizes
                    .validate(value, rules.maximum, rules.market.precision.amount)?;
            }
            for (trigger, text) in [
                (true, body.new_trigger_price.as_deref()),
                (false, body.new_tp_trigger_price.as_deref()),
                (false, body.new_sl_trigger_price.as_deref()),
            ] {
                if let Some(text) = text {
                    let value = number(text)?;
                    if self.remove_leg && !trigger && value == 0.0 {
                        continue;
                    }
                    positive(value, "native amend price")?;
                    if num::adjust(value, rules.market.precision.price) != value {
                        return Err("BloFin amend price violates tickSize".into());
                    }
                }
            }
        } else if self.effect == AppliedEffect::Settings {
            if let Some(value) = body.leverage.as_deref() {
                let value = number(value)?;
                positive(value, "leverage")?;
                let rules = self.rules.as_ref().ok_or("Missing BloFin leverage rules")?;
                if value < rules.market.limits.leverage.min
                    || value > rules.market.limits.leverage.max
                {
                    return Err("BloFin leverage violates native bounds".into());
                }
            }
        } else {
            return Err("Unsupported BloFin applied effect".into());
        }
        Ok(())
    }
}

fn number(value: &str) -> Result<f64, String> {
    value.parse::<f64>().map_err(|error| error.to_string())
}
