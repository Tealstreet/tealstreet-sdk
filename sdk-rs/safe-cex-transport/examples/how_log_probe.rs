use safe_cex_transport::how_diagnostics::{log_trade_tunnel_request, log_trade_tunnel_response};
use serde_json::json;
fn main() {
    let request = safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"url":"https://venue.test/order?apiKey=hidden&side=buy","method":"POST","headers":{"Authorization":"secret"},"params":{"token":"secret","symbol":"BTC"},"data":{"password":"secret","quantity":1}})).unwrap();
    let route = json!({"selectedRegion":"SG","tunnelHost":"singapore"});
    log_trade_tunnel_request(&request, Some(&route));
    log_trade_tunnel_response(
        &request,
        &safe_cex_types::retained_json::RetainedJsonInput::from_value(
            json!({"data":{"signature":"secret","accepted":true}}),
        )
        .unwrap(),
        Some(&route),
    );
}
