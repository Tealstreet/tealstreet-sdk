mod support;
use safe_cex_engine::model::{OrderStatus, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, Notice, Response};
use support::*;
fn order() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000}"#,
    )
    .unwrap()
}
fn reply(e: &mut TestEngine, r: &safe_cex_engine::HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn ready() -> TestEngine {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e
}
fn results(e: &mut TestEngine) -> Vec<Result<String, String>> {
    e.drain_actions()
        .filter_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn every_caller_id_rejects_the_whole_batch_before_a_write() {
    for id in ["mine", ""] {
        let mut e = ready();
        let mut invalid = order();
        invalid.client_order_id = Some(id.into());
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order(), invalid],
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone {result:Err(error),..}) if error=="mexc cannot honor a caller clientOrderId")));
    }
    assert!(serde_json::from_str::<PlaceOrderOpts>(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000,"clientOrderId":null}"#).is_err());
}

#[test]
fn unsupported_opening_conditional_and_trailing_reject_before_any_batch_write() {
    use safe_cex_engine::model::OrderType;
    for kind in [OrderType::Conditional, OrderType::TrailingStopLoss] {
        for prefix in [false, true] {
            let mut e = ready();
            let mut invalid = order();
            invalid.kind = kind;
            invalid.trigger_price = Some(61000.0);
            let orders = if prefix {
                vec![order(), invalid]
            } else {
                vec![invalid]
            };
            e.handle(
                200.0,
                Event::Command(Command::PlaceOrders { command: 1, orders }),
            );
            let actions: Vec<_> = e.drain_actions().collect();
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(e.store().orders().is_empty());
            assert!(actions.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandDone { result: Err(error), .. })
                if error == "MEXC opening conditional and trailing-stop placement is not implemented"
            )));
        }
    }
}

#[test]
fn denied_batch_falls_back_once_and_paces_subsequent_writes() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), order()],
        }),
    );
    let batch = requests(&mut e).remove(0);
    assert!(batch.url.ends_with("/submit_batch"));
    assert!(batch.body.as_ref().unwrap().contains("\"stpMode\":0"));
    assert!(batch.body.as_ref().unwrap().contains("\"vol\":60.0"));
    reply(
        &mut e,
        &batch,
        250.0,
        r#"{"success":false,"code":511,"message":"You do not have access to the interface"}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1700.0, Event::TimerFired { timer });
    let first = requests(&mut e).remove(0);
    assert!(first.url.ends_with("/submit"));
    reply(
        &mut e,
        &first,
        1800.0,
        r#"{"success":true,"code":0,"data":"one"}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(3200.0, Event::TimerFired { timer });
    let second = requests(&mut e).remove(0);
    reply(
        &mut e,
        &second,
        3300.0,
        r#"{"success":true,"code":0,"data":"two"}"#,
    );
    assert_eq!(results(&mut e), [Ok("[\"one\",\"two\"]".into())]);
    e.handle(
        5000.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![order()],
        }),
    );
    assert!(requests(&mut e)[0].url.ends_with("/submit"));
}

#[test]
fn lost_write_response_never_retries_and_finishes_the_command() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    e.handle(
        300.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost after acceptance",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 1,
            result: Err(_)
        })
    )));
}

#[test]
fn late_placement_response_preserves_the_newer_stream_order() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#,
    );
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    assert_eq!(e.store().order("one").unwrap().filled, 0.001);
}

#[test]
fn partial_batch_success_and_cancel_failures_preserve_known_venue_state() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0},{"errorCode":600,"errorMsg":"bad volume"}]}"#,
    );
    assert!(
        results(&mut e)[0]
            .as_ref()
            .unwrap_err()
            .contains("\"partialOrderIds\":[\"one\"]")
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
    e.handle(
        400.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["one".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert_eq!(r.body.as_deref(), Some("[\"one\"]"));
    reply(
        &mut e,
        &r,
        500.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":600,"errorMsg":"cannot cancel"}]}"#,
    );
    assert!(results(&mut e)[0].is_err());
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}

#[test]
fn leverage_updates_use_native_position_id_and_ignore_failed_envelopes() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
        }),
    );
    let first = requests(&mut e).remove(0);
    assert!(first.body.as_ref().unwrap().contains("\"positionType\":1"));
    reply(
        &mut e,
        &first,
        300.0,
        r#"{"success":false,"code":600,"message":"rejected"}"#,
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert!(results(&mut e)[0].is_err());
}

#[test]
fn plan_orders_use_seven_day_market_triggers_and_distinct_cancel_routes() {
    let mut e = ready();
    let mut opts = order();
    opts.kind = safe_cex_engine::model::OrderType::StopLoss;
    opts.side = safe_cex_engine::model::OrderSide::Sell;
    opts.price = Some(55000.0);
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/planorder/place/v2"));
    let body = r.body.as_ref().unwrap();
    assert!(body.contains("\"executeCycle\":2"));
    assert!(body.contains("\"side\":4"));
    assert!(body.contains("\"triggerType\":2"));
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":"plan"}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        400.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["plan".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/planorder/cancel"));
    assert_eq!(
        r.body.as_deref(),
        Some("[{\"symbol\":\"BTC_USDT\",\"orderId\":\"plan\"}]")
    );
}

#[test]
fn queued_margin_change_retains_the_leverage_accepted_before_it() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
        }),
    );
    let first = requests(&mut e).remove(0);
    e.handle(
        210.0,
        Event::Command(Command::SetMarginType {
            command: 2,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    assert!(requests(&mut e).is_empty());
    reply(&mut e, &first, 300.0, r#"{"success":true,"code":0}"#);
    let second = requests(&mut e).remove(0);
    reply(&mut e, &second, 400.0, r#"{"success":true,"code":0}"#);
    let margin = requests(&mut e).remove(0);
    let body = margin.body.unwrap();
    assert!(body.contains("\"leverage\":10.0"));
    assert!(body.contains("\"openType\":1"));
}

#[test]
fn mutation_ack_does_not_overwrite_newer_mode_or_position_stream_state() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::SetPositionMode {
            command: 1,
            hedged: false,
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.position.mode","data":{"positionMode":1}}"#,
    );
    reply(&mut e, &r, 300.0, r#"{"success":true,"code":0}"#);
    assert!(e.store().hedged());
    e.drain_actions().for_each(drop);
    e.handle(
        400.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.position","data":{"positionId":"native","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}}"#,
    );
    reply(&mut e, &r, 500.0, r#"{"success":true,"code":0}"#);
    assert_eq!(e.store().positions()[0].leverage, 7.0);
}

#[test]
fn sized_stop_chooses_the_closing_position_rail_without_reduce_only_flag() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::FetchLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"positionType":1,"leverage":10,"openType":2},{"positionType":2,"leverage":15,"openType":2}]}"#,
    );
    e.drain_actions().for_each(drop);
    let mut opts = order();
    opts.kind = safe_cex_engine::model::OrderType::StopLoss;
    opts.side = safe_cex_engine::model::OrderSide::Sell;
    opts.price = Some(55000.0);
    e.handle(
        400.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.body.unwrap().contains("\"leverage\":10.0"));
}

#[test]
fn amend_recomputes_remaining_quantity_after_fills_during_cancel() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let original = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#;
    ws(&mut e, socket, original);
    e.drain_actions().for_each(drop);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &original.replace("\"dealVol\":10", "\"dealVol\":30"),
    );
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    assert!(read.url.ends_with("/order/get/one"));
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
    let replace = requests(&mut e).remove(0);
    assert!(replace.body.unwrap().contains("\"vol\":30.0"));
}

#[test]
fn stale_terminal_amend_read_cannot_erase_a_newer_partial_fill_or_replace_it() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let original = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#;
    ws(&mut e, socket, original);
    e.drain_actions().for_each(drop);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &original.replace("\"dealVol\":10", "\"dealVol\":40"),
    );
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
    assert!((e.store().order("one").unwrap().filled - 0.004).abs() < 1e-12);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone {result:Err(e),..}) if e.contains("Stale"))));
}

#[test]
fn filled_order_is_neither_replaced_nor_changed_to_canceled_by_cancel_ack() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let original = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#;
    ws(&mut e, socket, original);
    e.drain_actions().for_each(drop);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &original
            .replace("\"dealVol\":0", "\"dealVol\":60")
            .replace("\"state\":2", "\"state\":3"),
    );
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}

#[test]
fn optimistic_quantity_matches_minimum_snapped_submission() {
    let mut e = ready();
    let mut opts = order();
    opts.amount = 0.1;
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.body.as_ref().unwrap().contains("\"vol\":1.0"));
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    assert_eq!(
        e.store()
            .order("one")
            .unwrap()
            .extra
            .as_ref()
            .unwrap()
            .remaining_native,
        Some(1.0)
    );
}

#[test]
fn open_position_writes_keep_native_ids_and_full_stops_have_their_own_route() {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        let body = if r.url.contains("open_positions") {
            r#"{"success":true,"code":0,"data":[{"positionId":739113577038255616,"symbol":"BTC_USDT","positionType":1,"positionMode":2,"holdVol":60,"holdAvgPrice":60000,"leverage":10,"openType":1}]}"#
        } else {
            r#"{"success":true,"code":0,"data":[]}"#
        };
        response(&mut e, &r, body);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 5.0,
        }),
    );
    let r = requests(&mut e).remove(0);
    assert_eq!(
        r.body.as_deref(),
        Some("{\"positionId\":\"739113577038255616\",\"leverage\":5.0}")
    );
    reply(&mut e, &r, 300.0, r#"{"success":true,"code":0}"#);
    let r = requests(&mut e).remove(0);
    reply(&mut e, &r, 400.0, r#"{"success":true,"code":0}"#);
    e.drain_actions().for_each(drop);
    let mut opts = order();
    opts.kind = safe_cex_engine::model::OrderType::StopLoss;
    opts.side = safe_cex_engine::model::OrderSide::Sell;
    opts.amount = 0.0;
    opts.price = Some(55000.0);
    e.handle(
        500.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/stoporder/place"));
    let body = r.body.as_ref().unwrap();
    assert!(body.contains("\"positionId\":\"739113577038255616\""));
    assert!(body.contains("\"vol\":60.0"));
    assert!(body.contains("\"volType\":2"));
    reply(
        &mut e,
        &r,
        600.0,
        r#"{"success":true,"code":0,"data":[{"id":"full"}]}"#,
    );
    e.drain_actions().for_each(drop);
    assert_eq!(e.store().order("full").unwrap().amount, 0.0);
    e.handle(
        700.0,
        Event::Command(Command::CancelOrders {
            command: 3,
            ids: vec!["full".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/stoporder/cancel"));
    assert_eq!(r.body.as_deref(), Some("[{\"stopPlanOrderId\":\"full\"}]"));
}

#[test]
fn amend_reads_final_quantity_when_fill_frames_arrive_after_cancel_response() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#,
    );
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    assert_eq!(read.method, safe_cex_engine::Method::Get);
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
    let replacement = requests(&mut e).remove(0);
    assert_eq!(replacement.method, safe_cex_engine::Method::Post);
    assert!(replacement.body.unwrap().contains("\"vol\":30.0"));
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
}

#[test]
fn short_stream_update_does_not_suppress_the_long_leverage_acknowledgement() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let long = r#"{"channel":"push.personal.position","data":{"positionId":"long","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":10,"openType":1}}"#;
    ws(&mut e, socket, long);
    e.handle(
        300.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 5.0,
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &long
            .replace("\"long\"", "\"short\"")
            .replace("\"positionType\":1", "\"positionType\":2")
            .replace("\"leverage\":10", "\"leverage\":7"),
    );
    reply(&mut e, &r, 400.0, r#"{"success":true,"code":0}"#);
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.side == safe_cex_engine::model::PositionSide::Long)
            .unwrap()
            .leverage,
        5.0
    );
}

#[test]
fn definitive_filled_amend_read_updates_local_state_and_withholds_replacement() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#,
    );
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":60,"price":59000,"state":3}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    assert_eq!(e.store().order("one").unwrap().filled, 0.006);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn amend_retains_attached_bracket_prices_and_independent_sources_across_lifecycles() {
    use safe_cex_engine::model::StopTriggerType;
    for lifecycle in 0..3 {
        let (mut e, socket, reads, _) = private_started();
        for r in reads {
            response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
        }
        e.drain_actions().for_each(drop);
        let raw = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#;
        if lifecycle < 2 {
            let mut opts = order();
            opts.stop_loss = Some(55000.0);
            opts.take_profit = Some(65000.0);
            opts.tp_trigger_type = Some(StopTriggerType::MarkPrice);
            opts.sl_trigger_type = Some(StopTriggerType::IndexPrice);
            e.handle(
                200.0,
                Event::Command(Command::PlaceOrders {
                    command: 1,
                    orders: vec![opts],
                }),
            );
            let place = requests(&mut e).remove(0);
            if lifecycle == 1 {
                ws(&mut e, socket, raw);
            }
            reply(
                &mut e,
                &place,
                300.0,
                r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
            );
            if lifecycle == 0 {
                ws(&mut e, socket, raw);
            }
        } else {
            ws(&mut e,socket,&raw.replace("\"state\":2","\"state\":2,\"stopLossPrice\":55000,\"takeProfitPrice\":65000,\"lossTrend\":3,\"profitTrend\":2"));
        }
        let stored = e.store().order("one").unwrap();
        assert_eq!(stored.stop_loss, Some(55000.0));
        assert_eq!(stored.take_profit, Some(65000.0));
        e.drain_actions().for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::AmendOrder {
                command: 2,
                id: "one".into(),
                price: Some(58000.0),
                amount: None,
            }),
        );
        let cancel = requests(&mut e).remove(0);
        reply(
            &mut e,
            &cancel,
            2100.0,
            r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
        );
        let read = requests(&mut e).remove(0);
        reply(
            &mut e,
            &read,
            2200.0,
            r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":4,"stopLossPrice":55000,"takeProfitPrice":65000}}"#,
        );
        let replacement = requests(&mut e).remove(0);
        let body = replacement.body.unwrap();
        for expected in [
            "\"stopLossPrice\":\"55000\"",
            "\"takeProfitPrice\":\"65000\"",
            "\"lossTrend\":3",
            "\"profitTrend\":2",
        ] {
            assert!(body.contains(expected), "{body}");
        }
    }
}

#[test]
fn plan_stop_amend_keeps_latest_native_source_after_ws() {
    use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, StopTriggerType};
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let opts = PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::StopLoss,
        side: OrderSide::Sell,
        amount: 60.0,
        price: Some(55000.0),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: Some(true),
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: Some(StopTriggerType::MarkPrice),
        client_order_id: None,
        close: None,
        stp_mode: None,
    };
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let place = requests(&mut e).remove(0);
    assert!(place.body.as_ref().unwrap().contains("\"trend\":2"));
    response(&mut e, &place, r#"{"success":true,"code":0,"data":"plan"}"#);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.plan.order","data":{"id":"plan","symbol":"BTC_USDT","side":4,"orderType":5,"vol":60,"triggerPrice":55000,"triggerType":2,"trend":3,"state":1}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "plan".into(),
            price: Some(54000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    e.handle(
        2100.0,
        Event::HttpResponse {
            request: cancel.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"code":0}"#,
            },
        },
    );
    let replacement = requests(&mut e).remove(0);
    assert!(
        replacement.body.as_ref().unwrap().contains("\"trend\":3"),
        "{}",
        replacement.body.unwrap()
    );
}

#[test]
fn native_bracket_clear_before_placement_ack_remains_authoritative() {
    use safe_cex_engine::model::StopTriggerType;
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let mut opts = order();
    opts.stop_loss = Some(55000.0);
    opts.take_profit = Some(65000.0);
    opts.tp_trigger_type = Some(StopTriggerType::IndexPrice);
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let place = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2,"stopLossPrice":0,"takeProfitPrice":66000,"profitTrend":2}}"#,
    );
    // A later sparse update must retain the explicit clear's authority.
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#,
    );
    reply(
        &mut e,
        &place,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let stored = e.store().order("one").unwrap();
    assert_eq!(stored.stop_loss, None);
    assert_eq!(stored.take_profit, Some(66000.0));
    assert_eq!(
        stored.extra.as_ref().unwrap().tp_trigger_type,
        Some(StopTriggerType::MarkPrice)
    );
}
