use safe_cex_venue_hyperliquid::{constants::*, spot_constants};
use serde_json::Value;
use std::borrow::Cow;

fn source() -> &'static Value {
    static SOURCE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/builder-constants.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "builder source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect()
}
fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}
fn number(bits: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap())
}
fn bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    }
}

#[test]
fn native_exported_constants_match_actual_source() {
    let source = source();
    let core = &source["constants"];
    let spot = &source["spotConstants"];
    assert_eq!(
        core.as_object().unwrap().len(),
        10,
        "BUILDER core source constant floor"
    );
    assert_eq!(
        spot.as_object().unwrap().len(),
        3,
        "BUILDER spot source constant floor"
    );
    let mut failures = Vec::new();
    let mut checked = 0;
    // Each root still executes if another root fails. The combined constant
    // control must expose all 13 named failures, not only the first panic.
    macro_rules! root {
        ($name:literal, $body:block) => {{
            checked += 1;
            if std::panic::catch_unwind(|| $body).is_err() {
                failures.push($name);
            }
        }};
    }
    root!("core.singularSubscriptionTypes", {
        assert_eq!(
            SINGULAR_SUBSCRIPTION_TYPES,
            strings(&core["singularSubscriptionTypes"]),
            "BUILDER native core.singularSubscriptionTypes"
        );
    });
    root!("core.systemSubscriptionTypes", {
        assert_eq!(
            SYSTEM_SUBSCRIPTION_TYPES,
            strings(&core["systemSubscriptionTypes"]),
            "BUILDER native core.systemSubscriptionTypes"
        );
    });
    root!("core.MAINNET_API_URL", {
        assert_eq!(
            MAINNET_API_URL,
            core["MAINNET_API_URL"].as_str().unwrap(),
            "BUILDER native core.MAINNET_API_URL"
        );
    });
    root!("core.TEALSTREET_BUILDER_ADDRESS_MAINNET", {
        assert_eq!(
            TEALSTREET_BUILDER_ADDRESS_MAINNET,
            core["TEALSTREET_BUILDER_ADDRESS_MAINNET"].as_str().unwrap(),
            "BUILDER native core.TEALSTREET_BUILDER_ADDRESS_MAINNET"
        );
    });
    root!("core.TEALSTREET_BUILDER_ADDRESS_TESTNET", {
        assert_eq!(
            TEALSTREET_BUILDER_ADDRESS_TESTNET,
            core["TEALSTREET_BUILDER_ADDRESS_TESTNET"].as_str().unwrap(),
            "BUILDER native core.TEALSTREET_BUILDER_ADDRESS_TESTNET"
        );
    });
    root!("core.TEALSTREET_BUILDER_FEE", {
        assert_eq!(
            TEALSTREET_BUILDER_FEE.to_bits(),
            core["TEALSTREET_BUILDER_FEE"].as_f64().unwrap().to_bits(),
            "BUILDER native core.TEALSTREET_BUILDER_FEE"
        );
    });
    root!("core.HYPERLIQUID_BUILDER_TIERS", {
        assert_eq!(
            HYPERLIQUID_BUILDER_TIERS.len(),
            3,
            "BUILDER native core.HYPERLIQUID_BUILDER_TIERS length"
        );
        let expected: Vec<_> = core["HYPERLIQUID_BUILDER_TIERS"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.parse::<f64>().unwrap(), value.as_str().unwrap()))
            .collect();
        assert_eq!(
            HYPERLIQUID_BUILDER_TIERS, expected,
            "BUILDER native core.HYPERLIQUID_BUILDER_TIERS"
        );
    });
    root!("core.SPOT_ASSET_INDEX_OFFSET", {
        assert_eq!(
            SPOT_ASSET_INDEX_OFFSET as u64,
            core["SPOT_ASSET_INDEX_OFFSET"].as_u64().unwrap(),
            "BUILDER native core.SPOT_ASSET_INDEX_OFFSET"
        );
    });
    root!("core.PERP_MAX_PRICE_DECIMALS", {
        assert_eq!(
            PERP_MAX_PRICE_DECIMALS as u64,
            core["PERP_MAX_PRICE_DECIMALS"].as_u64().unwrap(),
            "BUILDER native core.PERP_MAX_PRICE_DECIMALS"
        );
    });
    root!("core.SPOT_MAX_PRICE_DECIMALS", {
        assert_eq!(
            SPOT_MAX_PRICE_DECIMALS as u64,
            core["SPOT_MAX_PRICE_DECIMALS"].as_u64().unwrap(),
            "BUILDER native core.SPOT_MAX_PRICE_DECIMALS"
        );
    });
    root!("spot.singularSubscriptionTypes", {
        assert_eq!(
            spot_constants::SINGULAR_SUBSCRIPTION_TYPES,
            strings(&spot["singularSubscriptionTypes"]),
            "BUILDER native spot.singularSubscriptionTypes"
        );
    });
    root!("spot.systemSubscriptionTypes", {
        assert_eq!(
            spot_constants::SYSTEM_SUBSCRIPTION_TYPES,
            strings(&spot["systemSubscriptionTypes"]),
            "BUILDER native spot.systemSubscriptionTypes"
        );
    });
    root!("spot.MAINNET_API_URL", {
        assert_eq!(
            spot_constants::MAINNET_API_URL,
            spot["MAINNET_API_URL"].as_str().unwrap(),
            "BUILDER native spot.MAINNET_API_URL"
        );
    });
    assert_eq!(checked, 13, "BUILDER native constant root coverage");
    assert!(
        failures.is_empty(),
        "BUILDER named native constant failures: {failures:?}"
    );
}

#[test]
fn native_builder_functions_preserve_pair_bits_utf16_and_reference() {
    let source = source();
    let network = source["network"].as_array().unwrap();
    let lookups = source["lookups"].as_array().unwrap();
    let known = source["known"].as_array().unwrap();
    let rates = source["rates"].as_array().unwrap();
    let configs = source["configs"].as_array().unwrap();
    assert_eq!(
        [
            network.len(),
            lookups.len(),
            known.len(),
            rates.len(),
            configs.len()
        ],
        [2, 38, 43, 13, 9],
        "BUILDER independent source category totals"
    );
    for index in 0..2 {
        let row = network.get(index).expect("BUILDER native network index");
        assert_eq!(
            get_builder_address(row["testnet"].as_bool().unwrap()),
            row["address"].as_str().unwrap(),
            "BUILDER native network address {index}"
        );
    }
    for index in 0..38 {
        let row = lookups.get(index).expect("BUILDER native lookup index");
        assert_eq!(
            hyperliquid_builder_fee_tenths_bps(&units(&row["address"])),
            row["fee"].as_f64(),
            "BUILDER native address fee {index}"
        );
    }
    for index in 0..43 {
        let row = known.get(index).expect("BUILDER native known-pair index");
        assert_eq!(
            is_known_hyperliquid_builder(&units(&row["address"]), number(&row["feeBits"])),
            row["value"].as_bool().unwrap(),
            "BUILDER native known address/rate pair {index}"
        );
    }
    for index in 0..13 {
        let row = rates.get(index).expect("BUILDER native rate index");
        assert_eq!(
            builder_fee_rate_from_tenths_bps(number(&row["feeBits"])),
            row["value"].as_str().unwrap(),
            "BUILDER native rate text {index}"
        );
    }
    for index in 0..9 {
        let row = configs.get(index).expect("BUILDER native config index");
        let options = if row["options"].is_null() {
            None
        } else {
            let builder = &row["options"]["builder"];
            Some(HyperliquidBuilderOptions {
                testnet: row["options"]["testnet"].as_bool(),
                hyperliquid_builder: (!builder.is_null()).then(|| HyperliquidBuilderConfig {
                    address: units(&builder["address"]),
                    fee_tenths_bps: number(&builder["feeBits"]),
                }),
            })
        };
        let native = resolve_hyperliquid_builder(options.as_ref()).unwrap();
        assert_eq!(
            native.address,
            units(&row["value"]["address"]),
            "BUILDER native config address {index}"
        );
        assert_eq!(
            bits(native.fee_tenths_bps),
            bits(number(&row["value"]["feeBits"])),
            "BUILDER native config fee bits {index}"
        );
        match &native {
            Cow::Borrowed(builder) => {
                assert!(
                    row["sameReference"].as_bool().unwrap(),
                    "BUILDER native borrowed reference {index}"
                );
                assert!(
                    std::ptr::eq(
                        *builder,
                        options
                            .as_ref()
                            .unwrap()
                            .hyperliquid_builder
                            .as_ref()
                            .unwrap()
                    ),
                    "BUILDER native configured identity {index}"
                );
            }
            Cow::Owned(_) => assert!(
                !row["sameReference"].as_bool().unwrap(),
                "BUILDER native fresh fallback {index}"
            ),
        }
    }
}

#[test]
fn native_resolver_preserves_getter_order_errors_and_early_return() {
    struct Provider<'a> {
        mode: &'a str,
        builder: HyperliquidBuilderConfig,
        reads: std::cell::RefCell<Vec<&'static str>>,
    }
    impl HyperliquidBuilderProvider for Provider<'_> {
        type Error = &'static str;
        fn hyperliquid_builder(
            &self,
        ) -> Result<Option<Cow<'_, HyperliquidBuilderConfig>>, Self::Error> {
            self.reads.borrow_mut().push("builder");
            if self.mode == "builder-error" {
                return Err("builder getter failed");
            }
            Ok((self.mode == "provided").then_some(Cow::Borrowed(&self.builder)))
        }
        fn testnet(&self) -> Result<Option<bool>, Self::Error> {
            self.reads.borrow_mut().push("testnet");
            if self.mode == "net-error" || self.mode == "provided" {
                return Err("testnet getter failed");
            }
            Ok(Some(self.mode == "fallback-testnet"))
        }
    }
    let rows = source()["effects"].as_array().unwrap();
    assert_eq!(rows.len(), 5, "BUILDER source getter floor");
    for index in 0..5 {
        let row = rows.get(index).expect("BUILDER native getter index");
        let provider = Provider {
            mode: row["mode"].as_str().unwrap(),
            builder: HyperliquidBuilderConfig {
                address: HYPERLIQUID_BUILDER_TIERS[1].1.encode_utf16().collect(),
                fee_tenths_bps: 30.0,
            },
            reads: std::cell::RefCell::new(Vec::new()),
        };
        match resolve_hyperliquid_builder(Some(&provider)) {
            Ok(native) => {
                assert!(
                    row.get("error").is_none(),
                    "BUILDER native getter success {index}"
                );
                assert_eq!(
                    native.address,
                    units(&row["address"]),
                    "BUILDER native getter address {index}"
                );
                assert_eq!(
                    bits(native.fee_tenths_bps),
                    bits(number(&row["feeBits"])),
                    "BUILDER native getter fee {index}"
                );
                assert_eq!(
                    matches!(native, Cow::Borrowed(_)),
                    row["sameReference"].as_bool().unwrap(),
                    "BUILDER native getter identity {index}"
                );
            }
            Err(error) => assert_eq!(
                error,
                row["error"].as_str().unwrap(),
                "BUILDER native getter error identity {index}"
            ),
        }
        assert_eq!(
            *provider.reads.borrow(),
            strings(&row["reads"]),
            "BUILDER native getter read order {index}"
        );
    }
}
