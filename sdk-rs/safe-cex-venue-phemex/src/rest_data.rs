//! REST data bodies retain opaque models, arrays, cached values and errors.
use safe_cex_num::{math_max, math_min, multiply1, subtract};
use std::{
    future::{Future, poll_fn},
    task::Poll,
};
pub trait DataHost {
    type V: Clone;
    type E;
    type Call;
    type Read: Future<Output = Result<Self::V, Self::E>>;
    fn get(&mut self, v: &Self::V, key: &str) -> Result<Self::V, Self::E>;
    fn optional(&mut self, v: &Self::V, key: &str) -> Result<Self::V, Self::E>;
    fn key(&mut self, v: &Self::V, key: &str) -> Result<Self::V, Self::E>;
    fn truthy(&mut self, v: &Self::V) -> Result<bool, Self::E>;
    fn equals(&mut self, a: &Self::V, b: &Self::V) -> Result<bool, Self::E>;
    fn is_text(&mut self, v: &Self::V) -> Result<bool, Self::E>;
    fn coerce(&mut self, v: Self::V) -> Result<f64, Self::E>;
    fn number_value(&mut self, v: &Self::V) -> Result<Option<f64>, Self::E>;
    fn parse(&mut self, v: Self::V) -> Result<f64, Self::E>;
    fn text(&mut self, v: Self::V) -> Result<Vec<u16>, Self::E>;
    fn upper(&mut self, text: Vec<u16>) -> Result<Vec<u16>, Self::E>;
    fn interval_text(&mut self, value: Self::V) -> Result<Vec<u16>, Self::E>;
    fn lower(&mut self, text: Vec<u16>) -> Result<Vec<u16>, Self::E>;
    fn new_text(&mut self, text: Vec<u16>) -> Self::V;
    fn num(&mut self, n: f64) -> Self::V;
    fn boolean(&mut self, b: bool) -> Self::V;
    fn undefined(&mut self) -> Self::V;
    fn array(&mut self, items: Vec<Self::V>) -> Result<Self::V, Self::E>;
    fn object(&mut self, fields: Vec<(&'static str, Self::V)>) -> Result<Self::V, Self::E>;
    fn spread(
        &mut self,
        v: &Self::V,
        fields: Vec<(&'static str, Self::V)>,
    ) -> Result<Self::V, Self::E>;
    fn rows(&mut self, v: Self::V) -> Result<Vec<Self::V>, Self::E>;
    fn is_array(&mut self, v: &Self::V) -> Result<bool, Self::E>;
    fn length(&mut self, v: &Self::V) -> Result<usize, Self::E>;
    fn at(&mut self, v: &Self::V, index: usize) -> Result<Self::V, Self::E>;
    fn store(&mut self, key: &str) -> Result<Self::V, Self::E>;
    fn cached(&mut self, table: &str, symbol: &Self::V) -> Result<Self::V, Self::E>;
    fn format_in(&mut self, symbol: Self::V) -> Result<Self::V, Self::E>;
    fn format_out(&mut self, symbol: Self::V) -> Result<Self::V, Self::E>;
    fn table(&mut self, table: &str, key: Self::V) -> Result<Self::V, Self::E>;
    fn position_id(&mut self, symbol: &Self::V, side: &str, mode: &str)
    -> Result<Self::V, Self::E>;
    fn price_diff(&mut self, entry: f64, last: f64, side: &str) -> Result<f64, Self::E>;
    fn map_position(&mut self, row: Self::V) -> Result<Self::V, Self::E>;
    fn account_positions(&mut self) -> impl Future<Output = Result<Self::V, Self::E>>;
    fn symbol_positions(
        &mut self,
        symbol: Self::V,
    ) -> impl Future<Output = Result<Self::V, Self::E>>;
    fn flatten(&mut self, arrays: Vec<Self::V>) -> Result<Self::V, Self::E>;
    fn read_target(&mut self) -> Result<Self::Call, Self::E>;
    fn read(
        &mut self,
        call: Self::Call,
        endpoint: &'static str,
        params: Vec<(&'static str, Self::V)>,
    ) -> Result<Self::Read, Self::E>;
    fn error_value(&mut self, error: &Self::E) -> Self::V;
    fn error(&mut self, message: Self::V) -> Result<Self::E, Self::E>;
    fn emit(&mut self, message: Self::V) -> Result<(), Self::E>;
    fn emit_candle_error(&mut self, message: Self::V) -> Result<(), Self::E>;
    fn now(&mut self) -> Result<Self::V, Self::E>;
    fn includes(&mut self, value: &Self::V, needle: &str) -> Result<bool, Self::E>;
    fn nullish(&mut self, v: &Self::V) -> Result<bool, Self::E>;
    fn public_only(&mut self) -> Result<bool, Self::E>;
    fn date_seconds(&mut self, value: Self::V) -> Result<Option<f64>, Self::E>;
    fn end_time(&mut self, to: Self::V, daily: bool) -> Result<Self::V, Self::E>;
    fn start_time(
        &mut self,
        end: &Self::V,
        from: Self::V,
        use_from: bool,
        amount: f64,
        unit: Vec<u16>,
    ) -> Result<Self::V, Self::E>;
    fn milliseconds(&mut self, date: &Self::V) -> Result<f64, Self::E>;
    fn round_usd(&mut self, n: f64) -> Result<f64, Self::E>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn label<H: DataHost>(h: &mut H, s: &str) -> H::V {
    h.new_text(units(s))
}
fn is<H: DataHost>(h: &mut H, v: &H::V, s: &str) -> Result<bool, H::E> {
    let expected = label(h, s);
    h.equals(v, &expected)
}
fn choice<H: DataHost>(h: &mut H, row: &H::V, keys: &[&str], mapped: bool) -> Result<H::V, H::E> {
    let mut value = h.undefined();
    for key in keys {
        value = if mapped {
            h.key(row, key)?
        } else {
            h.get(row, key)?
        };
        if h.truthy(&value)? {
            break;
        }
    }
    Ok(value)
}
fn parsed<H: DataHost>(h: &mut H, row: &H::V, key: &str) -> Result<f64, H::E> {
    let v = h.key(row, key)?;
    h.parse(v)
}
fn number_field<H: DataHost>(h: &mut H, row: &H::V, key: &str) -> Result<f64, H::E> {
    let v = h.get(row, key)?;
    Ok(h.number_value(&v)?.unwrap_or(f64::NAN))
}
fn endpoint(key: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
}
fn error_message<H: DataHost>(
    h: &mut H,
    error: &H::E,
    data: bool,
    msg: bool,
) -> Result<H::V, H::E> {
    let error = h.error_value(error);
    let response = h.optional(&error, "response")?;
    let response = if data {
        h.optional(&response, "data")?
    } else {
        response
    };
    let message = h.optional(&response, "msg")?;
    if h.truthy(&message)? {
        return Ok(message);
    }
    if msg {
        let message = h.optional(&error, "msg")?;
        if h.truthy(&message)? {
            return Ok(message);
        }
    }
    h.optional(&error, "message")
}
pub async fn fetch_markets<H: DataHost>(h: &mut H) -> Result<H::V, H::E> {
    let result = async {
        let call = h.read_target()?;
        let reply = h.read(call, endpoint("MARKETS"), vec![])?.await?;
        let data = h.get(&reply, "data")?;
        let data = h.get(&data, "data")?;
        let products = h.get(&data, "perpProductsV2")?;
        let mut accepted = Vec::new();
        for row in h.rows(products)? {
            let ty = h.key(&row, "type")?;
            if !is(h, &ty, "PerpetualV2")? {
                continue;
            }
            let settle = h.key(&row, "settleCurrency")?;
            if is(h, &settle, "USDT")? || is(h, &settle, "USDC")? {
                accepted.push(row)
            }
        }
        let mut markets = Vec::new();
        for row in accepted {
            let base = h.key(&row, "baseCurrency")?;
            let quote = h.key(&row, "quoteCurrency")?;
            let margin = h.key(&row, "settleCurrency")?;
            let id = h
                .text(base.clone())?
                .into_iter()
                .chain([47])
                .chain(h.text(quote.clone())?)
                .chain([58])
                .chain(h.text(margin)?)
                .collect();
            let id = h.new_text(id);
            let symbol = h.get(&row, "symbol")?;
            let symbol = h.format_in(symbol)?;
            let active = h.boolean(true);
            let amount = parsed(h, &row, "qtyStepSize")?;
            let amount = h.num(amount);
            let price = parsed(h, &row, "tickSize")?;
            let price = h.num(price);
            let precision = h.object(vec![("amount", amount), ("price", price)])?;
            let min = h.num(0.0);
            let max = math_min(
                parsed(h, &row, "maxOrderQtyRq")?,
                parsed(h, &row, "maxOrderQtyRq")?,
            );
            let max = h.num(max);
            let amount = h.object(vec![("min", min), ("max", max)])?;
            let min = h.num(1.0);
            let max = parsed(h, &row, "maxLeverage")?;
            let max = h.num(max);
            let leverage = h.object(vec![("min", min), ("max", max)])?;
            let limits = h.object(vec![("amount", amount), ("leverage", leverage)])?;
            markets.push(h.object(vec![
                ("id", id),
                ("symbol", symbol),
                ("base", base),
                ("quote", quote),
                ("active", active),
                ("precision", precision),
                ("limits", limits),
                ("info", row),
            ])?);
        }
        h.array(markets)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let message = error_message(h, &e, false, true)?;
            h.emit(message)?;
            h.store("markets")
        }
    }
}
pub fn map_position<H: DataHost>(h: &mut H, row: H::V) -> Result<H::V, H::E> {
    let entry = choice(h, &row, &["avgEntryPrice", "avgEntryPriceRp"], true)?;
    let entry = h.parse(entry)?;
    let size = parsed(h, &row, "size")?;
    if !size.is_finite() || size == 0.0 || !entry.is_finite() {
        return Ok(h.undefined());
    }
    let raw = h.get(&row, "symbol")?;
    let symbol = h.format_in(raw)?;
    let ticker = h.cached("tickersBySymbol", &symbol)?;
    let mark = parsed(h, &row, "markPriceRp")?;
    let last = h.optional(&ticker, "last")?;
    let last = h
        .number_value(&last)?
        .filter(|v| v.is_finite())
        .unwrap_or(if mark.is_finite() { mark } else { entry });
    let pos_side = h.key(&row, "posSide")?;
    let raw_side = choice(h, &row, &["side", "Side"], true)?;
    let side = if is(h, &pos_side, "Long")? || is(h, &raw_side, "Buy")? {
        "long"
    } else {
        "short"
    };
    let diff = h.price_diff(entry, last, side)?;
    let pnl = multiply1(size, diff);
    let leverage = h.get(&row, "leverageRr")?;
    let leverage = h.parse(leverage)?;
    let mode = h.get(&row, "posMode")?;
    let mode = if is(h, &mode, "Hedged")? {
        "hedged"
    } else {
        "oneway"
    };
    let cross = if leverage < 0.0 {
        true
    } else {
        let cross = h.get(&row, "crossMargin")?;
        h.truthy(&cross)?
    };
    let margin = label(h, if cross { "cross" } else { "isolated" });
    let id = h.position_id(&symbol, side, mode)?;
    let side = label(h, side);
    let mode = label(h, mode);
    let notional = h.num(size.abs() * entry);
    let entry = h.num(entry);
    let leverage = h.num(leverage.abs());
    let pnl = h.num(pnl);
    let contracts = h.num(size.abs());
    let liq = parsed(h, &row, "liquidationPriceRp")?;
    let liq = h.num(liq);
    h.object(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", side),
        ("entryPrice", entry),
        ("notional", notional),
        ("leverage", leverage),
        ("unrealizedPnl", pnl),
        ("contracts", contracts),
        ("liquidationPrice", liq),
        ("marginType", margin),
        ("positionMode", mode),
        ("info", row),
    ])
}

pub async fn fetch_positions<H: DataHost>(h: &mut H, symbols: H::V) -> Result<H::V, H::E> {
    if !h.truthy(&symbols)? {
        let result = h.account_positions().await?;
        return h.get(&result, "positions");
    }
    let mut positions = Vec::new();
    for symbol in h.rows(symbols)? {
        positions.push(h.symbol_positions(symbol).await?);
    }
    h.flatten(positions)
}
pub async fn fetch_orders<H: DataHost>(h: &mut H) -> Result<H::V, H::E> {
    h.store("orders")
}
pub fn map_order_update<H: DataHost>(h: &mut H, row: H::V) -> Result<H::V, H::E> {
    let raw = choice(h, &row, &["ordStatus", "orderStatus"], false)?;
    let raw = if h.truthy(&raw)? { raw } else { label(h, "") };
    let raw = h.text(raw)?;
    let raw = h.lower(raw)?;
    let raw = h.new_text(raw);
    let status = h.table("ORDER_STATUS", raw)?;
    let status = if h.truthy(&status)? {
        status
    } else {
        label(h, "canceled")
    };
    let raw_type = choice(h, &row, &["ordType", "orderType"], false)?;
    let mut ty = "market";
    let mut conditional = false;
    for kind in ["Stop", "StopLimit", "MarketIfTouched", "LimitIfTouched"] {
        if is(h, &raw_type, kind)? {
            conditional = true;
            break;
        }
    }
    if is(h, &raw_type, "Limit")? {
        ty = "limit"
    } else if conditional {
        let direction = h.get(&row, "stopDirection")?;
        let side = h.get(&row, "side")?;
        if is(h, &direction, "Rising")? {
            ty = if is(h, &side, "Sell")? {
                "take_profit_market"
            } else {
                "stop_market"
            }
        } else if is(h, &direction, "Falling")? {
            ty = if is(h, &side, "Sell")? {
                "stop_market"
            } else {
                "take_profit_market"
            }
        }
    }
    let tif = h.get(&row, "timeInForce")?;
    let tif = h.table("TIME_IN_FORCE", tif)?;
    let reduce = h.get(&row, "reduceOnly")?;
    let true_value = h.boolean(true);
    let mut reduce_only = h.equals(&reduce, &true_value)? || is(h, &reduce, "true")?;
    if !reduce_only {
        for needle in ["ReduceOnly", "CloseOnTrigger"] {
            let exec = h.get(&row, "execInst")?;
            if !h.nullish(&exec)? && h.includes(&exec, needle)? {
                reduce_only = true;
                break;
            }
        }
    }
    let amount = choice(h, &row, &["orderQty", "orderQtyRq"], true)?;
    let amount = if h.truthy(&amount)? {
        amount
    } else {
        label(h, "0")
    };
    let amount = h.parse(amount)?;
    let filled = if is(h, &status, "canceled")? {
        0.0
    } else {
        let v = choice(h, &row, &["execQty"], true)?;
        let v = if h.truthy(&v)? { v } else { label(h, "0") };
        let n = h.parse(v)?;
        if n == 0.0 || n.is_nan() { 0.0 } else { n }
    };
    let mut remaining = 1.0;
    for key in ["leavesQty", "orderQty", "orderQtyRq"] {
        let n = parsed(h, &row, key)?;
        if n != 0.0 && !n.is_nan() {
            remaining = n;
            break;
        }
    }
    let trigger = choice(h, &row, &["trigger", "triggerType"], true)?;
    let id = h.get(&row, "orderID")?;
    let raw_symbol = h.get(&row, "symbol")?;
    let symbol = h.format_in(raw_symbol)?;
    let side = h.get(&row, "side")?;
    let buy = is(h, &side, "Buy")?;
    let side = label(h, if buy { "buy" } else { "sell" });
    let stop = parsed(h, &row, "stopPxRp")?;
    let price = if stop == 0.0 || stop.is_nan() {
        parsed(h, &row, "priceRp")?
    } else {
        stop
    };
    let price = h.num(price);
    let amount = h.num(amount);
    let reduce = h.boolean(reduce_only);
    let filled = h.num(filled);
    let remaining = h.num(remaining);
    let now = h.now()?;
    let ty_value = label(h, ty);
    let mut fields = vec![
        ("id", id),
        ("status", status),
        ("symbol", symbol),
        ("type", ty_value),
        ("side", side),
        ("price", price),
        ("amount", amount),
        ("reduceOnly", reduce),
        ("filled", filled),
        ("remaining", remaining),
        ("lastUpdate", now),
        ("info", row.clone()),
        ("timeInForce", tif),
    ];
    if ty == "take_profit_market" || ty == "stop_market" {
        let last = is(h, &trigger, "ByLastPrice")?;
        let trigger = label(h, if last { "last_price" } else { "mark_price" });
        fields.push((
            if ty == "take_profit_market" {
                "tpTriggerType"
            } else {
                "slTriggerType"
            },
            trigger,
        ));
    }
    let original = h.object(fields)?;
    let sl = parsed(h, &row, "stopLossRp")?;
    let tp = parsed(h, &row, "takeProfitRp")?;
    let inverse = label(h, if buy { "sell" } else { "buy" });
    let mut orders = vec![original.clone()];
    for (price, suffix, ty, trigger_field, field) in [
        (
            sl,
            "__stop_loss",
            "stop_market",
            "slTrigger",
            "slTriggerType",
        ),
        (
            tp,
            "__take_profit",
            "take_profit_market",
            "tpTrigger",
            "tpTriggerType",
        ),
    ] {
        if price > 0.0 {
            let id = h.get(&row, "orderID")?;
            let id = h.text(id)?.into_iter().chain(units(suffix)).collect();
            let id = h.new_text(id);
            let ty = label(h, ty);
            let price = h.num(price);
            let filled = h.num(0.0);
            let amount = h.num(0.0);
            let now = h.now()?;
            let trigger = h.key(&row, trigger_field)?;
            let last = is(h, &trigger, "ByLastPrice")?;
            let trigger = label(h, if last { "last_price" } else { "mark_price" });
            orders.push(h.spread(
                &original,
                vec![
                    ("id", id),
                    ("type", ty),
                    ("side", inverse.clone()),
                    ("price", price),
                    ("filled", filled),
                    ("amount", amount),
                    ("lastUpdate", now),
                    (field, trigger),
                ],
            )?);
        }
    }
    h.array(orders)
}
fn currency_matches<H: DataHost>(h: &mut H, position: &H::V, currency: &str) -> Result<bool, H::E> {
    let symbol = h.get(position, "symbol")?;
    let market = h.cached("marketsBySymbol", &symbol)?;
    let quote = h.optional(&market, "quote")?;
    if is(h, &quote, currency)? {
        return Ok(true);
    }
    let info = h.optional(&market, "info")?;
    let settle = h.optional(&info, "settleCurrency")?;
    if is(h, &settle, currency)? {
        return Ok(true);
    }
    let info = h.optional(position, "info")?;
    let own = h.optional(&info, "currency")?;
    is(h, &own, currency)
}
fn currency_balance<H: DataHost>(
    h: &mut H,
    total: f64,
    used: f64,
    upnl: f64,
    info: Option<H::V>,
) -> Result<H::V, H::E> {
    let free = h.num(subtract(total, used));
    let total = h.num(total);
    let used = h.num(used);
    let upnl = h.num(upnl);
    let mut fields = vec![
        ("total", total),
        ("used", used),
        ("free", free),
        ("upnl", upnl),
    ];
    if let Some(info) = info {
        fields.push(("info", info))
    }
    h.object(fields)
}
fn pnl_sum<H: DataHost>(h: &mut H, positions: &[H::V]) -> Result<f64, H::E> {
    let mut sum = 0.0;
    for position in positions {
        let v = h.get(position, "unrealizedPnl")?;
        let text = h.text(v)?;
        let value = h.new_text(text);
        sum += h.parse(value)?;
    }
    h.round_usd(sum)
}
fn validate_account<H: DataHost>(
    h: &mut H,
    outcome: Result<H::V, H::E>,
    currency: &str,
) -> Result<(H::V, H::V), H::E> {
    (|| {
        let reply = outcome?;
        let response = h.get(&reply, "data")?;
        let data = h.optional(&response, "data")?;
        let code_response = h.get(&reply, "data")?;
        let code = h.optional(&code_response, "code")?;
        let zero = h.num(0.0);
        let valid = h.equals(&code, &zero)?;
        let account = if valid {
            h.optional(&data, "account")?
        } else {
            h.undefined()
        };
        let account_valid = valid && h.truthy(&account)?;
        let positions = if account_valid {
            h.get(&data, "positions")?
        } else {
            h.undefined()
        };
        if !account_valid || !h.is_array(&positions)? {
            let error_response = h.get(&reply, "data")?;
            let msg = h.optional(&error_response, "msg")?;
            let msg = if h.truthy(&msg)? {
                msg
            } else {
                label(h, &format!("Invalid {currency} account-position response"))
            };
            return Err(h.error(msg)?);
        }
        let copied = h.spread(&data, vec![])?;
        Ok((h.get(&copied, "account")?, h.get(&copied, "positions")?))
    })()
}
pub async fn fetch_positions_and_balance<H: DataHost>(h: &mut H) -> Result<H::V, H::E> {
    let result = async {
        let mut reads = Vec::new();
        let mut outcomes: Vec<Option<Result<(H::V, H::V), H::E>>> =
            std::iter::repeat_with(|| None).take(2).collect();
        for (index, currency) in ["USDT", "USDC"].iter().enumerate() {
            let read = (|| {
                let call = h.read_target()?;
                let currency = label(h, currency);
                h.read(call, endpoint("ACCOUNT"), vec![("currency", currency)])
            })();
            match read {
                Ok(read) => reads.push(Some(Box::pin(read))),
                Err(e) => {
                    reads.push(None);
                    outcomes[index] = Some(Err(e));
                }
            }
        }
        poll_fn(|cx| {
            let mut pending = false;
            for (index, (read, outcome)) in reads.iter_mut().zip(outcomes.iter_mut()).enumerate() {
                if let Some(future) = read {
                    match future.as_mut().poll(cx) {
                        Poll::Ready(result) => {
                            *read = None;
                            *outcome = Some(validate_account(h, result, ["USDT", "USDC"][index]))
                        }
                        Poll::Pending => pending = true,
                    }
                }
            }
            if pending {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        let mut accounts = Vec::new();
        let mut balances = Vec::new();
        let mut parsed_positions = Vec::new();
        let mut failed = Vec::new();
        for (currency, validated) in ["USDT", "USDC"].into_iter().zip(outcomes) {
            let validated = validated.unwrap();
            let (account, positions) = match validated {
                Ok(v) => v,
                Err(error) => {
                    failed.push(currency);
                    let mut msg = error_message(h, &error, true, false)?;
                    if !h.truthy(&msg)? {
                        let value = h.error_value(&error);
                        let text = h.text(value)?;
                        msg = h.new_text(text);
                    }
                    h.emit(msg)?;
                    continue;
                }
            };
            accounts.push((currency, account.clone()));
            let mut current = Vec::new();
            let length = h.length(&positions)?;
            for index in 0..length {
                let position = h.at(&positions, index)?;
                let symbol = h.get(&position, "symbol")?;
                let symbol = h.format_in(symbol)?;
                let market = h.cached("marketsBySymbol", &symbol)?;
                if !h.truthy(&market)? {
                    continue;
                }
                let position = h.map_position(position)?;
                if !h.truthy(&position)? {
                    continue;
                }
                current.push(position.clone());
                parsed_positions.push(position);
            }
            let total = h.get(&account, "accountBalanceRv")?;
            let total = h.parse(total)?;
            let used = h.get(&account, "totalUsedBalanceRv")?;
            let used = h.parse(used)?;
            let upnl = pnl_sum(h, &current)?;
            let balance = currency_balance(h, total, used, upnl, Some(account))?;
            balances.push((currency, balance));
        }
        if balances.is_empty() {
            let balance = h.store("balance")?;
            let positions = h.store("positions")?;
            return h.object(vec![("balance", balance), ("positions", positions)]);
        }
        for currency in failed {
            let old = h.store("balance")?;
            let currencies = h.get(&old, "currencies")?;
            let previous = h.optional(&currencies, currency)?;
            let balance = if h.truthy(&previous)? {
                previous
            } else {
                currency_balance(h, 0.0, 0.0, 0.0, None)?
            };
            balances.push((currency, balance));
            let positions = h.store("positions")?;
            for position in h.rows(positions)? {
                if currency_matches(h, &position, currency)? {
                    parsed_positions.push(position)
                }
            }
        }
        let mut total = 0.0;
        for (_, balance) in &balances {
            total += number_field(h, balance, "total")?;
        }
        let mut used = 0.0;
        for (_, balance) in &balances {
            used += number_field(h, balance, "used")?;
        }
        let upnl = pnl_sum(h, &parsed_positions)?;
        let free = h.num(subtract(total, used));
        let used = h.num(used);
        let total = h.num(total);
        let upnl = h.num(upnl);
        let info = h.object(accounts)?;
        let currencies = h.object(balances)?;
        let balance = h.object(vec![
            ("used", used),
            ("free", free),
            ("total", total),
            ("upnl", upnl),
            ("info", info),
            ("currencies", currencies),
        ])?;
        let positions = h.array(parsed_positions)?;
        h.object(vec![("balance", balance), ("positions", positions)])
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let message = error_message(h, &e, true, false)?;
            h.emit(message)?;
            let balance = h.store("balance")?;
            let positions = h.store("positions")?;
            h.object(vec![("balance", balance), ("positions", positions)])
        }
    }
}
fn millis<H: DataHost>(h: &mut H, v: &H::V) -> Result<Option<f64>, H::E> {
    Ok(h.number_value(v)?.filter(|n| n.is_finite()).map(|n| {
        if n > 1_000_000_000_000.0 {
            n.floor()
        } else {
            (n * 1000.0).floor()
        }
    }))
}
fn extract<H: DataHost>(h: &mut H, payload: H::V) -> Result<H::V, H::E> {
    let data = h.optional(&payload, "data")?;
    let body = if h.nullish(&data)? { payload } else { data };
    let data = h.optional(&body, "data")?;
    let root = if !h.nullish(&data)? {
        data
    } else {
        let result = h.optional(&body, "result")?;
        if h.nullish(&result)? { body } else { result }
    };
    let rows = h.optional(&root, "rows")?;
    let list = h.optional(&root, "list")?;
    for value in [rows, list, root] {
        if h.is_array(&value)? {
            return Ok(value);
        }
    }
    h.array(vec![])
}
fn parse_fills<H: DataHost>(
    h: &mut H,
    rows: H::V,
    market: &H::V,
    order_id: &H::V,
) -> Result<Vec<H::V>, H::E> {
    let mut result = Vec::new();
    for row in h.rows(rows)? {
        let exec = h.get(&row, "execType")?;
        let exec = if h.nullish(&exec)? {
            h.get(&row, "tradeType")?
        } else {
            exec
        };
        if h.is_text(&exec)? && h.truthy(&exec)? {
            let text = h.text(exec)?;
            let text = h.lower(text)?;
            if text != units("trade") {
                continue;
            }
        }
        let symbol = choice(h, &row, &["symbol", "contract"], false)?;
        let symbol = if h.truthy(&symbol)? {
            symbol
        } else {
            label(h, "")
        };
        let symbol = h.format_in(symbol)?;
        let expected = h.get(market, "symbol")?;
        if !h.equals(&symbol, &expected)? {
            continue;
        }
        let side = h.get(&row, "side")?;
        let side = if h.nullish(&side)? {
            h.get(&row, "execSide")?
        } else {
            side
        };
        let side = if h.nullish(&side)? {
            label(h, "")
        } else {
            side
        };
        let side = h.text(side)?;
        let side = h.new_text(side);
        let text = h.text(side)?;
        let text = h.upper(text)?;
        let side = if text == units("BUY") {
            "buy"
        } else if text == units("SELL") {
            "sell"
        } else {
            continue;
        };
        let price = choice(
            h,
            &row,
            &["execPriceRp", "execPrice", "price", "avgPrice"],
            false,
        )?;
        let price = if h.truthy(&price)? {
            price
        } else {
            label(h, "0")
        };
        let price = h.parse(price)?;
        let qty = choice(
            h,
            &row,
            &["execQtyRq", "execQty", "lastQty", "qty", "size"],
            false,
        )?;
        let qty = if h.truthy(&qty)? { qty } else { label(h, "0") };
        let qty = h.parse(qty)?;
        if !price.is_finite() || !qty.is_finite() || price <= 0.0 || qty <= 0.0 {
            continue;
        }
        let timestamp = choice(
            h,
            &row,
            &[
                "createdAt",
                "execTime",
                "createTime",
                "transactTimeNs",
                "transactTime",
                "timestamp",
            ],
            false,
        )?;
        let Some(timestamp) = h.date_seconds(timestamp)? else {
            continue;
        };
        if timestamp == 0.0 || timestamp.is_nan() {
            continue;
        }
        let id = choice(h, &row, &["orderID", "orderId", "execID", "execId"], false)?;
        let id = if h.truthy(&id)? { id } else { label(h, "") };
        let id = h.text(id)?;
        let filter = h.truthy(order_id)?;
        if filter && !id.is_empty() && id != h.text(order_id.clone())? {
            continue;
        }
        let fee = choice(h, &row, &["execFeeRv", "execFee", "fee"], false)?;
        let fee = if h.truthy(&fee)? { fee } else { label(h, "0") };
        let fee = h.parse(fee)?;
        let fill_id = choice(h, &row, &["execID", "execId"], false)?;
        let mut fields = Vec::new();
        if h.truthy(&fill_id)? {
            let text = h.text(fill_id)?;
            let text = h.new_text(text);
            fields.push(("id", text));
        }
        let time = h.num(timestamp);
        let symbol = h.get(market, "symbol")?;
        fields.extend([("timestamp", time), ("symbol", symbol)]);
        if !id.is_empty() {
            let id = h.new_text(id);
            fields.push(("orderId", id));
        }
        let price = h.num(price);
        let qty = h.num(qty);
        let side = label(h, side);
        fields.extend([("price", price), ("qty", qty), ("side", side)]);
        if fee.is_finite() {
            let fee = h.num(fee.abs());
            fields.push(("commission", fee));
        }
        let asset = choice(h, &row, &["feeCurrency", "currency"], false)?;
        let asset = if h.truthy(&asset)? {
            asset
        } else {
            h.get(market, "quote")?
        };
        fields.push(("commissionAsset", asset));
        result.push(h.object(fields)?);
    }
    Ok(result)
}
pub async fn fetch_historical_fills<H: DataHost>(h: &mut H, opts: H::V) -> Result<H::V, H::E> {
    if h.public_only()? {
        return h.array(vec![]);
    }
    let symbol = h.get(&opts, "symbol")?;
    let order_id = h.get(&opts, "orderId")?;
    let from = h.get(&opts, "from")?;
    let to = h.get(&opts, "to")?;
    let limit = h.get(&opts, "limit")?;
    let undefined = h.undefined();
    let limit = if h.equals(&limit, &undefined)? {
        h.num(100.0)
    } else {
        limit
    };
    if !h.truthy(&symbol)? {
        return h.array(vec![]);
    }
    let market = h.cached("marketsBySymbol", &symbol)?;
    if !h.truthy(&market)? {
        return h.array(vec![]);
    }
    let limit = math_min(math_max(h.coerce(limit)?, 1.0), 200.0);
    let symbol = h.format_out(symbol)?;
    let from = millis(h, &from)?;
    let to = millis(h, &to)?;
    let result = async {
        let text = h.text(symbol.clone())?;
        let currency = label(
            h,
            if text.ends_with(&units("USDC")) {
                "USDC"
            } else {
                "USDT"
            },
        );
        let offset = h.num(0.0);
        let max = h.num(limit);
        let call = h.read_target()?;
        let response = h
            .read(
                call,
                endpoint("USER_TRADES"),
                vec![
                    ("symbol", symbol.clone()),
                    ("currency", currency),
                    ("offset", offset),
                    ("limit", max),
                ],
            )?
            .await?;
        let data = h.optional(&response, "data")?;
        let primary = extract(h, data)?;
        let fallback = if h.length(&primary)? > 0 {
            h.array(vec![])?
        } else {
            let mut fields = vec![("symbol", symbol)];
            if let Some(from) = from {
                fields.push(("start", h.num(from)))
            }
            if let Some(to) = to {
                fields.push(("end", h.num(to)))
            }
            fields.extend([
                ("offset", h.num(0.0)),
                ("limit", h.num(limit)),
                ("withCount", h.boolean(true)),
            ]);
            let call = h.read_target()?;
            let response = h
                .read(call, endpoint("USER_TRADES_LEGACY"), fields)?
                .await?;
            let data = h.optional(&response, "data")?;
            extract(h, data)?
        };
        let mut parsed = parse_fills(h, primary, &market, &order_id)?;
        parsed.extend(parse_fills(h, fallback, &market, &order_id)?);
        let mut dedup: Vec<(Vec<u16>, H::V)> = Vec::new();
        for execution in parsed {
            let timestamp = number_field(h, &execution, "timestamp")?;
            if from
                .filter(|n| *n != 0.0)
                .is_some_and(|n| timestamp * 1000.0 < n)
                || to
                    .filter(|n| *n != 0.0)
                    .is_some_and(|n| timestamp * 1000.0 > n)
            {
                continue;
            }
            let order = h.get(&execution, "orderId")?;
            let order = if h.truthy(&order)? {
                order
            } else {
                label(h, "no-order")
            };
            let mut key = h.text(order)?;
            for field in ["timestamp", "price", "qty", "side"] {
                key.push(58);
                let value = h.get(&execution, field)?;
                key.extend(h.text(value)?);
            }
            if let Some((_, v)) = dedup.iter_mut().find(|(k, _)| *k == key) {
                *v = execution
            } else {
                dedup.push((key, execution))
            }
        }
        let mut ordered = Vec::new();
        for (_, execution) in dedup {
            let time = number_field(h, &execution, "timestamp")?;
            ordered.push((time, execution))
        }
        ordered.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let count = if limit.is_nan() {
            0
        } else {
            limit.max(0.0).trunc() as usize
        };
        h.array(ordered.into_iter().take(count).map(|(_, v)| v).collect())
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let message = error_message(h, &e, true, false)?;
            h.emit(message)?;
            h.array(vec![])
        }
    }
}
fn interval_parts(text: &[u16]) -> (Vec<u16>, Vec<u16>) {
    let Some(start) = text.iter().position(|c| *c >= 48 && *c <= 57) else {
        return (vec![], vec![]);
    };
    let end = start
        + text[start..]
            .iter()
            .position(|c| *c < 48 || *c > 57)
            .unwrap_or(text.len() - start);
    let unit_end = end
        + text[end..]
            .iter()
            .position(|c| *c >= 48 && *c <= 57)
            .unwrap_or(text.len() - end);
    (text[start..end].to_vec(), text[end..unit_end].to_vec())
}
pub async fn fetch_ohlcv<H: DataHost>(h: &mut H, opts: H::V) -> Result<H::V, H::E> {
    let result = async {
        let interval = h.get(&opts, "interval")?;
        let resolution = h.table("timeframes", interval)?;
        let limit = h.get(&opts, "limit")?;
        let limit = if h.truthy(&limit)? {
            h.coerce(limit)?
        } else {
            500.0
        };
        let limit = math_min(limit, 1000.0);
        let interval = h.get(&opts, "interval")?;
        let text = h.interval_text(interval)?;
        let (amount, unit) = interval_parts(&text);
        let amount = h.new_text(amount);
        let amount = h.parse(amount)?;
        let to = h.get(&opts, "to")?;
        let daily = if h.truthy(&to)? {
            h.coerce(resolution.clone())? >= 86400.0
        } else {
            false
        };
        let end = h.end_time(to, daily)?;
        let raw_limit = h.get(&opts, "limit")?;
        let no_limit = !h.truthy(&raw_limit)?;
        let from = if no_limit {
            h.get(&opts, "from")?
        } else {
            h.undefined()
        };
        let use_from = no_limit && h.truthy(&from)?;
        let start = h.start_time(&end, from, use_from, amount * limit, unit)?;
        let call = h.read_target()?;
        let symbol = h.get(&opts, "symbol")?;
        let symbol = h.format_out(symbol)?;
        let from = h.milliseconds(&start)?;
        let from = h.num((from / 1000.0).floor());
        let to = h.milliseconds(&end)?;
        let to = h.num((to / 1000.0).ceil());
        let reply = h
            .read(
                call,
                endpoint("KLINE"),
                vec![
                    ("symbol", symbol),
                    ("resolution", resolution),
                    ("from", from),
                    ("to", to),
                ],
            )?
            .await?;
        let data = h.get(&reply, "data")?;
        let data = h.get(&data, "data")?;
        let rows = h.get(&data, "rows")?;
        let mut candles = Vec::new();
        for row in h.rows(rows)? {
            let time = h.at(&row, 0)?;
            let mut fields = vec![("timestamp", time)];
            for (field, index) in [
                ("open", 3),
                ("high", 4),
                ("low", 5),
                ("close", 6),
                ("volume", 7),
            ] {
                let value = h.at(&row, index)?;
                let n = h.parse(value)?;
                fields.push((field, h.num(n)));
            }
            candles.push(h.object(fields)?);
        }
        h.array(candles)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let message = error_message(h, &e, true, false)?;
            h.emit_candle_error(message)?;
            h.array(vec![])
        }
    }
}

#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn utf16_encoder_preserves_native_units() {
        assert_eq!(units(""), Vec::<u16>::new());
        assert_eq!(units("A🚀\0"), vec![65, 55357, 56960, 0]);
        assert_eq!(units("é雪"), vec![233, 38634]);
    }

    #[test]
    fn endpoint_lookup_preserves_selected_wire_paths() {
        assert_eq!(endpoint("MARKETS"), "/public/products");
        assert_eq!(endpoint("TICKERS"), "/md/v3/ticker/24hr/all");
        assert_eq!(endpoint("KLINE"), "/exchange/public/md/v2/kline/list");
        assert!(std::panic::catch_unwind(|| endpoint("not-an-endpoint")).is_err());
    }
}

#[cfg(test)]
mod accepted_outbound_contract {
    #[allow(dead_code, unused_imports)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/accepted_host_fixture.rs"
        ));
    }
    #[test]
    fn label_encodes_supplied_text_through_host() {
        use safe_cex_exchange::js_runtime::value::Js;
        let mut host = fixture::Host::new(&serde_json::Value::Null);
        for (text, expected) in [
            ("A🚀\0", vec![65, 0xd83d, 0xde80, 0]),
            ("é雪", vec![233, 38634]),
            ("", vec![]),
        ] {
            let value = super::label(&mut host, text);
            let Js::String(units) = value else {
                panic!("label must return native text")
            };
            assert_eq!(
                units.as_ref(),
                expected.as_slice(),
                "label native UTF16 units"
            );
        }
    }
}
