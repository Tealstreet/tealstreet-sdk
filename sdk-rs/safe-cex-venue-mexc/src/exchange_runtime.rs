//! Account polling, cancellation and configuration source bodies over bound owners.
use crate::method_tasks::{MethodObserver, TaskHost, observe, start};
use std::rc::Rc;
pub type Callback<V, E> = Rc<dyn Fn(V) -> Result<V, E>>;
pub type Iteration<'a, V, E> = dyn FnMut(V) -> Result<bool, E> + 'a;
/// Bound calls preserve receivers/getter order, live iteration and error identity.
/// All primitive values use the existing UTF-16 Runtime/SessionRuntime contract.
/// Timers and eager Promise.all operations remain owned until completion.
pub trait ExchangeRuntimeHost: TaskHost {
    fn exchange(&self) -> Self::Value;
    fn call(
        &self,
        callee: Self::Value,
        receiver: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn function(&self, callback: Callback<Self::Value, Self::Error>) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn indexed(&self, value: &Self::Value, key: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn destructure_pair(
        &self,
        value: Self::Value,
    ) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn maximum(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn minimum(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn endpoint(&self, name: &str) -> Self::Value;
    fn to_symbol(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn omit_undefined(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn reverse_position_mode(&self, mode: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn promise_all(&self, values: Vec<Self::Value>) -> Result<Rc<Self::Operation>, Self::Error>;
    fn for_of(
        &self,
        value: Self::Value,
        visitor: &mut Iteration<'_, Self::Value, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn schedule_loop(
        &self,
        callback: Rc<dyn Fn() -> Result<Self::Value, Self::Error>>,
        delay: Self::Value,
    ) -> Result<(), Self::Error>;
    fn clear_timeout(&self, handle: Self::Value) -> Result<(), Self::Error>;
    fn base_dispose(&self) -> Result<(), Self::Error>;
}
fn text<H: ExchangeRuntimeHost>(h: &H, value: &str) -> H::Value {
    h.text(value.encode_utf16().collect())
}
fn field<H: ExchangeRuntimeHost>(h: &H, key: &str) -> Result<H::Value, H::Error> {
    h.field(&h.exchange(), key)
}
fn method<H: ExchangeRuntimeHost>(
    h: &H,
    receiver: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let callee = h.field(&receiver, key)?;
    h.call(callee, receiver, args)
}
fn own<H: ExchangeRuntimeHost>(
    h: &H,
    key: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    method(h, h.exchange(), key, args)
}
async fn awaited<H: ExchangeRuntimeHost>(
    h: &H,
    value: Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    observe(h.resolve_return(value?)).await
}
fn optional<H: ExchangeRuntimeHost>(
    h: &H,
    value: H::Value,
    key: &str,
) -> Result<H::Value, H::Error> {
    if h.nullish(&value) {
        Ok(h.undefined())
    } else {
        h.field(&value, key)
    }
}
fn loaded<H: ExchangeRuntimeHost>(h: &H, keys: &[&str]) -> Result<H::Value, H::Error> {
    let target = h.spread(&h.field(&field(h, "store")?, "loaded")?)?;
    for key in keys {
        h.define_field(&target, key, h.boolean(true))?;
    }
    Ok(target)
}
fn error_message<H: ExchangeRuntimeHost>(h: &H, error: &H::Error) -> Result<H::Value, H::Error> {
    let err = h.error_value(error);
    let response = optional(h, err.clone(), "response")?;
    let data = optional(h, response, "data")?;
    let message = optional(h, data, "message")?;
    if h.truthy(&message) {
        Ok(message)
    } else {
        optional(h, err, "message")
    }
}
fn current_config<H: ExchangeRuntimeHost>(h: &H, symbol: H::Value) -> Result<H::Value, H::Error> {
    let value = method(h, field(h, "symbolSettings")?, "get", vec![symbol.clone()])?;
    if h.truthy(&value) {
        Ok(value)
    } else {
        own(h, "deriveSymbolConfig", vec![symbol])
    }
}
fn market<H: ExchangeRuntimeHost>(h: &H, symbol: &H::Value) -> Result<H::Value, H::Error> {
    h.indexed(&h.field(&field(h, "store")?, "marketsBySymbol")?, symbol)
}
fn require_market<H: ExchangeRuntimeHost>(h: &H, symbol: &H::Value) -> Result<H::Value, H::Error> {
    let market = market(h, symbol)?;
    if !h.truthy(&market) {
        let mut message = "Market ".encode_utf16().collect::<Vec<_>>();
        message.extend(h.abstract_string(symbol)?);
        message.extend(" not found".encode_utf16());
        Err(h.new_error(message))
    } else {
        Ok(market)
    }
}
fn clamp<H: ExchangeRuntimeHost>(
    h: &H,
    value: H::Value,
    market: &H::Value,
) -> Result<H::Value, H::Error> {
    let min = h.field(&h.field(&h.field(market, "limits")?, "leverage")?, "min")?;
    let maxed = h.maximum(value, min)?;
    let max = h.field(&h.field(&h.field(market, "limits")?, "leverage")?, "max")?;
    h.minimum(maxed, max)
}
/// ts: MexcExchange.fetchBalanceAndPositions.
pub fn fetch_balance_and_positions<H: ExchangeRuntimeHost>(
    host: Rc<H>,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let balance = own(h.as_ref(), "fetchBalance", vec![])?;
        let positions = own(h.as_ref(), "fetchPositions", vec![])?;
        let results = observe(h.promise_all(vec![balance, positions])).await?;
        let (balance, positions) = h.destructure_pair(results)?;
        Ok(h.object(vec![("balance", balance), ("positions", positions)]))
    })
}
/// ts: MexcExchange.tick.
pub fn tick<H: ExchangeRuntimeHost>(host: Rc<H>) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        if h.truthy(&field(h.as_ref(), "isDisposed")?)
            || h.truthy(&field(h.as_ref(), "isPublicDataOnly")?)
        {
            return Ok(h.undefined());
        }
        let balance: Result<bool, H::Error> = async {
            let pair = awaited(
                h.as_ref(),
                own(h.as_ref(), "fetchBalanceAndPositions", vec![]),
            )
            .await?;
            let balance = h.field(&pair, "balance")?;
            let positions = h.field(&pair, "positions")?;
            if h.truthy(&field(h.as_ref(), "isDisposed")?) {
                return Ok(true);
            }
            method(
                h.as_ref(),
                field(h.as_ref(), "store")?,
                "addOrUpdatePositionsFull",
                vec![positions],
            )?;
            let store = field(h.as_ref(), "store")?;
            let update = h.field(&store, "update")?;
            let next = h.object(vec![
                ("balance", balance),
                ("loaded", loaded(h.as_ref(), &["balance", "positions"])?),
            ]);
            h.call(update, store, vec![next])?;
            Ok(false)
        }
        .await;
        if matches!(balance, Ok(true)) {
            return Ok(h.undefined());
        }
        if let Err(err) = balance {
            let receiver = h.exchange();
            let emit = h.field(&receiver, "emitAndLogError")?;
            h.call(emit, receiver, vec![error_message(h.as_ref(), &err)?])?;
        }
        let orders = awaited(
            h.as_ref(),
            own(h.as_ref(), "fetchAndSetRegularOrders", vec![]),
        )
        .await;
        if let Err(err) = orders {
            let receiver = h.exchange();
            let emit = h.field(&receiver, "emitAndLogError")?;
            h.call(emit, receiver, vec![error_message(h.as_ref(), &err)?])?;
        }
        if !h.truthy(&field(h.as_ref(), "isDisposed")?) {
            let next = h.clone();
            let callback = Rc::new(move || own(next.as_ref(), "tick", vec![]));
            let delay = own(h.as_ref(), "getTickInterval", vec![])?;
            h.schedule_loop(callback, delay)?;
        }
        Ok(h.undefined())
    })
}
/// ts: MexcExchange.cancelSymbolOrders.
pub fn cancel_symbol_orders<H: ExchangeRuntimeHost>(
    host: Rc<H>,
    symbol: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let market = market(h.as_ref(), &symbol)?;
        let xhr = field(h.as_ref(), "xhr")?;
        let post = h.field(&xhr, "post")?;
        let endpoint = h.endpoint("ORDER_CANCEL_ALL");
        let id = optional(h.as_ref(), market, "id")?;
        let id = if h.truthy(&id) {
            id
        } else {
            h.to_symbol(symbol.clone())?
        };
        let payload = h.object(vec![("symbol", id)]);
        let payload = h.omit_undefined(payload)?;
        awaited(h.as_ref(), h.call(post, xhr, vec![endpoint, payload])).await?;
        let store = field(h.as_ref(), "store")?;
        let update = h.field(&store, "updateOrdersStatus")?;
        let orders = h.field(&field(h.as_ref(), "store")?, "orders")?;
        let next = h.clone();
        let wanted = symbol.clone();
        let filter = h.function(Rc::new(move |order| {
            Ok(next.boolean(next.equal(&next.field(&order, "symbol")?, &wanted)))
        }));
        let orders = method(h.as_ref(), orders, "filter", vec![filter])?;
        let next = h.clone();
        let map = h.function(Rc::new(move |order| {
            Ok(next.object(vec![("id", next.field(&order, "id")?)]))
        }));
        let orders = method(h.as_ref(), orders, "map", vec![map])?;
        h.call(update, store, vec![orders, text(h.as_ref(), "canceled")])?;
        Ok(h.undefined())
    })
}
/// ts: MexcExchange.cancelAllOrders.
pub fn cancel_all_orders<H: ExchangeRuntimeHost>(
    host: Rc<H>,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let xhr = field(h.as_ref(), "xhr")?;
        let post = h.field(&xhr, "post")?;
        awaited(
            h.as_ref(),
            h.call(
                post,
                xhr,
                vec![h.endpoint("ORDER_CANCEL_ALL"), h.object(vec![])],
            ),
        )
        .await?;
        let store = field(h.as_ref(), "store")?;
        let update = h.field(&store, "updateOrdersStatus")?;
        let orders = h.field(&field(h.as_ref(), "store")?, "orders")?;
        let next = h.clone();
        let map = h.function(Rc::new(move |order| {
            Ok(next.object(vec![("id", next.field(&order, "id")?)]))
        }));
        let orders = method(h.as_ref(), orders, "map", vec![map])?;
        h.call(update, store, vec![orders, text(h.as_ref(), "canceled")])?;
        Ok(h.undefined())
    })
}
/// ts: MexcExchange.setLeverage.
pub fn set_leverage<H: ExchangeRuntimeHost>(
    host: Rc<H>,
    symbol: H::Value,
    leverage: H::Value,
    side: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let market = require_market(h.as_ref(), &symbol)?;
        let leverage = clamp(h.as_ref(), leverage, &market)?;
        let config = current_config(h.as_ref(), symbol.clone())?;
        let types = if h.is_text(&side, "short") {
            vec![2.]
        } else if h.is_text(&side, "long") {
            vec![1.]
        } else {
            vec![1., 2.]
        };
        for ty in types {
            let xhr = field(h.as_ref(), "xhr")?;
            let post = h.field(&xhr, "post")?;
            let endpoint = h.endpoint("CHANGE_LEVERAGE");
            let format = h.field(&h.exchange(), "formatChangeLeveragePayload")?;
            let args = vec![
                symbol.clone(),
                market.clone(),
                h.number(ty),
                leverage.clone(),
                h.field(&config, "marginType")?,
            ];
            let payload = h.call(format, h.exchange(), args)?;
            let response = awaited(h.as_ref(), h.call(post, xhr, vec![endpoint, payload])).await?;
            own(
                h.as_ref(),
                "assertCommonSuccess",
                vec![
                    h.field(&response, "data")?,
                    text(h.as_ref(), "MEXC leverage change"),
                ],
            )?;
        }
        let next = h.spread(&config)?;
        h.define_field(
            &next,
            "buyLeverage",
            if h.is_text(&side, "short") {
                h.field(&config, "buyLeverage")?
            } else {
                leverage.clone()
            },
        )?;
        h.define_field(
            &next,
            "sellLeverage",
            if h.is_text(&side, "long") {
                h.field(&config, "sellLeverage")?
            } else {
                leverage
            },
        )?;
        method(
            h.as_ref(),
            field(h.as_ref(), "symbolSettings")?,
            "set",
            vec![symbol.clone(), next.clone()],
        )?;
        own(h.as_ref(), "updateFetchedLeverage", vec![symbol, next])?;
        Ok(h.undefined())
    })
}
/// ts: MexcExchange.changePositionMode.
pub fn change_position_mode<H: ExchangeRuntimeHost>(
    host: Rc<H>,
    hedged: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let mode = text(
            h.as_ref(),
            if h.truthy(&hedged) {
                "hedged"
            } else {
                "oneway"
            },
        );
        let xhr = field(h.as_ref(), "xhr")?;
        let post = h.field(&xhr, "post")?;
        let mode_number = h.reverse_position_mode(&mode)?;
        awaited(
            h.as_ref(),
            h.call(
                post,
                xhr,
                vec![
                    h.endpoint("CHANGE_POSITION_MODE"),
                    h.object(vec![("positionMode", mode_number)]),
                ],
            ),
        )
        .await?;
        h.set_field(&h.exchange(), "accountPositionMode", mode.clone())?;
        let settings = field(h.as_ref(), "symbolSettings")?;
        h.for_of(settings, &mut |entry| {
            let (symbol, config) = h.destructure_pair(entry)?;
            let next = h.spread(&config)?;
            h.define_field(&next, "positionMode", mode.clone())?;
            method(
                h.as_ref(),
                field(h.as_ref(), "symbolSettings")?,
                "set",
                vec![symbol.clone(), next.clone()],
            )?;
            own(h.as_ref(), "updateFetchedLeverage", vec![symbol, next])?;
            Ok(false)
        })?;
        method(
            h.as_ref(),
            field(h.as_ref(), "store")?,
            "setSetting",
            vec![text(h.as_ref(), "isHedged"), hedged],
        )?;
        let positions = awaited(h.as_ref(), own(h.as_ref(), "fetchPositions", vec![])).await?;
        method(
            h.as_ref(),
            field(h.as_ref(), "store")?,
            "addOrUpdatePositionsFull",
            vec![positions],
        )?;
        Ok(h.undefined())
    })
}
/// ts: MexcExchange.changeMarginType.
pub fn change_margin_type<H: ExchangeRuntimeHost>(
    host: Rc<H>,
    isolated: H::Value,
    symbol: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        if !h.truthy(&symbol) {
            return Err(h.new_error("Symbol is required".encode_utf16().collect()));
        }
        let market = require_market(h.as_ref(), &symbol)?;
        let margin = text(
            h.as_ref(),
            if h.truthy(&isolated) {
                "isolated"
            } else {
                "cross"
            },
        );
        let config = current_config(h.as_ref(), symbol.clone())?;
        let mut leverage = Vec::new();
        for key in ["buyLeverage", "sellLeverage"] {
            let value = h.field(&config, key)?;
            leverage.push(clamp(
                h.as_ref(),
                if h.truthy(&value) {
                    value
                } else {
                    h.number(20.)
                },
                &market,
            )?);
        }
        for (ty, leverage) in [1., 2.].into_iter().zip(leverage) {
            let xhr = field(h.as_ref(), "xhr")?;
            let post = h.field(&xhr, "post")?;
            let endpoint = h.endpoint("CHANGE_LEVERAGE");
            let format = h.field(&h.exchange(), "formatChangeLeveragePayload")?;
            let args = vec![
                symbol.clone(),
                market.clone(),
                h.number(ty),
                leverage,
                margin.clone(),
                h.object(vec![("includeOpenTypeWithPositionId", h.boolean(true))]),
            ];
            let payload = h.call(format, h.exchange(), args)?;
            let response = awaited(h.as_ref(), h.call(post, xhr, vec![endpoint, payload])).await?;
            own(
                h.as_ref(),
                "assertCommonSuccess",
                vec![
                    h.field(&response, "data")?,
                    text(h.as_ref(), "MEXC margin type change"),
                ],
            )?;
        }
        let next = h.spread(&config)?;
        h.define_field(&next, "marginType", margin)?;
        method(
            h.as_ref(),
            field(h.as_ref(), "symbolSettings")?,
            "set",
            vec![symbol.clone(), next.clone()],
        )?;
        own(h.as_ref(), "updateFetchedLeverage", vec![symbol, next])?;
        Ok(h.undefined())
    })
}
/// ts: MexcExchange.dispose.
pub fn dispose<H: ExchangeRuntimeHost>(h: &H) -> Result<(), H::Error> {
    let timers = field(h, "pendingStopSnapshotRefreshTimers")?;
    let values = method(h, timers, "values", vec![])?;
    h.for_of(values, &mut |timer| {
        h.clear_timeout(timer)?;
        Ok(false)
    })?;
    method(
        h,
        field(h, "pendingStopSnapshotRefreshTimers")?,
        "clear",
        vec![],
    )?;
    h.base_dispose()
}
