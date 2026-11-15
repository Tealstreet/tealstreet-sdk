//! Account/options output registrations; the numeric trait stays below consumers.
use super::JsNumberOutput;

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExtra {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { hyrotrader_mode, tick_interval, rust_command_timeout_ms, recv_window, asterdex_api_version, aster_builder_address, aster_builder_fee_rate_maker, aster_builder_fee_rate_taker, lighter_builder_fee_mode, lighter_integrator_account_index, lighter_stop_slippage_pct, kucoin_broker });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsPolymarket {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { market_slugs, api_key, api_secret, api_passphrase, signer_type, funder_address });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptions {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { key, secret, signing_mode, passphrase, secondary_key, secondary_secret, secondary_passphrase, secondary_proxy_url, auth, paradex_address, subkey_address, grvt_signer_address, grvt_trading_account_id, coinbase_jwt_algorithm, stark_private_key, vault, application_id, api_key_index, hyperliquid_builder, testnet, enable_spot_markets, enable_option_markets, enable_inverse_markets, bybit_site_id, public_data_only, private_only, tealstreet_proxy, tealstreet_tunnel, tealstreet_ws_route, override_proxy_url, proxy_region, omni_key_seed, subaccount, orderly_exchange_id, tuning, extra, polymarket, remote_signer, transport, payload_signer, hosted_mode, external_public_data, local_signer, hyperliquid_fast_book, caller_reports_placement_failures, worker_mirror, engine, public_feed });
    }
}

impl JsNumberOutput for safe_cex_types::options::SigningMode {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Local, Cosigned, Delegated });
    }
}

impl JsNumberOutput for safe_cex_types::options::CoinbaseJwtAlgorithm {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Es256, EdDsa });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsEngine {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Rust });
    }
}
impl JsNumberOutput for safe_cex_types::LocalSignerMetadataCapability {
    fn check_number_output(&self) {
        match *self {}
    }
}
impl JsNumberOutput for safe_cex_types::RustPublicFeedCapability {
    fn check_number_output(&self) {
        match *self {}
    }
}

#[cfg(test)]
mod tests {
    use crate::numeric_ingress::js_from_str;

    #[test]
    fn coinbase_algorithms_preserve_native_values_across_numeric_ingress() {
        use crate::numeric_ingress::{js_from_value, js_from_value_ref};
        use safe_cex_types::{ExchangeOptions, options::CoinbaseJwtAlgorithm};

        for (algorithm, expected) in [
            ("es256", CoinbaseJwtAlgorithm::Es256),
            ("edDsa", CoinbaseJwtAlgorithm::EdDsa),
        ] {
            let json = format!(
                r#"{{"key":"fixture","secret":"","coinbaseJwtAlgorithm":"{algorithm}","workerMirror":false,"extra":{{"tickInterval":-0}}}}"#
            );
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            for options in [
                js_from_str::<ExchangeOptions>(&json).unwrap(),
                js_from_value(value.clone()).unwrap(),
                js_from_value_ref(&value).unwrap(),
            ] {
                assert_eq!(options.coinbase_jwt_algorithm.as_ref(), Some(&expected));
                assert_eq!(options.worker_mirror, Some(false));
                assert_eq!(
                    options.extra.unwrap().tick_interval.unwrap().to_bits(),
                    (-0.0_f64).to_bits()
                );
            }
        }
    }

    #[test]
    fn option_controls_retain_native_number_bits_and_false_values() {
        for raw in ["-0", "0", "1e400", "-1e-4000"] {
            let expected: f64 = raw.parse().unwrap();
            let json = format!(
                r#"{{"key":"fixture","secret":"","workerMirror":false,"engine":"rust","extra":{{"tickInterval":{raw},"rustCommandTimeoutMs":{raw}}},"polymarket":{{"marketSlugs":["00017"],"signerType":0}}}}"#
            );
            let options: safe_cex_types::ExchangeOptions = js_from_str(&json).unwrap();
            let extra = options.extra.unwrap();
            assert_eq!(extra.tick_interval.unwrap().to_bits(), expected.to_bits());
            assert_eq!(
                extra.rust_command_timeout_ms.unwrap().to_bits(),
                expected.to_bits()
            );
            assert_eq!(options.worker_mirror, Some(false));
            assert_eq!(
                options.polymarket.unwrap().market_slugs,
                Some(vec!["00017".into()])
            );
        }
    }
}
