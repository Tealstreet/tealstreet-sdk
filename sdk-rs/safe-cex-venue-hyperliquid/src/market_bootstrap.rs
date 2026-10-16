//! Primary market bootstrap: parallel roots, bounded serial HIP-3 loads, retained successes.
use crate::{
    helper_tasks::{self, HelperObserver},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
#[derive(Clone)]
pub enum MarketBootstrapCallback<V> {
    CoreLog { exchange: V },
    DexName,
    Unreached { loaded: V, failed: V },
}
pub trait MarketBootstrapHost: SpotAccountHost {
    fn bootstrap_callback(
        &mut self,
        callback: MarketBootstrapCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    /// Native array binding, including IteratorClose and elisions.
    fn array_binding(
        &mut self,
        value: &Self::Value,
        count: usize,
    ) -> Result<Vec<Self::Value>, Self::Error>;
    fn spread_arguments(
        &mut self,
        args: &mut Vec<Self::Value>,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn iterator(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, iterator: &Self::Value);
    fn close_after_break(&mut self, iterator: &Self::Value) -> Result<(), Self::Error>;
}
fn lit<H: SpotAccountHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn method<H: SpotAccountHost>(
    h: &mut H,
    o: &H::Value,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, k)?;
    let args = args(h)?;
    h.call(&f, o, &args)
}
fn global<H: SpotAccountHost>(
    h: &mut H,
    o: &str,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let o = h.global(o)?;
    method(h, &o, k, args)
}
fn constant<H: SpotAccountHost>(h: &mut H, k: &str) -> Result<H::Value, H::Error> {
    let c = h.imported("HyperliquidExchange")?;
    h.get(&c, k)
}
fn log<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    args: impl FnOnce(&mut H) -> Result<H::Value, H::Error>,
    severity: &str,
) -> Result<H::Value, H::Error> {
    method(h, e, "log", |h| {
        let msg = args(h)?;
        let table = h.imported("LogSeverity")?;
        let sev = h.get(&table, severity)?;
        Ok(vec![msg, sev])
    })
}
fn template<H: SpotAccountHost>(
    h: &mut H,
    parts: &[(&str, Option<H::Value>)],
) -> Result<H::Value, H::Error> {
    let mut out = vec![];
    for (text, value) in parts {
        out.extend(text.encode_utf16());
        if let Some(v) = value {
            out.extend(h.template_string(v)?.0)
        }
    }
    Ok(h.literal(&JsString(out)))
}
pub fn run_market_bootstrap_callback<H: MarketBootstrapHost>(
    h: &mut H,
    callback: &MarketBootstrapCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match callback {
        MarketBootstrapCallback::CoreLog { exchange } => {
            let second = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let severity = if h.is_undefined(&second) {
                let table = h.imported("LogSeverity")?;
                h.get(&table, "Info")?
            } else {
                second
            };
            method(h, exchange, "log", |_| Ok(vec![first, severity]))
        }
        MarketBootstrapCallback::DexName => h.get(&first, "name"),
        MarketBootstrapCallback::Unreached { loaded, failed } => {
            let found = method(h, loaded, "includes", |_| Ok(vec![first.clone()]))?;
            if h.truthy(&found) {
                return Ok(h.boolean(false));
            }
            let found = method(h, failed, "includes", |_| Ok(vec![first]))?;
            Ok(h.boolean(!h.truthy(&found)))
        }
    }
}
async fn core_body<H: MarketBootstrapHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error> {
    let f = h.imported("fetchCoreMetaAndAssetCtxsWithFallback")?;
    let input = h.object()?;
    let info = h.get(&e, "infoXhr")?;
    h.define_data(&input, "infoXhr", &info)?;
    let xhr = h.get(&e, "xhr")?;
    h.define_data(&input, "xhr", &xhr)?;
    let logger = h.bootstrap_callback(MarketBootstrapCallback::CoreLog { exchange: e })?;
    h.define_data(&input, "log", &logger)?;
    let op = h.call(&f, &h.undefined(), &[input])?;
    h.await_value(op).await
}
fn push<H: SpotAccountHost>(h: &mut H, array: &H::Value, value: H::Value) -> Result<(), H::Error> {
    method(h, array, "push", |_| Ok(vec![value]))?;
    Ok(())
}
fn merge<H: MarketBootstrapHost>(
    h: &mut H,
    target: &H::Value,
    result: &H::Value,
    key: &str,
) -> Result<(), H::Error> {
    method(h, target, "push", |h| {
        let value = h.get(result, key)?;
        let mut args = vec![];
        h.spread_arguments(&mut args, &value)?;
        Ok(args)
    })?;
    Ok(())
}
async fn body<H: MarketBootstrapHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: 'static,
{
    // Promise.all's callee is evaluated before the array's eager task calls.
    let promise = h.global("Promise")?;
    let all = h.get(&promise, "all")?;
    let core = h.spawn_native(Box::pin(core_body(h.clone(), e.clone())));
    let fees = method(&mut h, &e, "fetchFee", |_| Ok(vec![]))?;
    let dexes = method(&mut h, &e, "withStartupTimeout", |h| {
        let op = method(h, &e, "fetchHip3Dexs", |_| Ok(vec![]))?;
        let timeout = constant(h, "HIP3_DEX_DISCOVERY_TIMEOUT_MS")?;
        let label = lit(h, "HIP-3 DEX discovery");
        let fallback = h.array(&[])?;
        Ok(vec![op, timeout, label, fallback])
    })?;
    let spot = method(&mut h, &e, "withStartupTimeout", |h| {
        let op = method(h, &e, "fetchSpotMarketsAndTickers", |_| Ok(vec![]))?;
        let timeout = constant(h, "SPOT_MARKETS_LOAD_TIMEOUT_MS")?;
        let label = lit(h, "spot markets loading");
        let fallback = h.object()?;
        let empty = h.array(&[])?;
        h.define_data(&fallback, "markets", &empty)?;
        let empty = h.array(&[])?;
        h.define_data(&fallback, "tickers", &empty)?;
        Ok(vec![op, timeout, label, fallback])
    })?;
    let batch = h.array(&[core, fees, dexes, spot])?;
    let combined = h.call(&all, &promise, &[batch])?;
    let combined = h.await_value(combined).await?;
    let bound = h.array_binding(&combined, 4)?;
    let (core, fees, dexes, spot) = (
        bound[0].clone(),
        bound[1].clone(),
        bound[2].clone(),
        bound[3].clone(),
    );
    let pair = h.array_binding(&core, 2)?;
    let result = method(&mut h, &e, "fetchMarketsHelper", |_| {
        Ok(vec![pair[0].clone(), pair[1].clone(), fees.clone()])
    })?;
    let markets = h.get(&result, "markets")?;
    let tickers = h.get(&result, "tickers")?;
    let length = h.get(&dexes, "length")?;
    let nonempty = h.binary(">", &length, &h.number(0.))?;
    if h.truthy(&nonempty) {
        let budget = global(&mut h, "Math", "min", |h| {
            let cap = constant(h, "HIP3_MARKETS_LOAD_TIMEOUT_CAP_MS")?;
            let floor = global(h, "Math", "max", |h| {
                let minimum = constant(h, "HIP3_MARKETS_LOAD_TIMEOUT_MS")?;
                let length = h.get(&dexes, "length")?;
                let each = constant(h, "HIP3_PER_DEX_LOAD_BUDGET_MS")?;
                let scaled = h.binary("*", &length, &each)?;
                Ok(vec![minimum, scaled])
            })?;
            Ok(vec![cap, floor])
        })?;
        let loaded = h.array(&[])?;
        let failed = h.array(&[])?;
        let results = h.array(&[])?;
        let time = global(&mut h, "Date", "now", |_| Ok(vec![]))?;
        let deadline = h.binary("+", &time, &budget)?;
        let mut timed_out = false;
        let iterator = h.iterator(&dexes)?;
        while let Some(dex) = h.next(&iterator)? {
            let step = async {
                let time = global(&mut h, "Date", "now", |_| Ok(vec![]))?;
                let remaining = h.binary("-", &deadline, &time)?;
                let exhausted = h.binary("<=", &remaining, &h.number(0.))?;
                if h.truthy(&exhausted) {
                    return Ok(true);
                }
                let op = method(&mut h, &e, "withStartupTimeout", |h| {
                    let fetch = method(h, &e, "fetchHip3Markets", |h| {
                        let name = h.get(&dex, "name")?;
                        let position = h.get(&dex, "dexPosition")?;
                        Ok(vec![name, position, fees.clone()])
                    })?;
                    let name = h.get(&dex, "name")?;
                    let label =
                        template(h, &[("HIP-3 markets loading (", Some(name)), (")", None)])?;
                    Ok(vec![fetch, remaining.clone(), label, lit(h, "timed-out")])
                })?;
                let result = h.await_value(op).await?;
                if h.strict_equal(&result, &lit(&h, "timed-out")) {
                    return Ok(true);
                }
                if h.strict_equal(&result, &h.null()) {
                    method(&mut h, &failed, "push", |h| Ok(vec![h.get(&dex, "name")?]))?;
                    return Ok(false);
                }
                push(&mut h, &results, result)?;
                method(&mut h, &loaded, "push", |h| Ok(vec![h.get(&dex, "name")?]))?;
                Ok(false)
            }
            .await;
            match step {
                Ok(true) => {
                    timed_out = true;
                    h.close_after_break(&iterator)?;
                    break;
                }
                Ok(false) => {}
                Err(err) => {
                    h.close_after_throw(&iterator);
                    return Err(err);
                }
            }
        }
        h.set(&e, "hip3DexNames", &loaded)?;
        let iterator = h.iterator(&results)?;
        while let Some(result) = h.next(&iterator)? {
            let merged = (|| {
                merge(&mut h, &markets, &result, "markets")?;
                merge(&mut h, &tickers, &result, "tickers")
            })();
            if let Err(err) = merged {
                h.close_after_throw(&iterator);
                return Err(err);
            }
        }
        let length = h.get(&loaded, "length")?;
        let nonempty = h.binary(">", &length, &h.number(0.))?;
        if h.truthy(&nonempty) {
            log(
                &mut h,
                &e,
                |h| {
                    let mut text = "Loaded ".encode_utf16().collect::<Vec<_>>();
                    let count = h.get(&loaded, "length")?;
                    text.extend(h.template_string(&count)?.0);
                    text.extend("/".encode_utf16());
                    let total = h.get(&dexes, "length")?;
                    text.extend(h.template_string(&total)?.0);
                    text.extend(" HIP-3 DEXes (".encode_utf16());
                    let names = method(h, &loaded, "join", |h| Ok(vec![lit(h, ", ")]))?;
                    text.extend(h.template_string(&names)?.0);
                    text.extend("); ".encode_utf16());
                    let count_markets = h.get(&markets, "length")?;
                    text.extend(h.template_string(&count_markets)?.0);
                    text.extend(" total markets".encode_utf16());
                    Ok(h.literal(&JsString(text)))
                },
                "Info",
            )?;
        }
        let length = h.get(&failed, "length")?;
        let nonempty = h.binary(">", &length, &h.number(0.))?;
        if h.truthy(&nonempty) {
            log(
                &mut h,
                &e,
                |h| {
                    let names = method(h, &failed, "join", |h| Ok(vec![lit(h, ", ")]))?;
                    template(h, &[("HIP-3 DEXes that failed to load: ", Some(names))])
                },
                "Warning",
            )?;
        }
        if timed_out {
            let mapped = method(&mut h, &dexes, "map", |h| {
                Ok(vec![
                    h.bootstrap_callback(MarketBootstrapCallback::DexName)?,
                ])
            })?;
            let unreached = method(&mut h, &mapped, "filter", |h| {
                Ok(vec![h.bootstrap_callback(
                    MarketBootstrapCallback::Unreached {
                        loaded: loaded.clone(),
                        failed: failed.clone(),
                    },
                )?])
            })?;
            log(
                &mut h,
                &e,
                |h| {
                    let seconds = global(h, "Math", "round", |h| {
                        let seconds = h.binary("/", &budget, &h.number(1000.))?;
                        Ok(vec![seconds])
                    })?;
                    let mut text = "HIP-3 market loading ran out of its "
                        .encode_utf16()
                        .collect::<Vec<_>>();
                    text.extend(h.template_string(&seconds)?.0);
                    text.extend("s budget; not loaded: ".encode_utf16());
                    let names = method(h, &unreached, "join", |h| Ok(vec![lit(h, ", ")]))?;
                    text.extend(h.template_string(&names)?.0);
                    Ok(h.literal(&JsString(text)))
                },
                "Warning",
            )?;
        }
    }
    let spot_markets = h.get(&spot, "markets")?;
    let length = h.get(&spot_markets, "length")?;
    let nonempty = h.binary(">", &length, &h.number(0.))?;
    if h.truthy(&nonempty) {
        merge(&mut h, &markets, &spot, "markets")?;
        merge(&mut h, &tickers, &spot, "tickers")?;
        log(
            &mut h,
            &e,
            |h| {
                let spot_markets = h.get(&spot, "markets")?;
                let count = h.get(&spot_markets, "length")?;
                template(
                    h,
                    &[
                        ("Loaded ", Some(count)),
                        (" Hyperliquid spot markets", None),
                    ],
                )
            },
            "Info",
        )?;
    }
    h.array(&[markets, tickers])
}
pub fn fetch_markets_and_tickers<H>(
    host: H,
    exchange: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: MarketBootstrapHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, body(host, exchange))
}
