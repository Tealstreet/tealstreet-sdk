use safe_cex_venue_bybit::{api, site};
use serde_json::{Value, json};
use std::io::{self, Read};
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Value = serde_json::from_str(&input).unwrap();
    let configs: Vec<_> = cases["configs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            json!([
                api::is_signed_request(value),
                api::is_instruments_info_request(value),
                api::has_retried_clock_sync(value),
                api::is_bybit_env_probe_request(value)
            ])
        })
        .collect();
    let messages: Vec<_> = cases["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let units: Vec<u16> = serde_json::from_value(value.clone()).unwrap();
            let text = String::from_utf16(&units).ok();
            json!([
                api::is_bybit_clock_sync_ret_msg(&units),
                api::parse_bybit_server_timestamp(&units),
                api::is_public_endpoint(text.as_deref())
            ])
        })
        .collect();
    let sites: Vec<_> = cases["sites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let exchange = value[0].as_str().unwrap();
            let options = json!({"bybitSiteId":value[1]});
            json!([
                site::is_bybit_site_id_exchange(exchange),
                site::normalize_bybit_site_id(exchange, &value[1]),
                site::get_bybit_site_headers(&options),
                site::get_bybit_web_socket_options(&options)
            ])
        })
        .collect();
    println!(
        "{}",
        json!({"configs":configs,"messages":messages,"sites":sites})
    );
}
