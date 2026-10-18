//! HyroTrader validated private REST reads over retained runtime values.
use crate::{
    hyro_private_session::SessionFuture, variant_account_refresh::AccountRefreshHost as Host,
};
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn endpoint<H: Host>(h: &H, key: &str) -> Result<H::Value, H::Error> {
    h.get(&h.intrinsic("ENDPOINTS", vec![])?, key)
}
fn fields<H: Host>(h: &H, keys: &[&str]) -> H::Value {
    h.array(keys.iter().map(|k| h.literal(k)).collect())
}
fn concat<H: Host>(h: &H, parts: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let mut units = vec![];
    for part in parts {
        units.extend(h.units(&part)?);
    }
    Ok(h.text(units))
}
fn fail<H: Host>(h: &H, owner: H::Value, message: &str) -> Result<H::Value, H::Error> {
    Ok(call!(h, owner, "fail", vec![h.literal(message)]))
}
fn finite<H: Host>(h: &H, value: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&h.intrinsic("Number.isFinite", vec![value])?))
}
fn numeric_field<H: Host>(
    h: &H,
    owner: H::Value,
    row: H::Value,
    key: H::Value,
) -> Result<bool, H::Error> {
    let selected = h.select(owner, "isFiniteNumeric")?;
    let value = get_field(h, row, key)?;
    Ok(h.truthy(&h.apply(selected, vec![value])?))
}
fn get_field<H: Host>(h: &H, row: H::Value, key: H::Value) -> Result<H::Value, H::Error> {
    h.intrinsic("computed property", vec![row, key])
}
fn finite_fields<H: Host>(
    h: &H,
    owner: H::Value,
    row: H::Value,
    keys: &[&str],
) -> Result<bool, H::Error> {
    let callback = h.callback(owner, "wallet-field", vec![row])?;
    Ok(h.truthy(&call!(h, fields(h, keys), "every", vec![callback])))
}
fn iterator<H: Host>(h: &H, value: H::Value) -> Result<(H::Value, H::Value), H::Error> {
    let iter = h.intrinsic("iterator", vec![value])?;
    let next = h.select(iter.clone(), "next")?;
    Ok((iter, next))
}
fn next<H: Host>(h: &H, selected: H::Value) -> Result<Option<H::Value>, H::Error> {
    let result = h.apply(selected, vec![])?;
    if h.truthy(&h.get(&result, "done")?) {
        Ok(None)
    } else {
        Ok(Some(h.get(&result, "value")?))
    }
}
fn spread_values<H: Host>(h: &H, value: H::Value) -> Result<Vec<H::Value>, H::Error> {
    let (_, selected) = iterator(h, value)?;
    let mut out = vec![];
    while let Some(value) = next(h, selected.clone())? {
        out.push(value);
    }
    Ok(out)
}
fn meta<H: Host>(h: &H, data: H::Value) -> Result<H::Value, H::Error> {
    Ok(call!(h, h.global(), "getBybitResponseMeta", vec![data]))
}
fn page_get<H: Host>(
    h: &H,
    owner: H::Value,
    endpoint: H::Value,
    params: H::Value,
    cursor: H::Value,
) -> Result<H::Value, H::Error> {
    let selected = h.select(h.get(&owner, "xhr")?, "get")?;
    let params = h.spread(params, vec![("limit", h.number(50.0)), ("cursor", cursor)])?;
    h.apply(selected, vec![endpoint, h.object(vec![("params", params)])])
}

/// Closed set of lexical callbacks created by this module and selected by the native host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    WalletHas,
    WalletField,
    WalletCoin,
    PositionNumeric,
    PositionRequired,
    Nonfinite,
    OrderNumber,
    OrdersInvalid,
    OrdersMap,
    PositionsMap,
}
impl Callback {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "wallet-has" => Some(Self::WalletHas),
            "wallet-field" => Some(Self::WalletField),
            "wallet-coin" => Some(Self::WalletCoin),
            "position-numeric" => Some(Self::PositionNumeric),
            "position-required" => Some(Self::PositionRequired),
            "nonfinite" => Some(Self::Nonfinite),
            "order-number" => Some(Self::OrderNumber),
            "orders-invalid" => Some(Self::OrdersInvalid),
            "orders-map" => Some(Self::OrdersMap),
            "positions-map" => Some(Self::PositionsMap),
            _ => None,
        }
    }
}

pub fn dispatch<H: Host>(
    h: &H,
    owner: &H::Value,
    code: Callback,
    captures: &[H::Value],
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let value = args.first().cloned().unwrap_or_else(|| h.undefined());
    let result = match code {
        Callback::WalletHas => {
            !h.equal(&value, &h.undefined())
                && !h.equal(&value, &h.intrinsic("null", vec![])?)
                && !h.equal(&value, &h.literal(""))
                && finite(h, h.intrinsic("Number", vec![value])?)?
        }
        Callback::WalletField => {
            let row = captures[0].clone();
            h.equal(&get_field(h, row.clone(), value.clone())?, &h.undefined())
                || h.equal(&get_field(h, row.clone(), value.clone())?, &h.literal(""))
                || numeric_field(h, owner.clone(), row, value)?
        }
        Callback::WalletCoin => {
            !h.truthy(&value)
                || !h.equal(
                    &h.intrinsic("typeof", vec![h.get(&value, "coin")?])?,
                    &h.literal("string"),
                )
                || !finite_fields(
                    h,
                    owner.clone(),
                    value,
                    &[
                        "equity",
                        "walletBalance",
                        "usdValue",
                        "totalPositionIM",
                        "totalOrderIM",
                        "locked",
                        "bonus",
                    ],
                )?
        }
        Callback::PositionNumeric => {
            let row = captures[0].clone();
            !h.equal(&get_field(h, row.clone(), value.clone())?, &h.undefined())
                && !h.equal(&get_field(h, row.clone(), value.clone())?, &h.literal(""))
                && !numeric_field(h, owner.clone(), row, value)?
        }
        Callback::PositionRequired => {
            let row = captures[0].clone();
            h.equal(&get_field(h, row.clone(), value.clone())?, &h.undefined())
                || h.equal(&get_field(h, row.clone(), value.clone())?, &h.literal(""))
                || !numeric_field(h, owner.clone(), row, value)?
        }
        Callback::Nonfinite => !finite(h, value)?,
        Callback::OrderNumber => !finite(h, value.clone())? || h.numeric(&value)? < 0.0,
        Callback::OrdersInvalid => {
            !h.truthy(&h.get(&value, "status")?)
                || !h.truthy(&h.get(&value, "type")?)
                || !h.truthy(&h.get(&value, "side")?)
                || {
                    let numbers = h.array(vec![
                        h.get(&value, "price")?,
                        h.get(&value, "amount")?,
                        h.get(&value, "remaining")?,
                        h.get(&value, "filled")?,
                    ]);
                    let callback = h.callback(owner.clone(), "order-number", vec![])?;
                    h.truthy(&call!(h, numbers, "some", vec![callback]))
                }
        }
        Callback::OrdersMap => return Ok(call!(h, owner.clone(), "mapOrder", vec![value])),
        Callback::PositionsMap => return Ok(call!(h, owner.clone(), "mapPosition", vec![value])),
    };
    Ok(h.boolean(result))
}

pub fn fetch_raw_balance<H: Host>(h: &H, owner: H::Value) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| {
        let selected = h.select(h.get(&owner, "xhr")?, "get")?;
        let ep = endpoint(h, "BALANCE")?;
        h.apply(
            selected,
            vec![
                ep,
                h.object(vec![(
                    "params",
                    h.object(vec![("accountType", h.literal("UNIFIED"))]),
                )]),
            ],
        )
    })();
    Box::pin(async move {
        let response = h.wait(request?).await?;
        let data = h.get(&response, "data")?;
        let status = meta(h, data.clone())?;
        if !h.truthy(&h.get(&status, "isSuccess")?) {
            let reason = h.get(&status, "retMsg")?;
            let reason = if h.truthy(&reason) {
                reason
            } else {
                h.literal("HyroTrader wallet request failed")
            };
            call!(h, owner.clone(), "fail", vec![reason]);
        }
        let account = h.optional(&h.optional(&h.optional(&data, "result")?, "list")?, "0")?;
        let values = h.array(vec![
            h.optional(&account, "totalEquity")?,
            h.optional(&account, "totalWalletBalance")?,
        ]);
        let callback = h.callback(owner.clone(), "wallet-has", vec![])?;
        let has_balance = call!(h, values, "some", vec![callback]);
        let list = h.optional(&h.optional(&data, "result")?, "list")?;
        if !h.truthy(&h.intrinsic("Array.isArray", vec![list])?)
            || !h.equal(&h.optional(&account, "accountType")?, &h.literal("UNIFIED"))
            || !h.truthy(&has_balance)
        {
            fail(
                h,
                owner.clone(),
                "HyroTrader wallet response is missing the UNIFIED account",
            )?;
        }
        let valid = finite_fields(
            h,
            owner.clone(),
            account.clone(),
            &[
                "totalEquity",
                "totalWalletBalance",
                "totalMarginBalance",
                "totalAvailableBalance",
                "totalInitialMargin",
                "totalMaintenanceMargin",
            ],
        )?;
        if !valid
            || (!h.equal(&h.get(&account, "coin")?, &h.undefined())
                && (!h.truthy(&h.intrinsic("Array.isArray", vec![h.get(&account, "coin")?])?) || {
                    let selected = h.select(h.get(&account, "coin")?, "some")?;
                    let callback = h.callback(owner.clone(), "wallet-coin", vec![])?;
                    h.truthy(&h.apply(selected, vec![callback])?)
                }))
        {
            fail(
                h,
                owner.clone(),
                "HyroTrader wallet response contains invalid monetary fields",
            )?;
        }
        h.wait(h.get(&h.get(&data, "result")?, "list")?).await
    })
}

pub fn assert_private_list_response<H: Host>(
    h: &H,
    owner: H::Value,
    data: H::Value,
    ep: H::Value,
) -> Result<H::Value, H::Error> {
    let status = meta(h, data.clone())?;
    if !h.truthy(&h.get(&status, "isSuccess")?) {
        let reason = h.get(&status, "retMsg")?;
        let reason = if h.truthy(&reason) {
            reason
        } else {
            concat(
                h,
                vec![
                    h.literal("HyroTrader "),
                    ep.clone(),
                    h.literal(" request failed"),
                ],
            )?
        };
        call!(h, owner.clone(), "fail", vec![reason]);
    }
    let list = h.optional(&h.optional(&data, "result")?, "list")?;
    if !h.truthy(&h.intrinsic("Array.isArray", vec![list])?) {
        let message = concat(
            h,
            vec![
                h.literal("HyroTrader "),
                ep.clone(),
                h.literal(" response has no list"),
            ],
        )?;
        call!(h, owner.clone(), "fail", vec![message]);
    }
    let (_, selected) = iterator(h, h.get(&h.get(&data, "result")?, "list")?)?;
    while let Some(row) = next(h, selected.clone())? {
        let valid = h.truthy(&row)
            && h.equal(
                &h.intrinsic("typeof", vec![h.get(&row, "symbol")?])?,
                &h.literal("string"),
            )
            && h.truthy(&call!(
                h,
                h.get(&row, "symbol")?,
                "endsWith",
                vec![h.literal("USDT")]
            ))
            && if h.equal(&ep, &endpoint(h, "POSITIONS")?) {
                !h.equal(&h.get(&row, "size")?, &h.undefined())
                    && !h.equal(&h.get(&row, "size")?, &h.literal(""))
                    && numeric_field(h, owner.clone(), row.clone(), h.literal("size"))?
                    && h.numeric(&h.get(&row, "size")?)? >= 0.0
                    && {
                        let idx = h.get(&row, "positionIdx")?;
                        let idx = if h.equal(&idx, &h.undefined())
                            || h.equal(&idx, &h.intrinsic("null", vec![])?)
                        {
                            h.number(0.0)
                        } else {
                            idx
                        };
                        h.numeric(&idx)? == 0.0
                    }
            } else {
                h.equal(
                    &h.intrinsic("typeof", vec![h.get(&row, "orderId")?])?,
                    &h.literal("string"),
                ) && h.numeric(&h.get(&h.get(&row, "orderId")?, "length")?)? > 0.0
            };
        if !valid {
            let message = concat(
                h,
                vec![
                    h.literal("HyroTrader "),
                    ep.clone(),
                    h.literal(" returned a malformed row"),
                ],
            )?;
            call!(h, owner.clone(), "fail", vec![message]);
        }
        if h.equal(&ep, &endpoint(h, "POSITIONS")?) {
            if !numeric_field(h, owner.clone(), row.clone(), h.literal("leverage"))?
                || h.numeric(&h.get(&row, "leverage")?)? <= 0.0
            {
                fail(
                    h,
                    owner.clone(),
                    "HyroTrader position response is missing valid leverage",
                )?;
            }
            let callback = h.callback(owner.clone(), "position-numeric", vec![row.clone()])?;
            if h.truthy(&call!(
                h,
                fields(
                    h,
                    &[
                        "avgPrice",
                        "markPrice",
                        "unrealisedPnl",
                        "leverage",
                        "positionValue",
                        "positionIM",
                        "curRealisedPnl",
                        "liqPrice"
                    ]
                ),
                "some",
                vec![callback]
            )) {
                fail(
                    h,
                    owner.clone(),
                    "HyroTrader position response contains an invalid numeric value",
                )?;
            }
        }
        if h.equal(&ep, &endpoint(h, "POSITIONS")?) && h.numeric(&h.get(&row, "size")?)? > 0.0 {
            let included = call!(
                h,
                h.array(vec![h.literal("Buy"), h.literal("Sell")]),
                "includes",
                vec![h.get(&row, "side")?]
            );
            if !h.truthy(&included) || {
                let callback = h.callback(owner.clone(), "position-required", vec![row.clone()])?;
                h.truthy(&call!(
                    h,
                    fields(h, &["avgPrice", "markPrice", "unrealisedPnl"]),
                    "some",
                    vec![callback]
                ))
            } {
                fail(
                    h,
                    owner.clone(),
                    "HyroTrader position response is missing side, price or unrealized PnL",
                )?;
            }
        }
    }
    Ok(h.undefined())
}

pub fn map_position<H: Host>(h: &H, owner: H::Value, row: H::Value) -> Result<H::Value, H::Error> {
    let selected = h.select(owner.clone(), "assertPrivateListResponse")?;
    let data = h.object(vec![
        ("retCode", h.number(0.0)),
        ("retMsg", h.literal("OK")),
        (
            "result",
            h.object(vec![("list", h.array(vec![row.clone()]))]),
        ),
    ]);
    let ep = endpoint(h, "POSITIONS")?;
    h.apply(selected, vec![data, ep])?;
    let mapped = h.intrinsic(
        "super.mapPosition",
        vec![
            owner.clone(),
            h.spread(row, vec![("positionIdx", h.number(0.0))])?,
        ],
    )?;
    let values = h.array(vec![
        h.get(&mapped, "entryPrice")?,
        h.get(&mapped, "contracts")?,
        h.get(&mapped, "notional")?,
        h.get(&mapped, "leverage")?,
        h.get(&mapped, "unrealizedPnl")?,
        h.get(&mapped, "realizedPnl")?,
        h.get(&mapped, "liquidationPrice")?,
    ]);
    let callback = h.callback(owner.clone(), "nonfinite", vec![])?;
    if h.truthy(&call!(h, values, "some", vec![callback]))
        || (!h.equal(&h.get(&mapped, "marginSize")?, &h.undefined())
            && !finite(h, h.get(&mapped, "marginSize")?)?)
    {
        fail(
            h,
            owner,
            "HyroTrader position mapping contains invalid numeric fields",
        )?;
    }
    Ok(mapped)
}

pub fn fetch_orders<H: Host>(h: &H, owner: H::Value) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| {
        Ok(call!(
            h,
            owner,
            "fetchOrdersHelper",
            vec![h.literal("linear"), h.literal("USDT")]
        ))
    })();
    Box::pin(async move { h.wait(request?).await })
}

pub fn fetch_private_rows<H: Host>(
    h: &H,
    owner: H::Value,
    ep: H::Value,
    params: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let initial = (|| {
        let rows = h.array(vec![]);
        let seen = h.intrinsic("new Set", vec![])?;
        let request = page_get(h, owner.clone(), ep.clone(), params.clone(), h.literal(""))?;
        Ok((rows, seen, request))
    })();
    Box::pin(async move {
        let (rows, seen, mut request) = initial?;
        for page in 0..100 {
            let response = h.wait(request).await?;
            let data = h.get(&response, "data")?;
            call!(
                h,
                owner.clone(),
                "assertPrivateListResponse",
                vec![data.clone(), ep.clone()]
            );
            let push = h.select(rows.clone(), "push")?;
            let list = h.get(&h.get(&data, "result")?, "list")?;
            h.apply(push, spread_values(h, list)?)?;
            let cursor = h.get(&h.get(&data, "result")?, "nextPageCursor")?;
            if h.equal(&cursor, &h.undefined()) || h.equal(&cursor, &h.literal("")) {
                return h.wait(rows).await;
            }
            if !h.equal(
                &h.intrinsic("typeof", vec![cursor.clone()])?,
                &h.literal("string"),
            ) || h.truthy(&call!(h, seen.clone(), "has", vec![cursor.clone()]))
            {
                fail(
                    h,
                    owner.clone(),
                    "HyroTrader private snapshot cursor did not advance",
                )?;
            }
            call!(h, seen.clone(), "add", vec![cursor.clone()]);
            if page != 99 {
                request = page_get(h, owner.clone(), ep.clone(), params.clone(), cursor)?;
            } else {
                break;
            }
        }
        let value = fail(h, owner, "HyroTrader private snapshot page limit exceeded")?;
        h.wait(value).await
    })
}

pub fn fetch_orders_helper<H: Host>(
    h: &H,
    owner: H::Value,
    category: H::Value,
    settle_coin: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| {
        let selected = h.select(owner.clone(), "fetchPrivateRows")?;
        let ep = endpoint(h, "UNFILLED_ORDERS")?;
        h.apply(
            selected,
            vec![
                ep,
                h.object(vec![("category", category), ("settleCoin", settle_coin)]),
            ],
        )
    })();
    Box::pin(async move {
        let rows = h.wait(request?).await?;
        let result = call!(h, owner, "mapValidatedOrders", vec![rows]);
        h.wait(result).await
    })
}

pub fn map_validated_orders<H: Host>(
    h: &H,
    owner: H::Value,
    rows: H::Value,
) -> Result<H::Value, H::Error> {
    let selected = h.select(owner.clone(), "assertPrivateListResponse")?;
    let data = h.object(vec![
        ("retCode", h.number(0.0)),
        ("retMsg", h.literal("OK")),
        ("result", h.object(vec![("list", rows.clone())])),
    ]);
    let ep = endpoint(h, "UNFILLED_ORDERS")?;
    h.apply(selected, vec![data, ep])?;
    let selected = h.select(rows, "flatMap")?;
    let callback = h.callback(owner.clone(), "orders-map", vec![])?;
    let orders = h.apply(selected, vec![callback])?;
    let selected = h.select(orders.clone(), "some")?;
    let callback = h.callback(owner.clone(), "orders-invalid", vec![])?;
    if h.truthy(&h.apply(selected, vec![callback])?) {
        fail(
            h,
            owner,
            "HyroTrader order response contains incomplete or invalid order fields",
        )?;
    }
    Ok(orders)
}

pub fn fetch_positions_helper<H: Host>(
    h: &H,
    owner: H::Value,
    symbols: H::Value,
    category: H::Value,
    settle_coin: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let category = if h.equal(&category, &h.undefined()) {
        h.literal("linear")
    } else {
        category
    };
    let settle_coin = if h.equal(&settle_coin, &h.undefined()) {
        h.literal("USDT")
    } else {
        settle_coin
    };
    let initial = (|| {
        let rows = h.array(vec![]);
        let values = if h.truthy(&h.optional(&symbols, "length")?) {
            symbols
        } else {
            h.array(vec![h.undefined()])
        };
        let (_, iter) = iterator(h, values)?;
        let symbol = next(h, iter.clone())?;
        if let Some(symbol) = symbol {
            let push = h.select(rows.clone(), "push")?;
            let selected = h.select(owner.clone(), "fetchPrivateRows")?;
            let ep = endpoint(h, "POSITIONS")?;
            let args = if h.truthy(&symbol) {
                h.object(vec![("category", category.clone()), ("symbol", symbol)])
            } else {
                h.object(vec![
                    ("category", category.clone()),
                    ("settleCoin", settle_coin.clone()),
                ])
            };
            let request = h.apply(selected, vec![ep, args])?;
            Ok((rows, iter, Some((push, request))))
        } else {
            Ok((rows, iter, None))
        }
    })();
    Box::pin(async move {
        let (rows, iter, mut pending) = initial?;
        while let Some((push, request)) = pending {
            let found = h.wait(request).await?;
            h.apply(push, spread_values(h, found)?)?;
            pending = if let Some(symbol) = next(h, iter.clone())? {
                let push = h.select(rows.clone(), "push")?;
                let selected = h.select(owner.clone(), "fetchPrivateRows")?;
                let ep = endpoint(h, "POSITIONS")?;
                let params = if h.truthy(&symbol) {
                    h.object(vec![("category", category.clone()), ("symbol", symbol)])
                } else {
                    h.object(vec![
                        ("category", category.clone()),
                        ("settleCoin", settle_coin.clone()),
                    ])
                };
                Some((push, h.apply(selected, vec![ep, params])?))
            } else {
                None
            };
        }
        let dedupe = h.select(owner.clone(), "dedupePositionsById")?;
        let map = h.select(rows, "map")?;
        let callback = h.callback(owner, "positions-map", vec![])?;
        let positions = h.apply(map, vec![callback])?;
        let result = h.apply(dedupe, vec![positions])?;
        h.wait(result).await
    })
}
