//! Native option queries preserve source coercion and live Store market objects.
use crate::binding::{
    host::get_key,
    promise::await_js,
    rt::Rt,
    value::{Js, Thrown, arg},
};
use safe_cex_exchange::js_runtime::ops;

fn num(value: Js) -> Result<Js, Thrown> {
    let value = if value.is_nullish() {
        Js::str("")
    } else {
        value
    };
    let parsed = Js::String(value.to_js_string()?).parse_float()?;
    Ok(Js::Number(if parsed.is_finite() { parsed } else { 0. }))
}

pub(crate) fn map_option_greeks(raw: &Js) -> Result<Js, Thrown> {
    let delta = raw.get("delta")?;
    if delta.is_undefined() || delta.strict_equals(&Js::str("")) {
        return Ok(Js::Undefined);
    }
    Ok(Js::object(vec![
        ("delta", num(raw.get("delta")?)?),
        ("gamma", num(raw.get("gamma")?)?),
        ("vega", num(raw.get("vega")?)?),
        ("theta", num(raw.get("theta")?)?),
        ("units", Js::str(crate::options::BYBIT_GREEK_UNITS)),
    ]))
}

fn map_option_ticker(raw: &Js) -> Result<Js, Thrown> {
    let greeks = map_option_greeks(raw)?;
    let quote = Js::object(vec![
        ("symbol", raw.get("symbol")?),
        ("bid", num(raw.get("bid1Price")?)?),
        ("ask", num(raw.get("ask1Price")?)?),
        ("mark", num(raw.get("markPrice")?)?),
        ("markIv", num(raw.get("markIv")?)?),
        ("bidIv", num(raw.get("bid1Iv")?)?),
        ("askIv", num(raw.get("ask1Iv")?)?),
        ("underlyingPrice", num(raw.get("underlyingPrice")?)?),
    ]);
    if greeks.truthy() {
        quote.define("greeks", greeks)?;
    }
    Ok(quote)
}

fn map_coin_greeks(raw: &Js) -> Result<Js, Thrown> {
    Ok(Js::object(vec![
        ("underlying", raw.get("baseCoin")?),
        ("delta", num(raw.get("totalDelta")?)?),
        ("gamma", num(raw.get("totalGamma")?)?),
        ("vega", num(raw.get("totalVega")?)?),
        ("theta", num(raw.get("totalTheta")?)?),
        ("units", Js::str(crate::options::BYBIT_GREEK_UNITS)),
    ]))
}

fn matches_option_query(market: &Js, query: &Js) -> Result<bool, Thrown> {
    let terms = market.get("option")?;
    if !terms.truthy()
        || !terms
            .get("underlying")?
            .strict_equals(&query.get("underlying")?)
    {
        return Ok(false);
    }
    for field in ["expiry", "right"] {
        let expected = query.get(field)?;
        if !expected.is_undefined() && !terms.get(field)?.strict_equals(&query.get(field)?) {
            return Ok(false);
        }
    }
    if !query.get("strikeMin")?.is_undefined()
        && ops::less_than(&terms.get("strike")?, &query.get("strikeMin")?)?
    {
        return Ok(false);
    }
    if !query.get("strikeMax")?.is_undefined()
        && ops::greater_than(&terms.get("strike")?, &query.get("strikeMax")?)?
    {
        return Ok(false);
    }
    Ok(true)
}

fn build_option_chain(rt: &Rt, markets: Js, quotes: Js, query: Js) -> Result<Js, Thrown> {
    let entries = quotes.call_method(
        "map",
        vec![Js::function("quoteEntry", |_, a| {
            let quote = arg(&a, 0);
            Ok(Js::array(vec![quote.get("symbol")?, quote]))
        })],
    )?;
    let by_symbol = rt.global("Map")?.construct(vec![entries], None)?;
    let markets = markets.call_method(
        "filter",
        vec![Js::function("matchesOptionQuery", move |_, a| {
            Ok(Js::Bool(matches_option_query(&arg(&a, 0), &query)?))
        })],
    )?;
    let rows = markets.call_method(
        "flatMap",
        vec![Js::function("joinOptionQuote", move |_, a| {
            let market = arg(&a, 0);
            let quote = by_symbol.call_method("get", vec![market.get("symbol")?])?;
            Ok(Js::array(if quote.truthy() {
                vec![Js::object(vec![("market", market), ("quote", quote)])]
            } else {
                vec![]
            }))
        })],
    )?;
    rows.call_method(
        "sort",
        vec![Js::function("compareOptionChain", |_, args| {
            let a = arg(&args, 0).get("market")?.get("option")?;
            let b = arg(&args, 1).get("market")?.get("option")?;
            for key in ["expiry", "strike"] {
                let difference = ops::subtract(&a.get(key)?, &b.get(key)?)?;
                if difference.truthy() {
                    return Ok(difference);
                }
            }
            a.get("right")?
                .call_method("localeCompare", vec![b.get("right")?])
        })],
    )
}

fn option_expiries(rt: &Rt, markets: Js, underlying: Js) -> Result<Js, Thrown> {
    let markets = markets.call_method(
        "filter",
        vec![Js::function("matchingUnderlying", move |_, a| {
            let terms = arg(&a, 0).get("option")?;
            let base = if terms.is_nullish() {
                Js::Undefined
            } else {
                terms.get("underlying")?
            };
            Ok(Js::Bool(base.strict_equals(&underlying)))
        })],
    )?;
    let expiries = markets.call_method(
        "map",
        vec![Js::function("expiry", |_, a| {
            arg(&a, 0).get("option")?.get("expiry")
        })],
    )?;
    let distinct = rt.global("Set")?.construct(vec![expiries], None)?;
    let values = rt.global("Array")?.call_method("from", vec![distinct])?;
    values.call_method(
        "sort",
        vec![Js::function("compareExpiry", |_, a| {
            ops::subtract(&arg(&a, 0), &arg(&a, 1))
        })],
    )
}

pub(crate) fn assert_options_enabled(rt: &Rt) -> Result<Js, Thrown> {
    if !rt.this.call_method("optionsEnabled", vec![])?.truthy() {
        return Err(Thrown::error(
            "Error",
            "Options are not enabled on this Bybit account",
        ));
    }
    Ok(Js::Undefined)
}

async fn request_list(rt: &Rt, endpoint: &str, config: Option<Js>) -> Result<Js, Thrown> {
    let mut args = vec![Js::str(endpoint)];
    if let Some(config) = config {
        args.push(config);
    }
    let response = await_js(&rt.exchange_runtime(), rt.this.get("xhr")?.call_method("get", args)?).await?;
    let data = response.get("data")?;
    if get_key(&data, "retCode")?.to_number()? != 0. {
        return Err(Thrown::new(
            rt.global("Error")?
                .call(Js::Undefined, vec![get_key(&data, "retMsg")?])?,
        ));
    }
    let result = data.get("result")?;
    let list = if result.is_nullish() {
        Js::Undefined
    } else {
        result.get("list")?
    };
    Ok(if list.is_nullish() {
        Js::array(vec![])
    } else {
        list
    })
}

pub(crate) fn fetch_option_chain(rt: &Rt, query: Js) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        rt.this.call_method("assertOptionsEnabled", vec![])?;
        let config = Js::object(vec![(
            "params",
            Js::object(vec![
                ("category", Js::str("option")),
                ("baseCoin", query.get("underlying")?),
            ]),
        )]);
        let quotes = request_list(&rt, "/v5/market/tickers", Some(config))
            .await?
            .call_method(
                "map",
                vec![Js::function("mapBybitOptionTicker", |_, a| {
                    map_option_ticker(&arg(&a, 0))
                })],
            )?;
        build_option_chain(&rt, rt.this.get("store")?.get("markets")?, quotes, query)
    })))
}

pub(crate) fn fetch_option_expiries(rt: &Rt, underlying: Js) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        rt.this.call_method("assertOptionsEnabled", vec![])?;
        option_expiries(&rt, rt.this.get("store")?.get("markets")?, underlying)
    })))
}

pub(crate) fn fetch_account_greeks(rt: &Rt) -> Result<Js, Thrown> {
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        rt.this.call_method("assertOptionsEnabled", vec![])?;
        request_list(&rt, "/v5/asset/coin-greeks", None)
            .await?
            .call_method(
                "map",
                vec![Js::function("mapBybitCoinGreeks", |_, a| {
                    map_coin_greeks(&arg(&a, 0))
                })],
            )
    })))
}
