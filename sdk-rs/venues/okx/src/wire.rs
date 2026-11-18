//! Borrowed OKX v5 wire records. Unused fields are skipped by serde.
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub code: &'a str,
    #[serde(default)]
    pub msg: &'a str,
    #[serde(borrow, default)]
    pub data: Option<&'a RawValue>,
}

pub fn envelope(text: &str) -> Result<&str, String> {
    let e: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if e.code != "0" {
        return Err(format!("OKX {}: {}", e.code, e.msg));
    }
    e.data
        .map(RawValue::get)
        .ok_or_else(|| "missing OKX data".into())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Instrument<'a> {
    #[serde(deserialize_with = "nullable")]
    pub inst_id: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub inst_type: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub rule_type: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub uly: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub inst_family: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub base_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub quote_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ct_type: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ct_val: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ct_val_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub settle_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub state: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub lot_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub tick_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub min_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_iceberg_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_lmt_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_mkt_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_stop_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_trigger_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_twap_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub lever: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Prices<'a> {
    #[serde(deserialize_with = "nullable")]
    pub inst_id: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub bid_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ask_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub last: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub open24h: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub vol_ccy24h: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub vol24h: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub mark_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub idx_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub oi_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub funding_rate: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub funding_time: &'a str,
}

// OKX sends `null` for some unset quote/catalog fields; treat it like a blank.
/// The fixed shape of an OKX public push, `{"arg":{"channel":C,"instId":I},"data":[R]}`
/// with `R` one flat object of plain strings: its arg channel and row, read the way
/// serde reads them. `None` (escapes, control bytes, null or non-string values,
/// whitespace, other keys or rows, a repeated known field) leaves it to serde.
pub fn scan_price(text: &str) -> Option<(&str, Prices<'_>)> {
    let (channel, rest) = plain(text.strip_prefix(r#"{"arg":{"channel":""#)?)?;
    let (_, rest) = plain(rest.strip_prefix(r#"","instId":""#)?)?;
    let mut rest = rest.strip_prefix(r#""},"data":[{""#)?;
    let mut row = Prices::default();
    let mut seen = 0u16;
    loop {
        let (key, after) = plain(rest)?;
        let (value, after) = plain(after.strip_prefix(r#"":""#)?)?;
        let field = match key {
            "instId" => Some((0, &mut row.inst_id)),
            "bidPx" => Some((1, &mut row.bid_px)),
            "askPx" => Some((2, &mut row.ask_px)),
            "last" => Some((3, &mut row.last)),
            "open24h" => Some((4, &mut row.open24h)),
            "volCcy24h" => Some((5, &mut row.vol_ccy24h)),
            "vol24h" => Some((6, &mut row.vol24h)),
            "markPx" => Some((7, &mut row.mark_px)),
            "idxPx" => Some((8, &mut row.idx_px)),
            "oiCcy" => Some((9, &mut row.oi_ccy)),
            "fundingRate" => Some((10, &mut row.funding_rate)),
            "fundingTime" => Some((11, &mut row.funding_time)),
            _ => None,
        };
        if let Some((bit, slot)) = field {
            if seen & 1 << bit != 0 {
                return None;
            }
            seen |= 1 << bit;
            *slot = value;
        }
        let after = after.strip_prefix('"')?;
        if let Some(next) = after.strip_prefix(",\"") {
            rest = next;
        } else {
            return (after == "}]}").then_some((channel, row));
        }
    }
}

/// A string body up to its closing quote, when it holds no escape or control byte.
fn plain(text: &str) -> Option<(&str, &str)> {
    let end = text
        .bytes()
        .position(|b| b == b'"' || b == b'\\' || b < 0x20)?;
    (text.as_bytes()[end] == b'"').then(|| (&text[..end], &text[end..]))
}

fn nullable<'de: 'a, 'a, D: serde::Deserializer<'de>>(d: D) -> Result<&'a str, D::Error> {
    Ok(Option::<&'a str>::deserialize(d)?.unwrap_or_default())
}

/// Identifies a catalog row that failed to decode as an [`Instrument`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentId<'a> {
    #[serde(default, deserialize_with = "nullable")]
    pub inst_id: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Arg<'a> {
    pub channel: &'a str,
    pub inst_type: &'a str,
    pub inst_id: &'a str,
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(default)]
    pub event: &'a str,
    #[serde(default)]
    pub code: &'a str,
    #[serde(default)]
    pub msg: &'a str,
    #[serde(default)]
    pub arg: Arg<'a>,
    #[serde(default)]
    pub action: &'a str,
    #[serde(borrow, default)]
    pub data: Option<&'a RawValue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Depth<'a> {
    pub seq_id: i64,
    #[serde(default)]
    pub prev_seq_id: Option<i64>,
    #[serde(borrow)]
    pub bids: Vec<Vec<&'a str>>,
    #[serde(borrow)]
    pub asks: Vec<Vec<&'a str>>,
}

pub fn candle(row: &[&str]) -> Option<safe_cex_engine::model::Candle> {
    use safe_cex_engine::{model::Candle, num::parse};
    (row.len() >= 8
        && [0, 1, 2, 3, 4, 7]
            .iter()
            .all(|&i| row[i].parse::<f64>().is_ok_and(|v| v.is_finite())))
    .then(|| Candle {
        timestamp: parse(row[0]) / 1000.0,
        open: parse(row[1]),
        high: parse(row[2]),
        low: parse(row[3]),
        close: parse(row[4]),
        volume: parse(row[7]),
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config<'a> {
    pub pos_mode: &'a str,
    pub acct_lv: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LeverageRow<'a> {
    pub inst_id: &'a str,
    pub mgn_mode: &'a str,
    pub lever: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PositionRow<'a> {
    pub inst_type: &'a str,
    pub margin: &'a str,
    pub inst_id: &'a str,
    pub pos: &'a str,
    pub pos_side: &'a str,
    pub avg_px: &'a str,
    pub mark_px: &'a str,
    pub ccy: &'a str,
    pub lever: &'a str,
    pub upl: &'a str,
    pub liq_px: &'a str,
    pub mgn_mode: &'a str,
    pub imr: &'a str,
    pub mmr: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OrderRow<'a> {
    pub inst_type: &'a str,
    pub inst_id: &'a str,
    pub ord_id: &'a str,
    pub algo_id: &'a str,
    pub cl_ord_id: &'a str,
    pub ccy: &'a str,
    pub algo_cl_ord_id: &'a str,
    pub ord_type: &'a str,
    pub state: &'a str,
    pub fail_code: &'a str,
    pub side: &'a str,
    pub sz: &'a str,
    pub acc_fill_sz: &'a str,
    pub px: &'a str,
    pub avg_px: &'a str,
    pub fill_px: &'a str,
    pub fill_sz: &'a str,
    pub fill_time: &'a str,
    pub u_time: &'a str,
    pub trade_id: &'a str,
    pub trigger_px: &'a str,
    pub trigger_px_type: &'a str,
    pub sl_trigger_px: &'a str,
    pub sl_ord_px: &'a str,
    pub sl_trigger_px_type: &'a str,
    pub tp_trigger_px: &'a str,
    pub tp_ord_px: &'a str,
    pub tp_trigger_px_type: &'a str,
    pub order_px: &'a str,
    pub ord_px: &'a str,
    pub td_mode: &'a str,
    pub pos_side: &'a str,
    pub tag: &'a str,
    pub active_px: &'a str,
    pub callback_ratio: &'a str,
    pub callback_spread: &'a str,
    pub move_trigger_px: &'a str,
    pub last: &'a str,
    pub lever: &'a str,
    #[serde(deserialize_with = "boolean")]
    pub reduce_only: bool,
}

/// A trailing stop's activePx while its activation is pending: one numeric test
/// shared by the row display and the edit refusal, so the two can never disagree.
pub fn pending_activation(active_px: &str) -> Option<f64> {
    Some(safe_cex_engine::num::parse(active_px)).filter(|px| *px > 0.0)
}

fn boolean<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Bool<'a> {
        Boolean(bool),
        Text(&'a str),
    }
    Ok(match Bool::deserialize(d)? {
        Bool::Boolean(b) => b,
        Bool::Text(s) => s == "true",
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Equity<'a> {
    pub ccy: &'a str,
    pub eq: &'a str,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk<'a> {
    #[serde(borrow)]
    pub bal_data: Vec<Equity<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPush<'a> {
    #[serde(default)]
    pub total_eq: &'a str,
    #[serde(borrow, default)]
    pub details: Vec<Equity<'a>>,
}

/// A row the borrowed records cannot read, rewritten so they can: null fields dropped,
/// numbers as text, JSON escapes resolved. Cold path: only after a row failed to parse.
pub fn readable_rows(text: &str) -> Option<String> {
    #[derive(Deserialize, serde::Serialize)]
    #[serde(untagged)]
    enum Json {
        Null,
        Bool(bool),
        Int(i64),
        Unsigned(u64),
        Float(f64),
        Text(String),
        Array(Vec<Json>),
        Object(std::collections::BTreeMap<String, Json>),
    }
    fn clean(row: &mut Json) {
        match row {
            Json::Array(rows) => rows.iter_mut().for_each(clean),
            Json::Object(fields) => {
                fields.retain(|_, value| !matches!(value, Json::Null));
                for value in fields.values_mut() {
                    let text = match value {
                        Json::Int(number) => number.to_string(),
                        Json::Unsigned(number) => number.to_string(),
                        Json::Float(number) => {
                            serde_json::to_string(number).ok().unwrap_or_default()
                        }
                        _ => continue,
                    };
                    *value = Json::Text(text);
                }
            }
            _ => {}
        }
    }
    let mut rows = serde_json::from_str::<Json>(text).ok()?;
    clean(&mut rows);
    serde_json::to_string(&rows).ok()
}

#[cfg(test)]
mod scan_price_tests {
    use super::*;
    use safe_cex_engine::json;

    const MARK: &str = r#"{"arg":{"channel":"mark-price","instId":"ETH-USD-SWAP"},"data":[{"instId":"ETH-USD-SWAP","instType":"SWAP","markPx":"2410.10","ts":"1792468506156"}]}"#;
    const INDEX: &str = r#"{"arg":{"channel":"index-tickers","instId":"ETH-USD"},"data":[{"instId":"ETH-USD","idxPx":"2411.18","open24h":"2420.88","high24h":"2443.66","low24h":"2370.75","sodUtc0":"2405.25","sodUtc8":"2391.55","ts":"1792468506113"}]}"#;
    const TICKER: &str = r#"{"arg":{"channel":"tickers","instId":"BTC-USD-SWAP"},"data":[{"instType":"SWAP","instId":"BTC-USD-SWAP","last":"81973.5","lastSz":"99.3","askPx":"81973.6","askSz":"174.9","bidPx":"81973.5","bidSz":"752.3","open24h":"82351.3","high24h":"83108.2","low24h":"81060.6","sodUtc0":"81810.2","sodUtc8":"81810.2","volCcy24h":"6134.9012","vol24h":"5038815.6","ts":"1792468506174"}]}"#;

    fn fields<'a>(p: &Prices<'a>) -> [&'a str; 12] {
        [
            p.inst_id,
            p.bid_px,
            p.ask_px,
            p.last,
            p.open24h,
            p.vol_ccy24h,
            p.vol24h,
            p.mark_px,
            p.idx_px,
            p.oi_ccy,
            p.funding_rate,
            p.funding_time,
        ]
    }

    /// What the generic path reads: the arg channel and every row it decodes.
    fn generic(text: &str) -> Option<(String, Vec<[String; 12]>)> {
        let message = serde_json::from_str::<Message<'_>>(text).ok()?;
        let mut rows = Vec::new();
        json::each::<Prices<'_>>(message.data?.get(), |p| {
            rows.push(fields(&p).map(str::to_owned));
        })
        .ok()?;
        Some((message.arg.channel.to_owned(), rows))
    }

    fn assert_scan_matches_serde(text: &str) -> bool {
        let Some((channel, row)) = scan_price(text) else {
            return false;
        };
        let (expected, rows) = generic(text).expect("a scanned push is valid JSON for serde");
        assert_eq!(channel, expected, "{text}");
        assert_eq!(rows, vec![fields(&row).map(str::to_owned)], "{text}");
        true
    }

    #[test]
    fn the_hot_pushes_scan_exactly_as_serde_reads_them() {
        for text in [MARK, INDEX, TICKER] {
            assert!(assert_scan_matches_serde(text), "{text}");
        }
        let empty = MARK.replace(r#""markPx":"2410.10""#, r#""markPx":"""#);
        let unicode = MARK.replace("SWAP\",\"markPx", "SWAP\u{e9}\",\"markPx");
        let repeated_unknown = MARK.replace(r#""ts":"#, r#""ts":"1","ts":"#);
        for text in [empty, unicode, repeated_unknown] {
            assert!(assert_scan_matches_serde(&text), "{text}");
        }
    }

    #[test]
    fn anything_off_the_fixed_shape_is_left_to_serde() {
        let off = [
            MARK.replace(r#""markPx":"2410.10""#, r#""markPx": "2410.10""#),
            MARK.replace("2410.10", r"2410\u002e10"),
            MARK.replace("2410.10", "2410\t10"),
            MARK.replace(r#""markPx":"2410.10""#, r#""markPx":null"#),
            MARK.replace(r#""markPx":"2410.10""#, r#""markPx":2410.10"#),
            MARK.replace(r#""ts":"#, r#""markPx":"1","ts":"#),
            MARK.replace(r#"[{"instId"#, r#"[{},{"instId"#),
            MARK.replace(r#""ts":"1792468506156"}]"#, r#""ts":"1"},{"instId":"X"}]"#),
            MARK.replace(r#""data":[{"instId":"ETH-USD-SWAP","instType":"SWAP","markPx":"2410.10","ts":"1792468506156"}]"#, r#""data":[{}]"#),
            MARK.replace(r#"},"data""#, r#","x":"y"},"data""#),
            MARK.replace(r#"{"arg""#, r#"{"event":"x","arg""#),
            MARK.replace("}]}", r#"}],"action":"snapshot"}"#),
            format!("{MARK}\n"),
            MARK.replace(r#""channel":"mark-price","instId":"ETH-USD-SWAP""#, r#""instId":"ETH-USD-SWAP","channel":"mark-price""#),
            MARK.trim_end_matches("}]}").to_owned(),
        ];
        for (case, text) in off.iter().enumerate() {
            assert!(scan_price(text).is_none(), "case {case}: {text:?}");
        }
    }

    /// Deterministic xorshift64 so a failure reproduces from its case number.
    fn next(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn differential_fuzz_never_scans_what_serde_reads_differently() {
        let tokens = [
            "\"",
            "\\",
            ",",
            ":",
            "{",
            "}",
            "[",
            "]",
            " ",
            "\n",
            "\t",
            "\u{1}",
            "0",
            "9",
            ".",
            "-",
            "e",
            "null",
            "true",
            "\"markPx\"",
            "\"instId\"",
            "\"x\"",
            "\\u0041",
            "\u{e9}",
            "\"\"",
        ];
        let seeds = [MARK, INDEX, TICKER];
        let (mut scanned, mut fallback, mut state) = (0, 0, 0x9e37_79b9_7f4a_7c15u64);
        for case in 0..20_000 {
            let mut text = seeds[case % seeds.len()].to_owned();
            for _ in 0..=next(&mut state) % 3 {
                let mut at = next(&mut state) as usize % (text.len() + 1);
                while !text.is_char_boundary(at) {
                    at -= 1;
                }
                let token = &tokens[next(&mut state) as usize % tokens.len()];
                match next(&mut state) % 3 {
                    0 => text.insert_str(at, token),
                    1 => {
                        let mut end = (at + 1 + next(&mut state) as usize % 4).min(text.len());
                        while !text.is_char_boundary(end) {
                            end += 1;
                        }
                        text.replace_range(at..end, "");
                    }
                    _ => {
                        let mut end = (at + 1).min(text.len());
                        while !text.is_char_boundary(end) {
                            end += 1;
                        }
                        text.replace_range(at..end, token);
                    }
                }
            }
            if assert_scan_matches_serde(&text) {
                scanned += 1;
            } else {
                fallback += 1;
            }
        }
        // Well-formed pushes with random keys, order, repeats and values.
        let keys = [
            "instId",
            "markPx",
            "idxPx",
            "last",
            "bidPx",
            "askPx",
            "open24h",
            "volCcy24h",
            "vol24h",
            "oiCcy",
            "fundingRate",
            "fundingTime",
            "ts",
            "instType",
            "sodUtc0",
            "x",
        ];
        let values = [
            "",
            "1",
            "2410.10",
            "-0.5",
            "1e5",
            "\u{e9}",
            "BTC-USD-SWAP",
            " 1",
        ];
        for _ in 0..5_000 {
            let mut row = Vec::new();
            for _ in 0..next(&mut state) % 8 {
                let key = keys[next(&mut state) as usize % keys.len()];
                let value = values[next(&mut state) as usize % values.len()];
                row.push(format!(r#""{key}":"{value}""#));
            }
            let text = format!(
                r#"{{"arg":{{"channel":"mark-price","instId":"X"}},"data":[{{{}}}]}}"#,
                row.join(",")
            );
            if assert_scan_matches_serde(&text) {
                scanned += 1;
            } else {
                fallback += 1;
            }
        }
        assert!(
            scanned > 5_000 && fallback > 5_000,
            "{scanned} scanned, {fallback} fallback"
        );
    }

    #[test]
    fn other_channels_scan_but_only_price_channels_take_the_fast_path() {
        let funding = MARK.replace("mark-price", "funding-rate");
        let (channel, _) = scan_price(&funding).unwrap();
        assert_eq!(channel, "funding-rate");
        assert!(assert_scan_matches_serde(&funding));
    }
}
