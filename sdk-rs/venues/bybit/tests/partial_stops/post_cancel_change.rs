//! AGT-57: a fill or a changed amount on the old partial stop, visible in the private
//! stream before the post-cancel proof completes, refuses the replacement on every
//! profile. It never sends a replacement sized for the original.
use super::*;

fn page(profile: Profile, rows: Vec<serde_json::Value>) -> String {
    serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":rows,"nextPageCursor":""}})
        .to_string()
}

/// Bybit's private row for the old stop after our cancel landed, showing `qty`/`cumExecQty`.
fn changed_frame(h: &mut Harness, profile: Profile, status: &str, qty: &str, filled: &str) {
    let category = profile.category();
    let symbol = profile.symbol();
    let leaves = (qty.parse::<f64>().unwrap() - filled.parse::<f64>().unwrap()).to_string();
    let leaves = if status == "Untriggered" {
        leaves.as_str()
    } else {
        "0"
    };
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"{category}","symbol":"{symbol}","orderId":"partial","side":"Sell","orderType":"Market","orderStatus":"{status}","cancelType":"CancelByUser","stopOrderType":"StopLoss","tpslMode":"Partial","qty":"{qty}","cumExecQty":"{filled}","cumExecValue":"0","avgPrice":"0","price":"0","leavesQty":"{leaves}","triggerPrice":"90","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0,"updatedTime":"{}"}}]}}"#, h.now as u64 + 50));
}

#[test]
fn a_fill_or_changed_amount_seen_before_the_proof_refuses_the_replacement_on_every_profile() {
    let mut failures = Vec::new();
    for profile in PROFILES {
        for (control, status, qty, filled) in [
            ("fill", "Cancelled", "7", "1"),
            ("fill", "Deactivated", "7", "1"),
            ("changed-amount", "Cancelled", "8", "0"),
            ("changed-amount", "Untriggered", "8", "0"),
            ("fill-and-amount", "Cancelled", "8", "1"),
        ] {
            let shape = format!("{profile:?}/{control}/{status}/qty={qty}/filled={filled}");
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            let original = h.engine.store().order("partial").unwrap().clone();
            amend(&mut h, Some(95.0), None);
            h.respond(
                profile.cancel_path(),
                200,
                r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
            );
            changed_frame(&mut h, profile, status, qty, filled);
            let target = native_partial_target(&h, profile, &original);
            h.respond("&orderId=partial", 200, &page(profile, vec![target]));
            for _ in 0..6 {
                if h.has_request("/v5/position/trading-stop") || h.result(91).is_some() {
                    break;
                }
                if h.has_request("&openOnly=0&limit=50") {
                    h.respond("&openOnly=0&limit=50", 200, &page(profile, vec![]));
                } else if h.has_request("/v5/order/history") {
                    h.respond("/v5/order/history", 200, &page(profile, vec![]));
                } else if h.has_request("&orderId=partial") {
                    h.respond("&orderId=partial", 200, &page(profile, vec![]));
                } else {
                    h.advance(200.0);
                }
            }
            let writes: Vec<String> = h
                .requests
                .iter()
                .filter(|request| request.url.contains("/v5/position/trading-stop"))
                .map(|request| body(request).to_owned())
                .collect();
            if !writes.is_empty() {
                failures.push(format!("{shape}: replacement sent: {writes:?}"));
                continue;
            }
            match h.result(91) {
                Some(Err(error)) if error.contains("position has no stop") => {}
                other => failures.push(format!(
                    "{shape}: must refuse naming the lost stop: {other:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
