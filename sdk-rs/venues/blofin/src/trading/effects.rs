use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct EffectRow {
    pub inst_id: Option<String>,
    pub order_id: Option<String>,
    pub tpsl_id: Option<String>,
    pub algo_id: Option<String>,
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
pub(super) struct EffectWrite {
    pub effect: AppliedEffect,
    pub targets: Vec<Option<String>>,
    pub position_key: Option<String>,
}
impl EffectWrite {
    pub fn applied(&self, index: usize) -> ElementResult {
        ElementResult::Applied {
            effect: self.effect,
            target_order_id: self.targets.get(index).cloned().flatten(),
            position_key: self.position_key.clone(),
        }
    }
}
