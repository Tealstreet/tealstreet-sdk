//! Oracles: Bitget's financial-records example/schema and explicit native types:
//! https://www.bitget.com/docs/catalog/account/assets-balance#get-financial-records
//! https://www.bitget.com/docs/uta/enum#type-financial-records
//! UTA amount is kept as the raw signed ledger change; no Classic gross-PnL
//! assumption is inherited. CCXT's Bitget parseLedgerEntry independently uses
//! before = balance - amount and preserves fee separately:
//! https://github.com/ccxt/ccxt/blob/master/ts/src/bitget.ts
use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_engine::{Command, Event, Query};
use safe_cex_venue_bitget::Product;
use serde_json::{Value, json};
use support::Harness;

fn uta() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h
}
fn query(h: &mut Harness, symbol: Option<&str>, kind: Option<&str>) {
    h.command(Command::Query {
        command: 42,
        query: Query::Income {
            symbol: symbol.map(Into::into),
            kind: kind.map(Into::into),
            from_ms: None,
            to_ms: None,
            limit: Some(500),
        },
    });
}
fn bill(id: &str, kind: &str, amount: &str, fee: &str) -> Value {
    // Documented schema, changing category/type to this futures lane's enums.
    json!({"category":"USDT-FUTURES","id":id,"symbol":"BTCUSDT","coin":"USDT","type":kind,"positionType":"crossed","fee":fee,"positionAmount":"0.001","positionBalance":"0.001","amount":amount,"balance":"55.10017801","ts":"1745853486185"})
}
fn page(h: &mut Harness, rows: Value, cursor: &str) {
    h.respond(
        "/financial-records",
        200,
        &json!({"code":"00000","data":{"list":rows,"cursor":cursor}}).to_string(),
    );
}
fn result(h: &Harness) -> Value {
    serde_json::from_str(
        h.done
            .iter()
            .find(|(id, _)| *id == 42)
            .unwrap()
            .1
            .as_ref()
            .unwrap(),
    )
    .unwrap()
}
fn failed(h: &Harness) -> bool {
    h.done.iter().any(|(id, r)| *id == 42 && r.is_err())
}

#[test]
fn documented_schema_keeps_native_delta_fee_asset_and_exact_id() {
    let mut h = uta();
    query(&mut h, None, None);
    let request = h.take("/financial-records");
    assert!(request.url.contains("/api/v3/account/financial-records?"));
    assert!(request.url.contains("category=USDT-FUTURES"));
    assert!(request.url.contains("limit=100"));
    assert!(!request.url.contains("productType="));
    assert!(request.headers.iter().any(|(key, _)| *key == "ACCESS-SIGN"));
    h.reply(request.id,200,&json!({"code":"00000","data":{"list":[bill("13111111111111111","CLOSE_LONG","0.00531168","-0.00000531")],"cursor":""}}).to_string());
    let rows = result(&h);
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["tranId"], "13111111111111111:other");
    assert_eq!(rows[0]["incomeType"], "CLOSE_LONG");
    assert_eq!(rows[0]["income"], 0.00531168);
    assert_eq!(rows[0]["fee"], -0.00000531);
    assert_eq!(rows[0]["category"], "other");
    assert_eq!(rows[0]["asset"], "USDT");
    assert_eq!(rows[0]["timestamp"], 1745853486185.0);
}

#[test]
fn explicit_types_classify_funding_fee_transfer_and_unknown_without_sign_rewrites() {
    let mut h = uta();
    query(&mut h, None, None);
    page(
        &mut h,
        json!([
            bill("1", "CONTRACT_MAIN_SETTLE_FEE_USER_OUT", "-2", "0"),
            bill("2", "MARGIN_SETTLE_FEE_USER_IN", "3", "0"),
            bill("3", "ORDER_PLF_FEE_OUT", "-0.3", "-0.3"),
            bill("4", "TRANSFER_OUT", "-5", "0"),
            bill("5", "FUTURE_NATIVE_TYPE", "9", "0.1"),
            bill("6", "OPEN_LONG", "0", "-0.1")
        ]),
        "",
    );
    let rows = result(&h);
    assert_eq!(rows.as_array().unwrap().len(), 6);
    for (i, category, amount) in [
        (0, "funding", -2.0),
        (1, "funding", 3.0),
        (2, "commission", -0.3),
        (3, "transfer", -5.0),
        (4, "other", 9.0),
        (5, "other", 0.0),
    ] {
        assert_eq!(rows[i]["category"], category);
        assert_eq!(rows[i]["income"], amount);
    }
    assert_eq!(rows[4]["fee"], 0.1);
    assert_eq!(rows[5]["fee"], -0.1);
}

#[test]
fn cursor_walks_short_pages_filters_and_deduplicates_without_losing_other_assets() {
    let mut h = uta();
    query(
        &mut h,
        Some("btcusdt"),
        Some("CONTRACT_MAIN_SETTLE_FEE_USER_IN"),
    );
    let row = bill(
        "9007199254740993",
        "CONTRACT_MAIN_SETTLE_FEE_USER_IN",
        "1",
        "0",
    );
    page(&mut h, json!([row]), "9007199254740992");
    assert!(h.done.iter().all(|(id, _)| *id != 42));
    h.timer_after(100.0);
    let request = h.take("/financial-records");
    assert!(request.url.contains("cursor=9007199254740992"));
    assert!(!request.url.contains("symbol="));
    assert!(!request.url.contains("type="));
    let mut other_asset = bill("2", "CONTRACT_MAIN_SETTLE_FEE_USER_IN", "0.2", "0");
    other_asset["coin"] = json!("BTC");
    let mut other_product = row.clone();
    other_product["category"] = json!("SPOT");
    let mut other_symbol = row.clone();
    other_symbol["symbol"] = json!("ETHUSDT");
    h.reply(request.id,200,&json!({"code":"00000","data":{"list":[row,other_asset,other_product,other_symbol,bill("3","TRANSFER_IN","2","0")],"cursor":""}}).to_string());
    let rows = result(&h);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[1]["asset"], "BTC");
    assert_eq!(rows[1]["income"], 0.2);
}

#[test]
fn empty_page_finishes_even_with_repeated_cursor_and_nonempty_cycle_fails() {
    for empty in [true, false] {
        let mut h = uta();
        query(&mut h, None, None);
        let row = bill("1", "TRANSFER_IN", "1", "0");
        page(&mut h, json!([row]), "1");
        h.timer_after(100.0);
        page(&mut h, if empty { json!([]) } else { json!([row]) }, "1");
        if empty {
            assert_eq!(result(&h).as_array().unwrap().len(), 1);
        } else {
            assert!(failed(&h));
        }
    }
}

#[test]
fn malformed_rows_lists_and_business_failures_never_become_partial_success() {
    let mut invalid = Vec::new();
    for (field, value) in [
        ("amount", json!("NaN")),
        ("fee", json!("oops")),
        ("ts", json!("0")),
        ("id", json!("")),
        ("type", json!("")),
        ("coin", json!("")),
    ] {
        let mut row = bill("1", "TRANSFER_IN", "1", "0");
        row[field] = value;
        invalid.push(json!({"code":"00000","data":{"list":[row],"cursor":""}}).to_string());
    }
    invalid.extend([
        r#"{"code":"00000","data":{}}"#.into(),
        r#"{"code":"00000","data":{"list":null}}"#.into(),
        r#"{"code":"00000","data":{"list":[],"cursor":{}}}"#.into(),
        r#"{"code":"429","msg":"rate limited","data":null}"#.into(),
    ]);
    for body in invalid {
        let mut h = uta();
        query(&mut h, None, None);
        for retry in 0..4 {
            h.respond("/financial-records", 200, &body);
            if retry < 3 {
                h.timer_after(250.0 * f64::from(retry + 1));
            }
        }
        assert!(failed(&h), "accepted {body}");
    }
}

#[test]
fn failed_second_page_retries_same_cursor_then_recovers_without_duplicate_rows() {
    let mut h = uta();
    query(&mut h, None, None);
    page(&mut h, json!([bill("1", "TRANSFER_IN", "1", "0")]), "next");
    h.timer_after(100.0);
    let request = h.take("/financial-records");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "offline",
    });
    h.timer_after(250.0);
    let request = h.take("/financial-records");
    assert!(request.url.contains("cursor=next"));
    h.reply(
        request.id,
        200,
        &json!({"code":"00000","data":{"list":[bill("2","TRANSFER_IN","2","0")],"cursor":""}})
            .to_string(),
    );
    assert_eq!(result(&h).as_array().unwrap().len(), 2);
}

#[test]
fn income_range_windows_remain_nonoverlapping_and_reset_cursor() {
    let mut h = uta();
    h.command(Command::Query {
        command: 42,
        query: Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(0.0),
            to_ms: Some(2_592_000_000.0),
            limit: None,
        },
    });
    let request = h.take("/financial-records");
    assert!(request.url.contains("startTime=0"));
    assert!(request.url.contains("endTime=2591999999"));
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
    );
    h.timer_after(100.0);
    let request = h.take("/financial-records");
    assert!(request.url.contains("startTime=2592000000"));
    assert!(!request.url.contains("cursor="));
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
    );
    assert_eq!(result(&h), json!([]));
}

#[test]
fn usdc_category_does_not_relabel_returned_currency() {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        product: Product::Usdc,
        ..Options::default()
    });
    h.respond(
        "/contracts",
        200,
        &support::CONTRACTS.replace("USDT", "USDC"),
    );
    h.respond("/tickers", 200, &support::TICKERS.replace("USDT", "USDC"));
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    query(&mut h, None, None);
    let request = h.take("/financial-records");
    assert!(request.url.contains("category=USDC-FUTURES"));
    let mut row = bill("1", "TRANSFER_IN", "1", "0");
    row["category"] = json!("USDC-FUTURES");
    row["coin"] = json!("USDC");
    h.reply(
        request.id,
        200,
        &json!({"code":"00000","data":{"list":[row],"cursor":""}}).to_string(),
    );
    assert_eq!(result(&h)[0]["asset"], "USDC");
}

#[test]
fn streamed_ledger_decodes_across_every_byte_boundary() {
    let mut h = uta();
    query(&mut h, None, None);
    let request = h.take("/financial-records").id;
    let body = json!({"code":"00000","data":{"list":[bill("9007199254740993","CLOSE_LONG","12.25","-0.05")],"cursor":""}}).to_string();
    h.event(Event::HttpStart {
        request,
        status: 200,
        headers: "",
    });
    for byte in body.as_bytes() {
        h.event(Event::HttpChunk {
            request,
            data: std::slice::from_ref(byte),
        });
    }
    assert!(h.done.iter().all(|(id, _)| *id != 42));
    h.event(Event::HttpEnd { request });
    assert_eq!(result(&h)[0]["tranId"], "9007199254740993:other");
    assert_eq!(result(&h)[0]["income"], 12.25);
}

#[test]
fn late_uta_signal_promotes_income_and_failed_later_page_discards_partial_results() {
    let mut h = Harness::classic();
    query(&mut h, None, None);
    h.respond(
        "/account/bill",
        200,
        r#"{"code":"40085","msg":"Classic Account API is not supported","data":null}"#,
    );
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    h.timer_after(100.0);
    page(&mut h, json!([bill("1", "TRANSFER_IN", "1", "0")]), "next");
    h.timer_after(100.0);
    for retry in 0..4 {
        let request = h.take("/financial-records");
        assert!(request.url.contains("cursor=next"));
        h.event(Event::HttpFailed {
            request: request.id,
            error: "offline",
        });
        if retry < 3 {
            h.timer_after(250.0 * f64::from(retry + 1));
        }
    }
    assert!(failed(&h));
    assert!(
        h.done
            .iter()
            .filter(|(id, _)| *id == 42)
            .all(|(_, r)| r.is_err())
    );
}
