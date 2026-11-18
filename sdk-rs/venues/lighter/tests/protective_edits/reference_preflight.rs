use super::*;
use safe_cex_engine::Method;

const MARK: &str = r#"{"code":200,"order_book_stats":[{"market_id":1,"mark_price":"100"}]}"#;

fn page(robinhood: bool, take_profit: bool) -> (Engine<Lighter>, Vec<u8>) {
    page_with_status(robinhood, take_profit, "mark-price")
}

fn page_with_status(
    robinhood: bool,
    take_profit: bool,
    status: &str,
) -> (Engine<Lighter>, Vec<u8>) {
    page_with_selector(robinhood, take_profit, status, "718895")
}

fn page_with_selector(
    robinhood: bool,
    take_profit: bool,
    status: &str,
    selector: &str,
) -> (Engine<Lighter>, Vec<u8>) {
    let mut opts = Options {
        robinhood,
        testnet: true,
        ..options()
    };
    let (mut source, feeds, _) = ready_feeds(opts.clone(), true);
    let mut row = original();
    row["type"] = json!(if take_profit { 4 } else { 2 });
    row["trigger_price"] = json!(if take_profit { "120" } else { "80" });
    row["price"] = json!(if take_profit { "119" } else { "79" });
    row["trigger_status"] = json!(status);
    message(
        &mut source,
        feeds.private,
        NOW,
        &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
    );
    mark(&mut source, &feeds, NOW, json!({"mark_price":"100"}));
    let frame = source.snapshot_frame(NOW).to_vec();
    opts.application_id = Some(selector.into());
    let mut e = engine(opts);
    e.set_frame_fed().unwrap();
    e.start(NOW);
    let catalogue = request(&mut e, NOW, "orderBookDetails");
    reply(&mut e, &catalogue, 200, META);
    safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    e.apply_frame(NOW, &frame, true).unwrap();
    (e, frame)
}

fn edit(e: &mut Engine<Lighter>, now: f64, tp: bool) {
    e.handle(
        now,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(if tp { 115. } else { 85. }),
            amount: Some(1.),
        }),
    );
}

fn read(e: &mut Engine<Lighter>, now: f64) -> HttpRequest {
    let out = safe_cex_engine::test_support::prepared_actions(e, now);
    let requests: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "{out:?}");
    let r = requests[0];
    assert!(r.url.ends_with("/api/v1/exchangeStats"), "{r:?}");
    assert_eq!(r.method, Method::Get);
    assert!(
        r.headers
            .iter()
            .all(|(h, _)| !h.eq_ignore_ascii_case("authorization"))
    );
    assert!(!r.url.contains("auth="));
    r.clone()
}

fn refused(e: &mut Engine<Lighter>, now: f64) {
    let out = safe_cex_engine::test_support::prepared_actions(e, now);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
    assert_eq!(out.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:8, error }) if error.outcome==Some(Outcome::NotSent))).count(), 1, "{out:?}");
    assert_eq!(
        e.store().order("42").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn fresh_public_mark_allows_frame_fed_sl_tp_once_on_both_deployments() {
    for robinhood in [false, true] {
        for tp in [false, true] {
            let (mut e, _) = page(robinhood, tp);
            edit(&mut e, NOW, tp);
            let reference = read(&mut e, NOW);
            reply(&mut e, &reference, 200, MARK);
            let nonce = request(&mut e, NOW, "nextNonce");
            reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
            let write = request(&mut e, NOW, "sendTx");
            assert_eq!(tx(&write)["TriggerPrice"], if tp { 11500 } else { 8500 });
            ack(&mut e, &write, NOW);
            let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
            assert_eq!(
                out.iter()
                    .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, .. })))
                    .count(),
                1,
                "{out:?}"
            );
            assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
            reply(&mut e, &reference, 200, MARK);
            assert!(safe_cex_engine::test_support::prepared_actions(&mut e, NOW).is_empty());
        }
    }
}

#[test]
fn failed_sparse_time_only_wrong_market_and_last_cannot_authorize_cancel() {
    for body in [
        r#"{"code":500,"order_book_stats":[]}"#,
        r#"{"code":200,"order_book_stats":[]}"#,
        r#"{"code":200,"order_book_stats":[{"market_id":1,"timestamp":1700000000000}]}"#,
        r#"{"code":200,"order_book_stats":[{"market_id":1,"last_trade_price":"100","index_price":"100"}]}"#,
        r#"{"code":200,"order_book_stats":[{"market_id":2,"mark_price":"100"}]}"#,
        r#"{"code":200,"order_book_stats":[{"market_id":1,"mark_price":"0"}]}"#,
        r#"{"code":200,"order_book_stats":[{"market_id":1,"mark_price":"NaN"}]}"#,
        r#"{"code":200,"order_book_stats":[{"market_id":1,"mark_price":"85"}]}"#,
        r#"{"code":200,"order_book_stats":[{"market_id":1,"mark_price":"100"},{"market_id":1,"mark_price":"100"}]}"#,
    ] {
        let (mut e, frame) = page(false, false);
        e.apply_frame(NOW + 6000., &frame, true).unwrap();
        edit(&mut e, NOW + 6000., false);
        let r = read(&mut e, NOW + 6000.);
        reply_at(&mut e, NOW + 6000., &r, 200, body);
        refused(&mut e, NOW + 6000.);
    }
    let (mut e, _) = page(false, false);
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "offline",
        },
    );
    refused(&mut e, NOW);
}

#[test]
fn delayed_envelope_and_replayed_frame_never_redate_the_mark() {
    let (mut e, frame) = page(false, false);
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    e.handle(
        NOW,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW,
        Event::HttpChunk {
            request: r.id,
            data: MARK.as_bytes(),
        },
    );
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, NOW).is_empty());
    e.apply_frame(NOW + 5001., &frame, true).unwrap();
    e.handle(NOW + 5001., Event::HttpEnd { request: r.id });
    refused(&mut e, NOW + 5001.);
}

#[test]
fn final_send_expiry_cannot_start_a_second_reference_read() {
    let (mut e, frame) = page(false, false);
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    reply(&mut e, &r, 200, MARK);
    let nonce = request(&mut e, NOW, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    e.apply_frame(NOW + 5001., &frame, true).unwrap();
    refused(&mut e, NOW + 5001.);
}

#[test]
fn changed_order_or_position_during_the_public_read_never_sends() {
    for filled in [false, true] {
        let (mut e, _) = page(false, false);
        edit(&mut e, NOW, false);
        let r = read(&mut e, NOW);
        if filled {
            e.store_mut().update_order("42", |o| {
                o.filled = 0.1;
                o.remaining -= 0.1;
            });
        } else {
            e.store_mut().set_positions(vec![]);
        }
        reply(&mut e, &r, 200, MARK);
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("sendTx"))),
            "{out:?}"
        );
        // A non-destructive nonce seed may precede the unchanged final guard.
        if let Some(nonce) = out.iter().find_map(|a| match a {
            Action::Http(r) if r.url.contains("nextNonce") => Some(r),
            _ => None,
        }) {
            reply(&mut e, nonce, 200, r#"{"code":200,"nonce":10}"#);
            refused(&mut e, NOW);
        } else {
            assert!(
                out.iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. }))),
                "{out:?}"
            );
        }
    }
}

#[test]
fn timed_out_public_reference_cannot_resume_from_a_late_response() {
    let (mut e, _) = page(false, false);
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    reply_at(&mut e, NOW + 20_001., &r, 200, MARK);
    refused(&mut e, NOW + 20_001.);
}

#[test]
fn public_snapshot_cannot_overwrite_a_newer_genuine_native_mark() {
    let (mut e, feeds, _) = boot();
    amend(&mut e, NOW + 6000.);
    let r = read(&mut e, NOW + 6000.);
    e.handle(
        NOW + 6000.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 6000.,
        Event::HttpChunk {
            request: r.id,
            data: MARK.as_bytes(),
        },
    );
    mark(&mut e, &feeds, NOW + 6001., json!({"mark_price":"85"}));
    e.handle(NOW + 6002., Event::HttpEnd { request: r.id });
    refused(&mut e, NOW + 6002.);
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 85.);
}

fn frame_replacement() -> (Engine<Lighter>, HttpRequest) {
    let (mut e, _) = page_with_status(false, false, "untriggered");
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    reply(&mut e, &r, 200, MARK);
    let nonce = request(&mut e, NOW, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let cancel = request(&mut e, NOW, "sendTx");
    ack(&mut e, &cancel, NOW);
    let mut timers = vec![];
    let out = advance(&mut e, &mut timers, NOW + 100.);
    let proof = out
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("accountOrders") => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{out:?}"));
    assert!(
        proof
            .headers
            .iter()
            .any(|(h, _)| h.eq_ignore_ascii_case("authorization"))
    );
    let mut row = original();
    row["status"] = json!("canceled");
    reply_at(
        &mut e,
        NOW + 100.,
        &proof,
        200,
        &json!({"code":200,"orders":[row]}).to_string(),
    );
    let replacement = request(&mut e, NOW + 100., "sendTx");
    (e, replacement)
}

#[test]
fn frame_fed_cancel_create_keeps_authenticated_proof_and_exact_put_back() {
    let (mut e, replacement) = frame_replacement();
    assert_eq!(tx(&replacement)["BaseAmount"], 1000);
    reply_at(
        &mut e,
        NOW + 100.,
        &replacement,
        200,
        r#"{"code":21702,"message":"refused"}"#,
    );
    let nonce = request(&mut e, NOW + 100., "nextNonce");
    reply_at(
        &mut e,
        NOW + 100.,
        &nonce,
        200,
        r#"{"code":200,"nonce":20}"#,
    );
    let restore = request(&mut e, NOW + 100., "sendTx");
    assert_eq!(tx(&restore)["BaseAmount"], 2000);
    assert_eq!(tx(&restore)["TriggerPrice"], 8000);
    assert_eq!(tx(&restore)["Price"], 7900);
}

#[test]
fn stale_mark_at_frame_fed_restore_never_sends_or_refetches() {
    let (mut e, replacement) = frame_replacement();
    reply_at(
        &mut e,
        NOW + 5001.,
        &replacement,
        200,
        r#"{"code":21702,"message":"refused"}"#,
    );
    let nonce = request(&mut e, NOW + 5001., "nextNonce");
    reply_at(
        &mut e,
        NOW + 5001.,
        &nonce,
        200,
        r#"{"code":200,"nonce":20}"#,
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 5001.);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
    assert!(
        out.iter().any(
            |a| matches!(a, Action::Notice(Notice::CommandFailed{command:8,error})
        if error.protection==Some(safe_cex_engine::error::Protection::Unprotected))
        ),
        "{out:?}"
    );
}

#[test]
fn older_native_mark_cannot_mask_a_newer_crossed_public_quote() {
    let (mut e, feeds, _) = boot();
    amend(&mut e, NOW + 6000.);
    let r = read(&mut e, NOW + 6000.);
    mark(&mut e, &feeds, NOW + 6001., json!({"mark_price":"100"}));
    reply_at(
        &mut e,
        NOW + 6002.,
        &r,
        200,
        &MARK.replace("\"100\"", "\"85\""),
    );
    refused(&mut e, NOW + 6002.);
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 85.);
}

#[test]
fn contradictory_frame_after_the_public_row_cannot_be_overwritten_at_envelope_completion() {
    let (mut e, _) = page(false, false);
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    e.handle(
        NOW,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW,
        Event::HttpChunk {
            request: r.id,
            data: MARK.as_bytes(),
        },
    );
    let mut source = safe_cex_engine::Store::default();
    let mut ticker = e.store().ticker("ETH").unwrap().clone();
    ticker.mark = 85.;
    source.set_tickers(vec![ticker]);
    let pending = source.take_pending();
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), NOW + 1.);
    source.write(&pending, &mut writer);
    e.apply_frame(NOW + 1., &writer.finish(), false).unwrap();
    e.handle(NOW + 2., Event::HttpEnd { request: r.id });
    refused(&mut e, NOW + 2.);
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 85.);
}

#[test]
fn a_replayed_pre_modify_frame_cannot_regrant_retired_native_provenance() {
    for unknown in [false, true] {
        let (mut e, frame) = page(false, false);
        edit(&mut e, NOW, false);
        let r = read(&mut e, NOW);
        reply(&mut e, &r, 200, MARK);
        let nonce = request(&mut e, NOW, "nextNonce");
        reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
        let modify = request(&mut e, NOW, "sendTx");
        if unknown {
            reply(&mut e, &modify, 429, r#"{"code":23000,"message":"rate"}"#);
        } else {
            ack(&mut e, &modify, NOW);
        }
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        e.apply_frame(NOW + 1., &frame, true).unwrap();
        e.handle(
            NOW + 2.,
            Event::Command(Command::AmendOrder {
                command: 9,
                id: "42".into(),
                price: Some(84.),
                amount: None,
            }),
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 2.);
        assert!(
            !out.iter().any(|a| matches!(a, Action::Http(_))),
            "{unknown}: {out:?}"
        );
        assert!(
            out.iter().any(
                |a| matches!(a, Action::Notice(Notice::CommandFailed{command:9,error})
            if error.outcome==Some(Outcome::NotSent))
            ),
            "{unknown}: {out:?}"
        );
    }
}

#[test]
fn a_foreign_account_frame_cannot_grant_protective_provenance() {
    use safe_cex_engine::Venue;
    let (mut e, _) = page(false, false);
    let mut state = e.venue().export_venue_state().unwrap();
    state[2..10].copy_from_slice(&718_896_u64.to_le_bytes());
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), NOW);
    writer.venue_state(&state);
    e.apply_frame(NOW, &writer.finish(), false).unwrap();
    edit(&mut e, NOW, false);
    refused(&mut e, NOW);
}

fn address_page() -> Engine<Lighter> {
    page_with_selector(
        false,
        false,
        "mark-price",
        "0x1111111111111111111111111111111111111111",
    )
    .0
}

fn account_frame(e: &mut Engine<Lighter>, account: u64) {
    use safe_cex_engine::Venue;
    let state = e.venue().export_venue_state().unwrap();
    account_frame_from(e, state, account);
}

fn account_frame_from(e: &mut Engine<Lighter>, mut state: Vec<u8>, account: u64) {
    state[2..10].copy_from_slice(&account.to_le_bytes());
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), NOW);
    writer.venue_state(&state);
    e.apply_frame(NOW, &writer.finish(), false).unwrap();
}

#[test]
fn address_receiver_same_account_reference_control() {
    let mut e = address_page();
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    account_frame(&mut e, 718_895);
    reply(&mut e, &r, 200, MARK);
    let nonce = request(&mut e, NOW, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let write = request(&mut e, NOW, "sendTx");
    assert_eq!(tx(&write)["AccountIndex"], 718_895);
    ack(&mut e, &write, NOW);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, .. })))
            .count(),
        1,
        "{out:?}"
    );
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
}

#[test]
fn address_receiver_foreign_frame_before_edit_cannot_rebind_account() {
    use safe_cex_engine::Venue;
    let mut e = address_page();
    account_frame(&mut e, 718_896);
    let state = e.venue().export_venue_state().unwrap();
    assert_eq!(
        u64::from_le_bytes(state[2..10].try_into().unwrap()),
        718_895
    );
    edit(&mut e, NOW, false);
    refused(&mut e, NOW);
}

#[test]
fn address_receiver_foreign_frame_during_reference_retires_edit() {
    use safe_cex_engine::Venue;
    let mut e = address_page();
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    let state = e.venue().export_venue_state().unwrap();
    account_frame_from(&mut e, state.clone(), 718_896);
    account_frame_from(&mut e, state, 718_895);
    reply(&mut e, &r, 200, MARK);
    refused(&mut e, NOW);
    let state = e.venue().export_venue_state().unwrap();
    assert_eq!(
        u64::from_le_bytes(state[2..10].try_into().unwrap()),
        718_895
    );
    reply(&mut e, &r, 200, MARK);
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, NOW).is_empty());
}

#[test]
fn address_receiver_foreign_frame_after_signing_cannot_revive_final_claim() {
    use safe_cex_engine::{
        Venue,
        request_context::{HttpAction, Prepared},
    };
    let mut e = address_page();
    let state = e.venue().export_venue_state().unwrap();
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    reply(&mut e, &r, 200, MARK);
    let nonce = request(&mut e, NOW, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    e.prepare_dispatch(NOW);
    let out = e.drain_actions().collect::<Vec<_>>();
    let prep = out
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.contains("sendTx") => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("{out:?}"));
    e.http_prepared(
        NOW,
        prep,
        Prepared {
            id: prep.attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    account_frame_from(&mut e, state.clone(), 718_896);
    account_frame_from(&mut e, state, 718_895);
    assert!(e.claim_http(NOW, permit).is_err());
    refused(&mut e, NOW);
}

#[test]
fn address_receiver_foreign_frame_after_handoff_retains_the_accepted_write_fact() {
    use safe_cex_engine::Venue;
    let mut e = address_page();
    let state = e.venue().export_venue_state().unwrap();
    edit(&mut e, NOW, false);
    let r = read(&mut e, NOW);
    reply(&mut e, &r, 200, MARK);
    let nonce = request(&mut e, NOW, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let write = request(&mut e, NOW, "sendTx");
    assert_eq!(tx(&write)["AccountIndex"], 718_895);
    account_frame_from(&mut e, state.clone(), 718_896);
    account_frame_from(&mut e, state, 718_895);
    ack(&mut e, &write, NOW);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, .. })))
            .count(),
        1,
        "{out:?}"
    );
    assert!(
        !out.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { .. }) | Action::Http(_)
        )),
        "{out:?}"
    );
}
