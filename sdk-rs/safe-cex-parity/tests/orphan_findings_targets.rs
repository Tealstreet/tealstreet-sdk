//! Resolution census over original source files. This is not the full invention gate.
use safe_cex_parity::{RustMapping, manifests, rust_index};
use std::{collections::BTreeSet, fs, path::PathBuf};

struct Scope(PathBuf);
impl Drop for Scope {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn assigned_finding_targets_resolve_in_the_real_source_index() {
    let sdk = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scope = Scope(std::env::temp_dir().join(format!(
            "orphan-targets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    fs::create_dir_all(&scope.0).unwrap();
    let sources: [(&str, &[&str]); 6] = [
        ("safe-cex-candle-cache", &["types.rs"]),
        ("safe-cex-exchange", &["proxy_token.rs", "websocket.rs"]),
        ("safe-cex-num", &["prototype_realm.rs"]),
        ("safe-cex-store", &["instrument_class.rs", "risk.rs"]),
        ("safe-cex-venue-okx", &["income.rs", "rest_operations.rs"]),
        (
            "safe-cex-venue-bitget",
            &["income.rs", "orders.rs", "history_reads.rs"],
        ),
    ];
    let members = sources
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        scope.0.join("Cargo.toml"),
        format!("[workspace]\nmembers=[{members}]\nresolver=\"3\"\n"),
    )
    .unwrap();
    for (name, files) in sources {
        let root = scope.0.join(name);
        fs::create_dir_all(root.join("src")).unwrap();
        // Actual declarations are indexed under their original crate/file IDs.
        // Selecting source roots does not add declarations or exemptions.
        let mut cargo = format!(
            "[package]\nname=\"{name}\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\npath=\"src/{}\"\n",
            files[0]
        );
        for (index, file) in files.iter().enumerate() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(
                sdk.join(name).join("src").join(file),
                root.join("src").join(file),
            )
            .unwrap();
            #[cfg(not(unix))]
            fs::copy(
                sdk.join(name).join("src").join(file),
                root.join("src").join(file),
            )
            .unwrap();
            if index != 0 {
                cargo.push_str(&format!(
                    "[[bin]]\nname=\"source-{index}\"\npath=\"src/{file}\"\n"
                ));
            }
        }
        fs::write(root.join("Cargo.toml"), cargo).unwrap();
    }
    let index = rust_index(&scope.0).expect("real source indexer must parse selected roots");
    let selected = [
        ("candleCache/types.ts", "CandleCacheChunkRecord"),
        (
            "exchanges/base.ws.ts",
            "BaseWebSocket.recordMessageHandlerStats",
        ),
        (
            "exchanges/base.ws.ts",
            "BaseWebSocket#reconcileMessageHandlerStats",
        ),
        (
            "exchanges/base.ws.ts",
            "BaseWebSocket.cleanupMessageHandlerStats",
        ),
        (
            "instrument-class/bybit.instrumentClass.ts",
            "bybitInstrumentClass",
        ),
        ("proxyTokenClient.ts", "initBroadcastChannel"),
        ("utils/risk.ts", "calculateOrdersRisk"),
        (
            "exchanges/okx/okx.exchange.ts",
            "OKXExchange.findOrderByClientOrderId",
        ),
        ("exchanges/okx/okx.income.ts", "okxBillCategory"),
        ("exchanges/okx/okx.income.ts", "mapOkxBillToIncomeRecords"),
        (
            "exchanges/bitget/bitget.exchange.ts",
            "BitgetExchange#_fetchBillPage",
        ),
        (
            "exchanges/bitget/bitget.exchange.ts",
            "BitgetExchange.mapOrder",
        ),
        ("exchanges/bitget/bitget.income.ts", "mapBitgetBill"),
        ("exchanges/bitget/bitget.income.ts", "mapBitgetBills"),
        (
            "exchanges/bitget/bitgetinverse.exchange.ts",
            "BitgetInverseExchange.mapOrder",
        ),
    ];
    let corpus = manifests(&sdk.join("parity/manifest")).unwrap();
    let mut targets = BTreeSet::new();
    for (file, symbol) in selected {
        let source = format!("packages/safe-cex/src/{file}");
        let manifest = corpus.iter().find(|m| m.file.ts == source).unwrap();
        let row = manifest.symbol.iter().find(|r| r.ts == symbol).unwrap();
        assert!(
            matches!(row.status.as_str(), "ported" | "divergent"),
            "assigned row must retain a real implementation: {symbol}"
        );
        let items: Vec<&str> = match &row.rs {
            RustMapping::Single(item) => vec![item],
            RustMapping::Split(items) => items.iter().map(String::as_str).collect(),
        };
        for target in items {
            assert!(
                index.all.contains(target),
                "ASSIGNED-TARGET-RESOLVES {source}::{symbol} -> {target}"
            );
            targets.insert(target.to_owned());
        }
    }
    // #1230 adds install_native_handler_stats to recordMessageHandlerStats.
    // Retain the original 48 targets plus that resolved registry installer.
    assert_eq!(
        targets.len(),
        49,
        "independent 15-row assignment floor, including original split targets, 27 prototype-realm targets and the native handler stats installer; stop inverse.mapOrder"
    );
}
