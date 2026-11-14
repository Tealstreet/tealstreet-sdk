use super::*;
use safe_cex_engine::{Command, Notice, Outcome};

fn amend_refused(r: &mut Rig, id: &str) {
    r.actions.clear();
    r.e.handle(
        r.now,
        Event::Command(Command::AmendOrder {
            command: 90,
            id: id.into(),
            price: Some(61000.),
            amount: None,
        }),
    );
    r.collect();
    assert!(
        r.http
            .iter()
            .all(|request| !request.url.ends_with("private/edit")),
        "linked native identity must never dispatch an ordinary edit"
    );
    assert!(r.actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:90,error}) if error.outcome==Some(Outcome::NotSent))));
}
fn poll_orders(r: &mut Rig, rows: Value) {
    for request in r.poll_requests() {
        if request.url.ends_with("get_open_orders") {
            r.rpc(request, rows.clone());
        } else if request.url.ends_with("get_positions") {
            r.rpc(request, json!([position_row()]));
        } else {
            r.rpc(request, account_row());
        }
    }
    r.collect();
}
#[test]
fn acknowledged_bracket_remains_non_amendable_after_snapshot_or_stream_refresh() {
    for stream in [false, true] {
        let mut r = Rig::ready();
        let o=serde_json::from_value(json!({"symbol":"BTC_USDC-PERPETUAL","type":"limit","side":"buy","amount":0.1,"price":60000.,"clientOrderId":"bracket-entry","stopLoss":55000.,"takeProfit":65000.})).unwrap();
        // Use the Rust model for TIF so the fixture does not depend on its JSON spelling.
        let mut o: PlaceOrderOpts = o;
        o.time_in_force = Some(TimeInForce::PostOnly);
        r.e.handle(
            r.now,
            Event::Command(Command::PlaceOrders {
                command: 80,
                orders: vec![o],
            }),
        );
        let request = r.request();
        assert!(request.url.ends_with("private/buy"), "{}", request.url);
        let mut row = order_row();
        row["order_id"] = json!("USDC-bracket");
        row["label"] = json!("bracket-entry");
        r.rpc(request, json!({"order":row,"trades":[]}));
        r.collect();
        assert_eq!(
            r.e.store().order("USDC-bracket").unwrap().stop_loss,
            Some(55000.)
        );
        row["last_update_timestamp"] = json!(1700000002000u64);
        if stream {
            let socket = r.subscribe();
            r.notification(socket, CHANNELS[0], json!({"orders":[row]}));
        } else {
            poll_orders(&mut r, json!([row]));
        }
        amend_refused(&mut r, "USDC-bracket");
    }
}
#[test]
fn discovered_linkage_survives_omission_and_snapshot_readmission() {
    for (field, value) in [
        ("oto_order_ids", json!(["USDC-child"])),
        ("is_primary_otoco", json!(true)),
        ("is_secondary_oto", json!(true)),
        ("primary_order_id", json!("USDC-parent")),
        ("oco_ref", json!("synthetic-group")),
    ] {
        let mut r = Rig::ready();
        let mut row = order_row();
        row[field] = value;
        poll_orders(&mut r, json!([row]));
        amend_refused(&mut r, "USDC-123");
        poll_orders(&mut r, json!([]));
        assert!(r.e.store().orders().is_empty());
        poll_orders(&mut r, json!([order_row()]));
        amend_refused(&mut r, "USDC-123");
    }
}

#[test]
fn stream_linkage_is_retained_but_invalid_siblings_cannot_commit_it() {
    for object_form in [false, true] {
        for invalid_sibling in [false, true] {
            let mut r = Rig::ready();
            let socket = r.subscribe();
            let mut linked = order_row();
            if object_form {
                linked["oto_order_ids"] = json!({"opaque-native-marker":true});
            } else {
                linked["is_primary_otoco"] = json!(true);
            }
            linked["last_update_timestamp"] = json!(1700000002000u64);
            let mut event = json!({"orders":[linked]});
            if invalid_sibling {
                event["positions"] = json!([{"instrument_name":"BTC_USDC-PERPETUAL"}]);
            }
            r.notification(socket, CHANNELS[0], event);
            if invalid_sibling {
                r.e.handle(
                    r.now,
                    Event::Command(Command::AmendOrder {
                        command: 90,
                        id: "USDC-123".into(),
                        price: Some(61000.),
                        amount: None,
                    }),
                );
                r.collect();
                assert!(
                    r.http
                        .iter()
                        .any(|request| request.url.ends_with("private/edit")),
                    "invalid event cannot partially install linkage on an ordinary order"
                );
            } else {
                amend_refused(&mut r, "USDC-123");
            }
        }
    }
}

#[test]
fn malformed_linkage_cannot_silently_become_an_ordinary_order() {
    for (field, value) in [
        ("is_primary_otoco", Value::Null),
        ("oto_order_ids", json!([42])),
        ("oco_ref", json!(false)),
    ] {
        let mut r = Rig::ready();
        let socket = r.subscribe();
        let before = r.e.store().order("USDC-123").unwrap().clone();
        let mut row = order_row();
        row[field] = value;
        row["filled_amount"] = json!(0.05);
        row["last_update_timestamp"] = json!(1700000002000u64);
        r.notification(socket, CHANNELS[0], json!({"orders":[row]}));
        assert_eq!(
            r.e.store().order("USDC-123"),
            Some(&before),
            "invalid linkage must reject the event atomically"
        );
    }
}

#[test]
fn linkage_discovered_while_auth_is_pending_refuses_the_queued_amendment() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let index = r
        .http
        .iter()
        .position(|request| request.url.ends_with("get_open_orders"))
        .unwrap();
    let request = r.http.remove(index).unwrap();
    r.rpc_error(request, 13009, "synthetic expired session");
    assert_eq!(r.e.venue().private_drb.socket, Some(socket));
    r.now += 1.; // A fresh exchange must begin strictly after invalidation.
    r.actions.clear();
    r.e.handle(
        r.now,
        Event::Command(Command::AmendOrder {
            command: 90,
            id: "USDC-123".into(),
            price: Some(61000.),
            amount: None,
        }),
    );
    r.collect();
    let index = r
        .http
        .iter()
        .position(|request| request.url.ends_with("public/auth"))
        .unwrap();
    let auth = r.http.remove(index).unwrap();
    assert!(
        r.http
            .iter()
            .all(|request| !request.url.ends_with("private/edit"))
    );
    let mut linked = order_row();
    linked["is_primary_otoco"] = json!(true);
    linked["last_update_timestamp"] = json!(1700000002000u64);
    r.notification(socket, CHANNELS[0], json!({"orders":[linked]}));
    r.rpc(auth,json!({"access_token":"fixture-refreshed","expires_in":900,"scope":"trade:read","token_type":"bearer"}));
    r.collect();
    assert!(
        r.http
            .iter()
            .all(|request| !request.url.ends_with("private/edit")),
        "dispatch must recheck linkage learned during auth"
    );
    assert_eq!(r.command_error(90).outcome, Some(Outcome::NotSent));
}

#[test]
fn uncertainty_learned_during_auth_refuses_a_queued_amendment_on_the_instrument() {
    let mut r = Rig::ready();
    r.subscribe();
    let mut other = r.e.store().order("USDC-123").unwrap().clone();
    other.id = "USDC-other".into();
    r.e.store_mut().upsert_order(other);
    r.e.handle(
        r.now,
        Event::Command(Command::CancelOrders {
            command: 80,
            ids: vec!["USDC-123".into()],
        }),
    );
    r.collect();
    let index = r
        .http
        .iter()
        .position(|request| request.url.ends_with("private/cancel"))
        .unwrap();
    let cancel = r.http.remove(index).unwrap();
    let index = r
        .http
        .iter()
        .position(|request| request.url.ends_with("get_open_orders"))
        .unwrap();
    let read = r.http.remove(index).unwrap();
    r.rpc_error(read, 13009, "synthetic expired session");
    r.now += 1.;
    r.e.handle(
        r.now,
        Event::Command(Command::AmendOrder {
            command: 90,
            id: "USDC-other".into(),
            price: Some(61000.),
            amount: None,
        }),
    );
    r.collect();
    let index = r
        .http
        .iter()
        .position(|request| request.url.ends_with("public/auth"))
        .unwrap();
    let auth = r.http.remove(index).unwrap();
    r.e.handle(
        r.now,
        Event::HttpFailed {
            request: cancel.id,
            error: "synthetic loss",
        },
    );
    r.collect();
    r.rpc(auth,json!({"access_token":"fixture-refreshed","expires_in":900,"scope":"trade:read","token_type":"bearer"}));
    r.collect();
    assert!(
        r.http
            .iter()
            .all(|request| !request.url.ends_with("private/edit")),
        "auth wait must not bypass newly learned instrument uncertainty"
    );
    assert_eq!(r.command_error(90).outcome, Some(Outcome::NotSent));
}
