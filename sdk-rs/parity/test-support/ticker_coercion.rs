use safe_cex_types::Ticker;
use serde_json::{Value, json};

pub fn source(venue: &str) -> Value {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            "sdk-rs/parity/scripts/ticker-native-source.cjs",
            venue,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

pub fn seed(venue: &str) -> Vec<Ticker> {
    ["BTC", "ETH", "ADA"]
        .into_iter()
        .map(|asset| {
            let (id, symbol) = match venue {
                "backpack" => (format!("{asset}_USDC_PERP"), format!("{asset}_USDC_PERP")),
                "paradex" => (format!("{asset}-USD-PERP"), format!("{asset}-USD-PERP")),
                "polymarket" => (format!("{asset}-token"), format!("{asset}-Yes")),
                "bitunix" => (format!("{asset}/USDT"), format!("{asset}USDT")),
                "bitrue" => (format!("E-{asset}-USDT"), format!("E-{asset}-USDT")),
                "apexomni" => (format!("{asset}-USDC"), format!("{asset}USDC")),
                "weex" | "phemex" | "binance" | "binancepm" | "binancespot" => {
                    (format!("{asset}USDT"), format!("{asset}USDT"))
                }
                "kucoin" => (
                    format!("{}USDTM", if asset == "BTC" { "XBT" } else { asset }),
                    format!("{asset}USDT"),
                ),
                "kucoinspot" => (format!("{asset}-USDT"), format!("{asset}-USDT")),
                "blofin" => (format!("{asset}-USDT"), format!("{asset}USDT")),
                "cryptocom" => (format!("{asset}USD-PERP"), format!("{asset}USD-PERP")),
                "coinbase" => (format!("{asset}-USDC"), format!("{asset}-USDC")),
                "hyperliquid" => (asset.to_string(), format!("{asset}/USDC")),
                "hyperliquidspot" => (
                    format!(
                        "@{}",
                        ["BTC", "ETH", "ADA"]
                            .iter()
                            .position(|v| *v == asset)
                            .unwrap()
                    ),
                    format!("{asset}/USDC"),
                ),
                "lighter" => (
                    (["BTC", "ETH", "ADA"]
                        .iter()
                        .position(|v| *v == asset)
                        .unwrap()
                        + 1)
                    .to_string(),
                    asset.to_string(),
                ),
                _ => panic!("unsupported ticker seed {venue}"),
            };
            let mut ticker: Ticker =
                serde_json::from_value(json!({"id":id,"symbol":symbol,"bid":101,"ask":102,
            "last":103,"mark":104,"index":105,"percentage":106,"volume":107,
            "quoteVolume":108,"fundingRate":109,"fundingTime":110,"openInterest":111}))
                .unwrap();
            if venue.starts_with("hyperliquid") {
                ticker.mid_price = Some(112.0);
            }
            ticker
        })
        .collect()
}

pub fn assert_native(tickers: &[Ticker], case: &Value) {
    let expected = case["expected"].as_array().unwrap();
    assert_eq!(tickers.len(), expected.len());
    for row in expected {
        let ticker = tickers
            .iter()
            .find(|t| t.id == row["id"].as_str().unwrap())
            .unwrap();
        assert_eq!(ticker.symbol, row["symbol"].as_str().unwrap());
        if row["fields"].get("midPrice").is_some() {
            let number = ticker.mid_price.expect("native ticker midPrice present");
            let bits = if number.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                number.to_bits()
            };
            assert_eq!(
                bits,
                u64::from_str_radix(row["fields"]["midPrice"].as_str().unwrap(), 16).unwrap(),
                "native ticker midPrice {case}"
            );
        }
        for (field, number) in [
            ("bid", ticker.bid),
            ("ask", ticker.ask),
            ("last", ticker.last),
            ("mark", ticker.mark),
            ("index", ticker.index),
            ("percentage", ticker.percentage),
            ("volume", ticker.volume),
            ("quoteVolume", ticker.quote_volume),
            ("fundingRate", ticker.funding_rate),
            ("fundingTime", ticker.funding_time),
            ("openInterest", ticker.open_interest),
        ] {
            let bits = if number.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                number.to_bits()
            };
            let source = u64::from_str_radix(row["fields"][field].as_str().unwrap(), 16).unwrap();
            assert_eq!(
                bits, source,
                "native ticker {}/{}/{}/{} {field}",
                case["channel"], case["field"], case["label"], ticker.symbol
            );
        }
    }
}
