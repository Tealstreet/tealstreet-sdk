use safe_cex_types::ExchangeName;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRegistration {
    pub name: ExchangeName,
    pub module: &'static str,
    pub constructor: &'static str,
}

pub const EXCHANGE_REGISTRY: &[ExchangeRegistration] = &[
    ExchangeRegistration {
        name: ExchangeName::Apexomni,
        module: "./apexomni/apexomni.exchange",
        constructor: "ApexOmniExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Asterdex,
        module: "./asterdex/asterdex.exchange",
        constructor: "AsterDEXExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Backpack,
        module: "./backpack/backpack.exchange",
        constructor: "BackpackExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Binance,
        module: "./binance/binance.exchange",
        constructor: "BinanceExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Binanceinverse,
        module: "./binance/binanceinverse.exchange",
        constructor: "BinanceInverseExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Binancepm,
        module: "./binance/binancepm.exchange",
        constructor: "BinancePMExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Binancespot,
        module: "./binance/binancespot.exchange",
        constructor: "BinanceSpotExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bingx,
        module: "./bingx/bingx.exchange",
        constructor: "BingxExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bitget,
        module: "./bitget/bitget.exchange",
        constructor: "BitgetExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bitgetinverse,
        module: "./bitget/bitgetinverse.exchange",
        constructor: "BitgetInverseExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bitgetusdc,
        module: "./bitget/bitgetusdc.exchange",
        constructor: "BitgetUSDCExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bitrue,
        module: "./bitrue/bitrue.exchange",
        constructor: "BitrueExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bitunix,
        module: "./bitunix/bitunix.exchange",
        constructor: "BitunixExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Blofin,
        module: "./blofin/blofin.exchange",
        constructor: "BlofinExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Breakoutprop,
        module: "./breakoutprop/breakoutprop.exchange",
        constructor: "BreakoutpropExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bybit,
        module: "./bybit/bybit.exchange",
        constructor: "BybitExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bybitdemo,
        module: "./bybit/bybitdemo.exchange",
        constructor: "BybitDemoExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Hyrotrader,
        module: "./bybit/hyrotrader.exchange",
        constructor: "HyrotraderExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bybitinverse,
        module: "./bybit/bybitinverse.exchange",
        constructor: "BybitInverseExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Bybitspot,
        module: "./bybit/bybitspot.exchange",
        constructor: "BybitSpotExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Coinbase,
        module: "./coinbase/coinbase.exchange",
        constructor: "CoinbaseExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Coinbasespot,
        module: "./coinbase/coinbasespot.exchange",
        constructor: "CoinbaseSpotExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Cryptocom,
        module: "./cryptocom/cryptocom.exchange",
        constructor: "CryptocomExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Grvt,
        module: "./grvt/grvt.exchange",
        constructor: "GrvtExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Hyperliquid,
        module: "./hyperliquid/hyperliquid.exchange",
        constructor: "HyperliquidExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Hyperliquidspot,
        module: "./hyperliquid/hyperliquidspot.exchange",
        constructor: "HyperliquidSpotExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Kraken,
        module: "./kraken/kraken.exchange",
        constructor: "KrakenExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Krakenspot,
        module: "./kraken/krakenspot.exchange",
        constructor: "KrakenSpotExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Kucoin,
        module: "./kucoin/kucoin.exchange",
        constructor: "KucoinExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Kucoinspot,
        module: "./kucoin/kucoinspot.exchange",
        constructor: "KucoinSpotExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Lighter,
        module: "./lighter/lighter.exchange",
        constructor: "LighterExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::LighterRobinhood,
        module: "./lighter/lighter.exchange",
        constructor: "LighterExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Mexc,
        module: "./mexc/mexc.exchange",
        constructor: "MexcExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Okx,
        module: "./okx/okx.exchange",
        constructor: "OKXExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Okxdemo,
        module: "./okx/okxdemo.exchange",
        constructor: "OKXDemoExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Okxeu,
        module: "./okx/okxeu.exchange",
        constructor: "OKXEUExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Okxeudemo,
        module: "./okx/okxeudemo.exchange",
        constructor: "OKXEUDemoExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Paradex,
        module: "./paradex/paradex.exchange",
        constructor: "ParadexExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Phemex,
        module: "./phemex/phemex.exchange",
        constructor: "PhemexExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Polymarket,
        module: "./polymarket/polymarket.exchange",
        constructor: "PolymarketExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Weex,
        module: "./weex/weex.exchange",
        constructor: "WeexExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Woo,
        module: "./woo/woo.exchange",
        constructor: "WOOXExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Kodiakfi,
        module: "./orderly/exchanges/kodiakfi/kodiakfi.exchange",
        constructor: "KodiakFIExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Mode,
        module: "./orderly/exchanges/mode/mode.exchange",
        constructor: "MODEExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Salsadex,
        module: "./orderly/exchanges/salsadex/salsadex.exchange",
        constructor: "SalsaDEXExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::TealstreetDex,
        module: "./orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange",
        constructor: "TealstreetDEXExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Txflow,
        module: "./txflow/txflow.exchange",
        constructor: "TxFlowExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::What,
        module: "./orderly/exchanges/what/what.exchange",
        constructor: "WhatExchange",
    },
    ExchangeRegistration {
        name: ExchangeName::Woofi,
        module: "./orderly/exchanges/woofi/woofi.exchange",
        constructor: "WoofiExchange",
    },
];

pub fn get_registered_exchange_names() -> Vec<ExchangeName> {
    EXCHANGE_REGISTRY
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

/// Wrap a synchronous native loader without invoking it until the returned
/// loader is called. The callback runs before the ready future is constructed.
/// Promise microtask scheduling is delegated to the Rust host's future executor.
pub fn load<'a, T: 'a, F: Fn() -> T + 'a>(
    loader: F,
) -> Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'a>> + 'a> {
    Box::new(move || {
        let value = loader();
        Box::pin(std::future::ready(value))
    })
}
