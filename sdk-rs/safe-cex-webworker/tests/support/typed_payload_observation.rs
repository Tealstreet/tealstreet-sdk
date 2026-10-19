//! Typed assertions observe the canonical native payload only after preparation.
use safe_cex_exchange::{
    js_runtime::value::{from_typed, to_typed_preserving_undefined},
    store_payload as native,
};
use safe_cex_store::StoreUpdate;
use safe_cex_types::StoreData;
pub fn sanitize_for_post_message(data: &StoreData, orders: bool, positions: bool) -> StoreUpdate {
    to_typed_preserving_undefined(
        &native::sanitize_for_post_message(&from_typed(data).unwrap(), orders, positions).unwrap(),
    )
    .unwrap()
}
#[derive(Default)]
pub struct StorePayloadPreparer(native::StorePayloadPreparer);
impl StorePayloadPreparer {
    pub fn prepare(
        &mut self,
        data: &StoreData,
        markets: f64,
        tickers: f64,
        hosted: bool,
        full: Option<bool>,
    ) -> StoreUpdate {
        to_typed_preserving_undefined(
            &self
                .0
                .prepare(&from_typed(data).unwrap(), markets, tickers, hosted, full)
                .unwrap(),
        )
        .unwrap()
    }
}
