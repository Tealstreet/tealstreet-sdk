use safe_cex_venue_hyperliquid::{agent::*, builder_fee::*, spot_types};
use serde_json::Value;

fn source() -> &'static Value {
    static SOURCE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/approval-leaves.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "APPROVAL source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn bits(value: &Value) -> u64 {
    u64::from_str_radix(value.as_str().unwrap(), 16).unwrap()
}
fn domain(native: ApprovalDomain, expected: &Value, root: &str) {
    assert_eq!(
        expected.as_object().unwrap().len(),
        4,
        "APPROVAL source domain field floor {root}"
    );
    assert_eq!(
        native.name,
        expected["name"].as_str().unwrap(),
        "APPROVAL native {root} name"
    );
    assert_eq!(
        native.version,
        expected["version"].as_str().unwrap(),
        "APPROVAL native {root} version"
    );
    assert_eq!(
        native.chain_id.to_bits(),
        expected["chainId"].as_f64().unwrap().to_bits(),
        "APPROVAL native {root} chain ID bits"
    );
    assert_eq!(
        native.verifying_contract,
        expected["verifyingContract"].as_str().unwrap(),
        "APPROVAL native {root} contract"
    );
}
fn fields(native: &[(&str, &[ApprovalField])], expected: &Value, root: &str) {
    assert_eq!(native.len(), 1, "APPROVAL native {root} type key floor");
    assert_eq!(
        expected.as_object().unwrap().len(),
        1,
        "APPROVAL source {root} type key floor"
    );
    let (key, rows) = expected.as_object().unwrap().iter().next().unwrap();
    assert_eq!(native[0].0, key, "APPROVAL native {root} type key");
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 4, "APPROVAL source {root} field floor");
    assert_eq!(native[0].1.len(), 4, "APPROVAL native {root} field floor");
    for index in 0..4 {
        assert_eq!(
            native[0].1[index].name,
            rows[index]["name"].as_str().unwrap(),
            "APPROVAL native {root} field name {index}"
        );
        assert_eq!(
            native[0].1[index].r#type,
            rows[index]["type"].as_str().unwrap(),
            "APPROVAL native {root} field type {index}"
        );
    }
}
#[test]
fn native_approval_constants() {
    let constants = &source()["constants"];
    assert_eq!(
        constants.as_object().unwrap().len(),
        6,
        "APPROVAL source constant floor"
    );
    let mut checked = 0;
    let mut failures = Vec::new();
    macro_rules! root {
        ($name:ident,$body:block) => {{
            checked += 1;
            if std::panic::catch_unwind(|| $body).is_err() {
                failures.push(stringify!($name));
            }
        }};
    }
    root!(DEFAULT_BUILDER_FEE_TENTHS_BPS, {
        assert_eq!(
            DEFAULT_BUILDER_FEE_TENTHS_BPS.to_bits(),
            constants["DEFAULT_BUILDER_FEE_TENTHS_BPS"]
                .as_f64()
                .unwrap()
                .to_bits(),
            "APPROVAL native DEFAULT_BUILDER_FEE_TENTHS_BPS bits"
        );
    });
    root!(DEFAULT_BUILDER_FEE_RATE, {
        assert_eq!(
            DEFAULT_BUILDER_FEE_RATE.as_str(),
            constants["DEFAULT_BUILDER_FEE_RATE"].as_str().unwrap(),
            "APPROVAL native DEFAULT_BUILDER_FEE_RATE text"
        );
    });
    root!(HYPERLIQUID_BUILDER_DOMAIN, {
        domain(
            HYPERLIQUID_BUILDER_DOMAIN,
            &constants["HYPERLIQUID_BUILDER_DOMAIN"],
            "HYPERLIQUID_BUILDER_DOMAIN",
        );
    });
    root!(HYPERLIQUID_BUILDER_DOMAIN_TESTNET, {
        domain(
            HYPERLIQUID_BUILDER_DOMAIN_TESTNET,
            &constants["HYPERLIQUID_BUILDER_DOMAIN_TESTNET"],
            "HYPERLIQUID_BUILDER_DOMAIN_TESTNET",
        );
    });
    root!(APPROVE_BUILDER_FEE_TYPES, {
        fields(
            APPROVE_BUILDER_FEE_TYPES,
            &constants["APPROVE_BUILDER_FEE_TYPES"],
            "APPROVE_BUILDER_FEE_TYPES",
        );
    });
    root!(APPROVE_AGENT_TYPES, {
        fields(
            APPROVE_AGENT_TYPES,
            &constants["APPROVE_AGENT_TYPES"],
            "APPROVE_AGENT_TYPES",
        );
    });
    assert_eq!(checked, 6, "APPROVAL constant iteration stop");
    assert!(
        failures.is_empty(),
        "APPROVAL named native constant failures: {failures:?}"
    );
}
#[test]
fn native_agent_name_and_random_effects() {
    let rows = source()["names"].as_array().unwrap();
    assert_eq!(rows.len(), 50, "APPROVAL independent name case floor");
    let sentinel = 71_u8;
    for index in 0..50 {
        let row = &rows[index];
        let mut slot = 0;
        let inputs = row["inputBits"].as_array().unwrap();
        assert_eq!(inputs.len(), 4, "APPROVAL source sampler input floor");
        let mut reads = Vec::new();
        let result = generate_agent_wallet_name(|| {
            reads.push(format!("get:{slot}"));
            if row["errorMode"] == "get" && row["errorAt"].as_u64() == Some(slot as u64) {
                return Err(&sentinel);
            }
            reads.push(format!("call:{slot}"));
            if row["errorMode"] == "call" && row["errorAt"].as_u64() == Some(slot as u64) {
                return Err(&sentinel);
            }
            let value = f64::from_bits(bits(&inputs[slot]));
            slot += 1;
            Ok(value)
        });
        match result {
            Ok(native) => assert_eq!(
                native,
                row["value"].as_str().unwrap(),
                "APPROVAL native generateAgentWalletName name case {index}"
            ),
            Err(error) => {
                assert_eq!(
                    row["errorIsSentinel"], true,
                    "APPROVAL native generateAgentWalletName source error case {index}"
                );
                assert!(
                    std::ptr::eq(error, &sentinel),
                    "APPROVAL native generateAgentWalletName error identity case {index}"
                );
            }
        }
        let expected: Vec<_> = row["reads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(
            reads, expected,
            "APPROVAL native generateAgentWalletName fresh read order case {index}"
        );
    }
}
#[test]
fn native_wallet_presence_and_getter_effects() {
    struct Window<'a> {
        mode: &'a str,
        sentinel: &'a u8,
        reads: Vec<&'static str>,
    }
    impl EthereumWindow for Window<'_> {
        type Error = *const u8;
        fn window_defined(&mut self) -> Result<bool, Self::Error> {
            if self.mode == "missing" {
                return Ok(false);
            }
            self.reads.push("window");
            if self.mode == "windowError" {
                return Err(self.sentinel);
            }
            Ok(true)
        }
        fn ethereum_truthy(&mut self) -> Result<bool, Self::Error> {
            self.reads.push("window");
            if self.mode == "secondWindowError" {
                return Err(self.sentinel);
            }
            self.reads.push("ethereum");
            if self.mode == "ethereumError" {
                return Err(self.sentinel);
            }
            // Host adapters provide JS ToBoolean, including UTF-16/BigInt/Symbol
            // values. This fixed adapter mapping is independent of oracle output.
            Ok(matches!(
                self.mode,
                "true" | "loneSurrogate" | "object" | "array" | "nonzeroBigInt" | "symbol"
            ))
        }
    }
    let rows = source()["wallets"].as_array().unwrap();
    assert_eq!(rows.len(), 18, "APPROVAL independent wallet case floor");
    let sentinel = 71_u8;
    for index in 0..18 {
        let row = &rows[index];
        let mut window = Window {
            mode: row["mode"].as_str().unwrap(),
            sentinel: &sentinel,
            reads: Vec::new(),
        };
        match has_ethereum_wallet(&mut window) {
            Ok(native) => assert_eq!(
                native,
                row["value"].as_bool().unwrap(),
                "APPROVAL native hasEthereumWallet value case {index}"
            ),
            Err(error) => {
                assert_eq!(
                    row["errorIsSentinel"], true,
                    "APPROVAL native hasEthereumWallet source error case {index}"
                );
                assert_eq!(
                    error, &sentinel as *const u8,
                    "APPROVAL native hasEthereumWallet error identity case {index}"
                );
            }
        }
        let expected: Vec<_> = row["reads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(
            window.reads, expected,
            "APPROVAL native hasEthereumWallet read order case {index}"
        );
    }
}
#[test]
fn native_spot_cancellation_list() {
    let rows: Vec<_> = source()["cancellations"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, row)| row["type"] == "CancelOrderRequests")
        .collect();
    assert_eq!(
        rows.len(),
        8,
        "APPROVAL independent cancellation list case floor"
    );
    for (index, row) in rows {
        let decoded =
            serde_json::from_str::<spot_types::CancelOrderRequests>(row["input"].as_str().unwrap());
        if row["accepted"].as_bool().unwrap() {
            let native = decoded.unwrap();
            let expected = row["native"].as_array().unwrap();
            let expected_count = match index {
                0 => 0,
                1 => 2,
                2 => 1,
                _ => panic!("unexpected accepted cancellation case"),
            };
            assert_eq!(
                native.len(),
                expected_count,
                "APPROVAL native CancelOrderRequests fixed item floor case {index}"
            );
            assert_eq!(
                expected.len(),
                expected_count,
                "APPROVAL source CancelOrderRequests fixed item floor case {index}"
            );
            for item in 0..expected_count {
                assert_eq!(
                    native[item].a.to_bits(),
                    bits(&expected[item]["aBits"]),
                    "APPROVAL native CancelOrderRequests asset bits case {index} item {item}"
                );
                assert_eq!(
                    native[item].o.to_bits(),
                    bits(&expected[item]["oBits"]),
                    "APPROVAL native CancelOrderRequests order bits case {index} item {item}"
                );
            }
        } else {
            assert!(
                decoded.is_err(),
                "APPROVAL native CancelOrderRequests rejection case {index}"
            );
        }
    }
}
