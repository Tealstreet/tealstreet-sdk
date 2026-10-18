//! Executable counterparts of the web and native registry entry functions.
use crate::module_host::{
    self, ConstructorFuture, NativeConstructorResult, NativeModuleHost, WebModuleHost,
};
use safe_cex_types::ExchangeName;

pub struct ExchangeLoaderRegistration {
    pub name: ExchangeName,
    pub web: for<'a> fn(&'a dyn WebModuleHost) -> ConstructorFuture<'a>,
    pub native: fn(&dyn NativeModuleHost) -> NativeConstructorResult,
}

// ts: exchangeRegistry.apexomni (web and native source files)
pub fn web_apexomni(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./apexomni/apexomni.exchange", "ApexOmniExchange")
}
pub fn native_apexomni(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./apexomni/apexomni.exchange", "ApexOmniExchange")
}

// ts: exchangeRegistry.asterdex (web and native source files)
pub fn web_asterdex(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./asterdex/asterdex.exchange", "AsterDEXExchange")
}
pub fn native_asterdex(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./asterdex/asterdex.exchange", "AsterDEXExchange")
}

// ts: exchangeRegistry.backpack (web and native source files)
pub fn web_backpack(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./backpack/backpack.exchange", "BackpackExchange")
}
pub fn native_backpack(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./backpack/backpack.exchange", "BackpackExchange")
}

// ts: exchangeRegistry.binance (web and native source files)
pub fn web_binance(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./binance/binance.exchange", "BinanceExchange")
}
pub fn native_binance(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./binance/binance.exchange", "BinanceExchange")
}

// ts: exchangeRegistry.binanceinverse (web and native source files)
pub fn web_binanceinverse(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./binance/binanceinverse.exchange",
        "BinanceInverseExchange",
    )
}
pub fn native_binanceinverse(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./binance/binanceinverse.exchange",
        "BinanceInverseExchange",
    )
}

// ts: exchangeRegistry.binancepm (web and native source files)
pub fn web_binancepm(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./binance/binancepm.exchange", "BinancePMExchange")
}
pub fn native_binancepm(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./binance/binancepm.exchange", "BinancePMExchange")
}

// ts: exchangeRegistry.binancespot (web and native source files)
pub fn web_binancespot(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./binance/binancespot.exchange",
        "BinanceSpotExchange",
    )
}
pub fn native_binancespot(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./binance/binancespot.exchange",
        "BinanceSpotExchange",
    )
}

// ts: exchangeRegistry.bingx (web and native source files)
pub fn web_bingx(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bingx/bingx.exchange", "BingxExchange")
}
pub fn native_bingx(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bingx/bingx.exchange", "BingxExchange")
}

// ts: exchangeRegistry.bitget (web and native source files)
pub fn web_bitget(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bitget/bitget.exchange", "BitgetExchange")
}
pub fn native_bitget(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bitget/bitget.exchange", "BitgetExchange")
}

// ts: exchangeRegistry.bitgetinverse (web and native source files)
pub fn web_bitgetinverse(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./bitget/bitgetinverse.exchange",
        "BitgetInverseExchange",
    )
}
pub fn native_bitgetinverse(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./bitget/bitgetinverse.exchange",
        "BitgetInverseExchange",
    )
}

// ts: exchangeRegistry.bitgetusdc (web and native source files)
pub fn web_bitgetusdc(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bitget/bitgetusdc.exchange", "BitgetUSDCExchange")
}
pub fn native_bitgetusdc(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bitget/bitgetusdc.exchange", "BitgetUSDCExchange")
}

// ts: exchangeRegistry.bitrue (web and native source files)
pub fn web_bitrue(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bitrue/bitrue.exchange", "BitrueExchange")
}
pub fn native_bitrue(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bitrue/bitrue.exchange", "BitrueExchange")
}

// ts: exchangeRegistry.bitunix (web and native source files)
pub fn web_bitunix(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bitunix/bitunix.exchange", "BitunixExchange")
}
pub fn native_bitunix(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bitunix/bitunix.exchange", "BitunixExchange")
}

// ts: exchangeRegistry.blofin (web and native source files)
pub fn web_blofin(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./blofin/blofin.exchange", "BlofinExchange")
}
pub fn native_blofin(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./blofin/blofin.exchange", "BlofinExchange")
}

// ts: exchangeRegistry.breakoutprop (web and native source files)
pub fn web_breakoutprop(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./breakoutprop/breakoutprop.exchange",
        "BreakoutpropExchange",
    )
}
pub fn native_breakoutprop(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./breakoutprop/breakoutprop.exchange",
        "BreakoutpropExchange",
    )
}

// ts: exchangeRegistry.bybit (web and native source files)
pub fn web_bybit(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bybit/bybit.exchange", "BybitExchange")
}
pub fn native_bybit(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bybit/bybit.exchange", "BybitExchange")
}

// ts: exchangeRegistry.bybitdemo (web and native source files)
pub fn web_bybitdemo(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bybit/bybitdemo.exchange", "BybitDemoExchange")
}
pub fn native_bybitdemo(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bybit/bybitdemo.exchange", "BybitDemoExchange")
}

// ts: exchangeRegistry.hyrotrader (web and native source files)
pub fn web_hyrotrader(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bybit/hyrotrader.exchange", "HyrotraderExchange")
}
pub fn native_hyrotrader(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bybit/hyrotrader.exchange", "HyrotraderExchange")
}

// ts: exchangeRegistry.bybitinverse (web and native source files)
pub fn web_bybitinverse(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./bybit/bybitinverse.exchange",
        "BybitInverseExchange",
    )
}
pub fn native_bybitinverse(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./bybit/bybitinverse.exchange",
        "BybitInverseExchange",
    )
}

// ts: exchangeRegistry.bybitspot (web and native source files)
pub fn web_bybitspot(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./bybit/bybitspot.exchange", "BybitSpotExchange")
}
pub fn native_bybitspot(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./bybit/bybitspot.exchange", "BybitSpotExchange")
}

// ts: exchangeRegistry.coinbase (web and native source files)
pub fn web_coinbase(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./coinbase/coinbase.exchange", "CoinbaseExchange")
}
pub fn native_coinbase(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./coinbase/coinbase.exchange", "CoinbaseExchange")
}

// ts: exchangeRegistry.coinbasespot (web and native source files)
pub fn web_coinbasespot(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./coinbase/coinbasespot.exchange",
        "CoinbaseSpotExchange",
    )
}
pub fn native_coinbasespot(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./coinbase/coinbasespot.exchange",
        "CoinbaseSpotExchange",
    )
}

// ts: exchangeRegistry.cryptocom (web and native source files)
pub fn web_cryptocom(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./cryptocom/cryptocom.exchange", "CryptocomExchange")
}
pub fn native_cryptocom(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./cryptocom/cryptocom.exchange", "CryptocomExchange")
}

// ts: exchangeRegistry.grvt (web and native source files)
pub fn web_grvt(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./grvt/grvt.exchange", "GrvtExchange")
}
pub fn native_grvt(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./grvt/grvt.exchange", "GrvtExchange")
}

// ts: exchangeRegistry.hyperliquid (web and native source files)
pub fn web_hyperliquid(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./hyperliquid/hyperliquid.exchange",
        "HyperliquidExchange",
    )
}
pub fn native_hyperliquid(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./hyperliquid/hyperliquid.exchange",
        "HyperliquidExchange",
    )
}

// ts: exchangeRegistry.hyperliquidspot (web and native source files)
pub fn web_hyperliquidspot(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./hyperliquid/hyperliquidspot.exchange",
        "HyperliquidSpotExchange",
    )
}
pub fn native_hyperliquidspot(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./hyperliquid/hyperliquidspot.exchange",
        "HyperliquidSpotExchange",
    )
}

// ts: exchangeRegistry.kraken (web and native source files)
pub fn web_kraken(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./kraken/kraken.exchange", "KrakenExchange")
}
pub fn native_kraken(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./kraken/kraken.exchange", "KrakenExchange")
}

// ts: exchangeRegistry.krakenspot (web and native source files)
pub fn web_krakenspot(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./kraken/krakenspot.exchange", "KrakenSpotExchange")
}
pub fn native_krakenspot(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./kraken/krakenspot.exchange", "KrakenSpotExchange")
}

// ts: exchangeRegistry.kucoin (web and native source files)
pub fn web_kucoin(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./kucoin/kucoin.exchange", "KucoinExchange")
}
pub fn native_kucoin(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./kucoin/kucoin.exchange", "KucoinExchange")
}

// ts: exchangeRegistry.kucoinspot (web and native source files)
pub fn web_kucoinspot(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./kucoin/kucoinspot.exchange", "KucoinSpotExchange")
}
pub fn native_kucoinspot(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./kucoin/kucoinspot.exchange", "KucoinSpotExchange")
}

// ts: exchangeRegistry.lighter (web and native source files)
pub fn web_lighter(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./lighter/lighter.exchange", "LighterExchange")
}
pub fn native_lighter(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./lighter/lighter.exchange", "LighterExchange")
}

// ts: exchangeRegistry.lighter-robinhood (web and native source files)
pub fn web_lighter_robinhood(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./lighter/lighter.exchange", "LighterExchange")
}
pub fn native_lighter_robinhood(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./lighter/lighter.exchange", "LighterExchange")
}

// ts: exchangeRegistry.mexc (web and native source files)
pub fn web_mexc(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./mexc/mexc.exchange", "MexcExchange")
}
pub fn native_mexc(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./mexc/mexc.exchange", "MexcExchange")
}

// ts: exchangeRegistry.okx (web and native source files)
pub fn web_okx(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./okx/okx.exchange", "OKXExchange")
}
pub fn native_okx(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./okx/okx.exchange", "OKXExchange")
}

// ts: exchangeRegistry.okxdemo (web and native source files)
pub fn web_okxdemo(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./okx/okxdemo.exchange", "OKXDemoExchange")
}
pub fn native_okxdemo(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./okx/okxdemo.exchange", "OKXDemoExchange")
}

// ts: exchangeRegistry.okxeu (web and native source files)
pub fn web_okxeu(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./okx/okxeu.exchange", "OKXEUExchange")
}
pub fn native_okxeu(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./okx/okxeu.exchange", "OKXEUExchange")
}

// ts: exchangeRegistry.okxeudemo (web and native source files)
pub fn web_okxeudemo(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./okx/okxeudemo.exchange", "OKXEUDemoExchange")
}
pub fn native_okxeudemo(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./okx/okxeudemo.exchange", "OKXEUDemoExchange")
}

// ts: exchangeRegistry.paradex (web and native source files)
pub fn web_paradex(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./paradex/paradex.exchange", "ParadexExchange")
}
pub fn native_paradex(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./paradex/paradex.exchange", "ParadexExchange")
}

// ts: exchangeRegistry.phemex (web and native source files)
pub fn web_phemex(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./phemex/phemex.exchange", "PhemexExchange")
}
pub fn native_phemex(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./phemex/phemex.exchange", "PhemexExchange")
}

// ts: exchangeRegistry.polymarket (web and native source files)
pub fn web_polymarket(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./polymarket/polymarket.exchange",
        "PolymarketExchange",
    )
}
pub fn native_polymarket(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./polymarket/polymarket.exchange",
        "PolymarketExchange",
    )
}

// ts: exchangeRegistry.weex (web and native source files)
pub fn web_weex(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./weex/weex.exchange", "WeexExchange")
}
pub fn native_weex(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./weex/weex.exchange", "WeexExchange")
}

// ts: exchangeRegistry.woo (web and native source files)
pub fn web_woo(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./woo/woo.exchange", "WOOXExchange")
}
pub fn native_woo(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./woo/woo.exchange", "WOOXExchange")
}

// ts: exchangeRegistry.kodiakfi (web and native source files)
pub fn web_kodiakfi(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./orderly/exchanges/kodiakfi/kodiakfi.exchange",
        "KodiakFIExchange",
    )
}
pub fn native_kodiakfi(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./orderly/exchanges/kodiakfi/kodiakfi.exchange",
        "KodiakFIExchange",
    )
}

// ts: exchangeRegistry.mode (web and native source files)
pub fn web_mode(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./orderly/exchanges/mode/mode.exchange",
        "MODEExchange",
    )
}
pub fn native_mode(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./orderly/exchanges/mode/mode.exchange",
        "MODEExchange",
    )
}

// ts: exchangeRegistry.salsadex (web and native source files)
pub fn web_salsadex(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./orderly/exchanges/salsadex/salsadex.exchange",
        "SalsaDEXExchange",
    )
}
pub fn native_salsadex(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./orderly/exchanges/salsadex/salsadex.exchange",
        "SalsaDEXExchange",
    )
}

// ts: exchangeRegistry.tealstreet-dex (web and native source files)
pub fn web_tealstreet_dex(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange",
        "TealstreetDEXExchange",
    )
}
pub fn native_tealstreet_dex(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange",
        "TealstreetDEXExchange",
    )
}

// ts: exchangeRegistry.txflow (web and native source files)
pub fn web_txflow(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(host, "./txflow/txflow.exchange", "TxFlowExchange")
}
pub fn native_txflow(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(host, "./txflow/txflow.exchange", "TxFlowExchange")
}

// ts: exchangeRegistry.what (web and native source files)
pub fn web_what(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./orderly/exchanges/what/what.exchange",
        "WhatExchange",
    )
}
pub fn native_what(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./orderly/exchanges/what/what.exchange",
        "WhatExchange",
    )
}

// ts: exchangeRegistry.woofi (web and native source files)
pub fn web_woofi(host: &dyn WebModuleHost) -> ConstructorFuture<'_> {
    module_host::web_load(
        host,
        "./orderly/exchanges/woofi/woofi.exchange",
        "WoofiExchange",
    )
}
pub fn native_woofi(host: &dyn NativeModuleHost) -> NativeConstructorResult {
    module_host::native_load(
        host,
        "./orderly/exchanges/woofi/woofi.exchange",
        "WoofiExchange",
    )
}

pub const EXCHANGE_LOADERS: &[ExchangeLoaderRegistration] = &[
    ExchangeLoaderRegistration {
        name: ExchangeName::Apexomni,
        web: web_apexomni,
        native: native_apexomni,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Asterdex,
        web: web_asterdex,
        native: native_asterdex,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Backpack,
        web: web_backpack,
        native: native_backpack,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Binance,
        web: web_binance,
        native: native_binance,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Binanceinverse,
        web: web_binanceinverse,
        native: native_binanceinverse,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Binancepm,
        web: web_binancepm,
        native: native_binancepm,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Binancespot,
        web: web_binancespot,
        native: native_binancespot,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bingx,
        web: web_bingx,
        native: native_bingx,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bitget,
        web: web_bitget,
        native: native_bitget,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bitgetinverse,
        web: web_bitgetinverse,
        native: native_bitgetinverse,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bitgetusdc,
        web: web_bitgetusdc,
        native: native_bitgetusdc,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bitrue,
        web: web_bitrue,
        native: native_bitrue,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bitunix,
        web: web_bitunix,
        native: native_bitunix,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Blofin,
        web: web_blofin,
        native: native_blofin,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Breakoutprop,
        web: web_breakoutprop,
        native: native_breakoutprop,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bybit,
        web: web_bybit,
        native: native_bybit,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bybitdemo,
        web: web_bybitdemo,
        native: native_bybitdemo,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Hyrotrader,
        web: web_hyrotrader,
        native: native_hyrotrader,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bybitinverse,
        web: web_bybitinverse,
        native: native_bybitinverse,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Bybitspot,
        web: web_bybitspot,
        native: native_bybitspot,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Coinbase,
        web: web_coinbase,
        native: native_coinbase,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Coinbasespot,
        web: web_coinbasespot,
        native: native_coinbasespot,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Cryptocom,
        web: web_cryptocom,
        native: native_cryptocom,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Grvt,
        web: web_grvt,
        native: native_grvt,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Hyperliquid,
        web: web_hyperliquid,
        native: native_hyperliquid,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Hyperliquidspot,
        web: web_hyperliquidspot,
        native: native_hyperliquidspot,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Kraken,
        web: web_kraken,
        native: native_kraken,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Krakenspot,
        web: web_krakenspot,
        native: native_krakenspot,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Kucoin,
        web: web_kucoin,
        native: native_kucoin,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Kucoinspot,
        web: web_kucoinspot,
        native: native_kucoinspot,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Lighter,
        web: web_lighter,
        native: native_lighter,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::LighterRobinhood,
        web: web_lighter_robinhood,
        native: native_lighter_robinhood,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Mexc,
        web: web_mexc,
        native: native_mexc,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Okx,
        web: web_okx,
        native: native_okx,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Okxdemo,
        web: web_okxdemo,
        native: native_okxdemo,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Okxeu,
        web: web_okxeu,
        native: native_okxeu,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Okxeudemo,
        web: web_okxeudemo,
        native: native_okxeudemo,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Paradex,
        web: web_paradex,
        native: native_paradex,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Phemex,
        web: web_phemex,
        native: native_phemex,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Polymarket,
        web: web_polymarket,
        native: native_polymarket,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Weex,
        web: web_weex,
        native: native_weex,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Woo,
        web: web_woo,
        native: native_woo,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Kodiakfi,
        web: web_kodiakfi,
        native: native_kodiakfi,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Mode,
        web: web_mode,
        native: native_mode,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Salsadex,
        web: web_salsadex,
        native: native_salsadex,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::TealstreetDex,
        web: web_tealstreet_dex,
        native: native_tealstreet_dex,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Txflow,
        web: web_txflow,
        native: native_txflow,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::What,
        web: web_what,
        native: native_what,
    },
    ExchangeLoaderRegistration {
        name: ExchangeName::Woofi,
        web: web_woofi,
        native: native_woofi,
    },
];
