use safe_cex_transport::routing::is_tokyo_egress;
use safe_cex_transport::{RoutingOptions, resolve_exchange_routing};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde::Deserialize;
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    exchange: ExchangeName,
    options: ExchangeOptions,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    cases: Vec<Case>,
    egress_keys: Vec<String>,
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Input = serde_json::from_str(&input).unwrap();
    let cases = input.cases;
    let result: Vec<_> = cases
        .iter()
        .map(|case| resolve_exchange_routing(&case.exchange, &RoutingOptions::from(&case.options)))
        .collect();
    let egress: Vec<_> = input
        .egress_keys
        .iter()
        .map(|key| is_tokyo_egress(key))
        .collect();
    println!("{}", serde_json::json!({"routes":result, "egress":egress}));
}
