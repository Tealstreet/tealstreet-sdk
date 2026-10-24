//! Venue symbol conversions through the Binance intermediate spelling.
use crate::ExchangeName;
use std::cell::RefCell;
use std::collections::HashMap;
fn name(exchange: &ExchangeName) -> &'static str {
    match exchange {
        ExchangeName::Apexomni => "apexomni",
        ExchangeName::Asterdex => "asterdex",
        ExchangeName::Backpack => "backpack",
        ExchangeName::Binance => "binance",
        ExchangeName::Binanceinverse => "binanceinverse",
        ExchangeName::Binancespot => "binancespot",
        ExchangeName::Binancepm => "binancepm",
        ExchangeName::Binanceoptions => "binanceoptions",
        ExchangeName::Bingx => "bingx",
        ExchangeName::Bitget => "bitget",
        ExchangeName::Bitgetinverse => "bitgetinverse",
        ExchangeName::Bitgetusdc => "bitgetusdc",
        ExchangeName::Bitrue => "bitrue",
        ExchangeName::Bitunix => "bitunix",
        ExchangeName::Blofin => "blofin",
        ExchangeName::Breakoutprop => "breakoutprop",
        ExchangeName::Bybit => "bybit",
        ExchangeName::Bybitdemo => "bybitdemo",
        ExchangeName::Bybitinverse => "bybitinverse",
        ExchangeName::Bybitspot => "bybitspot",
        ExchangeName::Hyrotrader => "hyrotrader",
        ExchangeName::Coinbase => "coinbase",
        ExchangeName::Coinbasespot => "coinbasespot",
        ExchangeName::Cryptocom => "cryptocom",
        ExchangeName::Grvt => "grvt",
        ExchangeName::Hyperliquid => "hyperliquid",
        ExchangeName::Hyperliquidspot => "hyperliquidspot",
        ExchangeName::Lighter => "lighter",
        ExchangeName::LighterRobinhood => "lighter-robinhood",
        ExchangeName::Mexc => "mexc",
        ExchangeName::Okx => "okx",
        ExchangeName::Okxdemo => "okxdemo",
        ExchangeName::Okxeu => "okxeu",
        ExchangeName::Okxeudemo => "okxeudemo",
        ExchangeName::Paradex => "paradex",
        ExchangeName::Phemex => "phemex",
        ExchangeName::Polymarket => "polymarket",
        ExchangeName::Kodiakfi => "kodiakfi",
        ExchangeName::Mode => "mode",
        ExchangeName::Salsadex => "salsadex",
        ExchangeName::TealstreetDex => "tealstreet-dex",
        ExchangeName::Txflow => "txflow",
        ExchangeName::What => "what",
        ExchangeName::Weex => "weex",
        ExchangeName::Woo => "woo",
        ExchangeName::Woofi => "woofi",
        ExchangeName::Kucoin => "kucoin",
        ExchangeName::Kucoinspot => "kucoinspot",
        ExchangeName::Kraken => "kraken",
        ExchangeName::Krakenspot => "krakenspot",
    }
}
pub fn parse_binance_symbol(symbol: &str) -> (&str, &str) {
    for quote in [
        "USDT", "USDC", "USDG", "BUSD", "FDUSD", "USDY", "USDD", "USDP", "USD", "EUR",
    ] {
        if symbol.len() > quote.len() && symbol.ends_with(quote) {
            return (&symbol[..symbol.len() - quote.len()], quote);
        }
    }
    (symbol, "USDT")
}
pub fn to_binance(symbol: &str, exchange: &ExchangeName) -> String {
    match name(exchange) {
        "phemex" => {
            if symbol.as_bytes().first() == Some(&b'U')
                && symbol.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
            {
                symbol[1..].into()
            } else {
                symbol.into()
            }
        }
        "bybitinverse" => {
            let b = symbol.as_bytes();
            if b.len() >= 3
                && b"FGHJKMNQUVXZ".contains(&b[b.len() - 3])
                && b[b.len() - 2..].iter().all(u8::is_ascii_digit)
            {
                symbol[..b.len() - 3].into()
            } else {
                symbol.into()
            }
        }
        "okx" | "okxdemo" | "okxeu" | "okxeudemo" => symbol
            .find('_')
            .filter(|i| *i > 0)
            .map(|i| &symbol[..i])
            .unwrap_or(symbol)
            .into(),
        "bybit" | "bybitdemo" | "hyrotrader" => {
            let clean = symbol
                .find('-')
                .filter(|i| *i > 0)
                .map(|i| &symbol[..i])
                .unwrap_or(symbol);
            match clean.strip_suffix("PERP") {
                Some(base) => format!("{base}USDC"),
                None => clean.into(),
            }
        }
        "bitgetusdc" => symbol
            .strip_suffix("PERP")
            .map(|base| format!("{base}USDC"))
            .unwrap_or_else(|| symbol.into()),
        "bitrue" => symbol.strip_prefix("E-").unwrap_or(symbol).replace('-', ""),
        "coinbase" => symbol
            .find("-PERP-INTX")
            .filter(|i| *i > 0)
            .map(|i| format!("{}USDC", &symbol[..i]))
            .unwrap_or_else(|| symbol.replace('-', "")),
        "coinbasespot" | "kucoinspot" | "txflow" => symbol.replace('-', ""),
        "paradex" => symbol
            .strip_suffix("-PERP")
            .unwrap_or(symbol)
            .replace('-', ""),
        "hyperliquid" => {
            let clean = symbol.find('|').map(|i| &symbol[i + 1..]).unwrap_or(symbol);
            format!("{}USDC", clean.strip_suffix("-PERP").unwrap_or(clean))
        }
        "hyperliquidspot" | "krakenspot" => symbol.replacen('/', "", 1),
        "kraken" | "kucoin" => {
            let exchange = name(exchange);
            let clean = if exchange == "kraken" {
                symbol.strip_prefix("PF_").unwrap_or(symbol)
            } else {
                symbol.strip_suffix('M').unwrap_or(symbol)
            };
            clean
                .strip_prefix("XBT")
                .map(|s| format!("BTC{s}"))
                .unwrap_or_else(|| clean.into())
        }
        "grvt" | "backpack" => symbol
            .strip_suffix("_PERP")
            .unwrap_or(symbol)
            .replace('_', ""),
        "mexc" => symbol.replace('_', ""),
        "cryptocom" => symbol.strip_suffix("-PERP").unwrap_or(symbol).into(),
        "lighter" | "lighter-robinhood" => {
            if symbol.contains('/') {
                symbol.replacen('/', "", 1)
            } else {
                format!(
                    "{symbol}{}",
                    if name(exchange) == "lighter" {
                        "USDC"
                    } else {
                        "USDG"
                    }
                )
            }
        }
        _ => symbol.into(),
    }
}
pub fn from_binance(symbol: &str, exchange: &ExchangeName) -> String {
    let (base, quote) = parse_binance_symbol(symbol);
    match name(exchange) {
        "bybit" | "bybitdemo" | "hyrotrader" => symbol
            .strip_suffix("USDC")
            .map(|s| format!("{s}PERP"))
            .unwrap_or_else(|| symbol.into()),
        "bitgetusdc" => format!("{base}PERP"),
        "bitrue" => format!("E-{base}-{quote}"),
        "coinbase" => format!("{base}-PERP-INTX"),
        "coinbasespot" | "kucoinspot" | "txflow" => format!("{base}-{quote}"),
        "paradex" => format!("{base}-{quote}-PERP"),
        "hyperliquid" => format!("{base}-PERP"),
        "hyperliquidspot" | "krakenspot" => format!("{base}/{quote}"),
        "kraken" => format!("PF_{}{quote}", if base == "BTC" { "XBT" } else { base }),
        "kucoin" => format!("{}{quote}M", if base == "BTC" { "XBT" } else { base }),
        "grvt" | "backpack" => format!("{base}_{quote}_PERP"),
        "mexc" => format!("{base}_{quote}"),
        "cryptocom" => format!("{symbol}-PERP"),
        "lighter" | "lighter-robinhood" => base.into(),
        _ => symbol.into(),
    }
}
type ConversionCache = HashMap<String, HashMap<String, HashMap<String, String>>>;
pub fn convert_symbol(symbol: &str, from: &ExchangeName, to: &ExchangeName) -> String {
    thread_local! {static CONVERSION_CACHE:RefCell<ConversionCache>=RefCell::default();}

    if from == to {
        return symbol.into();
    }
    let from_name = name(from);
    let to_name = name(to);
    CONVERSION_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(result) = cache
            .get(symbol)
            .and_then(|v| v.get(from_name))
            .and_then(|v| v.get(to_name))
        {
            return result.clone();
        }
        let result = from_binance(&to_binance(symbol, from), to);
        cache
            .entry(symbol.into())
            .or_default()
            .entry(from_name.into())
            .or_default()
            .insert(to_name.into(), result.clone());
        result
    })
}
