//! Private account feed bodies over original values and current Store bindings.
use crate::{
    rest_account::{Handle, ReadHost, ReadResult},
    trading::{eq, field, obj, opt, text, units},
};

pub trait AccountHost: ReadHost {
    fn add(&self, left: &Self::Value, right: &Self::Value) -> ReadResult<Self, Handle<Self>>;
    fn to_string_method(&self, value: &Self::Value) -> ReadResult<Self, Handle<Self>>;
    fn date_milliseconds(&self, value: &Self::Value) -> ReadResult<Self, f64>;
    fn position_id(&self, symbol: &Self::Value, side: &str) -> ReadResult<Self, Handle<Self>>;
    fn optional_preference(
        &self,
        cache: &str,
        symbol: &Self::Value,
    ) -> ReadResult<Self, Handle<Self>>;
    fn optional_set_margin(
        &self,
        symbol: Handle<Self>,
        margin: Handle<Self>,
    ) -> ReadResult<Self, ()>;
    fn update_order(&self, selector: Handle<Self>, patch: Handle<Self>) -> ReadResult<Self, ()>;
    fn order_status(&self, selector: Handle<Self>, status: Handle<Self>) -> ReadResult<Self, ()>;
    fn add_order(&self, order: Handle<Self>) -> ReadResult<Self, ()>;
    fn full_orders(&self, orders: Handle<Self>) -> ReadResult<Self, ()>;
    fn full_positions(&self, positions: Handle<Self>) -> ReadResult<Self, ()>;
    fn fill(&self, payload: Handle<Self>) -> ReadResult<Self, ()>;
    fn throttle(&self) -> ReadResult<Self, ()>;
    fn parse_order(&self, item: Handle<Self>) -> ReadResult<Self, Handle<Self>>;
    fn null(&self) -> Handle<Self>;
}
fn or<H: ReadHost>(
    h: &H,
    v: &H::Value,
    keys: &[&str],
    fallback: Handle<H>,
    keyed: bool,
) -> ReadResult<H, Handle<H>> {
    for key in keys {
        let value = if keyed {
            h.keyed(v, key)?
        } else {
            field(h, v, key)?
        };
        if h.truthy(&value) {
            return Ok(value);
        }
    }
    Ok(fallback)
}
fn either<H: ReadHost>(
    h: &H,
    v: &H::Value,
    a: &str,
    b: &str,
    keyed: bool,
) -> ReadResult<H, Handle<H>> {
    let first = if keyed {
        h.keyed(v, a)?
    } else {
        field(h, v, a)?
    };
    if h.truthy(&first) {
        Ok(first)
    } else {
        if keyed { h.keyed(v, b) } else { field(h, v, b) }
    }
}
fn pf<H: ReadHost>(h: &H, v: &H::Value, keys: &[&str], keyed: bool) -> ReadResult<H, f64> {
    h.parse_float(&*or(h, v, keys, h.number(0.0), keyed)?)
}
fn max0(n: f64) -> f64 {
    if n.is_nan() {
        n
    } else if n <= 0.0 {
        0.0
    } else {
        n
    }
}
fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a.min(b)
    }
}
fn existing<H: ReadHost>(h: &H, id: &H::Value) -> ReadResult<H, Option<Handle<H>>> {
    for order in h.items(&*h.current("orders")?)? {
        if h.strict_eq(&*field(h, &order, "id")?, id) {
            return Ok(Some(order));
        }
    }
    Ok(None)
}
pub fn futures_fills<H: AccountHost>(h: &H, msg: Handle<H>) -> ReadResult<H, ()> {
    let a = opt(h, &msg, "fills")?;
    let fills = if h.truthy(&a) {
        a
    } else {
        let b = opt(h, &msg, "executions")?;
        if h.truthy(&b) {
            b
        } else if h.is_array(&msg) {
            msg
        } else {
            h.array(vec![])
        }
    };
    if !h.is_array(&fills) {
        return Ok(());
    }
    for f in h.items(&fills)? {
        let id = h.text(&h.string(&*or(
            h,
            &f,
            &["order_id", "orderId", "uid"],
            text(h, ""),
            true,
        )?)?);
        let qty = pf(h, &f, &["qty", "quantity", "size", "filled"], true)?;
        let price = pf(h, &f, &["price", "fillPrice"], true)?;
        let symbol = either(h, &f, "instrument", "symbol", true)?;
        let buy = field(h, &f, "buy")?;
        let side = if !h.undefined(&buy) {
            if h.truthy(&buy) { "buy" } else { "sell" }
        } else {
            let side =
                h.lower(&h.string(&*or(h, &f, &["side", "direction"], text(h, "buy"), true)?)?);
            if side == units("buy") { "buy" } else { "sell" }
        };
        if h.truthy(&id) {
            if let Some(o) = existing(h, &id)? {
                let amount = field(h, &o, "amount")?;
                let addition = h.add(&*field(h, &o, "filled")?, &h.number(qty))?;
                let filled = min(h.numeric(&amount)?, h.numeric(&addition)?);
                let remaining = max0(h.numeric(&*field(h, &o, "amount")?)? - filled);
                h.update_order(
                    obj(h, vec![("id", id.clone())])?,
                    obj(
                        h,
                        vec![
                            ("filled", h.number(filled)),
                            ("remaining", h.number(remaining)),
                        ],
                    )?,
                )?;
                if remaining == 0.0 {
                    h.order_status(obj(h, vec![("id", id.clone())])?, text(h, "closed"))?;
                }
            }
        }
        h.fill(obj(
            h,
            vec![
                ("id", id),
                ("amount", h.number(qty)),
                ("price", h.number(price)),
                ("side", text(h, side)),
                ("symbol", symbol),
            ],
        )?)?;
    }
    h.throttle()
}
pub fn futures_open_orders<H: AccountHost>(h: &H, msg: Handle<H>) -> ReadResult<H, ()> {
    if h.truthy(&*opt(h, &msg, "is_cancel")?)
        && h.truthy(&*opt(h, &msg, "order_id")?)
        && !h.truthy(&*opt(h, &msg, "order")?)
    {
        h.order_status(
            obj(h, vec![("id", field(h, &msg, "order_id")?)])?,
            text(h, "canceled"),
        )?;
        h.throttle()?;
        return Ok(());
    }
    let raw = opt(h, &msg, "orders")?;
    let (raw, snapshot) = if h.truthy(&raw) && h.is_array(&*field(h, &msg, "orders")?) {
        (h.items(&*field(h, &msg, "orders")?)?, true)
    } else if h.truthy(&*opt(h, &msg, "order")?) {
        (vec![field(h, &msg, "order")?], false)
    } else {
        return Ok(());
    };
    let mut orders = vec![];
    for o in raw {
        let typ = h.lower(&h.string(&*or(h, &o, &["type"], text(h, "limit"), false)?)?);
        let tif = if typ == units("post") {
            "PostOnly"
        } else if typ == units("ioc") {
            "ImmediateOrCancel"
        } else if typ == units("fok") {
            "FillOrKill"
        } else {
            "GoodTillCancel"
        };
        let kind = if typ == units("stop") || typ == units("stp") {
            "stop_market"
        } else if typ == units("take_profit") {
            "take_profit_market"
        } else if typ == units("trailing_stop") {
            "trailing_stop_market"
        } else if typ.windows(5).any(|x| x == units("limit")) || typ == units("lmt") {
            "limit"
        } else {
            "market"
        };
        let direction = field(h, &o, "direction")?;
        let side = if h.strict_eq(&direction, &h.number(0.0))
            || eq(h, &*field(h, &o, "direction")?, "buy")
        {
            "buy"
        } else {
            "sell"
        };
        let amount = pf(h, &o, &["qty", "quantity", "size"], false)?;
        let filled = pf(h, &o, &["filled"], false)?;
        let remaining = max0(amount - filled);
        let price = pf(
            h,
            &o,
            if ["stop_market", "take_profit_market", "trailing_stop_market"].contains(&kind) {
                &["stop_price", "stopPrice"]
            } else {
                &["limit_price", "limitPrice", "price"]
            },
            false,
        )?;
        let id = h.text(&h.string(&*or(
            h,
            &o,
            &["order_id", "orderId", "uid"],
            text(h, ""),
            false,
        )?)?);
        let client = either(h, &o, "cli_ord_id", "cliOrdId", false)?;
        let symbol = either(h, &o, "instrument", "symbol", false)?;
        let reduce =
            h.truthy(&*field(h, &o, "reduce_only")?) || h.truthy(&*field(h, &o, "reduceOnly")?);
        let time = or(h, &o, &["last_update_time", "time"], h.null(), false)?;
        let time = if h.truthy(&time) {
            time
        } else {
            h.number(h.now()?)
        };
        orders.push(obj(
            h,
            vec![
                ("id", id),
                ("clientOrderId", client),
                ("status", text(h, "open")),
                ("symbol", symbol),
                ("type", text(h, kind)),
                ("side", text(h, side)),
                ("price", h.number(price)),
                ("amount", h.number(amount)),
                ("reduceOnly", h.boolean(reduce)),
                ("filled", h.number(filled)),
                ("lastUpdate", time),
                ("remaining", h.number(remaining)),
                ("timeInForce", text(h, tif)),
                ("info", o),
            ],
        )?);
    }
    if h.truthy(&*opt(h, &msg, "is_cancel")?) {
        for o in orders {
            h.order_status(
                obj(h, vec![("id", field(h, &o, "id")?)])?,
                text(h, "canceled"),
            )?;
        }
        h.throttle()
    } else if snapshot || eq(h, &*opt(h, &msg, "feed")?, "open_orders_snapshot") {
        h.full_orders(h.array(orders))
    } else {
        for o in orders {
            h.add_order(o)?;
        }
        h.throttle()
    }
}
pub fn futures_open_positions<H: AccountHost>(h: &H, msg: Handle<H>) -> ReadResult<H, ()> {
    let a = opt(h, &msg, "positions")?;
    let raw = if h.truthy(&a) {
        a
    } else {
        let b = opt(h, &msg, "openPositions")?;
        if h.truthy(&b) { b } else { h.array(vec![]) }
    };
    if !h.is_array(&raw) {
        return Ok(());
    }
    let mut active = vec![];
    for p in h.items(&raw)? {
        let symbol = either(h, &p, "instrument", "symbol", false)?;
        let balance = pf(h, &p, &["balance"], false)?;
        let side = if balance >= 0.0 { "long" } else { "short" };
        let contracts = balance.abs();
        let id = h.position_id(&symbol, side)?;
        let notional = contracts * pf(h, &p, &["mark_price"], false)?;
        let cached = h.optional_preference("margin", &symbol)?;
        let max = field(h, &p, "maxFixedLeverage")?;
        let max = if h.undefined(&max) {
            None
        } else {
            Some(h.parse_float(&*field(h, &p, "maxFixedLeverage")?)?)
        };
        let detected = text(h, if max.is_some() { "isolated" } else { "cross" });
        let margin = if h.nullish(&cached) {
            detected.clone()
        } else {
            cached.clone()
        };
        if !h.truthy(&cached) {
            h.optional_set_margin(symbol.clone(), detected)?;
        }
        // eq is an observable host read and the source's `&&` evaluates it
        // first, so the Option is narrowed only after it has run.
        let fixed = if eq(h, &margin, "isolated") {
            max
        } else {
            None
        };
        let leverage = if let Some(max) = fixed {
            h.number(max)
        } else {
            let cached = h.optional_preference("leverage", &symbol)?;
            if !h.undefined(&cached) {
                cached
            } else {
                let market =
                    h.field(&*h.current("marketsBySymbol")?, &h.string(&symbol)?, false)?;
                let max = if h.nullish(&market) {
                    h.null()
                } else {
                    field(
                        h,
                        &*field(h, &*field(h, &market, "limits")?, "leverage")?,
                        "max",
                    )?
                };
                if h.nullish(&max) { h.number(5.0) } else { max }
            }
        };
        let position = obj(
            h,
            vec![
                ("id", id),
                ("symbol", symbol),
                ("side", text(h, side)),
                (
                    "entryPrice",
                    h.number(pf(h, &p, &["entry_price", "price"], false)?),
                ),
                ("notional", h.number(notional)),
                ("leverage", leverage),
                (
                    "unrealizedPnl",
                    h.number(pf(h, &p, &["pnl", "unrealized_funding"], false)?),
                ),
                ("contracts", h.number(contracts)),
                (
                    "liquidationPrice",
                    h.number(pf(h, &p, &["liquidation_threshold"], false)?),
                ),
                ("marginType", margin),
                ("positionMode", text(h, "oneway")),
                ("info", p),
            ],
        )?;
        if contracts > 0.0 {
            active.push(position);
        }
    }
    h.full_positions(h.array(active))
}
fn cash_currency<H: ReadHost>(h: &H, amount: f64) -> ReadResult<H, Handle<H>> {
    obj(
        h,
        vec![
            ("free", h.number(amount)),
            ("used", h.number(0.0)),
            ("total", h.number(amount)),
            ("upnl", h.number(0.0)),
            ("info", obj(h, vec![("balance", h.number(amount))])?),
        ],
    )
}
pub fn futures_balances<H: AccountHost>(h: &H, msg: Handle<H>) -> ReadResult<H, ()> {
    let a = opt(h, &msg, "accounts")?;
    let accounts = if h.truthy(&a) {
        a
    } else {
        let b = opt(h, &msg, "balances")?;
        if h.truthy(&b) { b } else { msg }
    };
    if !h.truthy(&accounts) || !h.is_object(&accounts) {
        return Ok(());
    }
    let (mut total, mut free, mut used, mut upnl) = (0.0, 0.0, 0.0, 0.0);
    let currencies = obj(h, vec![])?;
    let cash = either(h, &accounts, "cash", "cashAccount", false)?;
    let flex = either(h, &accounts, "flex", "multiCollateral", false)?;
    if h.truthy(&*opt(h, &cash, "balances")?) {
        for (currency, balance) in h.entries(&*field(h, &cash, "balances")?)? {
            let value = if h.truthy(&balance) {
                balance
            } else {
                h.number(0.0)
            };
            let amount = h.parse_float(&h.text(&h.string(&value)?))?;
            if amount > 0.0 {
                h.assign(&currencies, &h.upper(&currency), cash_currency(h, amount)?)?;
                let upper = h.upper(&currency);
                if ["USD", "USDT", "USDC"].iter().any(|s| upper == units(s)) {
                    total += amount;
                    free += amount;
                }
            }
        }
    }
    if h.truthy(&flex)
        && (eq(
            h,
            &*field(h, &flex, "type")?,
            "multiCollateralMarginAccount",
        ) || h.truthy(&*field(h, &flex, "portfolioValue")?))
    {
        let portfolio = pf(h, &flex, &["portfolioValue"], false)?;
        let available = pf(h, &flex, &["availableMargin"], false)?;
        let balance = pf(h, &flex, &["balanceValue"], false)?;
        let pnl = pf(h, &flex, &["pnl"], false)?;
        let initial = pf(h, &flex, &["initialMargin"], false)?;
        let maintenance = pf(h, &flex, &["maintenanceMargin"], false)?;
        if h.truthy(&*field(h, &flex, "currencies")?) {
            for (currency, details) in h.entries(&*field(h, &flex, "currencies")?)? {
                if h.is_object(&details) && !h.strict_eq(&details, &h.null()) {
                    let quantity = pf(h, &details, &["quantity"], false)?;
                    let value = pf(h, &details, &["value"], false)?;
                    let available = pf(h, &details, &["available"], false)?;
                    let collateral = pf(h, &details, &["collateral"], false)?;
                    if quantity > 0.0 || value > 0.0 {
                        h.assign(
                            &currencies,
                            &h.upper(&currency),
                            obj(
                                h,
                                vec![
                                    ("free", h.number(available)),
                                    ("used", h.number(quantity - available)),
                                    ("total", h.number(quantity)),
                                    ("upnl", h.number(0.0)),
                                    ("value", h.number(value)),
                                    ("collateral", h.number(collateral)),
                                    ("info", details),
                                ],
                            )?,
                        )?;
                    }
                }
            }
        }
        if portfolio > 0.0 {
            total = portfolio;
            free = max0(available);
            used = initial + maintenance;
            upnl = pnl;
        } else if balance > 0.0 {
            total = balance;
            free = available;
            used = initial + maintenance;
            upnl = pnl;
        }
    }
    if !h.truthy(&cash) && !h.truthy(&flex) {
        for (_, account) in h.entries(&accounts)? {
            if h.is_object(&account) && !h.strict_eq(&account, &h.null()) {
                let aux = field(h, &account, "auxiliary")?;
                if h.truthy(&aux) {
                    let pv = pf(h, &aux, &["pv", "portfolioValue"], false)?;
                    let af = pf(h, &aux, &["af", "availableMargin"], false)?;
                    let pnl = pf(h, &aux, &["pnl"], false)?;
                    if pv > 0.0 {
                        let currency = or(h, &aux, &["currency"], text(h, "USD"), false)?;
                        let key = h.string(&currency)?;
                        if !h.truthy(&*h.field(&currencies, &key, false)?) {
                            h.assign(
                                &currencies,
                                &key,
                                obj(
                                    h,
                                    vec![
                                        ("free", h.number(0.0)),
                                        ("used", h.number(0.0)),
                                        ("total", h.number(0.0)),
                                        ("upnl", h.number(0.0)),
                                        ("info", obj(h, vec![])?),
                                    ],
                                )?,
                            )?;
                        }
                        for (key_field, n) in [
                            ("total", pv),
                            ("free", af),
                            ("used", pv - af),
                            ("upnl", pnl),
                        ] {
                            let target = h.field(&currencies, &key, false)?;
                            let value = h.add(&*field(h, &target, key_field)?, &h.number(n))?;
                            h.assign(&target, &units(key_field), value)?;
                        }
                        if eq(h, &currency, "USD") {
                            total += pv;
                            free += af;
                            used += pv - af;
                            upnl += pnl;
                        }
                    }
                } else if h.truthy(&*field(h, &account, "balances")?) {
                    for (currency, balance) in h.entries(&*field(h, &account, "balances")?)? {
                        let value = if h.truthy(&balance) {
                            balance
                        } else {
                            h.number(0.0)
                        };
                        let amount = h.parse_float(&h.text(&h.string(&value)?))?;
                        if amount > 0.0 {
                            h.assign(&currencies, &h.upper(&currency), cash_currency(h, amount)?)?;
                        }
                    }
                }
            }
        }
    }
    h.update_balance(obj(
        h,
        vec![(
            "balance",
            obj(
                h,
                vec![
                    ("free", h.number(free)),
                    ("used", h.number(used)),
                    ("total", h.number(total)),
                    ("upnl", h.number(upnl)),
                    ("currencies", currencies),
                    ("info", accounts),
                ],
            )?,
        )],
    )?)
}
pub fn spot_parse_order<H: AccountHost>(h: &H, item: Handle<H>) -> ReadResult<H, Handle<H>> {
    let id = field(h, &item, "order_id")?;
    let symbol = field(h, &item, "symbol")?;
    if !h.truthy(&id) || !h.truthy(&symbol) {
        return Ok(h.null());
    }
    let mut found = false;
    for market in h.items(&*h.current("markets")?)? {
        if h.strict_eq(&*field(h, &market, "symbol")?, &symbol) {
            found = true;
            break;
        }
    }
    if !found {
        return Ok(h.null());
    }
    let kind = h.order_type(&*field(h, &item, "order_type")?)?;
    let kind = if h.truthy(&kind) {
        kind
    } else {
        text(h, "limit")
    };
    let side = if eq(h, &*field(h, &item, "side")?, "buy") {
        "buy"
    } else {
        "sell"
    };
    let qty = h.parse_float(&*or(h, &item, &["order_qty"], text(h, "0"), false)?)?;
    let cum = h.parse_float(&*or(h, &item, &["cum_qty"], text(h, "0"), false)?)?;
    let remaining = max0(qty - cum);
    let exec = field(h, &item, "exec_type")?;
    let status = if eq(h, &exec, "canceled") || eq(h, &exec, "expired") {
        "canceled"
    } else if eq(h, &exec, "filled") || remaining == 0.0 {
        "closed"
    } else {
        "open"
    };
    let mut tif = if eq(h, &*field(h, &item, "time_in_force")?, "IOC") {
        "ImmediateOrCancel"
    } else {
        "GoodTillCancel"
    };
    if h.strict_eq(&*field(h, &item, "post_only")?, &h.boolean(true)) {
        tif = "PostOnly";
    }
    let price = h.parse_float(&*or(
        h,
        &item,
        &["limit_price", "stop_price", "trigger_price"],
        text(h, "0"),
        false,
    )?)?;
    let stamp = field(h, &item, "timestamp")?;
    let time = if h.truthy(&stamp) {
        h.date_milliseconds(&*field(h, &item, "timestamp")?)?
    } else {
        h.now()?
    };
    obj(
        h,
        vec![
            ("id", id),
            ("status", text(h, status)),
            ("symbol", symbol),
            ("type", kind),
            ("side", text(h, side)),
            ("price", h.number(price)),
            ("amount", h.number(qty)),
            ("filled", h.number(cum)),
            ("remaining", h.number(remaining)),
            ("reduceOnly", h.boolean(false)),
            ("info", item),
            ("timeInForce", text(h, tif)),
            ("lastUpdate", h.number(time)),
        ],
    )
}
pub fn spot_executions<H: AccountHost>(h: &H, msg: Handle<H>) -> ReadResult<H, ()> {
    let items = field(h, &msg, "data")?;
    if !h.is_array(&items) {
        return Ok(());
    }
    if eq(h, &*field(h, &msg, "type")?, "snapshot") {
        let mut orders = vec![];
        for item in h.items(&items)? {
            let o = h.parse_order(item)?;
            if h.truthy(&o) {
                orders.push(o);
            }
        }
        return h.full_orders(h.array(orders));
    }
    for item in h.items(&items)? {
        let exec = field(h, &item, "exec_type")?;
        if eq(h, &exec, "canceled") || eq(h, &exec, "expired") {
            let id = field(h, &item, "order_id")?;
            if h.truthy(&id) {
                if let Some(order) = existing(h, &id)? {
                    h.add_order(h.spread(&order, vec![(units("status"), text(h, "canceled"))])?)?;
                }
            }
            h.throttle()?;
            continue;
        }
        if eq(h, &exec, "filled") || eq(h, &exec, "trade") {
            let id = field(h, &item, "order_id")?;
            if h.truthy(&id) {
                if let Some(order) = existing(h, &id)? {
                    let cum = h.parse_float(&*or(h, &item, &["cum_qty"], text(h, "0"), false)?)?;
                    let qty = field(h, &item, "order_qty")?;
                    let qty = if h.truthy(&qty) {
                        qty
                    } else {
                        h.to_string_method(&*field(h, &order, "amount")?)?
                    };
                    let qty = h.parse_float(&qty)?;
                    let remaining = max0(qty - cum);
                    h.add_order(h.spread(
                        &order,
                        vec![
                            (units("filled"), h.number(cum)),
                            (units("remaining"), h.number(remaining)),
                            (
                                units("status"),
                                text(h, if remaining == 0.0 { "closed" } else { "open" }),
                            ),
                        ],
                    )?)?;
                    let side = if eq(h, &*field(h, &item, "side")?, "buy") {
                        "buy"
                    } else {
                        "sell"
                    };
                    h.fill(obj(
                        h,
                        vec![
                            ("side", text(h, side)),
                            ("symbol", field(h, &item, "symbol")?),
                            (
                                "price",
                                h.number(h.parse_float(&*or(
                                    h,
                                    &item,
                                    &["avg_price", "limit_price"],
                                    text(h, "0"),
                                    false,
                                )?)?),
                            ),
                            (
                                "amount",
                                h.number(h.parse_float(&*or(
                                    h,
                                    &item,
                                    &["last_qty", "cum_qty"],
                                    text(h, "0"),
                                    false,
                                )?)?),
                            ),
                        ],
                    )?)?;
                }
            }
            h.throttle()?;
            continue;
        }
        let order = h.parse_order(item)?;
        if h.truthy(&order) {
            h.add_order(order)?;
        }
    }
    Ok(())
}

pub type PageFuture<'a, H> =
    std::pin::Pin<Box<dyn std::future::Future<Output = ReadResult<H, Handle<H>>> + 'a>>;
pub type PageCallback<'a, H> = Box<dyn Fn(Handle<H>) -> ReadResult<H, PageFuture<'a, H>> + 'a>;
pub trait IncomeHost: ReadHost {
    fn date_iso(&self, value: &Self::Value) -> ReadResult<Self, Vec<u16>>;
    fn paginate<'a>(
        &'a self,
        options: Handle<Self>,
        page: PageCallback<'static, Self>,
    ) -> ReadResult<Self, impl std::future::Future<Output = ReadResult<Self, Handle<Self>>> + 'a>;
    fn account_log_page(
        &self,
        params: Handle<Self>,
    ) -> ReadResult<Self, impl std::future::Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn income_range(
        &self,
        start: Handle<Self>,
        end: Handle<Self>,
        income_type: Handle<Self>,
    ) -> ReadResult<Self, impl std::future::Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn warning(&self, message: Handle<Self>) -> ReadResult<Self, ()>;
    fn income_error(&self, message: Handle<Self>) -> ReadResult<Self, ()>;
}
pub async fn income_range<H: IncomeHost + 'static>(
    h: std::rc::Rc<H>,
    start: Handle<H>,
    end: Handle<H>,
    income_type: Handle<H>,
) -> ReadResult<H, Handle<H>>
where
    H::Value: 'static,
{
    if h.truthy(&*h.current("publicDataOnly")?) {
        return Ok(h.array(vec![]));
    }
    let options = obj(
        h.as_ref(),
        vec![
            ("startTime", start.clone()),
            ("endTime", end.clone()),
            ("chunkMs", h.number(30.0 * 24.0 * 60.0 * 60.0 * 1000.0)),
            ("pageLimit", h.number(500.0)),
        ],
    )?;
    let page_host = h.clone();
    let page: PageCallback<'static, H> = Box::new(move |params| {
        let h = page_host.clone();
        let params = obj(
            h.as_ref(),
            vec![
                ("startTime", field(h.as_ref(), &params, "startTime")?),
                ("limit", field(h.as_ref(), &params, "limit")?),
            ],
        )?;
        Ok(Box::pin(async move { h.account_log_page(params)?.await }))
    });
    let result = h.paginate(options, page)?.await?;
    let records = field(h.as_ref(), &result, "records")?;
    let truncated = field(h.as_ref(), &result, "truncated")?;
    let exhausted = field(h.as_ref(), &result, "exhaustedChunks")?;
    if h.truthy(&truncated) {
        let msg=[units("Income history for "),h.date_iso(&start)?,units(".."),h.date_iso(&end)?,units(" is INCOMPLETE — "),h.string(&exhausted)?,units(" chunk(s) could not be walked to completion. Any total derived from it is a lower bound.")].concat();
        h.warning(h.text(&msg))?;
    }
    if h.truthy(&income_type) {
        let mut selected = vec![];
        for record in h.items(&records)? {
            if h.strict_eq(&*field(h.as_ref(), &record, "incomeType")?, &income_type) {
                selected.push(record);
            }
        }
        Ok(h.array(selected))
    } else {
        Ok(records)
    }
}
pub async fn income_history<H: IncomeHost>(h: &H, opts: Handle<H>) -> ReadResult<H, Handle<H>> {
    if h.truthy(&*h.current("publicDataOnly")?) {
        return Ok(h.array(vec![]));
    }
    let end = opt(h, &opts, "endTime")?;
    let end = if h.nullish(&end) {
        h.number(h.now()?)
    } else {
        end
    };
    let start = opt(h, &opts, "startTime")?;
    let start = if h.nullish(&start) {
        h.number(h.numeric(&end)? - 7.0 * 24.0 * 60.0 * 60.0 * 1000.0)
    } else {
        start
    };
    let result = async {
        let records = h
            .income_range(start, end, opt(h, &opts, "incomeType")?)?
            .await?;
        let symbol = opt(h, &opts, "symbol")?;
        let scoped = if h.truthy(&symbol) {
            let mut selected = vec![];
            for record in h.items(&records)? {
                if h.strict_eq(&*field(h, &record, "symbol")?, &*field(h, &opts, "symbol")?) {
                    selected.push(record);
                }
            }
            h.array(selected)
        } else {
            records
        };
        let limit = opt(h, &opts, "limit")?;
        if h.truthy(&limit) && h.numeric(&*field(h, &opts, "limit")?)? > 0.0 {
            h.slice_limit(h.items(&scoped)?, &*field(h, &opts, "limit")?)
        } else {
            Ok(scoped)
        }
    }
    .await;
    match result {
        Ok(records) => Ok(records),
        Err(error) => {
            h.income_error(h.error_message(&error, "Failed to fetch income history")?)?;
            Ok(h.array(vec![]))
        }
    }
}
