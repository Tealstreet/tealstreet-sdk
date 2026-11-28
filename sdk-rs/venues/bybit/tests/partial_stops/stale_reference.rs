//! TEALSTREET-NEXT-EGG (2026-11-27, inverse BTCUSD): the cancel proof outlived the 5 s
//! reference freshness window, a quiet last price was not re-sent, and both the
//! replacement and the put-back were refused, leaving the position with no stop.
use super::*;

fn page(profile: Profile, rows: Vec<serde_json::Value>) -> String {
    serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":rows,"nextPageCursor":""}})
        .to_string()
}

/// Accepts the cancel, lets the reference go stale, then answers the proof reads.
fn cancel_then_stale_proof(h: &mut Harness, profile: Profile) {
    let original = h.engine.store().order("partial").unwrap().clone();
    h.respond(
        profile.cancel_path(),
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    // Longer than FRESH_TICKER_MS with no ticker frame: the history polls alone total 5.25 s.
    h.advance(6_000.0);
    let target = native_partial_target(h, profile, &original);
    h.respond("&orderId=partial", 200, &page(profile, vec![target]));
    for _ in 0..6 {
        if h.has_request("/v5/position/trading-stop") || h.result(91).is_some() {
            break;
        }
        if h.has_request("&openOnly=0&limit=50") {
            h.respond("&openOnly=0&limit=50", 200, &page(profile, vec![]));
        } else if h.has_request("/v5/order/history") {
            h.respond("/v5/order/history", 200, &page(profile, vec![]));
        } else {
            h.advance(200.0);
        }
    }
}

fn stop_writes(h: &Harness) -> Vec<serde_json::Value> {
    h.requests
        .iter()
        .filter(|request| request.url.contains("/v5/position/trading-stop"))
        .map(|request| serde_json::from_str(body(request)).unwrap())
        .collect()
}

#[test]
fn a_reference_gone_stale_during_the_proof_still_places_the_replacement() {
    let mut failures = Vec::new();
    for profile in PROFILES {
        for trigger in ["LastPrice", "MarkPrice", "IndexPrice"] {
            let shape = format!("{profile:?}/{trigger}");
            let mut h = ready(profile);
            seed(&mut h, profile, "Sell", "StopLoss", trigger, false, true);
            amend(&mut h, Some(95.0), None);
            cancel_then_stale_proof(&mut h, profile);
            let writes = stop_writes(&h);
            if writes.len() != 1 || writes[0]["stopLoss"] != "95" {
                failures.push(format!(
                    "{shape}: no replacement: {writes:?} {:?}",
                    h.result(91)
                ));
                continue;
            }
            h.respond("/v5/position/trading-stop", 200, OK);
            if !h.result(91).is_some_and(Result::is_ok) {
                failures.push(format!("{shape}: {:?}", h.result(91)));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_refused_replacement_with_a_stale_reference_puts_the_old_stop_back() {
    let mut failures = Vec::new();
    for profile in PROFILES {
        let shape = format!("{profile:?}");
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "LastPrice",
            false,
            true,
        );
        amend(&mut h, Some(95.0), None);
        cancel_then_stale_proof(&mut h, profile);
        if stop_writes(&h).len() != 1 {
            failures.push(format!("{shape}: no replacement: {:?}", h.result(91)));
            continue;
        }
        h.respond(
            "/v5/position/trading-stop",
            200,
            r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
        );
        h.advance(200.0);
        let writes = stop_writes(&h);
        if writes.len() != 1 || writes[0]["stopLoss"] != "90" {
            failures.push(format!(
                "{shape}: no put-back: {writes:?} {:?}",
                h.result(91)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
