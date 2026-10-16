//! Account REST bodies over opaque native values and supplied receiver operations.
//! `Value::clone` retains JS reference identity. Array/property/object primitives
//! bind JS semantics (including holes, prototypes and UTF-16 strings); they do not
//! serialize values. Futures wait for host-owned promises. Concrete Exchange/HTTP
//! assembly and the separately mapped position/numeric callees remain separate.
use std::{future::Future, pin::Pin};
pub type AccountFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;

pub trait AccountPositionsHost {
    type Value: Clone;
    type Error;
    fn receiver(&self, name: &str) -> Result<Self::Value, Self::Error>;
    fn property(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn same_value_zero(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn text(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn upper(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn undefined(&self) -> Self::Value;
    fn string(&self, value: &[u16]) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn length(&self, array: &Self::Value) -> Result<usize, Self::Error>;
    fn has_index(&self, array: &Self::Value, index: usize) -> Result<bool, Self::Error>;
    fn index(&self, array: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn keyed_property(
        &self,
        object: &Self::Value,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn spread_metadata(
        &self,
        target: Self::Value,
        metadata: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn numeric(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    /// Object-literal field definition (CreateDataProperty), distinct from ordinary assignment.
    fn define_key(
        &self,
        object: &Self::Value,
        key: &Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn set_key(
        &self,
        object: &Self::Value,
        key: &Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn await_value(&self, value: Self::Value) -> AccountFuture<'_, Self::Value, Self::Error>;
    fn call(
        &self,
        name: &str,
        args: Vec<Self::Value>,
    ) -> Result<AccountFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn read(
        &self,
        client: Self::Value,
        endpoint: &'static str,
        params: Vec<(&'static str, Self::Value)>,
    ) -> Result<AccountFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn finite(&self, value: &Self::Value, fallback: f64) -> Result<f64, Self::Error>;
    fn parse_float(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn parse_int(&self, value: &Self::Value, radix: Option<i32>) -> Result<f64, Self::Error>;
    fn success(&self, code: &Self::Value) -> Result<bool, Self::Error>;
    fn promote(&self, response: &Self::Value) -> Result<bool, Self::Error>;
    fn account_upnl(&self, fallback: f64) -> Result<f64, Self::Error>;
    fn error_message(
        &self,
        response: &Self::Value,
        fallback: &str,
    ) -> Result<Self::Value, Self::Error>;
    fn environment_mismatch(&self, message: &Self::Value) -> Result<bool, Self::Error>;
    fn emit(&self, error: Self::Value, detail: Option<Self::Value>) -> Result<(), Self::Error>;
    fn make_error(&self, message: Self::Value) -> Self::Error;
    fn caught_value(&self, error: &Self::Error) -> Self::Value;
    fn caught_detail(&self, error: &Self::Error) -> Result<Self::Value, Self::Error>;
    /// Array.map intrinsic with the captured `this.map[Uta]Position` callback.
    fn map_positions(&self, array: Self::Value, uta: bool) -> Result<Self::Value, Self::Error>;
    /// Source createPositionId operation, preserving the native string domain.
    fn position_id(
        &self,
        symbol: &Self::Value,
        side: &Self::Value,
        mode: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
fn s<H: AccountPositionsHost>(h: &H, s: &str) -> H::Value {
    h.string(&s.encode_utf16().collect::<Vec<_>>())
}
fn prop<H: AccountPositionsHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn is<H: AccountPositionsHost>(h: &H, v: &H::Value, s_: &str) -> bool {
    h.equal(v, &s(h, s_))
}
fn or<H: AccountPositionsHost>(
    h: &H,
    a: H::Value,
    b: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&a) { Ok(a) } else { b() }
}
fn find<H: AccountPositionsHost>(
    h: &H,
    a: &H::Value,
    mut p: impl FnMut(&H::Value) -> Result<bool, H::Error>,
) -> Result<H::Value, H::Error> {
    let len = h.length(a)?;
    for i in 0..len {
        let v = h.index(a, i)?;
        if p(&v)? {
            return Ok(v);
        }
    }
    Ok(h.undefined())
}
fn filter<H: AccountPositionsHost>(
    h: &H,
    a: &H::Value,
    mut p: impl FnMut(&H::Value) -> Result<bool, H::Error>,
) -> Result<H::Value, H::Error> {
    let len = h.length(a)?;
    let mut out = vec![];
    for i in 0..len {
        if h.has_index(a, i)? {
            let v = h.index(a, i)?;
            if p(&v)? {
                out.push(v)
            }
        }
    }
    Ok(h.array(out))
}
fn some<H: AccountPositionsHost>(
    h: &H,
    a: &H::Value,
    mut p: impl FnMut(&H::Value) -> Result<bool, H::Error>,
) -> Result<bool, H::Error> {
    let len = h.length(a)?;
    for i in 0..len {
        if h.has_index(a, i)? && p(&h.index(a, i)?)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn balance<H: AccountPositionsHost>(
    h: &H,
    equity: f64,
    available: f64,
    upnl: f64,
    info: H::Value,
) -> H::Value {
    h.object(vec![
        ("used", h.number(safe_cex_num::subtract(equity, available))),
        ("free", h.number(available)),
        ("total", h.number(safe_cex_num::subtract(equity, upnl))),
        ("upnl", h.number(upnl)),
        ("info", info),
    ])
}
// ts: BitgetExchange.fetchBalance
pub async fn fetch_balance<H: AccountPositionsHost>(
    h: &H,
    prefer_fast: bool,
) -> Result<H::Value, H::Error> {
    h.call("ensureAccountTypeDetected", vec![])?.await?;
    let client = h.receiver(if prefer_fast { "unlimitedXHR" } else { "xhr" })?;
    if h.truthy(&h.receiver("usesUtaRouting")?) {
        return h.call("fetchUtaBalance", vec![client])?.await;
    }
    let r = h
        .read(
            client.clone(),
            "BALANCE",
            vec![("productType", h.receiver("apiProductType")?)],
        )?
        .await?;
    if !h.success(&prop(h, &r, "code")?)? {
        if h.promote(&r)? {
            return h.call("fetchUtaBalance", vec![client])?.await;
        }
        return Err(h.make_error(h.error_message(&r, "Bitget account balance request failed")?));
    }
    let raw = prop(h, &r, "data")?;
    let rows = if h.is_array(&raw) {
        prop(h, &r, "data")?
    } else {
        h.array(vec![])
    };
    let row = find(h, &rows, |v| {
        Ok(h.equal(&prop(h, v, "marginCoin")?, &h.receiver("apiMarginCoin")?))
    })?;
    if !h.truthy(&row) {
        return h.receiver("store.balance");
    }
    let equity = h.finite(
        &prop(h, &row, "unionTotalMargin")?,
        h.finite(&prop(h, &row, "accountEquity")?, 0.)?,
    )?;
    let available = h.finite(
        &prop(h, &row, "unionAvailable")?,
        h.finite(&prop(h, &row, "available")?, 0.)?,
    )?;
    let api = h.finite(
        &prop(h, &row, "unrealizedPL")?,
        h.finite(&prop(h, &row, "crossedUnrealizedPL")?, 0.)?
            + h.finite(&prop(h, &row, "isolatedUnrealizedPL")?, 0.)?,
    )?;
    Ok(balance(h, equity, available, h.account_upnl(api)?, row))
}
// ts: BitgetExchange#fetchUtaBalance
pub async fn fetch_uta_balance<H: AccountPositionsHost>(
    h: &H,
    client: H::Value,
) -> Result<H::Value, H::Error> {
    let r = h.read(client, "UTA_ACCOUNT_ASSETS", vec![])?.await?;
    if !h.success(&prop(h, &r, "code")?)? {
        return Err(h.make_error(h.error_message(&r, "Bitget UTA account assets request failed")?));
    }
    let data = prop(h, &r, "data")?;
    if !h.truthy(&data) {
        return h.receiver("store.balance");
    }
    let assets = prop(h, &data, "assets")?;
    let asset = if h.nullish(&assets) {
        h.undefined()
    } else {
        find(h, &assets, |v| {
            Ok(h.equal(&prop(h, v, "coin")?, &h.receiver("apiMarginCoin")?))
        })?
    };
    let optional = |k| {
        if h.nullish(&asset) {
            Ok(h.undefined())
        } else {
            prop(h, &asset, k)
        }
    };
    let equity = h.finite(
        &prop(h, &data, "accountEquity")?,
        h.finite(
            &prop(h, &data, "usdtEquity")?,
            h.finite(&optional("usdValue")?, h.finite(&optional("equity")?, 0.)?)?,
        )?,
    )?;
    let available = h.finite(
        &prop(h, &data, "effEquity")?,
        h.finite(&optional("available")?, 0.)?,
    )?;
    let api = h.finite(
        &prop(h, &data, "unrealisedPnl")?,
        h.finite(&prop(h, &data, "usdtUnrealisedPnl")?, 0.)?,
    )?;
    Ok(balance(h, equity, available, h.account_upnl(api)?, data))
}
fn metadata<H: AccountPositionsHost>(
    h: &H,
    symbol: H::Value,
    side: &str,
    leverage: f64,
    mode: &str,
    margin: &str,
) -> H::Value {
    h.object(vec![
        ("symbol", symbol),
        ("side", s(h, side)),
        ("leverage", h.number(leverage)),
        ("positionMode", s(h, mode)),
        ("marginType", s(h, margin)),
    ])
}
// ts: BitgetExchange.fetchPositionMetadata
pub async fn fetch_position_metadata<H: AccountPositionsHost>(
    h: &H,
    symbols: H::Value,
) -> Result<H::Value, H::Error> {
    h.call("ensureAccountTypeDetected", vec![])?.await?;
    if h.truthy(&h.receiver("usesUtaRouting")?) {
        return h.call("fetchUtaPositionMetadata", vec![symbols])?.await;
    }
    let result = async {
        let mut series: Vec<Option<Vec<H::Value>>> = vec![];
        let mut i = 0;
        // p-iteration mapSeries re-reads length after every awaited callback.
        while i < h.length(&symbols)? {
            let index = i;
            i += 1;
            if !h.has_index(&symbols, index)? {
                continue;
            }
            while series.len() <= index {
                series.push(None)
            }
            series[index] = Some(vec![]);
            let symbol = h.await_value(h.index(&symbols, index)?).await?;
            if h.truthy(&h.receiver("isDisposed")?) {
                continue;
            }
            let product = h.receiver("apiProductType")?;
            let coin = h.receiver("apiMarginCoin")?;
            let r = h
                .read(
                    h.receiver("xhr")?,
                    "FETCH_SINGLE_ACCOUNT",
                    vec![
                        ("symbol", symbol.clone()),
                        ("productType", product),
                        ("marginCoin", coin),
                    ],
                )?
                .await?;
            if h.truthy(&h.receiver("isDisposed")?) {
                continue;
            }
            let code = or(h, prop(h, &r, "code")?, || Ok(s(h, "0")))?;
            if h.parse_int(&code, None)? != 0. {
                h.emit(prop(h, &r, "msg")?, None)?;
                continue;
            }
            let data = prop(h, &r, "data")?;
            let mode = if is(h, &prop(h, &data, "posMode")?, "hedge_mode") {
                "hedged"
            } else {
                "oneway"
            };
            let margin = if is(h, &prop(h, &data, "marginMode")?, "crossed") {
                "cross"
            } else {
                "isolated"
            };
            let buy = h.parse_int(&prop(h, &data, "isolatedLongLever")?, None)?;
            let sell = h.parse_int(&prop(h, &data, "isolatedShortLever")?, None)?;
            let cross = h.parse_int(&prop(h, &data, "crossedMarginLeverage")?, None)?;
            if mode == "oneway" && margin == "isolated" && buy != sell {
                let mut msg: Vec<u16> = "Mismatched one-way isolated leverages for "
                    .encode_utf16()
                    .collect();
                msg.extend(h.text(&symbol)?);
                msg.extend(". Please set them to be equal.".encode_utf16());
                h.emit(h.string(&msg), None)?;
            }
            series[index]
                .as_mut()
                .expect("present callback")
                .push(metadata(
                    h,
                    symbol.clone(),
                    "long",
                    if margin == "cross" { cross } else { buy },
                    mode,
                    margin,
                ));
            if mode == "hedged" {
                series[index]
                    .as_mut()
                    .expect("present callback")
                    .push(metadata(
                        h,
                        symbol,
                        "short",
                        if margin == "cross" { cross } else { sell },
                        mode,
                        margin,
                    ))
            }
        }
        let mut out = vec![];
        for row in series {
            match row {
                Some(rows) => out.extend(rows),
                None => out.push(h.undefined()),
            }
        }
        Ok(h.array(out))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            h.emit(h.caught_value(&e), Some(h.caught_detail(&e)?))?;
            Ok(h.array(vec![]))
        }
    }
}
// ts: BitgetExchange#fetchUtaPositionMetadata
pub async fn fetch_uta_position_metadata<H: AccountPositionsHost>(
    h: &H,
    symbols: H::Value,
) -> Result<H::Value, H::Error> {
    let result = async {
        let r = h
            .read(h.receiver("xhr")?, "ACCOUNT_SETTINGS_V3", vec![])?
            .await?;
        if !h.success(&prop(h, &r, "code")?)? {
            h.emit(prop(h, &r, "msg")?, None)?;
            return Ok(h.array(vec![]));
        }
        let settings = prop(h, &r, "data")?;
        if !h.truthy(&settings) {
            return Ok(h.array(vec![]));
        }
        let mode = if is(h, &prop(h, &settings, "holdMode")?, "hedge_mode") {
            "hedged"
        } else {
            "oneway"
        };
        let raw = prop(h, &settings, "symbolConfigList")?;
        let rows = if h.nullish(&raw) {
            h.array(vec![])
        } else {
            raw
        };
        let mut out = vec![];
        let mut i = 0;
        while i < h.length(&symbols)? {
            let symbol = h.index(&symbols, i)?;
            i += 1;
            let markets = h.receiver("store.marketsBySymbol")?;
            // Computed native-key property access; set_key's counterpart is the supplied lookup.
            let market = h.keyed_property(&markets, &symbol)?;
            let id = if h.nullish(&market) {
                h.undefined()
            } else {
                prop(h, &market, "id")?
            };
            let venue = if h.nullish(&id) { symbol.clone() } else { id };
            let config = find(h, &rows, |row| {
                if !h.equal(&prop(h, row, "symbol")?, &venue) {
                    return Ok(false);
                }
                let category = prop(h, row, "category")?;
                if !h.truthy(&category) {
                    return Ok(true);
                }
                Ok(h.equal(
                    &h.upper(&prop(h, row, "category")?)?,
                    &h.upper(&h.receiver("apiUtaCategory")?)?,
                ))
            })?;
            let optional = |k| {
                if h.nullish(&config) {
                    Ok(h.undefined())
                } else {
                    prop(h, &config, k)
                }
            };
            let margin = if is(h, &optional("marginMode")?, "isolated") {
                "isolated"
            } else {
                "cross"
            };
            let long = h.finite(
                &optional("longLeverage")?,
                h.finite(&optional("leverage")?, 1.)?,
            )?;
            let short = h.finite(
                &optional("shortLeverage")?,
                h.finite(&optional("leverage")?, 1.)?,
            )?;
            out.push(metadata(h, symbol.clone(), "long", long, mode, margin));
            if mode == "hedged" {
                out.push(metadata(h, symbol, "short", short, mode, margin))
            }
        }
        Ok(h.array(out))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            h.emit(h.caught_value(&e), Some(h.caught_detail(&e)?))?;
            Ok(h.array(vec![]))
        }
    }
}
// ts: BitgetExchange.fetchPositions
pub async fn fetch_positions<H: AccountPositionsHost>(
    h: &H,
    symbols: Option<H::Value>,
) -> Result<H::Value, H::Error> {
    let open = h.call("fetchOpenPositions", vec![])?.await?;
    let Some(symbols) = symbols.filter(|v| h.truthy(v)) else {
        return Ok(open);
    };
    let selected = filter(h, &open, |p| {
        let symbol = prop(h, p, "symbol")?;
        {
            let mut found = false;
            let len = h.length(&symbols)?;
            for i in 0..len {
                if h.same_value_zero(&h.index(&symbols, i)?, &symbol) {
                    found = true;
                    break;
                }
            }
            Ok(found)
        }
    })?;
    let needed = filter(h, &symbols, |symbol| {
        let matches = filter(
            h,
            &selected,
            |p| Ok(h.equal(&prop(h, p, "symbol")?, symbol)),
        )?;
        if h.length(&matches)? == 0 {
            return Ok(true);
        }
        if !some(h, &matches, |p| {
            Ok(is(h, &prop(h, p, "positionMode")?, "hedged"))
        })? {
            return Ok(false);
        }
        let long = some(h, &matches, |p| Ok(is(h, &prop(h, p, "side")?, "long")))?;
        let short = some(h, &matches, |p| Ok(is(h, &prop(h, p, "side")?, "short")))?;
        Ok(!long || !short)
    })?;
    let metadatas = h.call("fetchPositionMetadata", vec![needed])?.await?;
    let missing = filter(h, &metadatas, |m| {
        Ok(!some(h, &selected, |p| {
            Ok(h.equal(&prop(h, p, "symbol")?, &prop(h, m, "symbol")?)
                && h.equal(&prop(h, p, "side")?, &prop(h, m, "side")?)
                && h.equal(&prop(h, p, "positionMode")?, &prop(h, m, "positionMode")?))
        })?)
    })?;
    let mut fake_positions = vec![];
    let len = h.length(&missing)?;
    for i in 0..len {
        if !h.has_index(&missing, i)? {
            continue;
        }
        let m = h.index(&missing, i)?;
        let symbol = prop(h, &m, "symbol")?;
        let side = prop(h, &m, "side")?;
        let mode = prop(h, &m, "positionMode")?;
        let id = h.position_id(&symbol, &side, &mode)?;
        // Spread all own metadata fields, including id overriding the generated id.
        let fake = h.spread_metadata(h.object(vec![("id", id)]), m)?;
        for name in [
            "entryPrice",
            "notional",
            "unrealizedPnl",
            "realizedPnl",
            "contracts",
            "liquidationPrice",
        ] {
            h.define_key(&fake, &s(h, name), h.number(0.))?
        }
        fake_positions.push(fake);
    }
    let mut combined = vec![];
    let mut i = 0;
    while i < h.length(&selected)? {
        combined.push(h.index(&selected, i)?);
        i += 1
    }
    combined.extend(fake_positions);
    let mut ids = vec![];
    let mut out = vec![];
    for p in combined {
        let id = prop(h, &p, "id")?;
        if !ids.iter().any(|old| h.same_value_zero(old, &id)) {
            ids.push(id);
            out.push(p)
        }
    }
    Ok(h.array(out))
}
// ts: BitgetExchange.fetchOpenPositions
pub async fn fetch_open_positions<H: AccountPositionsHost>(h: &H) -> Result<H::Value, H::Error> {
    h.call("ensureAccountTypeDetected", vec![])?.await?;
    if h.truthy(&h.receiver("usesUtaRouting")?) {
        return h.call("fetchUtaOpenPositions", vec![])?.await;
    }
    let r = h
        .read(
            h.receiver("xhr")?,
            "POSITIONS_V2",
            vec![
                ("productType", h.receiver("apiProductType")?),
                ("marginCoin", h.receiver("apiMarginCoin")?),
            ],
        )?
        .await?;
    if !h.success(&prop(h, &r, "code")?)? {
        if h.promote(&r)? {
            return h.call("fetchUtaOpenPositions", vec![])?.await;
        }
        if h.environment_mismatch(&prop(h, &r, "msg")?)? {
            let current = if h.truthy(&h.receiver("options.testnet")?) {
                "Demo/Testnet"
            } else {
                "Live/Production"
            };
            let suggested = if h.truthy(&h.receiver("options.testnet")?) {
                "Live/Production"
            } else {
                "Demo/Testnet"
            };
            h.emit(s(h,&format!("API key environment mismatch: Your API key is for {suggested}, but configured for {current}. Check Testnet setting.")),None)?
        } else {
            h.emit(
                h.error_message(&r, "Bitget positions request failed")?,
                None,
            )?
        }
        return Ok(h.array(vec![]));
    }
    h.map_positions(prop(h, &r, "data")?, false)
}
// ts: BitgetExchange#fetchUtaOpenPositions
pub async fn fetch_uta_open_positions<H: AccountPositionsHost>(
    h: &H,
) -> Result<H::Value, H::Error> {
    let r = h
        .read(
            h.receiver("xhr")?,
            "UTA_POSITIONS",
            vec![("category", h.receiver("apiUtaCategory")?)],
        )?
        .await?;
    if !h.success(&prop(h, &r, "code")?)? {
        h.emit(
            h.error_message(&r, "Bitget UTA positions request failed")?,
            None,
        )?;
        return Ok(h.array(vec![]));
    }
    let data = prop(h, &r, "data")?;
    let list = if h.nullish(&data) {
        h.undefined()
    } else {
        prop(h, &data, "list")?
    };
    let rows = if h.is_array(&list) {
        prop(h, &prop(h, &r, "data")?, "list")?
    } else {
        h.array(vec![])
    };
    h.map_positions(rows, true)
}
// ts: BitgetInverseExchange.fetchBalance
pub async fn fetch_inverse_balance<H: AccountPositionsHost>(h: &H) -> Result<H::Value, H::Error> {
    let r = h
        .read(
            h.receiver("xhr")?,
            "BALANCE",
            vec![("productType", h.receiver("apiProductType")?)],
        )?
        .await?;
    let code = prop(h, &r, "code")?;
    let parsed = if h.nullish(&code) { h.number(0.) } else { code };
    // Source explicitly String(code ?? 0), then parseInt(..., 10).
    if h.parse_int(&h.string(&h.text(&parsed)?), Some(10))? != 0. {
        let code = prop(h, &r, "code")?;
        let code = if h.nullish(&code) { s(h, "") } else { code };
        let msg = if h.text(&code)? == "40085".encode_utf16().collect::<Vec<_>>() {
            s(
                h,
                crate::account_predicates::BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE,
            )
        } else {
            or(h, prop(h, &r, "msg")?, || {
                Ok(s(h, "Bitget inverse account balance request failed"))
            })?
        };
        return Err(h.make_error(msg));
    }
    let data = prop(h, &r, "data")?;
    if !h.truthy(&data) || h.length(&data)? == 0 {
        return h.receiver("store.balance");
    }
    let (mut total, mut upnl, mut used, mut free) = (0., 0., 0., 0.);
    let coins = h.object(vec![]);
    let mut i = 0;
    while i < h.length(&data)? {
        let b = h.index(&data, i)?;
        i += 1;
        let coin = prop(h, &b, "marginCoin")?;
        let equity = h.parse_float(&or(h, prop(h, &b, "unionTotalMargin")?, || {
            or(h, prop(h, &b, "accountEquity")?, || Ok(s(h, "0")))
        })?)?;
        let pnl = h.parse_float(&or(h, prop(h, &b, "unrealizedPL")?, || Ok(s(h, "0")))?)?;
        let available = h.parse_float(&or(h, prop(h, &b, "unionAvailable")?, || {
            or(h, prop(h, &b, "available")?, || Ok(s(h, "0")))
        })?)?;
        let used_coin = safe_cex_num::subtract(equity, available);
        let total_coin = safe_cex_num::subtract(equity, pnl);
        let mut usd = h.parse_float(&or(h, prop(h, &b, "usdtEquity")?, || Ok(s(h, "0")))?)?;
        if usd == 0. {
            let tickers = h.receiver("store.tickers")?;
            let ticker = find(h, &tickers, |t| {
                let symbol = prop(h, t, "symbol")?;
                let mut usd = h.text(&coin)?;
                usd.extend("USD".encode_utf16());
                if h.equal(&symbol, &h.string(&usd)) {
                    return Ok(true);
                }
                let symbol = prop(h, t, "symbol")?;
                let mut usdt = h.text(&coin)?;
                usdt.extend("USDT".encode_utf16());
                Ok(h.equal(&symbol, &h.string(&usdt)))
            })?;
            if h.truthy(&ticker) {
                usd = equity * h.numeric(&prop(h, &ticker, "mark")?)?
            }
        }
        total += if usd > 0. {
            safe_cex_num::subtract(usd, pnl * (usd / equity))
        } else {
            total_coin
        };
        upnl += pnl * if usd > 0. { usd / equity } else { 1. };
        used += used_coin * if usd > 0. { usd / equity } else { 1. };
        free += available * if usd > 0. { usd / equity } else { 1. };
        let notional = if usd != 0. && !usd.is_nan() {
            usd
        } else {
            equity
        };
        h.set_key(
            &coins,
            &coin,
            h.object(vec![
                ("base", coin.clone()),
                ("amount", h.number(equity)),
                ("notional", h.number(notional)),
            ]),
        )?;
    }
    Ok(h.object(vec![
        ("total", h.number(total)),
        ("upnl", h.number(upnl)),
        ("used", h.number(used)),
        ("free", h.number(free)),
        ("coins", coins),
        ("info", data),
    ]))
}
