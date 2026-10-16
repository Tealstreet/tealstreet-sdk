//! Spot market snapshot and order bodies, retaining source helper/signing boundaries.
use crate::{
    helper_tasks::{self, HelperObserver},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
    ws_ping_lifecycle::WsPingHost,
};
#[derive(Clone, Debug)]
pub enum TradingCallback<V> {
    Asset { market: V },
    Token { meta: V, index: V },
    RestingId,
    Cancel { markets: V },
    CancelCopy,
}
pub trait SpotTradingHost: SpotAccountHost {
    fn trading_callback(
        &mut self,
        capture: TradingCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn unary_number(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn has_property(&mut self, key: &str, value: &Self::Value) -> Result<bool, Self::Error>;
    fn trading_binding(
        &mut self,
        value: &Self::Value,
        keys: &[&str],
    ) -> Result<Vec<Self::Value>, Self::Error>;
}
fn lit<H: WsPingHost>(h: &H, v: &str) -> H::Value {
    h.literal(&JsString(v.encode_utf16().collect()))
}
fn method<H: WsPingHost>(
    h: &mut H,
    o: &H::Value,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, k)?;
    let a = args(h)?;
    h.call(&f, o, &a)
}
fn free<H: SpotAccountHost>(
    h: &mut H,
    name: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.imported(name)?;
    let a = args(h)?;
    h.call(&f, &h.undefined(), &a)
}
fn field<H: WsPingHost>(h: &mut H, o: &H::Value, k: &str, v: &H::Value) -> Result<(), H::Error> {
    h.define_data(o, k, v)
}
fn optional<H: SpotAccountHost>(h: &mut H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(v) {
        Ok(h.undefined())
    } else {
        h.get(v, k)
    }
}
fn enum_value<H: SpotAccountHost>(h: &mut H, name: &str, key: &str) -> Result<H::Value, H::Error> {
    let e = h.imported(name)?;
    h.get(&e, key)
}
fn parse<H: SpotAccountHost>(
    h: &mut H,
    object: &H::Value,
    key: &str,
) -> Result<H::Value, H::Error> {
    let f = h.global("parseFloat")?;
    let value = free(h, "v", |h| Ok(vec![object.clone(), lit(h, key)]))?;
    h.call(&f, &h.undefined(), &[value])
}
fn headers<H: SpotAccountHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let c = h.object()?;
    let hdr = h.object()?;
    field(h, &hdr, "Content-Type", &lit(h, "application/json"))?;
    field(h, &c, "headers", &hdr)?;
    Ok(c)
}
async fn markets_body<H: SpotTradingHost>(mut h: H, owner: H::Value) -> Result<H::Value, H::Error> {
    let xhr = h.get(&owner, "xhr")?;
    let response = method(&mut h, &xhr, "post", |h| {
        let url = enum_value(h, "ENDPOINTS", "INFO")?;
        let body = h.object()?;
        field(h, &body, "type", &lit(h, "spotMetaAndAssetCtxs"))?;
        Ok(vec![url, body, headers(h)?])
    })?;
    let response = h.await_value(response).await?;
    let data = h.response_data(&response)?;
    let [meta, assets] = h.pair_binding(&data)?;
    let markets = h.array(&[])?;
    let tickers = h.array(&[])?;
    let mut i = 0.;
    loop {
        let universe = h.get(&meta, "universe")?;
        let len = h.get(&universe, "length")?;
        let less = h.binary("<", &h.number(i), &len)?;
        if !h.truthy(&less) {
            break;
        }
        let universe = h.get(&meta, "universe")?;
        let source_market = h.index(&universe, &h.number(i))?;
        let source_ticker = method(&mut h, &assets, "find", |h| {
            Ok(vec![h.trading_callback(TradingCallback::Asset {
                market: source_market.clone(),
            })?])
        })?;
        let tokens = h.get(&meta, "tokens")?;
        let token = method(&mut h, &tokens, "find", |h| {
            Ok(vec![h.trading_callback(TradingCallback::Token {
                meta: meta.clone(),
                index: h.number(i),
            })?])
        })?;
        if !h.truthy(&token) {
            i += 1.;
            continue;
        }
        let base = free(&mut h, "v", |h| {
            Ok(vec![source_market.clone(), lit(h, "name")])
        })?;
        let quote = lit(&h, "USDC");
        let amount_str = free(&mut h, "safeString", |h| {
            Ok(vec![token.clone(), lit(h, "szDecimals")])
        })?;
        let parser = h.global("parseInt")?;
        let amount = h.call(&parser, &h.undefined(), &[amount_str.clone()])?;
        if !h.truthy(&source_ticker) {
            i += 1.;
            continue;
        }
        let price = parse(&mut h, &source_ticker, "midPx")?;
        let live_mid = h.get(&source_ticker, "midPx")?;
        if !h.truthy(&live_mid) {
            i += 1.;
            continue;
        }
        let price_precision = free(&mut h, "calculatePricePrecision", |h| {
            Ok(vec![price, amount, h.number(8.)])
        })?;
        let price_str = free(&mut h, "numberToString", |_| Ok(vec![price_precision]))?;
        let token_name = h.get(&token, "name")?;
        let mut symbol = h.template_string(&token_name)?.0;
        symbol.extend("/USDC".encode_utf16());
        let symbol = h.literal(&JsString(symbol));
        method(&mut h, &markets, "push", |h| {
            let market = h.object()?;
            field(h, &market, "id", &base)?;
            field(h, &market, "symbol", &symbol)?;
            let name = h.get(&token, "name")?;
            field(h, &market, "base", &name)?;
            field(h, &market, "quote", &quote)?;
            field(h, &market, "isSpot", &h.boolean(true))?;
            field(h, &market, "active", &h.boolean(true))?;
            let precision = h.object()?;
            for (key, arg) in [("amount", &amount_str), ("price", &price_str)] {
                let value = free(h, "parsePrecision", |_| Ok(vec![arg.clone()]))?;
                let value = if h.truthy(&value) {
                    value
                } else {
                    h.number(0.)
                };
                let value = h.unary_number(&value)?;
                field(h, &precision, key, &value)?
            }
            field(h, &market, "precision", &precision)?;
            let limits = h.object()?;
            let a = h.object()?;
            let min = free(h, "parsePrecision", |_| Ok(vec![amount_str.clone()]))?;
            let min = if h.truthy(&min) { min } else { h.number(0.) };
            let min = h.unary_number(&min)?;
            field(h, &a, "min", &min)?;
            let number = h.global("Number")?;
            let max = h.get(&number, "MAX_SAFE_INTEGER")?;
            field(h, &a, "max", &max)?;
            field(h, &limits, "amount", &a)?;
            let leverage = h.object()?;
            field(h, &leverage, "min", &h.number(1.))?;
            field(h, &leverage, "max", &h.number(1.))?;
            field(h, &limits, "leverage", &leverage)?;
            field(h, &market, "limits", &limits)?;
            let info = h.object()?;
            let value = free(h, "v", |h| Ok(vec![source_market.clone(), lit(h, "index")]))?;
            let value = h.unary_number(&value)?;
            let id = h.binary("+", &value, &h.number(10000.))?;
            field(h, &info, "hyperliquidId", &id)?;
            h.spread_into(&info, &source_market)?;
            field(h, &market, "info", &info)?;
            Ok(vec![market])
        })?;
        let mid = parse(&mut h, &source_ticker, "midPx")?;
        let mark = parse(&mut h, &source_ticker, "markPx")?;
        let mark = if h.truthy(&mark) { mark } else { mid.clone() };
        let prev = parse(&mut h, &source_ticker, "prevDayPx")?;
        let quote_volume = parse(&mut h, &source_ticker, "dayNtlVlm")?;
        let quote_volume = if h.truthy(&quote_volume) {
            quote_volume
        } else {
            h.number(0.)
        };
        let day_base = parse(&mut h, &source_ticker, "dayBaseVlm")?;
        let number = h.global("Number")?;
        let finite = method(&mut h, &number, "isFinite", |_| Ok(vec![day_base.clone()]))?;
        let use_base = if h.truthy(&finite) {
            let greater = h.binary(">", &day_base, &h.number(0.))?;
            h.truthy(&greater)
        } else {
            false
        };
        let volume = if use_base {
            day_base
        } else {
            let greater = h.binary(">", &mid, &h.number(0.))?;
            if h.truthy(&greater) {
                h.binary("/", &quote_volume, &mid)?
            } else {
                h.number(0.)
            }
        };
        method(&mut h, &tickers, "push", |h| {
            let ticker = h.object()?;
            field(h, &ticker, "id", &base)?;
            field(h, &ticker, "symbol", &symbol)?;
            field(h, &ticker, "bid", &h.number(0.))?;
            field(h, &ticker, "ask", &h.number(0.))?;
            field(h, &ticker, "last", &mid)?;
            field(h, &ticker, "mark", &mark)?;
            let index = parse(h, &source_ticker, "oraclePx")?;
            field(h, &ticker, "index", &index)?;
            field(h, &ticker, "midPrice", &mid)?;
            let positive = h.binary(">", &prev, &h.number(0.))?;
            let percentage = if h.truthy(&positive) {
                let change = h.binary("-", &mark, &prev)?;
                let fraction = h.binary("/", &change, &prev)?;
                h.binary("*", &fraction, &h.number(100.))?
            } else {
                h.number(0.)
            };
            field(h, &ticker, "percentage", &percentage)?;
            let funding = parse(h, &source_ticker, "funding")?;
            field(h, &ticker, "fundingRate", &funding)?;
            field(h, &ticker, "fundingTime", &h.number(0.))?;
            field(h, &ticker, "volume", &volume)?;
            field(h, &ticker, "quoteVolume", &quote_volume)?;
            field(h, &ticker, "openInterest", &h.number(0.))?;
            field(h, &ticker, "info", &source_ticker)?;
            Ok(vec![ticker])
        })?;
        i += 1.;
    }
    h.array(&[markets, tickers])
}
fn asset_id<H: SpotAccountHost>(h: &mut H, market: &H::Value) -> Result<H::Value, H::Error> {
    if h.is_nullish(market) {
        Ok(h.undefined())
    } else {
        let info = h.get(market, "info")?;
        h.get(&info, "hyperliquidId")
    }
}
fn order_input<H: SpotTradingHost>(
    h: &mut H,
    owner: &H::Value,
    market: &H::Value,
    opts: &H::Value,
    reverse: bool,
    trigger: Option<&str>,
    market_ticker: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let wire = h.object()?;
    let coin = h.get(market, "symbol")?;
    field(h, &wire, "coin", &coin)?;
    let side = h.get(opts, "side")?;
    let buy = enum_value(h, "OrderSide", "Buy")?;
    let is_buy = h.strict_equal(&side, &buy);
    field(
        h,
        &wire,
        "is_buy",
        &h.boolean(if reverse { !is_buy } else { is_buy }),
    )?;
    let sz = h.get(opts, "amount")?;
    field(h, &wire, "sz", &sz)?;
    let price = method(h, owner, "priceToPrecision", |h| {
        let symbol = if let Some(ticker) = market_ticker {
            h.get(ticker, "symbol")?
        } else {
            h.get(market, "symbol")?
        };
        let price = if let Some(ticker) = market_ticker {
            let last = optional(h, ticker, "last")?;
            let last = if h.truthy(&last) { last } else { h.number(0.) };
            let side = h.get(opts, "side")?;
            let buy = enum_value(h, "OrderSide", "Buy")?;
            let factor = h.number(if h.strict_equal(&side, &buy) {
                1.05
            } else {
                0.95
            });
            h.binary("*", &last, &factor)?
        } else {
            let price = h.get(opts, "price")?;
            if h.truthy(&price) {
                price
            } else {
                h.number(0.)
            }
        };
        Ok(vec![symbol, price])
    })?;
    let price = h.unary_number(&price)?;
    field(h, &wire, "limit_px", &price)?;
    let order_type = h.object()?;
    if let Some(kind) = trigger {
        let t = h.object()?;
        field(h, &t, "isMarket", &h.boolean(true))?;
        field(h, &t, "tpsl", &lit(h, kind))?;
        let value = h.get(
            opts,
            if kind == "sl" {
                "stopLoss"
            } else {
                "takeProfit"
            },
        )?;
        field(h, &t, "triggerPx", &value)?;
        field(h, &order_type, "trigger", &t)?;
    } else {
        let limit = h.object()?;
        let tif = if market_ticker.is_some() {
            lit(h, "Gtc")
        } else {
            let tif = h.get(opts, "timeInForce")?;
            let post_only = enum_value(h, "OrderTimeInForce", "PostOnly")?;
            lit(
                h,
                if h.strict_equal(&tif, &post_only) {
                    "Alo"
                } else {
                    "Gtc"
                },
            )
        };
        field(h, &limit, "tif", &tif)?;
        field(h, &order_type, "limit", &limit)?;
    }
    field(h, &wire, "order_type", &order_type)?;
    field(h, &wire, "reduce_only", &h.boolean(false))?;
    Ok(wire)
}
async fn signed_post<H: SpotTradingHost>(
    h: &mut H,
    owner: &H::Value,
    action: H::Value,
) -> Result<H::Value, H::Error> {
    let date = h.global("Date")?;
    let raw = method(h, &date, "now", |_| Ok(vec![]))?;
    let nonce = h.unary_number(&raw)?;
    let signature = free(h, "signL1Action", |h| {
        let wallet = free(h, "createWalletFromPrivateKey", |h| {
            let options = h.get(owner, "options")?;
            Ok(vec![h.get(&options, "secret")?])
        })?;
        Ok(vec![wallet, action.clone(), h.null(), nonce.clone()])
    })?;
    let signature = h.await_value(signature).await?;
    let payload = h.object()?;
    field(h, &payload, "action", &action)?;
    field(h, &payload, "nonce", &nonce)?;
    field(h, &payload, "signature", &signature)?;
    let xhr = h.get(owner, "xhr")?;
    let response = method(h, &xhr, "post", |h| {
        Ok(vec![
            enum_value(h, "ENDPOINTS", "EXCHANGE")?,
            payload,
            headers(h)?,
        ])
    })?;
    h.await_value(response).await
}
async fn place_body<H: SpotTradingHost>(
    mut h: H,
    owner: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    method(&mut h, &owner, "resolveCallerClientOrderId", |_| {
        Ok(vec![opts.clone()])
    })?;
    let ty = h.get(&opts, "type")?;
    let trailing = enum_value(&mut h, "OrderType", "TrailingStopLoss")?;
    if h.strict_equal(&ty, &trailing) {
        return unsupported(&mut h);
    }
    let store = h.get(&owner, "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let symbol = h.get(&opts, "symbol")?;
    let market = h.index(&markets, &symbol)?;
    if !h.truthy(&market) {
        let ctor = h.global("Error")?;
        let symbol = h.get(&opts, "symbol")?;
        let mut msg = "Market ".encode_utf16().collect::<Vec<_>>();
        msg.extend(h.template_string(&symbol)?.0);
        msg.extend(" not found".encode_utf16());
        let msg = h.literal(&JsString(msg));
        let error = h.construct(&ctor, &[msg])?;
        return Err(h.throw_value(error));
    }
    let ty = h.get(&opts, "type")?;
    let market_type = enum_value(&mut h, "OrderType", "Market")?;
    let action = if h.strict_equal(&ty, &market_type) {
        let asset = asset_id(&mut h, &market)?;
        let store = h.get(&owner, "store")?;
        let tickers = h.get(&store, "tickersBySymbol")?;
        let symbol = h.get(&market, "symbol")?;
        let ticker = h.index(&tickers, &symbol)?;
        if !h.truthy(&ticker) {
            return Ok(h.undefined());
        }
        let wire = free(&mut h, "orderRequestToOrderWire", |h| {
            Ok(vec![
                order_input(h, &owner, &market, &opts, false, None, Some(&ticker))?,
                asset,
            ])
        })?;
        free(&mut h, "orderWiresToOrderAction", |h| {
            Ok(vec![h.array(&[wire])?])
        })?
    } else {
        let ty = h.get(&opts, "type")?;
        let limit = enum_value(&mut h, "OrderType", "Limit")?;
        if h.strict_equal(&ty, &limit) {
            let asset = asset_id(&mut h, &market)?;
            let wire = free(&mut h, "orderRequestToOrderWire", |h| {
                Ok(vec![
                    order_input(h, &owner, &market, &opts, false, None, None)?,
                    asset.clone(),
                ])
            })?;
            let wires = h.array(&[wire])?;
            for (kind, key) in [("sl", "stopLoss"), ("tp", "takeProfit")] {
                let trigger = h.get(&opts, key)?;
                if h.truthy(&trigger) {
                    method(&mut h, &wires, "push", |h| {
                        Ok(vec![free(h, "orderRequestToOrderWire", |h| {
                            Ok(vec![
                                order_input(h, &owner, &market, &opts, true, Some(kind), None)?,
                                asset.clone(),
                            ])
                        })?])
                    })?;
                }
            }
            let tp = h.get(&opts, "takeProfit")?;
            let attached = if h.truthy(&tp) {
                tp
            } else {
                h.get(&opts, "stopLoss")?
            };
            let has_attached = h.truthy(&attached);
            free(
                &mut h,
                if has_attached {
                    "orderWiresToNormalTPSLAction"
                } else {
                    "orderWiresToOrderAction"
                },
                |_| Ok(vec![wires]),
            )?
        } else {
            let ty = h.get(&opts, "type")?;
            let tp = enum_value(&mut h, "OrderType", "TakeProfit")?;
            let supported = if h.strict_equal(&ty, &tp) {
                true
            } else {
                let ty = h.get(&opts, "type")?;
                let sl = enum_value(&mut h, "OrderType", "StopLoss")?;
                h.strict_equal(&ty, &sl)
            };
            if supported {
                return unsupported(&mut h);
            }
            let ctor = h.global("Error")?;
            let ty = h.get(&opts, "type")?;
            let mut msg = "Unknown order type ".encode_utf16().collect::<Vec<_>>();
            msg.extend(h.template_string(&ty)?.0);
            let msg = h.literal(&JsString(msg));
            let error = h.construct(&ctor, &[msg])?;
            return Err(h.throw_value(error));
        }
    };
    let response = signed_post(&mut h, &owner, action).await?;
    let data = h.response_data(&response)?;
    method(&mut h, &owner, "requestSpotBalanceRefresh", |h| {
        Ok(vec![h.boolean(true)])
    })?;
    free(&mut h, "omitUndefined", |h| {
        let r = h.get(&data, "response")?;
        let d = h.get(&r, "data")?;
        let statuses = h.get(&d, "statuses")?;
        let mapped = method(h, &statuses, "map", |h| {
            Ok(vec![h.trading_callback(TradingCallback::RestingId)?])
        })?;
        Ok(vec![mapped])
    })
}
fn unsupported<H: SpotTradingHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let ctor = h.global("Error")?;
    let error = h.construct(&ctor, &[lit(h, "Not supported")])?;
    Err(h.throw_value(error))
}
async fn cancel_body<H: SpotTradingHost>(
    mut h: H,
    owner: H::Value,
    orders: H::Value,
) -> Result<H::Value, H::Error> {
    let store = h.get(&owner, "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let cancels = method(&mut h, &orders, "map", |h| {
        Ok(vec![
            h.trading_callback(TradingCallback::Cancel { markets })?,
        ])
    })?;
    let action = h.object()?;
    let kind = lit(&h, "cancel");
    field(&mut h, &action, "type", &kind)?;
    let copies = method(&mut h, &cancels, "map", |h| {
        Ok(vec![h.trading_callback(TradingCallback::CancelCopy)?])
    })?;
    field(&mut h, &action, "cancels", &copies)?;
    signed_post(&mut h, &owner, action).await?;
    method(&mut h, &owner, "requestSpotBalanceRefresh", |h| {
        Ok(vec![h.boolean(true)])
    })?;
    Ok(h.undefined())
}
async fn update_body<H: SpotTradingHost>(
    mut h: H,
    owner: H::Value,
    options: H::Value,
) -> Result<H::Value, H::Error> {
    let parts = h.trading_binding(&options, &["order", "update"])?;
    let order = &parts[0];
    let update = &parts[1];
    let next = h.object()?;
    for key in ["symbol", "type", "side", "price", "amount"] {
        let value = h.get(order, key)?;
        field(&mut h, &next, key, &value)?
    }
    let reduce = h.get(order, "reduceOnly")?;
    let reduce = if h.truthy(&reduce) {
        reduce
    } else {
        h.boolean(false)
    };
    field(&mut h, &next, "reduceOnly", &reduce)?;
    for key in ["price", "amount"] {
        if h.has_property(key, update)? {
            let value = h.get(update, key)?;
            h.set(&next, key, &value)?
        }
    }
    let cancel = method(&mut h, &owner, "cancelOrders", |h| {
        Ok(vec![h.array(&[order.clone()])?])
    })?;
    h.await_value(cancel).await?;
    let dead = h.get(&owner, "_deadOrders")?;
    method(&mut h, &dead, "add", |h| Ok(vec![h.get(order, "id")?]))?;
    let placed = method(&mut h, &owner, "placeOrder", |_| Ok(vec![next]))?;
    h.await_value(placed).await
}
pub fn run_trading_callback<H: SpotTradingHost>(
    h: &mut H,
    c: &TradingCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match c {
        TradingCallback::Asset { market } => {
            let coin = h.get(&first, "coin")?;
            let name = h.get(market, "name")?;
            Ok(h.boolean(h.strict_equal(&coin, &name)))
        }
        TradingCallback::Token { meta, index } => {
            let n = h.get(&first, "index")?;
            let universe = h.get(meta, "universe")?;
            let market = h.index(&universe, index)?;
            let tokens = h.get(&market, "tokens")?;
            let id = h.index(&tokens, &h.number(0.))?;
            Ok(h.boolean(h.strict_equal(&n, &id)))
        }
        TradingCallback::RestingId => {
            let resting = optional(h, &first, "resting")?;
            optional(h, &resting, "oid")
        }
        TradingCallback::Cancel { markets } => {
            let symbol = h.get(&first, "symbol")?;
            let market = h.index(markets, &symbol)?;
            let row = h.object()?;
            let asset = asset_id(h, &market)?;
            let asset = h.unary_number(&asset)?;
            field(h, &row, "a", &asset)?;
            let id = h.get(&first, "id")?;
            let id = h.unary_number(&id)?;
            field(h, &row, "o", &id)?;
            Ok(row)
        }
        TradingCallback::CancelCopy => {
            let parts = h.trading_binding(&first, &["a", "o"])?;
            let row = h.object()?;
            field(h, &row, "a", &parts[0])?;
            field(h, &row, "o", &parts[1])?;
            Ok(row)
        }
    }
}
pub fn fetch_markets_and_tickers<H>(host: H, owner: H::Value) -> HelperObserver<H::Value, H::Error>
where
    H: SpotTradingHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, markets_body(host, owner))
}
pub fn place_order<H>(
    host: H,
    owner: H::Value,
    opts: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotTradingHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, place_body(host, owner, opts))
}
pub fn cancel_orders<H>(
    host: H,
    owner: H::Value,
    orders: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotTradingHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, cancel_body(host, owner, orders))
}
pub fn update_order<H>(
    host: H,
    owner: H::Value,
    options: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotTradingHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, update_body(host, owner, options))
}
