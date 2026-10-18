//! Account preferences and credential forwarding from createExchangeFromAccount.ts.
use crate::{
    factory::ExchangeFactory,
    options_patch::{
        ExchangeOptionsPatch, apply_extra_patch, apply_patch, empty_extra_options, empty_options,
        extra_patch_present,
    },
};
use safe_cex_store::account_environment::{
    is_lighter_exchange, resolve_public_kucoin_broker_config,
};
use safe_cex_transport::{TransportProvider, TransportRegistry};
use safe_cex_types::{
    ExchangeAccountConfig, ExchangeName, ExchangeOptions, ExchangeOptionsBybitSiteId,
    exchange_metadata::get_exchange_metadata,
};
use std::sync::Arc;

pub struct AccountOptionsHost<'a> {
    pub transport: &'a TransportRegistry,
    pub instance: Option<&'a Arc<dyn TransportProvider>>,
    pub environment: &'a dyn Fn(&str) -> Option<String>,
    pub warn: &'a dyn Fn(&str),
}
pub fn account_to_exchange_options(
    account: &ExchangeAccountConfig,
    overrides: Option<&ExchangeOptionsPatch>,
    host: &AccountOptionsHost<'_>,
) -> Result<ExchangeOptions, String> {
    let name_value = serde_json::to_value(&account.exchange).expect("exchange name");
    let name = name_value.as_str().expect("exchange name string");
    let metadata = get_exchange_metadata(name, false)?;
    let forward_application =
        metadata.requires_application_id || is_lighter_exchange(name) || name == "asterdex";
    let forward_index = metadata.requires_api_key_index || is_lighter_exchange(name);
    let broker = resolve_public_kucoin_broker_config(name, host.environment);
    let mut extra = empty_extra_options();
    extra.kucoin_broker = broker;
    if account.exchange == ExchangeName::Hyrotrader {
        extra.hyrotrader_mode = Some(if account.hyrotrader_demo == Some(true) {
            safe_cex_types::ExchangeOptionsExtraHyrotraderMode::Demo
        } else {
            safe_cex_types::ExchangeOptionsExtraHyrotraderMode::Production
        });
    }
    let mut has_extra = extra.kucoin_broker.is_some() || extra.hyrotrader_mode.is_some();
    if let Some(patch) = overrides.and_then(|patch| patch.extra.as_ref()) {
        has_extra |= extra_patch_present(patch);
        apply_extra_patch(patch, &mut extra);
    }
    if metadata.requires_cors && metadata.forbid_cors {
        return Err(format!(
            "[{name}] Invalid metadata: requiresCors and forbidCors are both true"
        ));
    }
    if metadata.enforce_ws_tunnel == Some(true) && metadata.supports_tunnel == Some(false) {
        (host.warn)(&format!(
            "[{name}] Metadata warning: enforceWsTunnel is true but supportsTunnel is false. enforceWsTunnel takes precedence — tunnel will be enabled."
        ));
    }
    let mut proxy_region = account.proxy_region.clone();
    if metadata.requires_cors && proxy_region.as_deref().is_none_or(str::is_empty) {
        proxy_region = Some(
            metadata
                .private_proxy_regions
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "SG".into()),
        );
    }
    let proxy = if account.tealstreet_proxy == Some(true) || metadata.requires_cors {
        Some(true)
    } else {
        metadata.enforce_ws_tunnel
    };
    let tunnel = if proxy == Some(true)
        && (metadata.supports_tunnel != Some(false) || metadata.enforce_ws_tunnel == Some(true))
    {
        Some(true)
    } else {
        account.tealstreet_tunnel
    };
    let secondary_proxy = if account
        .secondary_api_key
        .as_deref()
        .is_some_and(|key| !key.is_empty())
    {
        host.transport
            .get_transport(host.instance)
            .public_cors_proxy()
    } else {
        None
    };
    let bybit_site = if matches!(
        account.exchange,
        ExchangeName::Bybit | ExchangeName::Bybitinverse | ExchangeName::Bybitspot
    ) && account.bybit_site_id.is_some()
    {
        Some(ExchangeOptionsBybitSiteId::BraBtl)
    } else {
        None
    };
    let nonempty = |value: &Option<String>| value.clone().filter(|value| !value.is_empty());
    let mut options = empty_options();
    options.key = account.api_key.clone();
    options.secret = account.api_secret.clone();
    if metadata.requires_passphrase {
        options.passphrase = nonempty(&account.passphrase);
    }
    options.secondary_key = nonempty(&account.secondary_api_key);
    options.secondary_secret = nonempty(&account.secondary_api_secret);
    options.secondary_passphrase = nonempty(&account.secondary_passphrase);
    options.secondary_proxy_url = secondary_proxy.filter(|value| !value.is_empty());
    options.tealstreet_proxy = proxy;
    options.tealstreet_tunnel = tunnel;
    options.tealstreet_ws_route = account.tealstreet_ws_route;
    options.proxy_region = proxy_region.filter(|value| !value.is_empty());
    options.testnet = account.testnet.filter(|value| *value);
    options.bybit_site_id = bybit_site;
    if metadata.supports_spot_toggle == Some(true) && account.enable_spot_markets == Some(true) {
        options.enable_spot_markets = Some(true);
    }
    if metadata.supports_options_toggle == Some(true) && account.enable_option_markets == Some(true)
    {
        options.enable_option_markets = Some(true);
    }
    if metadata.supports_inverse_toggle == Some(true)
        && account.enable_inverse_markets == Some(true)
    {
        options.enable_inverse_markets = Some(true);
    }
    if forward_application {
        options.application_id = nonempty(&account.application_id);
    }
    if forward_index {
        options.api_key_index = nonempty(&account.api_key_index);
    }
    if metadata.supports_vault == Some(true) {
        options.vault = nonempty(&account.vault);
    }
    if metadata.requires_omni_key_seed == Some(true) {
        options.omni_key_seed = nonempty(&account.omni_key_seed);
    }
    if metadata.requires_paradex_address == Some(true) {
        options.paradex_address = account.paradex_address.clone();
        options.subkey_address = account.subkey_address.clone();
    }
    if name == "grvt" {
        options.grvt_signer_address = nonempty(&account.grvt_signer_address);
        options.grvt_trading_account_id = nonempty(&account.grvt_trading_account_id);
    }
    if metadata.supports_subaccount == Some(true) {
        options.subaccount = nonempty(&account.subaccount);
    }
    options.orderly_exchange_id = nonempty(&account.orderly_exchange_id);
    if has_extra {
        options.extra = Some(extra);
    }
    if let Some(patch) = overrides {
        apply_patch(patch, &mut options);
    }
    Ok(options)
}
pub async fn create_exchange_from_account<I>(
    factory: &ExchangeFactory<I>,
    account: &ExchangeAccountConfig,
    overrides: Option<&ExchangeOptionsPatch>,
    host: &AccountOptionsHost<'_>,
) -> Result<I, String> {
    let options = account_to_exchange_options(account, overrides, host)?;
    let name = serde_json::to_value(&account.exchange).expect("exchange name");
    factory
        .create_exchange(name.as_str().expect("exchange name string"), options, None)
        .await
}
