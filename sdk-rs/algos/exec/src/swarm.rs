//! Concurrent one-shot market slices; account credits are checked before release.
use crate::placement::{Placement, n, prepare_order};
use crate::{Host, resolve, rows};
use serde_json::{Value, json};
pub(crate) fn prepare(
    symbol: &str,
    args: &[String],
    store: &Value,
    host: &mut impl Host,
) -> Result<(Vec<Value>, String, String), String> {
    let side = args
        .get(1)
        .filter(|s| matches!(s.as_str(), "buy" | "sell"))
        .ok_or("swarm: first argument must be 'buy' or 'sell'")?;
    let side = if side == "buy" { "buy" } else { "sell" };
    let mut index = 2;
    let mut target = symbol.to_string();
    if let Some(token) = args.get(index)
        && let Some(s) = resolve(store, token)
    {
        target = s;
        index += 1;
    }
    let size = args.get(index).ok_or("swarm: missing size")?;
    index += 1;
    if args.get(index).map(String::as_str) != Some("into") {
        return Err("swarm: expected 'into <count>' after size".into());
    }
    let count = args
        .get(index + 1)
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0 && *n <= crate::MAX_PARALLEL)
        .ok_or("swarm: native slice count must be from 1 to 32")?;
    index += 2;
    let mut reduce = false;
    let mut irregular = false;
    for token in args.iter().skip(index) {
        match token.as_str() {
            "reduce" => reduce = true,
            "irregular" => irregular = true,
            _ => {
                target = resolve(store, token)
                    .ok_or_else(|| format!("swarm: unexpected token \"{token}\""))?;
            }
        }
    }
    if !reduce
        && store["socketHealth"]["privateInitialized"] == true
        && store["socketHealth"]["privateHealthy"] == false
    {
        return Err("Account connection is degraded (private data stream is stale) — refusing to open or increase exposure. Wait for reconnect (run `health`); close / cancel / reduce-only orders stay allowed.".into());
    }
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == target)
        .ok_or_else(|| format!("Market not found for {target}"))?;
    let last = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == target)
        .map(|t| n(t, "last"))
        .unwrap_or(0.);
    let lot = market["precision"]["amount"]
        .as_f64()
        .filter(|n| *n != 0.)
        .unwrap_or(1e-8);
    let sized = if reduce && size.ends_with('%') {
        format!("{size}pos")
    } else {
        size.clone()
    };
    let total = Placement::sized(&target, side, &sized, false).size(store, market, last, lot)?;
    if total <= 0. || !total.is_finite() {
        return Err("swarm: total size resolved to zero contracts".into());
    }
    let mut weights = Vec::with_capacity(count);
    for _ in 0..count {
        weights.push(if irregular {
            let r = host.random()?;
            if !(0.0..1.0).contains(&r) {
                return Err("Invalid random distribution".into());
            }
            0.5 + r
        } else {
            1.
        });
    }
    let sum = weights.iter().sum::<f64>();
    let mut slices: Vec<_> = weights
        .iter()
        .map(|w| crate::placement::adjust(w / sum * total, lot))
        .collect();
    let sum = slices.iter().sum::<f64>();
    let delta = crate::placement::adjust(total - sum, lot);
    let last = slices.len() - 1;
    slices[last] = crate::placement::adjust(slices[last] + delta, lot);
    if slices.iter().any(|n| *n <= 0.) {
        return Err(format!(
            "swarm: total size too small to split into {count} (precision {lot})"
        ));
    }
    let orders=slices.into_iter().map(|amount|prepare_order(json!({"symbol":target,"side":side,"type":"market","amount":amount,"reduceOnly":reduce,"timeInForce":"GoodTillCancel"}),store)).collect::<Result<Vec<_>,_>>()?;
    Ok((orders, target, side.into()))
}
