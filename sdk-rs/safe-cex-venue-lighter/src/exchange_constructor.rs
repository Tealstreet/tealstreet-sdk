//! Derived constructor body. Full base construction and derived instance-element
//! initialization are a required language boundary; this body does not claim
//! to instantiate the complete native exchange class.
use crate::{
    exchange_account_access::ExchangeAccountAccessHost, sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};

pub trait ExchangeConstructorHost: ExchangeAccountAccessHost {
    /// Actual super(...) construction followed by all derived instance-element
    /// initialization, with original receiver/newTarget, descriptors, bound
    /// arrows, allocation order, aliases, reentrancy and original throws. This
    /// must not substitute Rust BaseExchange::new's different initialization.
    fn super_and_instance_elements(
        &self,
        exchange_name: AccountValue,
        options: AccountValue,
        store: AccountValue,
    ) -> SigningResult<AccountValue>;
}

// ts: LighterExchange.constructor
pub fn construct_exchange_body(
    h: &dyn ExchangeConstructorHost,
    exchange_name: AccountValue,
    options: AccountValue,
    store: AccountValue,
) -> SigningResult<AccountValue> {
    let owner = h.super_and_instance_elements(exchange_name.clone(), options.clone(), store)?;
    initialize_exchange_clients(h, owner, exchange_name, options)
}

/// Constructor statements after super and derived field initialization.
pub fn initialize_exchange_clients(
    h: &dyn ExchangeAccountAccessHost,
    owner: AccountValue,
    exchange_name: AccountValue,
    options: AccountValue,
) -> SigningResult<AccountValue> {
    let resolve = h.global("resolveLighterDeploymentProfile")?;
    let testnet = h.get(&options, "testnet")?;
    let profile =
        resolve.call(&[h.literal(&[("exchangeName", exchange_name), ("testnet", testnet)])?])?;
    h.set(&owner, "lighterDeploymentProfile", profile)?;
    let create = h.global("createLighterSDKClient")?;
    let api = create.call(&[options, owner.clone()])?;
    h.set(&owner, "lighterApiClient", api)?;
    let order_constructor = h.global("OrderApi")?;
    let api = h.get(&owner, "lighterApiClient")?;
    let order_api = order_constructor.construct(&[api])?;
    h.set(&owner, "orderApi", order_api)?;
    let candle_constructor = h.global("CandlestickApi")?;
    let api = h.get(&owner, "lighterApiClient")?;
    let candle_api = candle_constructor.construct(&[api])?;
    h.set(&owner, "candlestickApi", candle_api)?;
    let public_constructor = h.global("LighterPublicWebsocket")?;
    let public_socket = public_constructor.construct(&[owner.clone()])?;
    h.set(&owner, "publicWebsocket", public_socket)?;
    let private_constructor = h.global("LighterPrivateWebsocket")?;
    let private_socket = private_constructor.construct(&[owner.clone()])?;
    h.set(&owner, "privateWebsocket", private_socket)?;
    Ok(owner)
}
