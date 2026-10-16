//! Account initialization, authoritative holdings and private trade method bodies.
//! Imported helpers, current child methods, object/array/Map intrinsics and
//! scheduling are bindings; complete classes, transport and signing stay separate.
use crate::{
    account_reads::{AccountHost, returned, start, wait},
    order_adapters::NativeFill,
};
use std::{future::Future, rc::Rc};
/// Runtime capabilities actually used by these eight source bodies.
pub trait StateHost: AccountHost {
    fn disposed(&self) -> bool;
    fn public_only(&self) -> bool;
    fn assign_credentials(&self, value: Self::Value) -> Result<(), Self::Error>;
    fn funder(&self) -> Result<Self::Value, Self::Error>;
    fn ticker(&self, symbol: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn loaded(&self) -> Result<Self::Value, Self::Error>;
    fn spread_loaded(
        &self,
        value: Self::Value,
        flags: &[&'static str],
    ) -> Result<Self::Value, Self::Error>;
    fn call_sync(
        &self,
        call: Self::Callable,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn finite(&self, value: &Self::Value) -> bool;
    fn less_equal(&self, left: &Self::Value, right: &Self::Value) -> Result<bool, Self::Error>;
    fn multiply(&self, left: &Self::Value, right: &Self::Value)
    -> Result<Self::Value, Self::Error>;
    fn subtract(&self, left: &Self::Value, right: &Self::Value)
    -> Result<Self::Value, Self::Error>;
    /// for-of's iterator uses live slots/length; None means iterator done,
    /// while a hole yields Some(undefined). Implementations own iterator cleanup.
    type Iterator;
    fn iterate(&self, value: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn sum_by(
        &self,
        positions: &Self::Value,
        key: &'static str,
    ) -> Result<Self::Value, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    /// None only for a nullish parent token map; capture get before argument reads.
    fn optional_token_get(&self) -> Result<Option<Self::Callable>, Self::Error>;
    fn side_lookup(&self, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn emit_fill(
        &self,
        call: Self::Callable,
        fill: NativeFill<Self::Value>,
    ) -> Result<(), Self::Error>;
    fn schedule_tick(
        &self,
        callback: Rc<dyn Fn() -> Result<Self::Value, Self::Error>>,
        interval: f64,
    ) -> Result<(), Self::Error>;
    /// Promise.catch's detached chained operation is retained and monitored;
    /// throwing callback rejects it, not the already-returned refreshPositions call.
    fn catch_rejection(
        &self,
        operation: Rc<Self::Operation>,
        callback: Rc<dyn Fn(Self::Error) -> Result<(), Self::Error>>,
    ) -> Result<(), Self::Error>;
}
fn log<H: StateHost>(
    host: &H,
    receiver: &'static str,
    prefix: &str,
    error: Option<&H::Error>,
    severity: &'static str,
) -> Result<(), H::Error> {
    let call = host.capture(receiver, "log")?;
    let mut message = prefix.encode_utf16().collect::<Vec<_>>();
    if let Some(error) = error {
        message.extend(host.string(&host.error_value(error))?);
    }
    host.invoke_log(call, &message, severity)
}
fn commit<H: StateHost>(
    host: &H,
    field: Option<(&'static str, H::Value)>,
    flags: &[&'static str],
) -> Result<(), H::Error> {
    let call = host.capture("store", "update")?;
    let mut fields = vec![];
    if let Some((key, value)) = field {
        fields.push((key, value));
    }
    fields.push(("loaded", host.spread_loaded(host.loaded()?, flags)?));
    host.call_sync(call, vec![host.object(fields)?])?;
    Ok(())
}
async fn call<H: StateHost>(
    host: &H,
    receiver: &'static str,
    method: &'static str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let captured = host.capture(receiver, method)?;
    wait(host.invoke(captured, args)).await
}
/// PolymarketExchange#deriveApiCredentials. Missing-wallet refusal precedes try.
pub fn derive_api_credentials<H: StateHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            if !host.truthy(&host.wallet()?) {
                return Err(host.new_error("No wallet configured"));
            }
            let result = async {
                let nonce = host.number(0.0);
                let server = call(host.as_ref(), "api", "getServerTime", vec![]).await?;
                let signing = host.capture("imports", "signApiKeyDerivationMessage")?;
                let signed =
                    wait(host.invoke(signing, vec![host.wallet()?, nonce.clone(), server])).await?;
                let address = host.field(&signed, "address")?;
                let signature = host.field(&signed, "signature")?;
                let timestamp = host.field(&signed, "timestamp")?;
                let credentials = call(
                    host.as_ref(),
                    "api",
                    "deriveApiKey",
                    vec![address, nonce, signature, timestamp],
                )
                .await?;
                host.assign_credentials(credentials)?;
                log(
                    host.as_ref(),
                    "exchange",
                    "Successfully derived API credentials",
                    None,
                    "info",
                )?;
                Ok(host.undefined())
            }
            .await;
            match result {
                Ok(value) => returned(host.as_ref(), value).await,
                Err(error) => {
                    log(
                        host.as_ref(),
                        "exchange",
                        "Failed to derive API credentials: ",
                        Some(&error),
                        "error",
                    )?;
                    Err(error)
                }
            }
        }
    })
}
/// PolymarketExchange.start, with every disposal check after its source await.
pub fn start_exchange<H: StateHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let external =
                call(host.as_ref(), "exchange", "initExternalPublicData", vec![]).await?;
            if !host.truthy(&external) {
                let markets = call(host.as_ref(), "exchange", "fetchMarkets", vec![]).await?;
                if host.disposed() {
                    return Ok(host.undefined());
                }
                commit(
                    host.as_ref(),
                    Some(("markets", markets.clone())),
                    &["markets"],
                )?;
                let tickers = call(host.as_ref(), "exchange", "fetchTickers", vec![]).await?;
                if host.disposed() {
                    return Ok(host.undefined());
                }
                let logger = host.capture("exchange", "log")?;
                let count = host.length(&tickers)?.min(host.length(&markets)?) as f64;
                let mut message = "Loaded ".encode_utf16().collect::<Vec<_>>();
                message.extend(host.string(&host.number(count))?);
                message.extend(" Polymarket markets".encode_utf16());
                host.invoke_log(logger, &message, "default")?;
                commit(host.as_ref(), Some(("tickers", tickers)), &["tickers"])?;
            }
            if host.public_only() {
                commit(host.as_ref(), None, &["balance", "positions", "orders"])?;
                return Ok(host.undefined());
            }
            if host.truthy(&host.exchange_credentials()?) {
                let c = host.capture("privateWebsocket", "connectAndSubscribe")?;
                host.call_sync(c, vec![])?;
            }
            call(host.as_ref(), "exchange", "tick", vec![]).await?;
            if host.disposed() {
                return Ok(host.undefined());
            }
            log(
                host.as_ref(),
                "exchange",
                "Ready to trade on Polymarket",
                None,
                "default",
            )?;
            if host.truthy(&host.exchange_credentials()?) {
                let orders = call(host.as_ref(), "exchange", "fetchOrders", vec![]).await?;
                if host.disposed() {
                    return Ok(host.undefined());
                }
                commit(host.as_ref(), Some(("orders", orders)), &["orders"])?;
            }
            Ok(host.undefined())
        }
    })
}
/// PolymarketExchange#tick; errors are logged before scheduling its next call.
pub fn tick<H: StateHost>(host: Rc<H>) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            if host.disposed() {
                return Ok(host.undefined());
            }
            let result = async {
                if host.truthy(&host.exchange_credentials()?) {
                    let positions =
                        call(host.as_ref(), "exchange", "fetchAndSetPositions", vec![]).await?;
                    if host.disposed() {
                        return Ok(true);
                    }
                    let balance = call(host.as_ref(), "exchange", "fetchBalance", vec![]).await?;
                    if host.disposed() {
                        return Ok(true);
                    }
                    let upnl = if host.length(&positions)? > 0 {
                        host.sum_by(&positions, "unrealizedPnl")?
                    } else {
                        host.number(0.0)
                    };
                    host.assign(&balance, "upnl", upnl)?;
                    commit(host.as_ref(), Some(("balance", balance)), &["balance"])?;
                    let orders = call(host.as_ref(), "exchange", "fetchOrders", vec![]).await?;
                    if host.disposed() {
                        return Ok(true);
                    }
                    commit(host.as_ref(), Some(("orders", orders)), &["orders"])?;
                }
                Ok(false)
            }
            .await;
            if matches!(result, Ok(true)) {
                return Ok(host.undefined());
            }
            if let Err(error) = result {
                let c = host.capture("exchange", "log")?;
                let mut message = "Error in tick: ".encode_utf16().collect::<Vec<_>>();
                message.extend(host.string(&host.error_message(&error)?)?);
                host.invoke_log(c, &message, "error")?;
            }
            if !host.disposed() {
                let current = host.clone();
                host.schedule_tick(
                    Rc::new(move || {
                        let c = current.capture("exchange", "tick")?;
                        current.call_sync(c, vec![])
                    }),
                    10_000.0,
                )?;
            }
            Ok(host.undefined())
        }
    })
}
/// PolymarketExchange#fetchBalance, preserving parseFloat and 6-decimal division.
pub fn fetch_balance<H: StateHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let result = async {
                let c = host.capture("api", "getBalanceAllowance")?;
                let data = wait(host.invoke(c, vec![host.signer_type()?])).await?;
                let total = safe_cex_num::parse_float(&String::from_utf16_lossy(
                    &host.string(&host.field(&data, "balance")?)?,
                )) / 1e6;
                host.object(vec![
                    ("total", host.number(total)),
                    ("free", host.number(total)),
                    ("used", host.number(0.0)),
                    ("upnl", host.number(0.0)),
                ])
            }
            .await;
            match result {
                Ok(v) => returned(host.as_ref(), v).await,
                Err(e) => {
                    log(
                        host.as_ref(),
                        "exchange",
                        "Error fetching balance: ",
                        Some(&e),
                        "error",
                    )?;
                    Err(e)
                }
            }
        }
    })
}
/// PolymarketExchange.fetchPositions, using the Data API and original field reads.
pub fn fetch_positions<H: StateHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let result = async {
                let funder = host.funder()?;
                let user = if host.truthy(&funder) {
                    funder
                } else {
                    let wallet = host.wallet()?;
                    if host.nullish(&wallet) {
                        host.undefined()
                    } else {
                        host.field(&wallet, "address")?
                    }
                };
                if !host.truthy(&user) {
                    return Ok(host.array(vec![]));
                }
                let holdings = call(host.as_ref(), "api", "getPositions", vec![user]).await?;
                let positions = host.array(vec![]);
                let mut iter = host.iterate(holdings)?;
                while let Some(holding) = host.next(&mut iter)? {
                    let token = host.field(&holding, "asset")?;
                    if !host.finite(&host.field(&holding, "size")?)
                        || host.less_equal(&host.field(&holding, "size")?, &host.number(0.0))?
                    {
                        continue;
                    }
                    let get = host.capture("tokenToSymbol", "get")?;
                    let symbol = host.call_sync(get, vec![token.clone()])?;
                    if !host.truthy(&symbol) {
                        continue;
                    }
                    let avg = host.field(&holding, "avgPrice")?;
                    let set = host.capture("costBasis", "set")?;
                    host.call_sync(set, vec![token.clone(), avg.clone()])?;
                    let ticker = host.ticker(&symbol)?;
                    let current = host.field(&holding, "curPrice")?;
                    let current = if host.truthy(&current) {
                        current
                    } else {
                        let last = if host.nullish(&ticker) {
                            host.undefined()
                        } else {
                            host.field(&ticker, "last")?
                        };
                        if host.truthy(&last) {
                            last
                        } else {
                            avg.clone()
                        }
                    };
                    let pnl = if host.finite(&host.field(&holding, "cashPnl")?) {
                        host.field(&holding, "cashPnl")?
                    } else {
                        host.multiply(
                            &host.subtract(&current, &avg)?,
                            &host.field(&holding, "size")?,
                        )?
                    };
                    let id = host.capture("imports", "createPositionId")?;
                    let id = host.call_sync(
                        id,
                        vec![symbol.clone(), host.text("long"), host.text("oneway")],
                    )?;
                    let notional = host.multiply(&host.field(&holding, "size")?, &current)?;
                    let contracts = host.field(&holding, "size")?;
                    let info = host.object(vec![
                        ("tokenId", token),
                        ("conditionId", host.field(&holding, "conditionId")?),
                        ("redeemable", host.field(&holding, "redeemable")?),
                        ("mergeable", host.field(&holding, "mergeable")?),
                    ])?;
                    let position = host.object(vec![
                        ("id", id),
                        ("symbol", symbol),
                        ("side", host.text("long")),
                        ("entryPrice", avg),
                        ("notional", notional),
                        ("leverage", host.number(1.0)),
                        ("unrealizedPnl", pnl),
                        ("contracts", contracts),
                        ("liquidationPrice", host.number(0.0)),
                        ("positionMode", host.text("oneway")),
                        ("info", info),
                    ])?;
                    host.push_spread(&positions, &host.array(vec![Some(position)]))?;
                }
                Ok(positions)
            }
            .await;
            match result {
                Ok(v) => returned(host.as_ref(), v).await,
                Err(e) => {
                    log(
                        host.as_ref(),
                        "exchange",
                        "Error fetching positions: ",
                        Some(&e),
                        "error",
                    )?;
                    Err(e)
                }
            }
        }
    })
}
/// PolymarketExchange.fetchAndSetPositions: commit only success, then return same array.
pub fn fetch_and_set_positions<H: StateHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let positions = call(host.as_ref(), "exchange", "fetchPositions", vec![]).await?;
            if !host.disposed() {
                commit(
                    host.as_ref(),
                    Some(("positions", positions.clone())),
                    &["positions"],
                )?;
            }
            returned(host.as_ref(), positions).await
        }
    })
}
/// PolymarketPrivateWebSocket#refreshPositions: install a detached rejection handler.
pub fn refresh_positions<H: StateHost>(host: Rc<H>, reason: H::Value) -> Result<(), H::Error> {
    let c = host.capture("parent", "fetchAndSetPositions")?;
    let operation = host.invoke(c, vec![])?;
    let current = host.clone();
    host.catch_rejection(
        operation,
        Rc::new(move |error| {
            let c = current.capture("parent", "log")?;
            let mut message = "Error refreshing positions after "
                .encode_utf16()
                .collect::<Vec<_>>();
            message.extend(current.string(&reason)?);
            message.extend(": ".encode_utf16());
            message.extend(current.string(&current.error_value(&error))?);
            current.invoke_log(c, &message, "error")
        }),
    )
}
/// PolymarketPrivateWebSocket#handleTradeUpdate: fill, log, then current refresh child.
pub fn handle_trade_update<H: StateHost>(host: &H, data: H::Value) -> Result<(), H::Error> {
    let result = (|| {
        let get = host.optional_token_get()?;
        let symbol = if let Some(get) = get {
            host.call_sync(get, vec![host.field(&data, "asset_id")?])?
        } else {
            host.undefined()
        };
        let symbol = if host.truthy(&symbol) {
            symbol
        } else {
            host.field(&data, "asset_id")?
        };
        let emitter = host.capture("emitter", "emit")?;
        let id = host.field(&data, "id")?;
        let side = host.side_lookup(host.field(&data, "side")?)?;
        let side = if host.truthy(&side) {
            side
        } else {
            host.text("buy")
        };
        let price = safe_cex_num::parse_float(&String::from_utf16_lossy(
            &host.string(&host.field(&data, "price")?)?,
        ));
        let amount = safe_cex_num::parse_float(&String::from_utf16_lossy(
            &host.string(&host.field(&data, "size")?)?,
        ));
        host.emit_fill(
            emitter,
            NativeFill {
                id,
                side,
                symbol: symbol.clone(),
                price,
                amount,
            },
        )?;
        let c = host.capture("parent", "log")?;
        let mut message = "Trade: ".encode_utf16().collect::<Vec<_>>();
        message.extend(host.string(&host.field(&data, "side")?)?);
        message.push(32);
        message.extend(host.string(&host.field(&data, "size")?)?);
        message.push(32);
        message.extend(host.string(&symbol)?);
        message.extend(" @ ".encode_utf16());
        message.extend(host.string(&host.field(&data, "price")?)?);
        host.invoke_log(c, &message, "debug")?;
        let c = host.capture("private", "refreshPositions")?;
        host.call_sync(c, vec![host.text("trade update")])?;
        Ok(())
    })();
    if let Err(e) = result {
        log(
            host,
            "parent",
            "Error handling trade update: ",
            Some(&e),
            "error",
        )?;
    }
    Ok(())
}
