#![allow(dead_code)]
use safe_cex_exchange::{exchange::{Exchange, BaseExchange, ValidatedOrder}, capabilities::ExchangeCapabilities};
struct Venue;
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange { unimplemented!() }
    fn capabilities(&self) -> &ExchangeCapabilities { unimplemented!() }
}
fn duplicate(token: ValidatedOrder<Venue>) -> ValidatedOrder<Venue> { token.clone() }
fn main() {}
