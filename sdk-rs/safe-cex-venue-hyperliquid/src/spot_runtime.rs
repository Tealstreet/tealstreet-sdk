//! Complete spot class composition; imported dependencies and native allocation bind at the host.
use crate::{
    account_position_leaves as positions, balance_state as balances,
    lifecycle_delegates as lifecycle, market_data_selectors as markets,
    order_request_leaves as requests,
    spot_account_reads::{self as account, SpotAccountHost},
    spot_public_subscriptions::{self as public, SpotPublicHost},
    spot_trading::{self as trading, SpotTradingHost},
    spot_ws_dispatch as sockets,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeMethod {
    Coin,
    Validate,
    Start,
    Tick,
    Dispose,
    Markets,
    MarkPrice,
    Balance,
    ApplyBalances,
    State,
    RequestRefresh,
    Refresh,
    Candles,
    Orders,
    GeneratePositions,
    Positions,
    FetchPositions,
    ListenCandles,
    ListenBook,
    Leverage,
    Margin,
    Place,
    Cancel,
    Update,
    Precision,
    Configuration,
}
pub const EXCHANGE_ARROWS: &[(&str, u8, ExchangeMethod)] = &[
    ("symbolOrIdToCoin", 1, ExchangeMethod::Coin),
    ("validateAccount", 0, ExchangeMethod::Validate),
    ("start", 0, ExchangeMethod::Start),
    ("tick", 0, ExchangeMethod::Tick),
    ("dispose", 0, ExchangeMethod::Dispose),
    ("fetchMarketsAndTickers", 0, ExchangeMethod::Markets),
    ("getSpotCoinMarkPrice", 2, ExchangeMethod::MarkPrice),
    ("buildSpotBalance", 1, ExchangeMethod::Balance),
    ("applySpotBalances", 1, ExchangeMethod::ApplyBalances),
    ("fetchSpotClearinghouseState", 0, ExchangeMethod::State),
    (
        "requestSpotBalanceRefresh",
        0,
        ExchangeMethod::RequestRefresh,
    ),
    ("refreshSpotBalance", 0, ExchangeMethod::Refresh),
    ("fetchOHLCV", 1, ExchangeMethod::Candles),
    ("fetchOrders", 1, ExchangeMethod::Orders),
    (
        "generateFakePositionsFromMarkets",
        1,
        ExchangeMethod::GeneratePositions,
    ),
    ("getFakePositions", 0, ExchangeMethod::Positions),
    ("fetchPositions", 1, ExchangeMethod::FetchPositions),
    ("listenOHLCV", 2, ExchangeMethod::ListenCandles),
    ("listenOrderBook", 2, ExchangeMethod::ListenBook),
    ("setLeverage", 3, ExchangeMethod::Leverage),
    ("changeMarginType", 2, ExchangeMethod::Margin),
    ("placeOrder", 1, ExchangeMethod::Place),
    ("cancelOrders", 1, ExchangeMethod::Cancel),
    ("updateOrder", 1, ExchangeMethod::Update),
    ("priceToPrecision", 2, ExchangeMethod::Precision),
];
pub trait SpotRuntimeHost:
    SpotTradingHost
    + SpotPublicHost
    + markets::MarketDataHost
    + lifecycle::LifecycleHost
    + requests::OrderRequestHost
{
    fn base_exchange(
        &mut self,
        args: &[Self::Value; 3],
        target: &Self::Value,
        prototype: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Allocate a lexical own arrow invoking run_exchange_method with the captured owner.
    fn exchange_arrow(
        &mut self,
        owner: &Self::Value,
        name: &str,
        length: u8,
        method: ExchangeMethod,
    ) -> Result<Self::Value, Self::Error>;
    fn exchange_method(
        &mut self,
        name: &str,
        length: u8,
        method: ExchangeMethod,
    ) -> Result<Self::Value, Self::Error>;
}
pub struct HyperliquidSpotExchange<V> {
    pub constructor: V,
    pub prototype: V,
    pub public: public::HyperliquidSpotPublicWebSocket<V>,
    pub private: sockets::PrivateSocketClass<V>,
}
fn n<H: SpotAccountHost>(h: &H, v: f64) -> H::Value {
    SpotAccountHost::number(h, v)
}
fn imported<H: SpotAccountHost>(h: &mut H, key: &str) -> Result<H::Value, H::Error> {
    SpotAccountHost::imported(h, key)
}
fn invoke<H: SpotAccountHost>(
    h: &mut H,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = imported(h, key)?;
    let args = args(h)?;
    h.call(&f, &h.undefined(), &args)
}
pub fn spot_exchange_class<H: SpotRuntimeHost>(
    h: &mut H,
) -> Result<HyperliquidSpotExchange<H::Value>, H::Error> {
    let (constructor, prototype) =
        h.class_pair("HyperliquidSpotExchange", 3, Some("BaseExchange"))?;
    h.define_data(&constructor, "SPOT_BALANCE_REFRESH_MS", &n(h, 10000.))?;
    h.define_data(
        &constructor,
        "SPOT_BALANCE_FORCE_REFRESH_COOLDOWN_MS",
        &n(h, 1500.),
    )?;
    let object = h.global("Object")?;
    let freeze = h.get(&object, "freeze")?;
    let config = h.object()?;
    for (key, table, member) in [
        ("marginType", "MarginType", "Cross"),
        ("positionMode", "PositionMode", "OneWay"),
    ] {
        let table = imported(h, table)?;
        let v = h.get(&table, member)?;
        h.define_data(&config, key, &v)?;
    }
    h.define_data(&config, "leverage", &n(h, 1.))?;
    let config = h.call(&freeze, &object, &[config])?;
    h.define_data(&constructor, "SPOT_SYMBOL_CONFIG", &config)?;
    let f = h.exchange_method(
        "_getSymbolConfigurationFromAssetData",
        1,
        ExchangeMethod::Configuration,
    )?;
    h.define_hidden(&prototype, "_getSymbolConfigurationFromAssetData", &f)?;
    let public = public::public_socket_class(h)?;
    let private = sockets::private_socket_class(h)?;
    Ok(HyperliquidSpotExchange {
        constructor,
        prototype,
        public,
        private,
    })
}
pub fn new_spot_exchange<H: SpotRuntimeHost>(
    h: &mut H,
    class: &HyperliquidSpotExchange<H::Value>,
    args: &[H::Value; 3],
    target: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let owner = h.base_exchange(args, target.unwrap_or(&class.constructor), &class.prototype)?;
    for key in ["xhr", "unlimitedXHR"] {
        h.define_data(&owner, key, &h.undefined())?;
    }
    h.define_data(&owner, "isSpot", &SpotAccountHost::boolean(h, true))?;
    h.define_data(
        &owner,
        "spotBalanceRefreshInFlight",
        &SpotAccountHost::null(h),
    )?;
    for key in ["lastSpotBalanceRefreshAt", "lastForcedSpotBalanceRefreshAt"] {
        h.define_data(&owner, key, &n(h, 0.))?;
    }
    for key in ["publicWebsocket", "privateWebsocket"] {
        h.define_data(&owner, key, &h.undefined())?;
    }
    for (key, import) in [
        ("grafanaEndpointsEvery", "spot.GRAFANA_ENDPOINTS_EVERY"),
        (
            "grafanaEndpointsPeriodic",
            "spot.GRAFANA_ENDPOINTS_PERIODIC",
        ),
    ] {
        let v = imported(h, import)?;
        h.define_data(&owner, key, &v)?;
    }
    let ctor = h.global("Map")?;
    let map = SpotAccountHost::construct(h, &ctor, &[])?;
    h.define_data(&owner, "activeAssetData", &map)?;
    for (name, length, body) in EXCHANGE_ARROWS {
        if *body == ExchangeMethod::Update {
            let ctor = h.global("Set")?;
            let set = SpotAccountHost::construct(h, &ctor, &[])?;
            h.define_data(&owner, "_deadOrders", &set)?;
        }
        let f = h.exchange_arrow(&owner, name, *length, *body)?;
        h.define_data(&owner, name, &f)?;
    }
    let xhr = invoke(h, "rateLimit", |h| {
        let api = invoke(h, "perp.createAPI", |_| {
            Ok(vec![args[1].clone(), owner.clone()])
        })?;
        let c = h.object()?;
        h.define_data(&c, "maxRPS", &n(h, 3.))?;
        Ok(vec![api, c])
    })?;
    h.set(&owner, "xhr", &xhr)?;
    let unlimited = invoke(h, "perp.createAPI", |_| {
        Ok(vec![args[1].clone(), owner.clone()])
    })?;
    h.set(&owner, "unlimitedXHR", &unlimited)?;
    let socket = public::new_public_socket(h, &class.public, &owner, None)?;
    h.set(&owner, "publicWebsocket", &socket)?;
    let socket = sockets::new_private_socket(h, &class.private, &owner, None)?.native;
    h.set(&owner, "privateWebsocket", &socket)?;
    Ok(owner)
}
pub fn run_exchange_method<H: SpotRuntimeHost>(
    h: &mut H,
    owner: &H::Value,
    body: ExchangeMethod,
    args: &[H::Value],
) -> Result<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let a = args.first().cloned().unwrap_or_else(|| h.undefined());
    let b = args.get(1).cloned().unwrap_or_else(|| h.undefined());
    let c = args.get(2).cloned().unwrap_or_else(|| h.undefined());
    let result = match body {
        ExchangeMethod::Coin => return markets::spot_symbol_or_id_to_coin(h, &a),
        ExchangeMethod::Precision => return markets::spot_price_to_precision(h, owner, &a, &b),
        ExchangeMethod::Configuration => {
            return positions::spot_symbol_configuration_from_asset_data(h);
        }
        ExchangeMethod::MarkPrice => return balances::get_spot_coin_mark_price(h, owner, &a, &b),
        ExchangeMethod::Balance => return balances::build_spot_balance(h, owner, &a),
        ExchangeMethod::ApplyBalances => {
            balances::apply_spot_balances(h, owner, &a)?;
            return Ok(h.undefined());
        }
        ExchangeMethod::RequestRefresh => {
            let a = if SpotAccountHost::is_undefined(h, &a) {
                SpotAccountHost::boolean(h, false)
            } else {
                a
            };
            balances::request_spot_balance_refresh(h, owner, &a)?;
            return Ok(h.undefined());
        }
        ExchangeMethod::GeneratePositions => {
            return account::generate_fake_positions_from_markets(h, owner, &a);
        }
        ExchangeMethod::Positions => return positions::spot_get_fake_positions(h, owner),
        ExchangeMethod::ListenCandles => return lifecycle::spot_listen_ohlcv(h, owner, &a, &b),
        ExchangeMethod::ListenBook => return lifecycle::spot_listen_order_book(h, owner, &a, &b),
        ExchangeMethod::Dispose => {
            lifecycle::spot_dispose(h, owner)?;
            return Ok(h.undefined());
        }
        ExchangeMethod::Validate => account::validate_account(h.clone(), owner.clone()),
        ExchangeMethod::Start => account::start(h.clone(), owner.clone()),
        ExchangeMethod::Tick => account::tick(h.clone(), owner.clone()),
        ExchangeMethod::State => account::fetch_spot_clearinghouse_state(h.clone(), owner.clone()),
        ExchangeMethod::Refresh => account::refresh_spot_balance(h.clone(), owner.clone(), a),
        ExchangeMethod::Candles => account::fetch_ohlcv(h.clone(), owner.clone(), a),
        ExchangeMethod::Orders => account::fetch_orders(h.clone(), owner.clone(), a),
        ExchangeMethod::FetchPositions => account::fetch_positions(h.clone(), owner.clone(), a),
        ExchangeMethod::Markets => trading::fetch_markets_and_tickers(h.clone(), owner.clone()),
        ExchangeMethod::Place => trading::place_order(h.clone(), owner.clone(), a),
        ExchangeMethod::Cancel => trading::cancel_orders(h.clone(), owner.clone(), a),
        ExchangeMethod::Update => trading::update_order(h.clone(), owner.clone(), a),
        ExchangeMethod::Leverage | ExchangeMethod::Margin => {
            let mut runtime = h.clone();
            let owner = owner.clone();
            return Ok(h.spawn_native(Box::pin(async move {
                if body == ExchangeMethod::Leverage {
                    requests::spot_set_leverage(&mut runtime, &owner, &a, &b, &c).await?
                } else {
                    requests::spot_change_margin_type(&mut runtime, &owner, &a, &b).await?
                }
                Ok(runtime.undefined())
            })));
        }
    };
    Ok(h.spawn_native(Box::pin(result)))
}
