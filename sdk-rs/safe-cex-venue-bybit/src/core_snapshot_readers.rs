//! Primary private-snapshot bodies over eager, independently owned host operations.
use std::{future::Future, pin::Pin, rc::Rc};
pub type Operation<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
/// Standard value/method/Promise primitives, with retained receiver and handle identity.
/// `launch` polls once immediately, owns the task thereafter, and returns its Promise handle. Returned Promise values are adopted at resolution.
/// `wait` implements await; intrinsic operations are named maintained dependencies/builtins.
pub trait SnapshotHost: 'static {
    type Value: Clone + 'static;
    type Error: Clone + 'static;
    fn get(&self, owner: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn optional(&self, owner: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn set(&self, owner: &Self::Value, key: &str, value: Self::Value) -> Result<(), Self::Error>;
    fn select(&self, owner: Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn apply(
        &self,
        selected: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn undefined(&self) -> Self::Value;
    fn literal(&self, s: &str) -> Self::Value;
    fn boolean(&self, b: bool) -> Self::Value;
    fn number(&self, n: f64) -> Self::Value;
    fn numeric(&self, v: &Self::Value) -> Result<f64, Self::Error>;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn values(&self, array: &Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn is_array(&self, v: &Self::Value) -> bool;
    fn spread(
        &self,
        v: Self::Value,
        fields: Vec<(&'static str, Self::Value)>,
    ) -> Result<Self::Value, Self::Error>;
    fn intrinsic(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn callback(
        &self,
        owner: Self::Value,
        code: &'static str,
        captures: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn launch(&self, body: Operation<Self::Value, Self::Error>) -> Self::Value;
    fn wait(&self, value: Self::Value) -> Operation<Self::Value, Self::Error>;
    fn error(&self, message: Self::Value) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: Self::Error) -> Self::Value;
}
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn update_loaded<H: SnapshotHost>(
    h: &H,
    owner: &H::Value,
    key: &'static str,
    value: Option<H::Value>,
) -> Result<(), H::Error> {
    let update = h.select(h.get(owner, "store")?, "update")?;
    let mut fields = vec![];
    if let Some(v) = value {
        fields.push((key, v))
    }
    let loaded = h.spread(
        h.get(&h.get(owner, "store")?, "loaded")?,
        vec![(key, h.boolean(true))],
    )?;
    fields.push(("loaded", loaded));
    h.apply(update, vec![h.object(fields)])?;
    Ok(())
}
fn merge_positions<H: SnapshotHost>(
    h: &H,
    owner: &H::Value,
    fetched: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.equal(
        &h.get(owner, "lastWebsocketPositionsUpdate")?,
        &h.undefined(),
    ) && h.numeric(&h.intrinsic("virtualClock.getCurrentTimeMs", vec![])?)?
        - h.numeric(&h.get(owner, "lastWebsocketPositionsUpdate")?)?
        < 1000.
    {
        h.intrinsic(
            "mergeUpdatesUsingLastUpdate",
            vec![fetched, h.get(&h.get(owner, "store")?, "positions")?],
        )
    } else {
        Ok(fetched)
    }
}
/// Bodies for lexical then/mapSeries/loop callbacks; constructor binding remains separate.
pub fn dispatch<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    code: &str,
    captures: Vec<H::Value>,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let launcher = h.clone();
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match code {
        "orders_then" => {
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.array(vec![]));
            }
            update_loaded(h.as_ref(), &owner, "orders", Some(first.clone()))?;
            Ok(first)
        }
        "positions_then" => {
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.array(vec![]));
            }
            let positions = merge_positions(h.as_ref(), &owner, first)?;
            call!(
                h,
                h.get(&owner, "store")?,
                "addOrUpdatePositionsFull",
                vec![positions.clone()]
            );
            update_loaded(h.as_ref(), &owner, "positions", None)?;
            Ok(positions)
        }
        "order_settle" => Ok(launcher.launch(Box::pin(async move {
            Ok(call!(
                h,
                owner,
                "fetchOrdersHelper",
                vec![h.literal("linear"), first]
            ))
        }))),
        "position_settle" => Ok(launcher.launch(Box::pin(async move {
            Ok(call!(
                h,
                owner,
                "fetchPositionsHelper",
                vec![h.undefined(), h.literal("linear"), first]
            ))
        }))),
        "position_symbol" => Ok(launcher.launch(Box::pin(async move {
            let get = h.select(owner.clone(), "getCategoryForSymbol")?;
            let category = h.apply(get, vec![first.clone()])?;
            let rows = fetch_position_pages(
                h.clone(),
                owner.clone(),
                h.object(vec![("symbol", first), ("category", category)]),
                true,
                h.literal(""),
                h.array(vec![]),
            );
            let rows = h.wait(rows).await?;
            map_positions(h.as_ref(), &owner, rows)
        }))),
        "option_orders_fetch" => Ok(call!(
            h,
            owner,
            "fetchOrdersHelper",
            vec![h.literal("option")]
        )),
        "inverse_positions_fetch" | "option_positions_fetch" => {
            let category = if code == "inverse_positions_fetch" {
                "inverse"
            } else {
                "option"
            };
            Ok(call!(
                h,
                owner,
                "fetchPositionsHelper",
                vec![
                    h.undefined(),
                    h.literal(category),
                    h.undefined(),
                    h.boolean(true)
                ]
            ))
        }
        "keep_category" => {
            let get = h.select(owner, "getCategoryForSymbol")?;
            let category = h.apply(get, vec![h.get(&first, "symbol")?])?;
            Ok(h.boolean(h.equal(&category, &captures[0])))
        }
        "ignore" => Ok(h.undefined()),
        "tick" => {
            let _ = captures;
            Ok(call!(h, owner, "tick", vec![]))
        }
        _ => panic!("unknown snapshot lexical callback {code}"),
    }
}
pub fn fetch_orders_positions_and_balance<H: SnapshotHost>(h: Rc<H>, owner: H::Value) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let result = async {
            let orders = call!(h, owner.clone(), "fetchOrders", vec![]);
            let orders = call!(
                h,
                orders,
                "then",
                vec![h.callback(owner.clone(), "orders_then", vec![])?]
            );
            let positions = call!(h, owner.clone(), "fetchPositions", vec![]);
            let positions = call!(
                h,
                positions,
                "then",
                vec![h.callback(owner.clone(), "positions_then", vec![])?]
            );
            let balance = call!(h, owner.clone(), "fetchRawBalance", vec![]);
            let all = h.intrinsic(
                "Promise.all",
                vec![h.array(vec![orders, positions, balance])],
            )?;
            let values = h.wait(all).await?;
            let raw = h.get(&values, "2")?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
            let balance = call!(h, owner.clone(), "formatBalance", vec![raw]);
            update_loaded(h.as_ref(), &owner, "balance", Some(balance))?;
            Ok(h.undefined())
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                call!(h, owner, "emitAndLogError", vec![h.error_value(e)]);
                Ok(h.undefined())
            }
        }
    }))
}
pub fn fetch_and_set_private_snapshot<H: SnapshotHost>(h: Rc<H>, owner: H::Value) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let p = call!(h, owner, "fetchAndSetPrivateSnapshotHelper", vec![]);
        h.wait(p).await?;
        Ok(h.undefined())
    }))
}
pub fn fetch_and_set_private_snapshot_helper<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    fetch_orders: bool,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let result = async {
            let orders = if fetch_orders {
                call!(h, owner.clone(), "fetchOrders", vec![])
            } else {
                h.undefined()
            };
            let positions = call!(h, owner.clone(), "fetchPositions", vec![]);
            let balance = call!(h, owner.clone(), "fetchBalance", vec![]);
            if !h.equal(&orders, &h.undefined()) && !h.equal(&orders, &h.intrinsic("null", vec![])?)
            {
                call!(
                    h,
                    orders.clone(),
                    "catch",
                    vec![h.callback(owner.clone(), "ignore", vec![])?]
                );
            }
            call!(
                h,
                positions.clone(),
                "catch",
                vec![h.callback(owner.clone(), "ignore", vec![])?]
            );
            call!(
                h,
                balance.clone(),
                "catch",
                vec![h.callback(owner.clone(), "ignore", vec![])?]
            );
            if h.truthy(&orders) {
                let rows = h.wait(orders).await?;
                if h.truthy(&h.get(&owner, "isDisposed")?) {
                    return Ok(h.undefined());
                }
                update_loaded(h.as_ref(), &owner, "orders", Some(rows))?;
            }
            let fetched = h.wait(positions).await?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
            let positions = merge_positions(h.as_ref(), &owner, fetched)?;
            call!(
                h,
                h.get(&owner, "store")?,
                "addOrUpdatePositionsFull",
                vec![positions]
            );
            update_loaded(h.as_ref(), &owner, "positions", None)?;
            let balance = h.wait(balance).await?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
            update_loaded(h.as_ref(), &owner, "balance", Some(balance))?;
            Ok(h.undefined())
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                call!(h, owner, "emitAndLogError", vec![h.error_value(e)]);
                Ok(h.undefined())
            }
        }
    }))
}
pub fn tick<H: SnapshotHost>(h: Rc<H>, owner: H::Value) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        if !h.truthy(&h.get(&owner, "isDisposed")?) {
            let signer = call!(h, owner.clone(), "waitForRemoteSigner", vec![]);
            if !h.truthy(&h.wait(signer).await?) {
                return Ok(h.undefined());
            }
            if h.truthy(&h.get(&owner, "needsReinit")?)
                && h.numeric(&h.intrinsic("Date.now", vec![])?)?
                    - h.numeric(&h.get(&owner, "lastReinitAt")?)?
                    >= h.numeric(&h.get(&owner, "reinitRetryMs")?)?
            {
                h.set(&owner, "needsReinit", h.boolean(false))?;
                h.set(&owner, "lastReinitAt", h.intrinsic("Date.now", vec![])?)?;
                let p = call!(h, owner.clone(), "fetchOrdersPositionsAndBalance", vec![]);
                h.wait(p).await?;
                let retry = if h.truthy(&h.get(&owner, "needsReinit")?) {
                    h.numeric(&h.intrinsic(
                        "Math.min",
                        vec![
                            h.number(h.numeric(&h.get(&owner, "reinitRetryMs")?)? * 2.),
                            h.number(300000.),
                        ],
                    )?)?
                } else {
                    15000.
                };
                h.set(&owner, "reinitRetryMs", h.number(retry))?;
            }
            let result = async {
                let p = call!(
                    h,
                    owner.clone(),
                    "fetchAndSetPrivateSnapshotHelper",
                    vec![h.boolean(false)]
                );
                h.wait(p).await
            }
            .await;
            if let Err(e) = result {
                call!(
                    h,
                    owner.clone(),
                    "emitAndLogError",
                    vec![h.optional(&h.error_value(e), "message")?]
                );
            }
            if !h.truthy(&h.get(&owner, "isDisposed")?) {
                h.intrinsic(
                    "loop",
                    vec![
                        h.callback(owner.clone(), "tick", vec![])?,
                        call!(h, owner, "getTickInterval", vec![]),
                    ],
                )?;
            }
        }
        Ok(h.undefined())
    }))
}
pub fn sync_balance_upnl_from_positions<H: SnapshotHost>(
    h: &H,
    owner: &H::Value,
) -> Result<(), H::Error> {
    let balance = h.intrinsic(
        "syncUnifiedBalanceUpnlFromPositions",
        vec![
            h.get(&h.get(owner, "store")?, "balance")?,
            h.get(&h.get(owner, "store")?, "positions")?,
            h.get(owner, "accountType")?,
        ],
    )?;
    if !h.truthy(&balance) || h.equal(&balance, &h.get(&h.get(owner, "store")?, "balance")?) {
        return Ok(());
    }
    call!(
        h,
        h.get(owner, "store")?,
        "update",
        vec![h.object(vec![("balance", balance)])]
    );
    Ok(())
}
fn key<H: SnapshotHost>(h: &H, v: H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.intrinsic("v", vec![v, h.literal(k)])
}
pub fn fetch_raw_balance<H: SnapshotHost>(h: Rc<H>, owner: H::Value) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let get = h.select(h.get(&owner, "xhr")?, "get")?;
        let account = if h.truthy(&h.get(&owner, "forceUnifiedBalance")?) {
            h.literal("UNIFIED")
        } else {
            h.get(&owner, "accountType")?
        };
        let p = h.apply(
            get,
            vec![
                h.literal(crate::types::ENDPOINTS["BALANCE"].as_str().unwrap()),
                h.object(vec![("params", h.object(vec![("accountType", account)]))]),
            ],
        )?;
        let response = h.wait(p).await?;
        let data = h.get(&response, "data")?;
        if !h.equal(&key(h.as_ref(), data.clone(), "retMsg")?, &h.literal("OK"))
            && h.numeric(&key(h.as_ref(), data.clone(), "retCode")?)? != 0.
        {
            if h.equal(
                &key(h.as_ref(), data.clone(), "retMsg")?,
                &h.literal("accountType only support UNIFIED."),
            ) && !h.truthy(&h.get(&owner, "forceUnifiedBalance")?)
            {
                h.set(&owner, "forceUnifiedBalance", h.boolean(true))?;
                h.set(&owner, "unifiedMarginStatus", h.number(6.))?;
                return Ok(call!(h, owner, "fetchRawBalance", vec![]));
            }
            call!(
                h,
                owner.clone(),
                "emitAndLogError",
                vec![key(h.as_ref(), data, "retMsg")?]
            );
            let balance = h.get(&h.get(&owner, "store")?, "balance")?;
            let info = h.optional(&balance, "info")?;
            return Ok(if h.truthy(&info) {
                info
            } else {
                h.array(vec![])
            });
        }
        h.get(&h.get(&data, "result")?, "list")
    }))
}
pub fn fetch_orders<H: SnapshotHost>(h: Rc<H>, owner: H::Value) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let coins = call!(
            h,
            owner.clone(),
            "getSettleCoinsForCategory",
            vec![h.literal("linear")]
        );
        let p = h.intrinsic(
            "mapSeries",
            vec![coins, h.callback(owner.clone(), "order_settle", vec![])?],
        )?;
        let orders = h.wait(p).await?;
        if h.truthy(&call!(h, owner.clone(), "inverseEnabled", vec![])) {
            let push = h.select(orders.clone(), "push")?;
            let p = call!(
                h,
                owner.clone(),
                "fetchOrdersHelper",
                vec![h.literal("inverse")]
            );
            h.apply(push, vec![h.wait(p).await?])?;
        }
        if h.truthy(&call!(h, owner.clone(), "optionsEnabled", vec![])) {
            let push = h.select(orders.clone(), "push")?;
            let fetch = h.callback(owner.clone(), "option_orders_fetch", vec![])?;
            let p =
                fetch_category_rows_or_keep(h.clone(), owner.clone(), "option", "orders", fetch);
            h.apply(push, vec![h.wait(p).await?])?;
        }
        let preserve = h.select(owner, "preserveRecentlyPlacedOrders")?;
        let flat = call!(h, orders, "flat", vec![]);
        h.apply(preserve, vec![flat])
    }))
}
/// A failed read of an opted-in category keeps that category's stored rows, so one bad
/// poll neither wipes them nor moves the balance by their PnL.
pub fn fetch_category_rows_or_keep<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    category: &'static str,
    what: &'static str,
    fetch: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let attempt = async {
            let p = h.apply(fetch, vec![])?;
            h.wait(p).await
        };
        let error = match attempt.await {
            Ok(rows) => return Ok(rows),
            Err(error) => h.error_value(error),
        };
        let message = h.optional(&error, "message")?;
        let message = if h.equal(&message, &h.undefined())
            || h.equal(&message, &h.intrinsic("null", vec![])?)
        {
            error
        } else {
            message
        };
        let text = h.intrinsic(
            "template",
            vec![
                h.literal(&format!("Failed to refresh Bybit {category} {what}: ")),
                message,
            ],
        )?;
        let severity = h.intrinsic("LogSeverity.Warning", vec![])?;
        call!(h, owner.clone(), "log", vec![text, severity]);
        let filter = h.select(h.get(&h.get(&owner, "store")?, what)?, "filter")?;
        let keep = h.callback(owner, "keep_category", vec![h.literal(category)])?;
        h.apply(filter, vec![keep])
    }))
}
fn fetch_order_pages<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    category: H::Value,
    settle: H::Value,
    cursor: H::Value,
    mut orders: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let reinit = h.select(owner.clone(), "reinitOnThrow")?;
        let get = h.select(h.get(&owner, "xhr")?, "get")?;
        let p = h.apply(
            get,
            vec![
                h.literal(crate::types::ENDPOINTS["UNFILLED_ORDERS"].as_str().unwrap()),
                h.object(vec![(
                    "params",
                    h.object(vec![
                        ("category", category.clone()),
                        ("settleCoin", settle.clone()),
                        ("limit", h.number(50.)),
                        ("cursor", cursor),
                    ]),
                )]),
            ],
        )?;
        let p = h.apply(reinit, vec![p])?;
        let response = h.wait(p).await?;
        let data = h.get(&response, "data")?;
        if !h.equal(&key(h.as_ref(), data.clone(), "retMsg")?, &h.literal("OK")) {
            call!(
                h,
                owner.clone(),
                "emitAndLogError",
                vec![key(h.as_ref(), data.clone(), "retMsg")?]
            );
            h.set(&owner, "needsReinit", h.boolean(true))?;
            return Err(h.error(key(h.as_ref(), data, "retMsg")?)?);
        }
        let result = h.optional(&data, "result")?;
        let list = h.optional(&result, "list")?;
        let list = if h.is_array(&list) {
            h.get(&h.get(&data, "result")?, "list")?
        } else {
            h.array(vec![])
        };
        if h.numeric(&h.get(&list, "length")?)? == 0. {
            return Ok(orders);
        }
        let next = h.get(&h.get(&data, "result")?, "nextPageCursor")?;
        let recursive_cursor = if h.truthy(&next) {
            Some(h.get(&h.get(&data, "result")?, "nextPageCursor")?)
        } else {
            None
        };
        let mut all = h.values(&orders)?;
        all.extend(h.values(&list)?);
        orders = h.array(all);
        if let Some(next) = recursive_cursor {
            Ok(fetch_order_pages(h, owner, category, settle, next, orders))
        } else {
            Ok(orders)
        }
    }))
}
pub fn fetch_orders_helper<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    category: H::Value,
    settle: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let request = fetch_order_pages(
            h.clone(),
            owner.clone(),
            category,
            settle,
            h.literal(""),
            h.array(vec![]),
        );
        let orders = h.wait(request).await?;
        let flat_map = h.select(orders, "flatMap")?;
        let map = h.get(&owner, "mapOrder")?;
        let bound = call!(h, map, "bind", vec![owner]);
        h.apply(flat_map, vec![bound])
    }))
}
pub fn fetch_positions<H: SnapshotHost>(h: Rc<H>, owner: H::Value, symbols: H::Value) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        if h.truthy(&symbols) && h.numeric(&h.optional(&symbols, "length")?)? > 0. {
            let p = call!(h, owner, "fetchPositionsHelper", vec![symbols]);
            return h.wait(p).await;
        }
        let coins = call!(
            h,
            owner.clone(),
            "getSettleCoinsForCategory",
            vec![h.literal("linear")]
        );
        let p = h.intrinsic(
            "mapSeries",
            vec![coins, h.callback(owner.clone(), "position_settle", vec![])?],
        )?;
        let positions = h.wait(p).await?;
        for (enabled, category, code) in [
            ("inverseEnabled", "inverse", "inverse_positions_fetch"),
            ("optionsEnabled", "option", "option_positions_fetch"),
        ] {
            if h.truthy(&call!(h, owner.clone(), enabled, vec![])) {
                let push = h.select(positions.clone(), "push")?;
                let fetch = h.callback(owner.clone(), code, vec![])?;
                let p = fetch_category_rows_or_keep(
                    h.clone(),
                    owner.clone(),
                    category,
                    "positions",
                    fetch,
                );
                h.apply(push, vec![h.wait(p).await?])?;
            }
        }
        Ok(call!(h, positions, "flat", vec![]))
    }))
}
fn fetch_position_pages<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    params: H::Value,
    fail_closed: bool,
    cursor: H::Value,
    accumulated: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let reinit = h.select(owner.clone(), "reinitOnThrow")?;
        let get = h.select(h.get(&owner, "xhr")?, "get")?;
        let options = h.spread(
            params.clone(),
            vec![("limit", h.number(200.)), ("cursor", cursor.clone())],
        )?;
        let request = h.apply(
            get,
            vec![
                h.literal(crate::types::ENDPOINTS["POSITIONS"].as_str().unwrap()),
                h.object(vec![("params", options)]),
            ],
        )?;
        let request = h.apply(reinit, vec![request])?;
        let response = h.wait(request).await?;
        let data = h.get(&response, "data")?;
        if !h.equal(&key(h.as_ref(), data.clone(), "retMsg")?, &h.literal("OK")) {
            call!(
                h,
                owner.clone(),
                "emitAndLogError",
                vec![key(h.as_ref(), data.clone(), "retMsg")?]
            );
            if fail_closed {
                return Err(h.error(key(h.as_ref(), data, "retMsg")?)?);
            }
            h.set(&owner, "needsReinit", h.boolean(true))?;
            return Ok(h.array(vec![]));
        }
        let result = h.optional(&data, "result")?;
        let list = h.optional(&result, "list")?;
        let page = if h.is_array(&list) {
            h.get(&h.get(&data, "result")?, "list")?
        } else {
            h.array(vec![])
        };
        let mut all = h.values(&accumulated)?;
        all.extend(h.values(&page)?);
        let accumulated = h.array(all);
        let next = h.optional(&h.optional(&data, "result")?, "nextPageCursor")?;
        if h.truthy(&next) && !h.equal(&next, &cursor) {
            Ok(fetch_position_pages(
                h,
                owner,
                params,
                fail_closed,
                next,
                accumulated,
            ))
        } else {
            Ok(accumulated)
        }
    }))
}
fn map_positions<H: SnapshotHost>(
    h: &H,
    owner: &H::Value,
    rows: H::Value,
) -> Result<H::Value, H::Error> {
    let dedupe = h.select(owner.clone(), "dedupePositionsById")?;
    let map = h.select(rows, "map")?;
    let mapper = h.get(owner, "mapPosition")?;
    let bound = call!(h, mapper, "bind", vec![owner.clone()]);
    let mapped = h.apply(map, vec![bound])?;
    h.apply(dedupe, vec![mapped])
}
pub fn fetch_positions_helper<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    symbols: H::Value,
    category: H::Value,
    settle: H::Value,
    fail_closed: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        if h.truthy(&symbols) && h.numeric(&h.get(&symbols, "length")?)? > 0. {
            let p = h.intrinsic(
                "mapSeries",
                vec![symbols, h.callback(owner, "position_symbol", vec![])?],
            )?;
            let sets = h.wait(p).await?;
            return Ok(call!(h, sets, "flat", vec![]));
        }
        let rows = fetch_position_pages(
            h.clone(),
            owner.clone(),
            h.object(vec![("category", category), ("settleCoin", settle)]),
            h.truthy(&fail_closed),
            h.literal(""),
            h.array(vec![]),
        );
        let rows = h.wait(rows).await?;
        map_positions(h.as_ref(), &owner, rows)
    }))
}
