//! Order-policy flags from safe-cex/exchangeMetadata.ts, pinned by source parity tests.
pub(crate) fn infinity(venue: &str) -> bool {
    matches!(
        venue,
        "asterdex"
            | "bybit"
            | "bybitdemo"
            | "hyrotrader"
            | "bybitspot"
            | "bybitinverse"
            | "binance"
            | "binanceinverse"
            | "binancepm"
            | "bitget"
            | "bitgetinverse"
            | "bitgetusdc"
            | "bitunix"
            | "mexc"
            | "phemex"
            | "bingx"
            | "blofin"
            | "okx"
            | "okxdemo"
            | "okxeu"
            | "okxeudemo"
            | "woo"
            | "woofi"
            | "kodiakfi"
            | "mode"
            | "what"
            | "hyperliquid"
            | "hyperliquidspot"
            | "kucoin"
            | "kucoinspot"
    )
}
pub(crate) fn trailing(venue: &str) -> bool {
    matches!(
        venue,
        "bybit"
            | "bybitdemo"
            | "hyrotrader"
            | "bybitspot"
            | "bybitinverse"
            | "binance"
            | "binanceinverse"
            | "binancepm"
            | "bingx"
            | "woo"
            | "kraken"
            | "krakenspot"
    )
}
pub(crate) fn clamp(venue: &str) -> bool {
    matches!(
        venue,
        "bitget"
            | "bitgetinverse"
            | "bitgetusdc"
            | "bitunix"
            | "weex"
            | "mexc"
            | "coinbase"
            | "grvt"
            | "bitrue"
            | "woo"
            | "coinbasespot"
            | "hyperliquid"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    #[test]
    fn order_policy_flags_match_the_typescript_metadata_including_inheritance() {
        let source = include_str!("../../../../packages/safe-cex/src/exchangeMetadata.ts");
        let mut blocks = HashMap::<String, Vec<&str>>::new();
        let mut current = None;
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with("const ") && line.ends_with(" = {") {
                let name = line
                    .trim_start_matches("const ")
                    .split([':', ' '])
                    .next()
                    .unwrap()
                    .to_string();
                blocks.insert(name.clone(), Vec::new());
                current = Some(name);
            } else if line == "};" {
                current = None;
            } else if let Some(name) = &current {
                blocks.get_mut(name).unwrap().push(line);
            }
        }
        fn flag(blocks: &HashMap<String, Vec<&str>>, name: &str, key: &str) -> bool {
            let lines = blocks
                .get(name)
                .unwrap_or_else(|| panic!("missing metadata {name}"));
            if let Some(value) = lines
                .iter()
                .find_map(|line| line.strip_prefix(&format!("{key}: ")))
            {
                return value.trim_end_matches(',') == "true";
            }
            let parent = lines
                .iter()
                .find_map(|line| line.strip_prefix("..."))
                .unwrap_or_else(|| panic!("missing flag {name}.{key}"))
                .trim_end_matches(',');
            flag(blocks, parent, key)
        }
        let map = source
            .split("export const exchangeMetadata:")
            .nth(1)
            .unwrap()
            .split("} = {")
            .nth(1)
            .unwrap()
            .split("};")
            .next()
            .unwrap();
        let types = include_str!("../../../../packages/safe-cex/src/types.ts");
        let names = types
            .split_once("export type ExchangeName =")
            .expect("ExchangeName declaration")
            .1
            .split_once(';')
            .expect("ExchangeName terminator")
            .0;
        let expected: HashSet<_> = names
            .lines()
            .filter_map(|line| line.trim().strip_prefix("| '")?.strip_suffix('\''))
            .collect();
        assert!(expected.contains("bingxspot"));
        let mut extracted = HashSet::new();
        for line in map.lines() {
            let Some((venue, name)) = line.trim().split_once(": ") else {
                continue;
            };
            let venue = venue.trim_matches(['\'', '"']);
            let name = name.trim_end_matches(',');
            assert!(extracted.insert(venue), "duplicate metadata entry {venue}");
            assert_eq!(
                infinity(venue),
                flag(&blocks, name, "supportsInfinityStops"),
                "{venue} infinity"
            );
            assert_eq!(
                trailing(venue),
                flag(&blocks, name, "supportsTrailingStopLoss"),
                "{venue} trailing"
            );
            assert_eq!(
                clamp(venue),
                flag(&blocks, name, "autoDownSizeReduceOrders"),
                "{venue} clamp"
            );
        }
        assert_eq!(
            extracted, expected,
            "metadata extraction must not silently skip source"
        );
    }

    #[test]
    fn bingx_spot_uses_default_order_policies_instead_of_perpetual_overrides() {
        assert!(!infinity("bingxspot"));
        assert!(!trailing("bingxspot"));
        assert!(!clamp("bingxspot"));
        assert!(infinity("bingx"));
        assert!(trailing("bingx"));
        assert!(!clamp("bingx"));
    }
}
