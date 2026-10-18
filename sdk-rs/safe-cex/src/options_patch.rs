//! Presence-aware options patches: None means absent; Some(None) explicitly clears a field.
use safe_cex_types::{
    ExchangeOptions, ExchangeOptionsAuth, ExchangeOptionsBybitSiteId,
    ExchangeOptionsExternalPublicData, ExchangeOptionsExtra,
    ExchangeOptionsExtraAsterdexApiVersion, ExchangeOptionsExtraKucoinBroker,
    ExchangeOptionsExtraLighterBuilderFeeMode, ExchangeOptionsHyperliquidBuilder,
    ExchangeOptionsPolymarket, ExchangeOptionsTuning, PayloadSignerCapability,
    RemoteSignerCapability, TransportCapability,
};

#[derive(Clone, Default)]
pub struct ExchangeOptionsExtraPatch {
    pub hyrotrader_mode: Option<Option<safe_cex_types::ExchangeOptionsExtraHyrotraderMode>>,
    pub tick_interval: Option<Option<f64>>,
    pub recv_window: Option<Option<f64>>,
    pub asterdex_api_version: Option<Option<ExchangeOptionsExtraAsterdexApiVersion>>,
    pub aster_builder_address: Option<Option<String>>,
    pub aster_builder_fee_rate_maker: Option<Option<String>>,
    pub aster_builder_fee_rate_taker: Option<Option<String>>,
    pub lighter_builder_fee_mode: Option<Option<ExchangeOptionsExtraLighterBuilderFeeMode>>,
    pub lighter_integrator_account_index: Option<Option<String>>,
    pub lighter_stop_slippage_pct: Option<Option<f64>>,
    pub kucoin_broker: Option<Option<ExchangeOptionsExtraKucoinBroker>>,
}

pub(crate) fn apply_extra_patch(
    patch: &ExchangeOptionsExtraPatch,
    options: &mut ExchangeOptionsExtra,
) {
    if let Some(value) = &patch.hyrotrader_mode {
        options.hyrotrader_mode = value.clone();
    }
    if let Some(value) = &patch.tick_interval {
        options.tick_interval = *value;
    }
    if let Some(value) = &patch.recv_window {
        options.recv_window = *value;
    }
    if let Some(value) = &patch.asterdex_api_version {
        options.asterdex_api_version = value.clone();
    }
    if let Some(value) = &patch.aster_builder_address {
        options.aster_builder_address = value.clone();
    }
    if let Some(value) = &patch.aster_builder_fee_rate_maker {
        options.aster_builder_fee_rate_maker = value.clone();
    }
    if let Some(value) = &patch.aster_builder_fee_rate_taker {
        options.aster_builder_fee_rate_taker = value.clone();
    }
    if let Some(value) = &patch.lighter_builder_fee_mode {
        options.lighter_builder_fee_mode = value.clone();
    }
    if let Some(value) = &patch.lighter_integrator_account_index {
        options.lighter_integrator_account_index = value.clone();
    }
    if let Some(value) = &patch.lighter_stop_slippage_pct {
        options.lighter_stop_slippage_pct = *value;
    }
    if let Some(value) = &patch.kucoin_broker {
        options.kucoin_broker = value.clone();
    }
}

pub(crate) fn extra_patch_present(patch: &ExchangeOptionsExtraPatch) -> bool {
    patch.hyrotrader_mode.is_some()
        || patch.tick_interval.is_some()
        || patch.recv_window.is_some()
        || patch.asterdex_api_version.is_some()
        || patch.aster_builder_address.is_some()
        || patch.aster_builder_fee_rate_maker.is_some()
        || patch.aster_builder_fee_rate_taker.is_some()
        || patch.lighter_builder_fee_mode.is_some()
        || patch.lighter_integrator_account_index.is_some()
        || patch.lighter_stop_slippage_pct.is_some()
        || patch.kucoin_broker.is_some()
}

pub(crate) fn empty_extra_options() -> ExchangeOptionsExtra {
    ExchangeOptionsExtra {
        hyrotrader_mode: None,
        tick_interval: None,
        recv_window: None,
        asterdex_api_version: None,
        aster_builder_address: None,
        aster_builder_fee_rate_maker: None,
        aster_builder_fee_rate_taker: None,
        lighter_builder_fee_mode: None,
        lighter_integrator_account_index: None,
        lighter_stop_slippage_pct: None,
        kucoin_broker: None,
    }
}

#[derive(Clone, Default)]
pub struct ExchangeOptionsPatch {
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<Option<String>>,
    pub secondary_key: Option<Option<String>>,
    pub secondary_secret: Option<Option<String>>,
    pub secondary_passphrase: Option<Option<String>>,
    pub secondary_proxy_url: Option<Option<String>>,
    pub auth: Option<Option<ExchangeOptionsAuth>>,
    pub paradex_address: Option<Option<String>>,
    pub subkey_address: Option<Option<String>>,
    pub grvt_signer_address: Option<Option<String>>,
    pub grvt_trading_account_id: Option<Option<String>>,
    pub stark_private_key: Option<Option<String>>,
    pub vault: Option<Option<String>>,
    pub application_id: Option<Option<String>>,
    pub api_key_index: Option<Option<String>>,
    pub hyperliquid_builder: Option<Option<ExchangeOptionsHyperliquidBuilder>>,
    pub polymarket: Option<Option<ExchangeOptionsPolymarket>>,
    pub testnet: Option<Option<bool>>,
    pub enable_spot_markets: Option<Option<bool>>,
    pub enable_option_markets: Option<Option<bool>>,
    pub enable_inverse_markets: Option<Option<bool>>,
    pub bybit_site_id: Option<Option<ExchangeOptionsBybitSiteId>>,
    pub public_data_only: Option<Option<bool>>,
    pub private_only: Option<Option<bool>>,
    pub tealstreet_proxy: Option<Option<bool>>,
    pub tealstreet_tunnel: Option<Option<bool>>,
    pub tealstreet_ws_route: Option<Option<bool>>,
    pub override_proxy_url: Option<Option<String>>,
    pub proxy_region: Option<Option<String>>,
    pub omni_key_seed: Option<Option<String>>,
    pub subaccount: Option<Option<String>>,
    pub orderly_exchange_id: Option<Option<String>>,
    pub tuning: Option<Option<ExchangeOptionsTuning>>,
    pub extra: Option<ExchangeOptionsExtraPatch>,
    pub remote_signer: Option<Option<RemoteSignerCapability>>,
    pub transport: Option<Option<TransportCapability>>,
    pub payload_signer: Option<Option<PayloadSignerCapability>>,
    pub hosted_mode: Option<Option<bool>>,
    pub external_public_data: Option<Option<ExchangeOptionsExternalPublicData>>,
}

pub(crate) fn apply_patch(patch: &ExchangeOptionsPatch, options: &mut ExchangeOptions) {
    if let Some(value) = &patch.key {
        options.key = value.clone();
    }
    if let Some(value) = &patch.secret {
        options.secret = value.clone();
    }
    if let Some(value) = &patch.passphrase {
        options.passphrase = value.clone();
    }
    if let Some(value) = &patch.secondary_key {
        options.secondary_key = value.clone();
    }
    if let Some(value) = &patch.secondary_secret {
        options.secondary_secret = value.clone();
    }
    if let Some(value) = &patch.secondary_passphrase {
        options.secondary_passphrase = value.clone();
    }
    if let Some(value) = &patch.secondary_proxy_url {
        options.secondary_proxy_url = value.clone();
    }
    if let Some(value) = &patch.auth {
        options.auth = value.clone();
    }
    if let Some(value) = &patch.paradex_address {
        options.paradex_address = value.clone();
    }
    if let Some(value) = &patch.subkey_address {
        options.subkey_address = value.clone();
    }
    if let Some(value) = &patch.grvt_signer_address {
        options.grvt_signer_address = value.clone();
    }
    if let Some(value) = &patch.grvt_trading_account_id {
        options.grvt_trading_account_id = value.clone();
    }
    if let Some(value) = &patch.stark_private_key {
        options.stark_private_key = value.clone();
    }
    if let Some(value) = &patch.vault {
        options.vault = value.clone();
    }
    if let Some(value) = &patch.application_id {
        options.application_id = value.clone();
    }
    if let Some(value) = &patch.api_key_index {
        options.api_key_index = value.clone();
    }
    if let Some(value) = &patch.hyperliquid_builder {
        options.hyperliquid_builder = value.clone();
    }
    if let Some(value) = &patch.polymarket {
        options.polymarket = value.clone();
    }
    if let Some(value) = &patch.testnet {
        options.testnet = *value;
    }
    if let Some(value) = &patch.enable_spot_markets {
        options.enable_spot_markets = *value;
    }
    if let Some(value) = &patch.enable_option_markets {
        options.enable_option_markets = *value;
    }
    if let Some(value) = &patch.enable_inverse_markets {
        options.enable_inverse_markets = *value;
    }
    if let Some(value) = &patch.bybit_site_id {
        options.bybit_site_id = value.clone();
    }
    if let Some(value) = &patch.public_data_only {
        options.public_data_only = *value;
    }
    if let Some(value) = &patch.private_only {
        options.private_only = *value;
    }
    if let Some(value) = &patch.tealstreet_proxy {
        options.tealstreet_proxy = *value;
    }
    if let Some(value) = &patch.tealstreet_tunnel {
        options.tealstreet_tunnel = *value;
    }
    if let Some(value) = &patch.tealstreet_ws_route {
        options.tealstreet_ws_route = *value;
    }
    if let Some(value) = &patch.override_proxy_url {
        options.override_proxy_url = value.clone();
    }
    if let Some(value) = &patch.proxy_region {
        options.proxy_region = value.clone();
    }
    if let Some(value) = &patch.omni_key_seed {
        options.omni_key_seed = value.clone();
    }
    if let Some(value) = &patch.subaccount {
        options.subaccount = value.clone();
    }
    if let Some(value) = &patch.orderly_exchange_id {
        options.orderly_exchange_id = value.clone();
    }
    if let Some(value) = &patch.tuning {
        options.tuning = value.clone();
    }
    if let Some(value) = &patch.remote_signer {
        options.remote_signer = value.clone();
    }
    if let Some(value) = &patch.transport {
        options.transport = value.clone();
    }
    if let Some(value) = &patch.payload_signer {
        options.payload_signer = value.clone();
    }
    if let Some(value) = &patch.hosted_mode {
        options.hosted_mode = *value;
    }
    if let Some(value) = &patch.external_public_data {
        options.external_public_data = value.clone();
    }
}

pub(crate) fn empty_options() -> ExchangeOptions {
    ExchangeOptions {
        key: String::new(),
        secret: String::new(),
        passphrase: None,
        secondary_key: None,
        secondary_secret: None,
        secondary_passphrase: None,
        secondary_proxy_url: None,
        auth: None,
        paradex_address: None,
        subkey_address: None,
        grvt_signer_address: None,
        grvt_trading_account_id: None,
        stark_private_key: None,
        vault: None,
        application_id: None,
        api_key_index: None,
        hyperliquid_builder: None,
        polymarket: None,
        testnet: None,
        enable_spot_markets: None,
        bybit_site_id: None,
        public_data_only: None,
        private_only: None,
        tealstreet_proxy: None,
        tealstreet_tunnel: None,
        tealstreet_ws_route: None,
        override_proxy_url: None,
        proxy_region: None,
        omni_key_seed: None,
        subaccount: None,
        orderly_exchange_id: None,
        tuning: None,
        extra: None,
        remote_signer: None,
        transport: None,
        payload_signer: None,
        hosted_mode: None,
        external_public_data: None,
        enable_inverse_markets: None,
        enable_option_markets: None,
    }
}
