//! A frame is provenance, never a receipt for an executable trigger reference.
use super::*;
use safe_cex_engine::{Outcome, Store, Venue};

fn page(working_type: &str, take_profit: bool) -> (Harness, Vec<u8>) {
    let orders = serde_json::json!({"code":0,"data":{"orders":[{
        "orderId":ORDER_ID,"symbol":"BTC-USDT",
        "type":if take_profit {"TAKE_PROFIT_MARKET"} else {"STOP_MARKET"},
        "side":"SELL","status":"NEW","origQty":"0.1","executedQty":"0",
        "stopPrice":if take_profit {"61000"} else {"59000"},
        "positionSide":"BOTH","reduceOnly":true,"workingType":working_type
    }]}})
    .to_string();
    let source = Harness::ready_with_orders(&orders);
    let mut store = Store::default();
    store.set_markets(source.engine.store().markets().to_vec());
    store.set_tickers(source.engine.store().tickers().to_vec());
    store.set_orders(source.engine.store().orders().to_vec());
    store.set_positions(source.engine.store().positions().to_vec());
    let pending = store.take_pending();
    let mut writer = wire::FrameWriter::new(Vec::new(), source.now);
    store.write(&pending, &mut writer);
    writer.venue_state(&source.engine.venue().export_venue_state().unwrap());
    let frame = writer.finish();
    let mut h = Harness::unstarted_with_mode(|provider| provider, true);
    h.respond("/quote/contracts", 200, MARKETS);
    h.engine.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    (h, frame)
}

fn amend(h: &mut Harness, take_profit: bool) {
    h.event(Event::Command(Command::AmendOrder {
        command: 900,
        id: ORDER_ID.into(),
        price: Some(if take_profit { 60900. } else { 59100. }),
        amount: None,
    }));
}

fn no_write(h: &Harness) {
    assert!(
        h.requests.iter().all(|r| r.method == Method::Get),
        "{:?}",
        h.requests
    );
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Open
    );
}

fn refused(h: &Harness) {
    no_write(h);
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 900).count(), 1);
    let error = &h.failures.iter().find(|(id, _)| *id == 900).unwrap().1;
    assert_eq!(error.outcome, Some(Outcome::NotSent), "{error:?}");
}

#[test]
fn fresh_public_reference_allows_frame_fed_sl_and_tp_exactly_once() {
    for (working_type, path, response) in [
        ("CONTRACT_PRICE", "/quote/ticker", DAILY),
        ("MARK_PRICE", "/quote/premiumIndex", PREMIUM),
    ] {
        for take_profit in [false, true] {
            let (mut h, _) = page(working_type, take_profit);
            amend(&mut h, take_profit);
            no_write(&h);
            assert!(h.done.is_empty(), "{:?}", h.done);
            let read = h.take(path);
            assert_eq!(read.method, Method::Get);
            assert!(
                !read.url.contains("symbol="),
                "reuse the streamed all-symbol endpoint"
            );
            assert!(!read.url.contains("signature"));
            assert!(
                !read
                    .headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("X-BX-APIKEY"))
            );
            assert!(h.requests.is_empty(), "one bounded public read only");
            h.reply(read.id, 200, response);
            assert!(h.failures.is_empty(), "{:?}; {:?}", h.failures, h.logs);
            let write = h.take("/trade/cancelReplace");
            assert_eq!(write.method, Method::Post);
            let body: serde_json::Value =
                serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["workingType"], working_type);
            assert_eq!(body["reduceOnly"], true);
            h.reply(write.id, 200, &format!(r#"{{"code":0,"data":{{"cancelOrderId":"{ORDER_ID}","cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":"replacement"}}}}"#));
            assert_eq!(h.done, vec![(900, Ok("[\"replacement\"]".into()))]);
            assert!(h.requests.iter().all(|r| r.method == Method::Get));
            h.reply(read.id, 200, response);
            assert_eq!(h.done.len(), 1, "replayed response cannot re-admit a write");
            assert!(h.requests.iter().all(|r| r.method == Method::Get));
        }
    }
}

#[test]
fn failed_sparse_wrong_source_or_refused_reads_never_reach_destructive_leg() {
    for (working_type, path, opposite) in [
        ("CONTRACT_PRICE", "/quote/ticker", PREMIUM),
        ("MARK_PRICE", "/quote/premiumIndex", DAILY),
    ] {
        for body in [
            r#"{"code":100400,"data":[]}"#,
            r#"{"code":109400,"msg":"timestamp outside recvWindow"}"#,
            r#"{"code":0,"data":{"symbol":"BTC-USDT","lastPrice":"60000"}}"#,
            r#"{"code":0,"data":[]}"#,
            r#"{"code":0,"data":[{"symbol":"BTC-USDT","time":1000}]}"#,
            r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","markPrice":"60001"}]}"#,
            opposite,
            &opposite.replace("BTC-USDT", "ETH-USDT"),
        ] {
            let (mut h, frame) = page(working_type, false);
            h.now += 6000.;
            h.engine.apply_frame(h.now, &frame, true).unwrap();
            amend(&mut h, false);
            no_write(&h);
            let read = h.take(path);
            h.reply(read.id, 200, body);
            refused(&h);
            assert!(h.requests.is_empty(), "no retry or cross-source fallback");
        }
        let (mut h, _) = page(working_type, false);
        amend(&mut h, false);
        let read = h.take(path);
        h.event(Event::HttpFailed {
            request: read.id,
            error: "offline",
        });
        refused(&h);
        assert!(h.requests.is_empty());
    }
}

#[test]
fn a_delayed_envelope_and_replayed_frame_cannot_redate_the_reference_row() {
    for (working_type, path, response) in [
        ("CONTRACT_PRICE", "/quote/ticker", DAILY),
        ("MARK_PRICE", "/quote/premiumIndex", PREMIUM),
    ] {
        let (mut h, frame) = page(working_type, false);
        amend(&mut h, false);
        let read = h.take(path);
        h.event(Event::HttpStart {
            request: read.id,
            status: 200,
            headers: "",
        });
        h.event(Event::HttpChunk {
            request: read.id,
            data: response.as_bytes(),
        });
        no_write(&h);
        h.now += 5001.;
        h.engine.apply_frame(h.now, &frame, true).unwrap();
        h.event(Event::HttpEnd { request: read.id });
        refused(&h);
        assert!(h.requests.is_empty());
    }
}

#[test]
fn public_reference_cannot_cross_a_retired_owner() {
    let (mut h, _) = page("CONTRACT_PRICE", false);
    amend(&mut h, false);
    let read = h.take("/quote/ticker");
    h.now += 20_001.;
    for (timer, _) in h.timers.clone() {
        h.event(Event::TimerFired { timer });
    }
    h.reply(read.id, 200, DAILY);
    refused(&h);
}

#[test]
fn successful_read_cannot_bypass_a_later_claim_veto_or_restart_preflight() {
    for replay_frame in [false, true] {
        let (mut h, frame) = page("CONTRACT_PRICE", false);
        amend(&mut h, false);
        let read = h.take("/quote/ticker");
        // Admit the write recipe but leave signing and the final Claim deferred.
        h.engine.handle(
            h.now,
            Event::HttpResponse {
                request: read.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: DAILY,
                },
            },
        );
        if replay_frame {
            h.engine.apply_frame(h.now, &frame, true).unwrap();
        } else {
            h.now += 5001.;
        }
        h.collect();
        refused(&h);
        assert!(h.requests.is_empty(), "the final veto cannot perform I/O");
    }
}

#[test]
fn crossed_nonpositive_nonfinite_and_duplicate_target_quotes_refuse_before_cancel() {
    for (working_type, path, body, field) in [
        ("CONTRACT_PRICE", "/quote/ticker", DAILY, "lastPrice"),
        ("MARK_PRICE", "/quote/premiumIndex", PREMIUM, "markPrice"),
    ] {
        for value in ["59100", "0", "-1", "NaN", "Infinity"] {
            let (mut h, _) = page(working_type, false);
            amend(&mut h, false);
            let read = h.take(path);
            let mut payload: serde_json::Value = serde_json::from_str(body).unwrap();
            payload["data"][0][field] = value.into();
            h.reply(read.id, 200, &payload.to_string());
            refused(&h);
            assert!(h.requests.is_empty());
        }
        let (mut h, _) = page(working_type, false);
        amend(&mut h, false);
        let read = h.take(path);
        let mut payload: serde_json::Value = serde_json::from_str(body).unwrap();
        let duplicate = payload["data"][0].clone();
        payload["data"].as_array_mut().unwrap().push(duplicate);
        h.reply(read.id, 200, &payload.to_string());
        refused(&h);
        assert!(h.requests.is_empty());
    }
}

#[test]
fn order_fill_or_position_change_while_reading_cannot_rebuild_the_edit() {
    for changed_order in [false, true] {
        let (mut h, _) = page("CONTRACT_PRICE", false);
        amend(&mut h, false);
        let read = h.take("/quote/ticker");
        if changed_order {
            let mut orders = h.engine.store().orders().to_vec();
            orders[0].filled = 0.05;
            orders[0].remaining = 0.05;
            h.engine.store_mut().set_orders(orders);
        } else {
            h.engine.store_mut().set_positions(Vec::new());
        }
        h.reply(read.id, 200, DAILY);
        assert!(h.requests.is_empty());
        assert_eq!(h.failures.len(), 1);
        assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
    }
}

fn two_order_page(working_type: &str) -> Harness {
    let rows: Vec<_> = [(ORDER_ID, "59000"), ("8389765519993909001", "58000")].into_iter()
        .map(|(id, price)| serde_json::json!({"orderId":id,"symbol":"BTC-USDT","type":"STOP_MARKET","side":"SELL",
            "status":"NEW","origQty":"0.1","executedQty":"0","stopPrice":price,"positionSide":"BOTH",
            "reduceOnly":true,"workingType":working_type})).collect();
    let source = Harness::ready_with_orders(
        &serde_json::json!({"code":0,"data":{"orders":rows}}).to_string(),
    );
    let mut store = Store::default();
    store.set_markets(source.engine.store().markets().to_vec());
    store.set_tickers(source.engine.store().tickers().to_vec());
    store.set_orders(source.engine.store().orders().to_vec());
    store.set_positions(source.engine.store().positions().to_vec());
    let pending = store.take_pending();
    let mut writer = wire::FrameWriter::new(Vec::new(), source.now);
    store.write(&pending, &mut writer);
    writer.venue_state(&source.engine.venue().export_venue_state().unwrap());
    let mut h = Harness::unstarted_with_mode(|provider| provider, true);
    h.respond("/quote/contracts", 200, MARKETS);
    h.engine.apply_frame(h.now, &writer.finish(), true).unwrap();
    h.collect();
    h
}

#[test]
fn fix_round_older_public_reply_never_overwrites_a_newer_receipt_after_a_frame() {
    for (working_type, path, old, newer, field) in [
        (
            "CONTRACT_PRICE",
            "/quote/ticker",
            DAILY,
            DAILY.replace("60000", "59000"),
            "last",
        ),
        (
            "MARK_PRICE",
            "/quote/premiumIndex",
            PREMIUM,
            PREMIUM.replace("60001", "59000"),
            "mark",
        ),
    ] {
        for frame in 0..3 {
            let mut h = two_order_page(working_type);
            amend(&mut h, false);
            let a = h.take(path);
            h.event(Event::HttpStart {
                request: a.id,
                status: 200,
                headers: "",
            });
            h.event(Event::HttpChunk {
                request: a.id,
                data: old.as_bytes(),
            });
            h.now += 1.;
            h.event(Event::Command(Command::AmendOrder {
                command: 901,
                id: "8389765519993909001".into(),
                price: Some(59100.),
                amount: None,
            }));
            let b = h.take(path);
            h.reply(b.id, 200, &newer);
            assert!(
                h.failures
                    .iter()
                    .any(|(id, error)| *id == 901 && error.outcome == Some(Outcome::NotSent))
            );
            assert!(h.requests.is_empty(), "B's crossed edit cannot write");
            h.now += 1.;
            if frame != 0 {
                let mut source = Store::default();
                let mut tickers = h.engine.store().tickers().to_vec();
                if field == "last" {
                    tickers[0].last = 60000.;
                } else {
                    tickers[0].mark = 60001.;
                }
                source.set_tickers(tickers);
                let pending = source.take_pending();
                let mut writer = wire::FrameWriter::new(Vec::new(), h.now);
                source.write(&pending, &mut writer);
                if frame == 2 {
                    writer.venue_state(&h.engine.venue().export_venue_state().unwrap());
                }
                h.engine
                    .apply_frame(h.now, &writer.finish(), false)
                    .unwrap();
            }
            h.event(Event::HttpEnd { request: a.id });
            refused(&h);
            assert!(h.requests.is_empty(), "{working_type} frame={frame}");
        }
    }
}
