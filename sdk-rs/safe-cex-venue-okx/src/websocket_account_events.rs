use crate::market_account_reads::{MarketAccountOps, ReadMapper};
use std::rc::Rc;

pub struct SocketEventOps<V, E> {
    pub value: Rc<MarketAccountOps<V, E>>,
    pub callback: Rc<dyn Fn(ReadMapper<V, E>) -> V>,
}
pub struct PrivateSocketState<V> {
    pub recent_fill_keys: V,
    pub direct_fallback: V,
    pub fill_dedupe_window_ms: V,
}
pub struct PublicSocketState<V> {
    pub order_book_sequences: V,
    pub recovering_order_books: V,
    pub direct_fallback: V,
    pub topics: V,
    pub message_handlers: V,
}
pub struct BusinessSocketState<V> {
    pub base: PrivateSocketState<V>,
    pub topics: V,
}
fn call<V, E>(
    h: &SocketEventOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.value.capture)(name)?;
    (h.value.invoke)(callee, args()?)
}
fn construct<V, E>(h: &SocketEventOps<V, E>, name: &str) -> Result<V, E> {
    let constructor = (h.value.capture)(name)?;
    (h.value.intrinsic)("construct", vec![constructor])
}
fn or<V, E>(h: &SocketEventOps<V, E>, value: V, fallback: impl FnOnce() -> V) -> V {
    if (h.value.truthy)(&value) {
        value
    } else {
        fallback()
    }
}
fn field_is<V, E>(h: &SocketEventOps<V, E>, row: &V, key: &str, text: &str) -> Result<bool, E> {
    Ok((h.value.equal)(
        &(h.value.get)(row, key)?,
        &(h.value.literal)(text),
    ))
}
fn define<V: Clone, E>(h: &SocketEventOps<V, E>, object: &V, key: &str, value: V) -> Result<(), E> {
    (h.value.intrinsic)(
        "define-literal-field",
        vec![object.clone(), (h.value.literal)(key), value],
    )?;
    Ok(())
}
impl<V> PrivateSocketState<V> {
    pub fn initialize<E>(h: &SocketEventOps<V, E>) -> Result<Self, E> {
        Ok(Self {
            recent_fill_keys: construct(h, "Map")?,
            direct_fallback: construct(h, "WsDirectFallback")?,
            fill_dedupe_window_ms: (h.value.number)(60_000.),
        })
    }
}
impl<V: Clone + 'static> PublicSocketState<V> {
    pub fn initialize<E: 'static>(h: Rc<SocketEventOps<V, E>>) -> Result<Self, E> {
        let order_book_sequences = construct(&h, "Map")?;
        let recovering_order_books = construct(&h, "Set")?;
        let direct_fallback = construct(&h, "WsDirectFallback")?;
        let topics = (h.value.object)(vec![]);
        let mut handlers = Vec::new();
        for (channel, method) in [
            ("tickers", "this.handleTickerEvents"),
            ("mark-price", "this.handleMarkPriceEvents"),
            ("index-tickers", "this.handleIndexTickerEvents"),
            ("open-interest", "this.handleOpenInterestEvents"),
            ("funding-rate", "this.handleFundingRateEvents"),
        ] {
            let ops = h.clone();
            let callback = (h.callback)(Rc::new(move |data| call(&ops, method, || Ok(vec![data]))));
            handlers.push((channel, (h.value.array)(vec![callback])));
        }
        Ok(Self {
            order_book_sequences,
            recovering_order_books,
            direct_fallback,
            topics,
            message_handlers: (h.value.object)(handlers),
        })
    }
}
impl<V> BusinessSocketState<V> {
    pub fn initialize<E>(h: &SocketEventOps<V, E>) -> Result<Self, E> {
        Ok(Self {
            base: PrivateSocketState::initialize(h)?,
            topics: (h.value.object)(vec![]),
        })
    }
}
pub fn handle_order_topic<V: Clone + 'static, E: 'static>(
    h: Rc<SocketEventOps<V, E>>,
    input: V,
) -> Result<V, E> {
    let rows = (h.value.get)(&input, "data")?;
    let ops = h.clone();
    (h.value.for_of)(
        rows,
        Rc::new(move |o| {
            let orders = call(&ops, "this.parent.mapOrders", || {
                Ok(vec![(ops.value.array)(vec![o.clone()])])
            })?;
            if (ops.value.truthy)(&(ops.value.get)(&orders, "length")?) {
                if field_is(&ops, &o, "state", "filled")?
                    || field_is(&ops, &o, "state", "canceled")?
                    || field_is(&ops, &o, "state", "mmp_canceled")?
                {
                    call(&ops, "this.store.markOrdersTerminal", || {
                        let status =
                            (ops.value.capture)(if field_is(&ops, &o, "state", "filled")? {
                                "OrderStatus.Closed"
                            } else {
                                "OrderStatus.Canceled"
                            })?;
                        Ok(vec![orders.clone(), status])
                    })?;
                }
                if field_is(&ops, &o, "state", "live")?
                    || field_is(&ops, &o, "state", "partially_filled")?
                {
                    call(&ops, "this.store.addOrUpdateOrders", || {
                        Ok(vec![orders.clone()])
                    })?;
                }
                if field_is(&ops, &o, "state", "filled")?
                    || field_is(&ops, &o, "state", "partially_filled")?
                {
                    let markets = (ops.value.capture)("this.store.marketsById")?;
                    let market = (ops.value.intrinsic)(
                        "get-index",
                        vec![markets, (ops.value.get)(&o, "instId")?],
                    )?;
                    let amount = call(&ops, "parseFloat", || {
                        Ok(vec![or(&ops, (ops.value.get)(&o, "fillSz")?, || {
                            (ops.value.literal)("0")
                        })])
                    })?;
                    let price = call(&ops, "parseFloat", || {
                        Ok(vec![or(&ops, (ops.value.get)(&o, "fillPx")?, || {
                            (ops.value.literal)("0")
                        })])
                    })?;
                    if (ops.value.truthy)(&market)
                        && (ops.value.truthy)(&(ops.value.intrinsic)(
                            "greater-than",
                            vec![amount.clone(), (ops.value.number)(0.)],
                        )?)
                        && !(ops.value.truthy)(&call(&ops, "this.isDuplicateFillEvent", || {
                            Ok(vec![o.clone()])
                        })?)
                    {
                        let emit = (ops.value.capture)("this.emitter.emit")?;
                        let id = (ops.value.get)(&(ops.value.get)(&orders, "0")?, "id")?;
                        let side = (ops.value.get)(&(ops.value.get)(&orders, "0")?, "side")?;
                        let symbol = (ops.value.get)(&(ops.value.get)(&orders, "0")?, "symbol")?;
                        let base = call(&ops, "this.parent.contractsToBase", || {
                            Ok(vec![amount, market, price.clone()])
                        })?;
                        let fill = (ops.value.object)(vec![
                            ("id", id),
                            ("side", side),
                            ("symbol", symbol),
                            ("price", price),
                            ("amount", base),
                        ]);
                        (ops.value.invoke)(emit, vec![(ops.value.literal)("fill"), fill])?;
                    }
                }
            }
            Ok(())
        }),
    )?;
    Ok((h.value.undefined)())
}
pub fn is_duplicate_fill_event<V: Clone + 'static, E: 'static>(
    h: Rc<SocketEventOps<V, E>>,
    o: V,
) -> Result<V, E> {
    let now = call(&h, "Date.now", || Ok(vec![]))?;
    let cutoff = (h.value.intrinsic)(
        "subtract",
        vec![now.clone(), (h.value.capture)("this.fillDedupeWindowMs")?],
    )?;
    let keys = (h.value.capture)("this.recentFillKeys")?;
    let ops = h.clone();
    (h.value.for_of)(
        keys,
        Rc::new(move |entry| {
            let key = (ops.value.get)(&entry, "0")?;
            let seen = (ops.value.get)(&entry, "1")?;
            if (ops.value.truthy)(&(ops.value.intrinsic)(
                "less-than",
                vec![seen, cutoff.clone()],
            )?) {
                call(&ops, "this.recentFillKeys.delete", || Ok(vec![key]))?;
            }
            Ok(())
        }),
    )?;
    let trade = call(&h, "String", || {
        Ok(vec![or(&h, (h.value.get)(&o, "tradeId")?, || {
            (h.value.literal)("")
        })])
    })?;
    let time = call(&h, "String", || {
        let fill = (h.value.get)(&o, "fillTime")?;
        let time = if (h.value.truthy)(&fill) {
            fill
        } else {
            (h.value.get)(&o, "uTime")?
        };
        Ok(vec![or(&h, time, || (h.value.literal)(""))])
    })?;
    let price = call(&h, "String", || {
        Ok(vec![or(&h, (h.value.get)(&o, "fillPx")?, || {
            (h.value.literal)("")
        })])
    })?;
    let size = call(&h, "String", || {
        Ok(vec![or(&h, (h.value.get)(&o, "fillSz")?, || {
            (h.value.literal)("")
        })])
    })?;
    let mut key = (h.value.intrinsic)("template-string", vec![(h.value.get)(&o, "instId")?])?;
    key = (h.value.intrinsic)("colon-append", vec![key, (h.value.get)(&o, "ordId")?])?;
    let parts = if (h.value.truthy)(&trade) && !(h.value.equal)(&trade, &(h.value.literal)("0")) {
        vec![trade]
    } else {
        vec![time, price, size]
    };
    for part in parts {
        key = (h.value.intrinsic)("colon-append", vec![key, part])?;
    }
    let found = call(&h, "this.recentFillKeys.has", || Ok(vec![key.clone()]))?;
    if (h.value.truthy)(&found) {
        return Ok((h.value.boolean)(true));
    }
    call(&h, "this.recentFillKeys.set", || Ok(vec![key, now]))?;
    Ok((h.value.boolean)(false))
}
pub fn handle_position_topic<V: Clone + 'static, E: 'static>(
    h: Rc<SocketEventOps<V, E>>,
    input: V,
) -> Result<V, E> {
    let rows = (h.value.get)(&input, "data")?;
    let positions = call(&h, "this.parent.mapPositions", || Ok(vec![rows.clone()]))?;
    if (h.value.truthy)(&(h.value.get)(&positions, "length")?) {
        let round = (h.value.capture)("roundUSD")?;
        let sum = (h.value.capture)("this.sumFinite")?;
        let ops = h.clone();
        let margins = (h.value.map)(
            rows,
            Rc::new(move |p| {
                let markets = (ops.value.capture)("this.store.marketsById")?;
                let market = (ops.value.intrinsic)(
                    "get-index",
                    vec![markets, (ops.value.get)(&p, "instId")?],
                )?;
                call(&ops, "this.parent.convertMarginValueToUsd", || {
                    let mmr = call(&ops, "this.parseFinite", || {
                        Ok(vec![(ops.value.get)(&p, "mmr")?])
                    })?;
                    let ccy = (ops.value.get)(&p, "ccy")?;
                    let mark = call(&ops, "this.parseFinite", || {
                        Ok(vec![(ops.value.get)(&p, "markPx")?])
                    })?;
                    Ok(vec![mmr, ccy, market, mark])
                })
            }),
        )?;
        let used = (h.value.invoke)(round, vec![(h.value.invoke)(sum, vec![margins])?])?;
        let round = (h.value.capture)("roundUSD")?;
        let sum = (h.value.capture)("this.sumFinite")?;
        let ops = h.clone();
        let pnls = (h.value.map)(
            positions.clone(),
            Rc::new(move |p| (ops.value.get)(&p, "unrealizedPnl")),
        )?;
        let upnl = (h.value.invoke)(round, vec![(h.value.invoke)(sum, vec![pnls])?])?;
        call(&h, "this.store.addOrUpdatePositions", || {
            Ok(vec![positions])
        })?;
        let update = (h.value.capture)("this.store.update")?;
        let old = (h.value.capture)("this.store.balance")?;
        let balance = (h.value.intrinsic)("object-spread", vec![old])?;
        define(&h, &balance, "used", used)?;
        define(&h, &balance, "upnl", upnl)?;
        (h.value.invoke)(update, vec![(h.value.object)(vec![("balance", balance)])])?;
    }
    Ok((h.value.undefined)())
}
pub fn handle_account_topic<V: Clone + 'static, E: 'static>(
    h: Rc<SocketEventOps<V, E>>,
    input: V,
) -> Result<V, E> {
    let data = (h.value.get)(&input, "data")?;
    if !(h.value.truthy)(&call(&h, "Array.isArray", || Ok(vec![data.clone()]))?) {
        return Ok((h.value.undefined)());
    }
    let ops = h.clone();
    let values = (h.value.map)(
        data,
        Rc::new(move |row| {
            call(&ops, "parseFloat", || {
                Ok(vec![call(&ops, "String", || {
                    let value = (ops.value.get)(&row, "totalEq")?;
                    Ok(vec![if (ops.value.nullish)(&value) {
                        (ops.value.literal)("")
                    } else {
                        value
                    }])
                })?])
            })
        }),
    )?;
    let ops = h.clone();
    let values = (h.value.filter)(
        values,
        Rc::new(move |v| {
            Ok((ops.value.truthy)(&call(&ops, "Number.isFinite", || {
                Ok(vec![v])
            })?))
        }),
    )?;
    if !(h.value.truthy)(&(h.value.get)(&values, "length")?) {
        return Ok((h.value.undefined)());
    }
    let collateral = call(&h, "roundUSD", || {
        Ok(vec![call(&h, "sumBy", || Ok(vec![values]))?])
    })?;
    if !(h.value.truthy)(&call(&h, "Number.isFinite", || {
        Ok(vec![collateral.clone()])
    })?) {
        return Ok((h.value.undefined)());
    }
    let used = if (h.value.truthy)(&call(&h, "Number.isFinite", || {
        Ok(vec![(h.value.capture)("this.store.balance.used")?])
    })?) {
        (h.value.capture)("this.store.balance.used")?
    } else {
        (h.value.number)(0.)
    };
    let upnl = if (h.value.truthy)(&call(&h, "Number.isFinite", || {
        Ok(vec![(h.value.capture)("this.store.balance.upnl")?])
    })?) {
        (h.value.capture)("this.store.balance.upnl")?
    } else {
        (h.value.number)(0.)
    };
    let update = (h.value.capture)("this.store.update")?;
    let old = (h.value.capture)("this.store.balance")?;
    let balance = (h.value.intrinsic)("object-spread", vec![old])?;
    define(
        &h,
        &balance,
        "free",
        call(&h, "subtract", || Ok(vec![collateral.clone(), used]))?,
    )?;
    define(
        &h,
        &balance,
        "total",
        call(&h, "subtract", || Ok(vec![collateral, upnl]))?,
    )?;
    (h.value.invoke)(update, vec![(h.value.object)(vec![("balance", balance)])])?;
    Ok((h.value.undefined)())
}
pub fn on_markets_changed<V: Clone + 'static, E: 'static>(
    h: Rc<SocketEventOps<V, E>>,
    input: V,
) -> Result<V, E> {
    let added = (h.value.get)(&input, "added")?;
    if (h.value.equal)(&(h.value.get)(&added, "length")?, &(h.value.number)(0.))
        || !(h.value.truthy)(&(h.value.capture)("this.ws")?)
        || (h.value.truthy)(&(h.value.capture)("this.isDisposed")?)
    {
        return Ok((h.value.undefined)());
    }
    let mut groups = Vec::new();
    for channel in [
        "tickers",
        "mark-price",
        "index-tickers",
        "open-interest",
        "funding-rate",
    ] {
        let ops = h.clone();
        let topics = (h.value.map)(
            added.clone(),
            Rc::new(move |m| {
                let id = if channel == "index-tickers" {
                    call(&ops, "this.parent.getIndexTickerInstId", || Ok(vec![m]))?
                } else {
                    (ops.value.get)(&m, "id")?
                };
                Ok((ops.value.object)(vec![
                    ("channel", (ops.value.literal)(channel)),
                    ("instId", id),
                ]))
            }),
        )?;
        groups.push((channel, topics));
    }
    for (channel, topics) in &groups {
        let receiver = (h.value.capture)("this.topics")?;
        let existing = (h.value.get)(&(h.value.capture)("this.topics")?, channel)?;
        let existing = or(&h, existing, || (h.value.array)(vec![]));
        let combined = (h.value.intrinsic)("array-spread", vec![existing, topics.clone()])?;
        (h.value.set)(&receiver, channel, combined)?;
    }
    let all = (h.value.intrinsic)("array-spread", groups.into_iter().map(|(_, v)| v).collect())?;
    let mut offset = 0.;
    while (h.value.truthy)(&(h.value.intrinsic)(
        "less-than",
        vec![(h.value.number)(offset), (h.value.get)(&all, "length")?],
    )?) {
        let slice = (h.value.member)(all.clone(), "slice")?;
        let batch = (h.value.invoke)(
            slice,
            vec![(h.value.number)(offset), (h.value.number)(offset + 100.)],
        )?;
        let send = (h.value.capture)("this.send")?;
        let wire = call(&h, "JSON.stringify", || {
            Ok(vec![(h.value.object)(vec![
                ("op", (h.value.literal)("subscribe")),
                ("args", batch),
            ])])
        })?;
        (h.value.invoke)(send, vec![wire])?;
        offset += 100.;
    }
    Ok((h.value.undefined)())
}
