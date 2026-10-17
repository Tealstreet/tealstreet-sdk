//! Synced-clock model timestamps through actual constructors, signing and REST.
#[path = "support/account_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use fixture::*;
use harness::*;
use safe_cex_types::ExchangeName;
use serde_json::json;

struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        let clock = &safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
        clock.sync_from_server_time((clock.now)());
    }
}

#[test]
fn native_synced_clock_reaches_order_and_position_models() {
    let _reset = ResetClock;
    let clock = &safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    clock.sync_from_server_time((clock.now)() + 40_000.0);
    let mut failures = Vec::new();
    for (name, uta) in [
        (ExchangeName::Bitget, false),
        (ExchangeName::Bitgetusdc, false),
        (ExchangeName::Bitgetinverse, false),
        (ExchangeName::Bitget, true),
        (ExchangeName::Bitgetusdc, true),
    ] {
        let h = harness(name.clone(), options(), |http| {
            if uta {
                http.ok(
                    "GET",
                    "/api/v3/account/settings",
                    json!({"holdMode":"one_way_mode"}),
                );
                http.ok("GET", MARKETS, json!([market_row()]));
                http.ok(
                    "GET",
                    "/api/v3/position/current-position",
                    json!({"list":[position_row()]}),
                );
                http.ok(
                    "GET",
                    "/api/v3/trade/unfilled-orders",
                    json!({"list":[order_row()]}),
                );
                http.ok(
                    "GET",
                    "/api/v3/trade/unfilled-strategy-orders",
                    json!({"list":[]}),
                );
            } else {
                classic(http);
                if name == ExchangeName::Bitgetinverse {
                    for (_, path, replies) in http.routes.borrow_mut().iter_mut() {
                        if path == MARKETS {
                            for (_, body) in replies {
                                body["data"][0]["quoteCoin"] = json!("USD");
                            }
                        }
                    }
                }
            }
        });
        let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let observed = errors.clone();
        h.venue().base().on(
            "error",
            std::rc::Rc::new(move |_, error| {
                observed.borrow_mut().push(error.clone());
                Ok(())
            }),
        );
        loaded(&h);
        let before = clock.get_current_time_ms();
        let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
        let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
        let after = clock.get_current_time_ms();
        assert_eq!(
            positions.len(),
            1,
            "{name:?} UTA={uta} actual position reply; errors={:?}",
            errors.borrow()
        );
        assert_eq!(orders.len(), 1, "{name:?} UTA={uta} actual order reply");
        for (kind, stamp) in [
            ("position", positions[0].last_update.value().copied()),
            ("order", Some(orders[0].last_update)),
        ] {
            if !stamp.is_some_and(|stamp| (before..=after).contains(&stamp)) {
                failures.push(format!("{name:?} UTA={uta} {kind}: {stamp:?}"));
            }
        }
        assert!(
            h.http
                .requests
                .borrow()
                .iter()
                .any(|r| r.headers["ACCESS-SIGN"].is_string())
        );
        assert!(h.runtime.reports.borrow().is_empty());
        h.venue().dispose().unwrap();
    }
    assert!(
        failures.is_empty(),
        "native models must use synced clock: {failures:?}"
    );
}
