use safe_cex_candle_cache::CandleCacheChunkRecord;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use serde_json::Value;

#[test]
fn chunk_record_numeric_ingress_preserves_native_bits() {
    const FIELDS: [&str; 7] = [
        "lastAccessed",
        "timestamp",
        "open",
        "high",
        "low",
        "close",
        "volume",
    ];
    const LEXEMES: [&str; 11] = [
        "0",
        "-0",
        "-0.0",
        "1.25",
        "1e3",
        "1e400",
        "-1e400",
        "-1e-4000",
        "9007199254740993",
        "1.0000000000000002",
        "5e-324",
    ];
    let source = std::process::Command::new("node").args(["-e", "let ignored=null;for(let i=0;i<160;i++)ignored=[ignored];if(JSON.parse(JSON.stringify({candles:[],lastAccessed:1,ignored})).lastAccessed!==1)throw Error('source metadata acceptance');process.stdout.write(JSON.stringify(process.argv.slice(1).map(raw=>{const n=JSON.parse(raw);const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.readBigUInt64BE().toString()})))"]).args(LEXEMES).output().unwrap();
    assert!(
        source.status.success(),
        "Node numeric source: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let bits: Vec<String> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(bits.len(), 11, "independent numeric spelling floor");
    let mut assertions = 0;
    for field in FIELDS {
        for (i, lexeme) in LEXEMES.into_iter().enumerate() {
            let raw = r#"{"candles":[{"timestamp":1,"open":1,"high":1,"low":1,"close":1,"volume":1,"lastSide":"sell"}],"lastAccessed":1}"#.replace(&format!("\"{field}\":1"), &format!("\"{field}\":{lexeme}"));
            let value: Value = serde_json::from_str(&raw).unwrap();
            let routes = [
                (
                    "bare-str",
                    serde_json::from_str::<CandleCacheChunkRecord>(&raw),
                ),
                ("bare-slice", serde_json::from_slice(raw.as_bytes())),
                ("bare-owned", serde_json::from_value(value.clone())),
                ("bare-borrowed", serde::Deserialize::deserialize(&value)),
                ("shared-str", js_from_str(&raw)),
                ("shared-owned", js_from_value(value.clone())),
                ("shared-borrowed", js_from_value_ref(&value)),
            ];
            for (route, record) in routes {
                let record = record.unwrap_or_else(|e| {
                    panic!("CACHE-NATIVE-DECODE {field}/{lexeme}/{route}: {e}")
                });
                assert_eq!(record.candles.len(), 1, "native candle cardinality");
                let c = &record.candles[0];
                assert_eq!(
                    c.last_side,
                    Some(safe_cex_types::OrderSide::Sell),
                    "nested enum unaffected"
                );
                let actual = match field {
                    "lastAccessed" => record.last_accessed,
                    "timestamp" => c.timestamp,
                    "open" => c.open,
                    "high" => c.high,
                    "low" => c.low,
                    "close" => c.close,
                    "volume" => c.volume,
                    _ => unreachable!(),
                };
                assert_eq!(
                    actual.to_bits(),
                    bits[i].parse::<u64>().unwrap(),
                    "CACHE-NATIVE-BITS {field}/{lexeme}/{route}"
                );
                assertions += 1;
            }
        }
    }
    assert_eq!(
        assertions, 539,
        "7 fields x 11 spellings x 7 named routes; stop volume/5e-324/shared-borrowed"
    );
    for invalid in [
        "[]",
        "[[],0]",
        "null",
        r#"{"candles":[]}"#,
        r#"{"candles":[{}],"lastAccessed":0}"#,
    ] {
        assert!(
            serde_json::from_str::<CandleCacheChunkRecord>(invalid).is_err(),
            "record shape remains strict: {invalid}"
        );
    }
    let native = serde_json::json!({"candles":[],"lastAccessed":-0.0});
    let native: CandleCacheChunkRecord = serde_json::from_value(native).unwrap();
    assert_eq!(
        native.last_accessed.to_bits(),
        (-0.0f64).to_bits(),
        "native negative-zero positive control"
    );
}

#[test]
fn chunk_record_prices_shared_depth_budget() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LegacyWire {
        candles: Vec<safe_cex_types::Candle>,
        last_accessed: f64,
    }
    fn value(depth: usize) -> Value {
        let mut ignored = Value::Null;
        for _ in 0..depth {
            ignored = Value::Array(vec![ignored]);
        }
        serde_json::json!({"candles":[],"lastAccessed":1,"ignored":ignored})
    }
    let shallow = value(8);
    let deep = value(160);
    let legacy: LegacyWire = serde_json::from_value(deep.clone()).unwrap();
    assert!(legacy.candles.is_empty());
    assert_eq!(
        legacy.last_accessed.to_bits(),
        1.0f64.to_bits(),
        "legacy companion accepts ignored owned metadata"
    );
    for (name, input) in [("shallow", shallow), ("deep", deep)] {
        let bare = serde_json::from_value::<CandleCacheChunkRecord>(input.clone());
        let borrowed: Result<CandleCacheChunkRecord, serde_json::Error> =
            serde::Deserialize::deserialize(&input);
        let shared = js_from_value::<CandleCacheChunkRecord>(input.clone());
        let shared_borrowed = js_from_value_ref::<CandleCacheChunkRecord>(&input);
        for result in [bare, borrowed, shared, shared_borrowed] {
            assert_eq!(
                result.is_ok(),
                name == "shallow",
                "CACHE-DEPTH-CHOICE {name}: bounded adapter cost, not invalid JSON"
            );
        }
    }
}
