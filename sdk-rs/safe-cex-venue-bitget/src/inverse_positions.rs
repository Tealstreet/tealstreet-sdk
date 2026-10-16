//! Coin-by-coin inverse position read. Native values preserve row aliases and UTF-16.
use crate::account_positions::AccountPositionsHost;
pub trait InversePositionsHost: AccountPositionsHost {
    fn capture_replace(&self, receiver: Self::Value) -> Result<Self::Value, Self::Error>;
    fn invoke_replace(
        &self,
        method: Self::Value,
        needle: Self::Value,
        replacement: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Invoke synchronous source collaborators without adopting returned promises.
    fn sync_call(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
}
fn s<H: AccountPositionsHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn p<H: AccountPositionsHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn opt<H: AccountPositionsHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        p(h, v, k)
    }
}
fn or<H: AccountPositionsHost>(
    h: &H,
    a: H::Value,
    b: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&a) { Ok(a) } else { b() }
}
// ts: BitgetInverseExchange.fetchOpenPositions
pub async fn fetch_open_positions<H: InversePositionsHost>(h: &H) -> Result<H::Value, H::Error> {
    let markets = h.receiver("store.markets")?;
    let mut coins = vec![];
    // Array.map skips holes; array spread into Set visits holes as undefined.
    for i in 0..h.length(&markets)? {
        let coin = if h.has_index(&markets, i)? {
            p(h, &h.index(&markets, i)?, "base")?
        } else {
            h.undefined()
        };
        if !coins.iter().any(|old| h.same_value_zero(old, &coin)) {
            coins.push(coin)
        }
    }
    let mut all = vec![];
    for coin in coins {
        let result = async {
            let response = h
                .read(
                    h.receiver("xhr")?,
                    "POSITIONS_V2",
                    vec![
                        ("productType", h.receiver("apiProductType")?),
                        ("marginCoin", coin),
                    ],
                )?
                .await?;
            let code = or(h, p(h, &response, "code")?, || Ok(s(h, "0")))?;
            if h.parse_int(&code, None)? != 0. {
                h.emit(p(h, &response, "msg")?, None)?;
                return Ok(Vec::new());
            }
            let data = p(h, &response, "data")?;
            let mut mapped = vec![];
            for i in 0..h.length(&data)? {
                if !h.has_index(&data, i)? {
                    mapped.push(h.undefined());
                    continue;
                }
                let raw = h.index(&data, i)?;
                let native = h.parse_float(&p(h, &raw, "total")?)?;
                let mut contracts = native;
                let entry = h.parse_float(&p(h, &raw, "openPriceAvg")?)?;
                let replace = h.capture_replace(p(h, &raw, "symbol")?)?;
                let mut suffix = vec![b'_' as u16];
                suffix.extend(h.text(&h.upper(&h.receiver("apiProductType")?)?)?);
                let symbol = h.invoke_replace(replace, h.string(&suffix), s(h, ""))?;
                let mode = s(
                    h,
                    if h.equal(&p(h, &raw, "posMode")?, &s(h, "hedge_mode")) {
                        "hedged"
                    } else {
                        "oneway"
                    },
                );
                let margin = s(
                    h,
                    if h.equal(&p(h, &raw, "marginMode")?, &s(h, "isolated")) {
                        "isolated"
                    } else {
                        "cross"
                    },
                );
                let side =
                    h.keyed_property(&h.receiver("POSITION_SIDE")?, &p(h, &raw, "holdSide")?)?;
                let id = h.position_id(&symbol, &side, &mode)?;
                let upnl = h.parse_float(&p(h, &raw, "unrealizedPL")?)?;
                let market = h.keyed_property(&h.receiver("store.marketsBySymbol")?, &symbol)?;
                let mut notional =
                    safe_cex_num::multiply(native, h.parse_float(&p(h, &raw, "markPrice")?)?);
                if h.truthy(&opt(h, &market, "isInverse")?) {
                    let ticker =
                        h.keyed_property(&h.receiver("store.tickersBySymbol")?, &symbol)?;
                    let reference = or(h, opt(h, &ticker, "mark")?, || {
                        or(h, opt(h, &ticker, "last")?, || Ok(h.number(entry)))
                    })?;
                    if h.truthy(&reference) && h.numeric(&reference)? > 0. {
                        contracts =
                            safe_cex_num::math_round(native / h.numeric(&reference)? * 1e8) / 1e8;
                        if native > 0. {
                            let mut text = "BitgetInverse position: "
                                .encode_utf16()
                                .collect::<Vec<_>>();
                            text.extend(h.text(&symbol)?);
                            for (label, value) in [
                                (" - USD contracts: ", h.number(native)),
                                (", price: ", reference),
                                (", crypto amount: ", h.number(contracts)),
                            ] {
                                text.extend(label.encode_utf16());
                                text.extend(h.text(&value)?);
                            }
                            h.sync_call("log", vec![h.string(&text)])?;
                        }
                    }
                    notional = native;
                }
                let size = p(h, &raw, "marginSize")?;
                let size = if h.nullish(&size) {
                    f64::NAN
                } else {
                    h.parse_float(&p(h, &raw, "marginSize")?)?
                };
                let profit = |k| -> Result<f64, H::Error> {
                    let v = p(h, &raw, k)?;
                    if h.truthy(&v) {
                        h.parse_float(&p(h, &raw, k)?)
                    } else {
                        Ok(0.)
                    }
                };
                let leverage = h.parse_float(&p(h, &raw, "leverage")?)?;
                let realized =
                    profit("achievedProfits")? + profit("totalFee")? - profit("deductedFee")?;
                let liquidation = h.sync_call(
                    "resolveLiquidationPrice",
                    vec![id.clone(), p(h, &raw, "liquidationPrice")?],
                )?;
                let position = h.object(vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("side", side),
                    ("entryPrice", h.number(entry)),
                    ("notional", h.number(notional)),
                    ("leverage", h.number(leverage)),
                    ("unrealizedPnl", h.number(upnl)),
                    ("contracts", h.number(contracts)),
                    ("realizedPnl", h.number(realized)),
                    ("liquidationPrice", liquidation),
                    (
                        "marginSize",
                        if size > 0. {
                            h.number(size)
                        } else {
                            h.undefined()
                        },
                    ),
                    ("positionMode", mode),
                    ("marginType", margin),
                    (
                        "extra",
                        h.object(vec![
                            ("contractsNative", h.number(native)),
                            ("unrealizedPnlNative", h.number(upnl)),
                        ]),
                    ),
                    ("info", raw),
                    (
                        "lastUpdate",
                        h.sync_call("virtualClock.getCurrentTimeMs", vec![])?,
                    ),
                ]);
                mapped.push(position);
            }
            Ok(mapped)
        }
        .await;
        match result {
            Ok(rows) => all.extend(rows),
            Err(e) => {
                let e = h.caught_value(&e);
                let msg = or(
                    h,
                    opt(h, &opt(h, &opt(h, &e, "response")?, "data")?, "msg")?,
                    || opt(h, &e, "message"),
                )?;
                h.emit(msg, None)?;
            }
        }
    }
    Ok(h.array(all))
}
