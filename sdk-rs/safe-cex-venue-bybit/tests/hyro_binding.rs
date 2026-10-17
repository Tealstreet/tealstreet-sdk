//! Production HyroTrader facade, with the same build-time origins as the source.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::ExchangeName;

fn configured() -> bool {
    safe_cex_venue_bybit::types::HYROTRADER_SIMULATOR_URL["livenet"]
        .as_str()
        .is_some_and(|value| !value.is_empty())
}

#[test]
fn hyro_constructor_matches_source_configuration_boundary() {
    let result = support::try_harness_as(ExchangeName::Hyrotrader, support::options(), |_| {});
    if configured() {
        let h = result.unwrap_or_else(|error| panic!("configured source constructor: {error}"));
        assert_eq!(h.venue().class_name(), "HyrotraderExchange");
        assert!(h.venue().base().is_demo);
        assert!(!h.venue().base().is_spot);
        assert!(h.wire.requests.borrow().is_empty());
        assert!(h.runtime.sockets.borrow().is_empty());
    } else {
        let error = match result {
            Ok(_) => panic!("unconfigured source HyroTrader constructor must refuse"),
            Err(error) => error,
        };
        assert_eq!(
            error.message,
            "HyroTrader simulator REST URL is not configured"
        );
    }
}

#[test]
fn hyro_configured_facade_preserves_metadata_and_mode_refusals() {
    if !configured() {
        // Cargo tracks option_env! inputs. The parent process retains its empty
        // source environment; this child exercises an independently built venue.
        assert!(std::env::var_os("BYBIT_HYRO_CONFIGURED_CHILD").is_none());
        let output =
            std::process::Command::new(std::env::var_os("CARGO").unwrap_or("cargo".into()))
                .args([
                    "test",
                    "--locked",
                    "--manifest-path",
                    concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"),
                    "-p",
                    "safe-cex-venue-bybit",
                    "--test",
                    "hyro_binding",
                    "--",
                    "--exact",
                    "hyro_configured_facade_preserves_metadata_and_mode_refusals",
                    "--nocapture",
                ])
                .env("BYBIT_HYRO_CONFIGURED_CHILD", "1")
                .env(
                    "NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL",
                    "https://hyro.example",
                )
                .env(
                    "NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL",
                    "wss://hyro.example/v5/public/linear",
                )
                .env(
                    "NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL",
                    "wss://hyro.example/v5/private",
                )
                .output()
                .expect("configured native HyroTrader witness");
        assert!(
            output.status.success(),
            "configured native witness failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        return;
    }
    let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |_| {});
    assert_eq!(h.venue().class_name(), "HyrotraderExchange");
    assert!(h.venue().base().is_demo);
    assert_eq!(
        h.drive(h.venue().get_account()).unwrap_err().message,
        "HyroTrader does not provide account identity metadata"
    );
    h.drive(h.venue().change_position_mode(false, None))
        .unwrap();
    assert_eq!(
        h.drive(h.venue().change_position_mode(true, None))
            .unwrap_err()
            .message,
        "HyroTrader supports one-way position mode only"
    );
    h.drive(h.venue().change_margin_type(false, None)).unwrap();
    assert_eq!(
        h.drive(h.venue().change_margin_type(true, None))
            .unwrap_err()
            .message,
        "HyroTrader supports cross margin only"
    );
    assert!(
        h.wire.requests.borrow().is_empty(),
        "source metadata/mode guards do not send HTTP"
    );
    h.venue().dispose().unwrap();
}
