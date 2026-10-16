#![allow(dead_code, unused_imports)]
use safe_cex_exchange::{exchange::{Exchange, BaseExchange, ValidatedOrder, ValidatedBatch}, capabilities::ExchangeCapabilities};
struct A;
struct B;
impl Exchange for A {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
 fn base(&self) -> &BaseExchange { unimplemented!() }
 fn capabilities(&self) -> &ExchangeCapabilities { unimplemented!() }
}
impl Exchange for B {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
 fn base(&self) -> &BaseExchange { unimplemented!() }
 fn capabilities(&self) -> &ExchangeCapabilities { unimplemented!() }
}
fn transfer(token: ValidatedOrder<A>, target: &B) { drop(target.place_order(token)); }
fn main() {}
