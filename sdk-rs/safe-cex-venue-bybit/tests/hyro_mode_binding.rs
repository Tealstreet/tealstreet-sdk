//! Source mode/URL policy through real construction, signed REST and socket dials.
#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_exchange::socket_lifecycle::SocketSlot;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Order};
use serde_json::{Value, json};

#[test]
fn hyro_configured_mode_profiles_match_source() {
    use std::{
        fs,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    // A single serialized child build at a time: origins are source bundle inputs.
    for (profile, rest, public, private, simulator) in [
        (
            "production-derived",
            " HTTPS://prod-mode.example/// ",
            "",
            "",
            "https://sim-mode.example",
        ),
        (
            "production-overrides",
            "https://prod-mode.example",
            " wss://prod-public.example/// ",
            " wss://prod-private.example/// ",
            "https://sim-mode.example",
        ),
        (
            "simulator-backed-production",
            "",
            "wss://unused-prod.example",
            "wss://unused-prod.example",
            "http://sim-mode.example:8012/",
        ),
    ] {
        let log_path =
            std::env::temp_dir().join(format!("hyro-mode-{}-{profile}.log", std::process::id()));
        let log = fs::File::create(&log_path).unwrap();
        let mut child = Command::new(std::env::var_os("CARGO").unwrap_or("cargo".into()))
            .args([
                "test",
                "--locked",
                "--manifest-path",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"),
                "-p",
                "safe-cex-venue-bybit",
                "--jobs",
                "1",
                "--test",
                "hyro_mode_binding",
                "hyro_mode_profile_matches_source",
                "--",
                "--exact",
                "--nocapture",
            ])
            .env("NEXT_PUBLIC_HYROTRADER_PROD_REST_URL", rest)
            .env("NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL", public)
            .env("NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL", private)
            .env("NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL", simulator)
            .env("NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL", "")
            .env("NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL", "")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(600);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{profile} timed out: {}", log_path.display());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = fs::read_to_string(&log_path).unwrap();
        assert!(status.success(), "{profile}: {output}");
        assert!(output.contains("1 passed; 0 failed"), "{profile}: {output}");
    }
}

#[test]
fn hyro_mode_profile_matches_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/hyro_mode_source.cjs"
        ))
        .output()
        .expect("maintained TS constructor and hooks");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        let mut raw = serde_json::to_value(support::options()).unwrap();
        if row["mode"] != "absent" {
            raw["extra"] = json!({"hyrotraderMode":row["mode"]});
        }
        let options = serde_json::from_value(raw).unwrap();
        let result = support::try_harness_as(ExchangeName::Hyrotrader, options, |wire| {
            wire.ok(
                "GET",
                "/v5/order/realtime",
                json!({"list":[],"nextPageCursor":""}),
            );
            wire.ok("POST", "/v5/order/cancel-all", json!({}));
        });
        if let Some(error) = row["rest"].get("error") {
            let error = error.as_str().unwrap();
            assert_eq!(
                result.err().expect("source configuration refusal").message,
                error
            );
            continue;
        }
        let h = result.unwrap_or_else(|e| panic!("mode {}: {}", row["mode"], e.message));
        assert_eq!(
            h.venue().base().is_demo,
            row["isDemo"].as_bool().unwrap(),
            "mode {} source account classification",
            row["mode"]
        );
        h.drive(h.venue().fetch_orders(None)).unwrap();
        assert_eq!(h.wire.calls("/v5/order/realtime").len(), 1);
        assert_eq!(
            h.wire.calls("/v5/order/realtime")[0]
                .config
                .base_url
                .as_deref(),
            row["rest"].as_str()
        );

        // BaseWebSocket connects through the actual per-venue URL selectors.
        for (slot, key) in [
            (SocketSlot::Public, "public"),
            (SocketSlot::Private, "private"),
        ] {
            let socket = h.venue().base().get_socket(slot).unwrap();
            socket.connect_and_subscribe(false).unwrap();
            h.runtime.pump();
            assert!(
                h.runtime
                    .sockets
                    .borrow()
                    .iter()
                    .any(|(url, _)| url.split('?').next() == row[key].as_str()),
                "mode {} {key}: {:?}",
                row["mode"],
                h.runtime
                    .sockets
                    .borrow()
                    .iter()
                    .map(|(url, _)| url)
                    .collect::<Vec<_>>()
            );
        }
        let order: Order = serde_json::from_value(json!({"id":"mode-order","symbol":"BTCUSDT","status":"open","type":"limit","side":"buy","price":1,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"timeInForce":"GoodTillCancel","lastUpdate":1})).unwrap();
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                orders: Some(vec![order]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        h.drive(h.venue().cancel_symbol_orders("BTCUSDT".into()))
            .unwrap();
        assert_eq!(row["removed"], json!([{"id":"mode-order"}]));
        assert!(
            h.store.orders().unwrap().is_empty(),
            "Hyro production retains the local REST cancellation fallback"
        );
        h.venue().dispose().unwrap();
    }
}
