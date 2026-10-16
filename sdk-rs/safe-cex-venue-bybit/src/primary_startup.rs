//! Maintained startup flow with visible detached cache rejection reporting.
use crate::core_snapshot_readers::SnapshotHost;
use std::rc::Rc;

fn call<H: SnapshotHost>(
    h: &H,
    owner: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let selected = h.select(owner, key)?;
    h.apply(selected, args)
}
fn loaded<H: SnapshotHost>(
    h: &H,
    owner: &H::Value,
    fields: Vec<(&'static str, H::Value)>,
    flags: &[&'static str],
) -> Result<(), H::Error> {
    let update = h.select(h.get(owner, "store")?, "update")?;
    let current = h.get(&h.get(owner, "store")?, "loaded")?;
    let next = h.spread(
        current,
        flags.iter().map(|key| (*key, h.boolean(true))).collect(),
    )?;
    let mut fields = fields;
    fields.push(("loaded", next));
    h.apply(update, vec![h.object(fields)])?;
    Ok(())
}
pub fn start_callback<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    code: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    match code {
        "start_fee_overlay" => {
            let launcher = h.clone();
            Ok(launcher.launch(Box::pin(async move {
                let global = h.intrinsic("globalThis", vec![])?;
                let overlay = h.get(&global, "bybitFeeOverlay")?;
                let fees = call(h.as_ref(), owner, "fetchFees", vec![h.literal("linear")])?;
                let fees = h.wait(fees).await?;
                h.apply(overlay, vec![fees])
            })))
        }
        "start_markets_loaded" => {
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.array(vec![]));
            }
            let markets = args.first().cloned().unwrap_or_else(|| h.undefined());
            let log = h.select(owner.clone(), "log")?;
            let count = h.intrinsic("template", vec![h.get(&markets, "length")?])?;
            let message = h.intrinsic(
                "string.concat",
                vec![h.literal("Loaded "), count, h.literal(" Bybit markets")],
            )?;
            h.apply(log, vec![message])?;
            loaded(
                h.as_ref(),
                &owner,
                vec![("markets", markets.clone())],
                &["markets"],
            )?;
            call(
                h.as_ref(),
                h.get(&owner, "publicWebsocket")?,
                "connectAndSubscribe",
                vec![],
            )?;
            let inverse = h.get(&owner, "inversePublicWebsocket")?;
            let null = h.intrinsic("null", vec![])?;
            if !h.equal(&inverse, &h.undefined()) && !h.equal(&inverse, &null) {
                call(h.as_ref(), inverse, "connectAndSubscribe", vec![])?;
            }
            Ok(markets)
        }
        "start_probe_success" => Ok(h.undefined()),
        "start_cache_failure" => {
            let failure = args.first().cloned().unwrap_or_else(|| h.undefined());
            call(h.as_ref(), owner, "emitAndLogError", vec![failure])?;
            Ok(h.undefined())
        }
        "start_probe_rejection" => {
            let error = args.first().cloned().unwrap_or_else(|| h.undefined());
            let global = h.intrinsic("globalThis", vec![])?;
            let stringify = h.get(&global, "String")?;
            let message = h.optional(&error, "message")?;
            let value = if h.truthy(&message) { message } else { error };
            let text = h.apply(stringify, vec![value])?;
            Ok(if h.truthy(&text) {
                text
            } else {
                h.literal("request failed")
            })
        }
        _ => panic!("unknown primary start callback {code}"),
    }
}
pub fn start<H: SnapshotHost>(h: Rc<H>, owner: H::Value) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let global = h.intrinsic("globalThis", vec![])?;
        let clock = h.get(&global, "virtualClock")?;
        let started = call(h.as_ref(), clock, "start", vec![])?;
        h.wait(started).await?;
        h.set(&owner, "batchLimit", h.number(10.))?;
        let public_only = h.get(&owner, "isPublicDataOnly")?;
        if h.truthy(&h.get(&owner, "needsAccountMarketOverlay")?) {
            let overlay = h.select(owner.clone(), "loadAccountMarketOverlay")?;
            let body = h.callback(owner.clone(), "start_fee_overlay", vec![])?;
            let result = h.apply(overlay, vec![body])?;
            h.wait(result).await?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
        }
        let external = call(h.as_ref(), owner.clone(), "initExternalPublicData", vec![])?;
        let external = h.wait(external).await?;
        // The probe's handlers attach at creation: a rejection must never sit unhandled.
        let probe = if h.truthy(&public_only) {
            h.intrinsic("null", vec![])?
        } else {
            let margin = call(h.as_ref(), owner.clone(), "fetchMarginAccountInfos", vec![])?;
            let then = h.select(margin, "then")?;
            let ignore = h.callback(owner.clone(), "start_probe_success", vec![])?;
            let failure = h.callback(owner.clone(), "start_probe_rejection", vec![])?;
            h.apply(then, vec![ignore, failure])?
        };
        if !h.truthy(&public_only) {
            let cache = call(h.as_ref(), owner.clone(), "fetchAndCacheApiKeyInfo", vec![])?;
            let catch = h.select(cache, "catch")?;
            let failure = h.callback(owner.clone(), "start_cache_failure", vec![])?;
            h.apply(catch, vec![failure])?;
        }
        if !h.truthy(&external) {
            let all = h.select(h.get(&global, "Promise")?, "all")?;
            let markets = call(h.as_ref(), owner.clone(), "fetchMarkets", vec![])?;
            let then = h.select(markets, "then")?;
            let body = h.callback(owner.clone(), "start_markets_loaded", vec![])?;
            let markets = h.apply(then, vec![body])?;
            let tickers = call(
                h.as_ref(),
                owner.clone(),
                "fetchTickersWithoutParse",
                vec![],
            )?;
            let both = h.apply(all, vec![h.array(vec![markets, tickers])])?;
            let both = h.wait(both).await?;
            let bindings = h.intrinsic("iterator.bindings", vec![both, h.number(2.)])?;
            let raw_tickers = h
                .values(&bindings)?
                .into_iter()
                .nth(1)
                .unwrap_or_else(|| h.undefined());
            let parse = h.select(owner.clone(), "parseTickers")?;
            let tickers = h.apply(parse, vec![raw_tickers])?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
            loaded(h.as_ref(), &owner, vec![("tickers", tickers)], &["tickers"])?;
        }
        let probe = if h.truthy(&probe) {
            h.wait(probe).await?
        } else {
            h.undefined()
        };
        if h.truthy(&probe) {
            let log = h.select(owner.clone(), "log")?;
            let text = h.intrinsic("template", vec![probe])?;
            let message = h.intrinsic(
                "string.concat",
                vec![
                    h.literal("Bybit market data is ready, but the private API was rejected ("),
                    text,
                    h.literal(") — trading is unavailable"),
                ],
            )?;
            let warning = h.get(&h.get(&global, "LogSeverity")?, "Warning")?;
            h.apply(log, vec![message, warning])?;
        } else {
            call(
                h.as_ref(),
                owner.clone(),
                "log",
                vec![h.literal("Ready to trade on Bybit")],
            )?;
        }
        if h.truthy(&public_only) {
            loaded(
                h.as_ref(),
                &owner,
                vec![],
                &["balance", "positions", "orders"],
            )?;
            return Ok(h.undefined());
        }
        let key = h.get(&h.get(&owner, "options")?, "key")?;
        let missing = !h.truthy(&key)
            || (!h.truthy(&h.get(&h.get(&owner, "options")?, "secret")?)
                && !h.truthy(&h.get(&h.get(&owner, "options")?, "remoteSigner")?)
                && !h.truthy(&h.get(&h.get(&owner, "options")?, "payloadSigner")?));
        if missing {
            let log = h.select(owner.clone(), "log")?;
            let warning = h.get(&h.get(&global, "LogSeverity")?, "Warning")?;
            h.apply(
                log,
                vec![
                    h.literal("Skipping private websocket connection - no API credentials"),
                    warning,
                ],
            )?;
            return Ok(h.undefined());
        }
        call(
            h.as_ref(),
            h.get(&owner, "privateWebsocket")?,
            "connectAndSubscribe",
            vec![],
        )?;
        let snapshots = call(
            h.as_ref(),
            owner.clone(),
            "fetchOrdersPositionsAndBalance",
            vec![],
        )?;
        h.wait(snapshots).await?;
        call(h.as_ref(), owner, "tick", vec![])?;
        Ok(h.undefined())
    }))
}
