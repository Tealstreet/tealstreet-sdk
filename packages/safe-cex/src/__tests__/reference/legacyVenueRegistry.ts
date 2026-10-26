// Test-only pre-beta adapter characterization; never imported by SDK runtime.

import type { BaseExchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';

type ExchangeLoader = () => Promise<typeof BaseExchange>;

export const legacyVenueRegistry: Record<ExchangeName, ExchangeLoader> = {
  // Apex Omni
  apexomni: () =>
    import(/* webpackChunkName: "exchange-apexomni" */ '../../exchanges/apexomni/apexomni.exchange').then(
      (m) => m.ApexOmniExchange,
    ),

  // AsterDEX
  asterdex: () =>
    import(/* webpackChunkName: "exchange-asterdex" */ '../../exchanges/asterdex/asterdex.exchange').then(
      (m) => m.AsterDEXExchange,
    ),

  // Backpack
  backpack: () =>
    import(/* webpackChunkName: "exchange-backpack" */ '../../exchanges/backpack/backpack.exchange').then(
      (m) => m.BackpackExchange,
    ),

  // Binance
  binance: () =>
    import(/* webpackChunkName: "exchange-binance" */ '../../exchanges/binance/binance.exchange').then(
      (m) => m.BinanceExchange,
    ),

  // Binance Options (European options, eapi)
  binanceoptions: () =>
    import(/* webpackChunkName: "exchange-binanceoptions" */ '../../exchanges/binance/binanceoptions.exchange').then(
      (m) => m.BinanceOptionsExchange,
    ),

  // Binance Inverse (COIN-M Futures)
  binanceinverse: () =>
    import(/* webpackChunkName: "exchange-binanceinverse" */ '../../exchanges/binance/binanceinverse.exchange').then(
      (m) => m.BinanceInverseExchange,
    ),

  // Binance PM (Portfolio Margin)
  binancepm: () =>
    import(/* webpackChunkName: "exchange-binancepm" */ '../../exchanges/binance/binancepm.exchange').then(
      (m) => m.BinancePMExchange,
    ),

  // Binance Spot
  binancespot: () =>
    import(/* webpackChunkName: "exchange-binancespot" */ '../../exchanges/binance/binancespot.exchange').then(
      (m) => m.BinanceSpotExchange,
    ),

  // BingX
  bingx: () =>
    import(/* webpackChunkName: "exchange-bingx" */ '../../exchanges/bingx/bingx.exchange').then(
      (m) => m.BingxExchange,
    ),

  // Bitget
  bitget: () =>
    import(/* webpackChunkName: "exchange-bitget" */ '../../exchanges/bitget/bitget.exchange').then(
      (m) => m.BitgetExchange,
    ),

  // Bitget Inverse
  bitgetinverse: () =>
    import(/* webpackChunkName: "exchange-bitgetinverse" */ '../../exchanges/bitget/bitgetinverse.exchange').then(
      (m) => m.BitgetInverseExchange,
    ),

  // Bitget USDC
  bitgetusdc: () =>
    import(/* webpackChunkName: "exchange-bitgetusdc" */ '../../exchanges/bitget/bitgetusdc.exchange').then(
      (m) => m.BitgetUSDCExchange,
    ),

  // Bitrue
  bitrue: () =>
    import(/* webpackChunkName: "exchange-bitrue" */ '../../exchanges/bitrue/bitrue.exchange').then(
      (m) => m.BitrueExchange,
    ),

  // Bitunix
  bitunix: () =>
    import(/* webpackChunkName: "exchange-bitunix" */ '../../exchanges/bitunix/bitunix.exchange').then(
      (m) => m.BitunixExchange,
    ),

  // BloFin
  blofin: () =>
    import(/* webpackChunkName: "exchange-blofin" */ '../../exchanges/blofin/blofin.exchange').then(
      (m) => m.BlofinExchange,
    ),

  // Breakoutprop
  breakoutprop: () =>
    import(/* webpackChunkName: "exchange-breakoutprop" */ '../../exchanges/breakoutprop/breakoutprop.exchange').then(
      (m) => m.BreakoutpropExchange,
    ),

  // Bybit
  bybit: () =>
    import(/* webpackChunkName: "exchange-bybit" */ '../../exchanges/bybit/bybit.exchange').then(
      (m) => m.BybitExchange,
    ),

  // Bybit Demo
  bybitdemo: () =>
    import(/* webpackChunkName: "exchange-bybitdemo" */ '../../exchanges/bybit/bybitdemo.exchange').then(
      (m) => m.BybitDemoExchange,
    ),

  // HyroTrader
  hyrotrader: () =>
    import(/* webpackChunkName: "exchange-hyrotrader" */ '../../exchanges/bybit/hyrotrader.exchange').then(
      (m) => m.HyrotraderExchange,
    ),

  // Bybit Inverse
  bybitinverse: () =>
    import(/* webpackChunkName: "exchange-bybitinverse" */ '../../exchanges/bybit/bybitinverse.exchange').then(
      (m) => m.BybitInverseExchange,
    ),

  // Bybit Spot
  bybitspot: () =>
    import(/* webpackChunkName: "exchange-bybitspot" */ '../../exchanges/bybit/bybitspot.exchange').then(
      (m) => m.BybitSpotExchange,
    ),

  // Coinbase Advanced Trade and saved spot-only accounts
  coinbase: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),
  coinbasespot: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),

  // Crypto.com
  cryptocom: () =>
    import(/* webpackChunkName: "exchange-cryptocom" */ '../../exchanges/cryptocom/cryptocom.exchange').then(
      (m) => m.CryptocomExchange,
    ),

  // GRVT (heavy - ethers + SDK dependencies)
  grvt: () =>
    import(/* webpackChunkName: "exchange-grvt" */ '../../exchanges/grvt/grvt.exchange').then((m) => m.GrvtExchange),

  // Hyperliquid
  hyperliquid: () =>
    import(/* webpackChunkName: "exchange-hyperliquid" */ '../../exchanges/hyperliquid/hyperliquid.exchange').then(
      (m) => m.HyperliquidExchange,
    ),

  // Hyperliquid Spot
  hyperliquidspot: () =>
    import(
      /* webpackChunkName: "exchange-hyperliquidspot" */ '../../exchanges/hyperliquid/hyperliquidspot.exchange'
    ).then((m) => m.HyperliquidSpotExchange),

  // Kraken
  kraken: () =>
    import(/* webpackChunkName: "exchange-kraken" */ '../../exchanges/kraken/kraken.exchange').then(
      (m) => m.KrakenExchange,
    ),

  // Kraken Spot
  krakenspot: () =>
    import(/* webpackChunkName: "exchange-krakenspot" */ '../../exchanges/kraken/krakenspot.exchange').then(
      (m) => m.KrakenSpotExchange,
    ),

  // KuCoin
  kucoin: () =>
    import(/* webpackChunkName: "exchange-kucoin" */ '../../exchanges/kucoin/kucoin.exchange').then(
      (m) => m.KucoinExchange,
    ),

  // KuCoin Spot
  kucoinspot: () =>
    import(/* webpackChunkName: "exchange-kucoinspot" */ '../../exchanges/kucoin/kucoinspot.exchange').then(
      (m) => m.KucoinSpotExchange,
    ),

  // Lighter (heavy - WASM crypto dependencies)
  lighter: () =>
    import(/* webpackChunkName: "exchange-lighter" */ '../../exchanges/lighter/lighter.exchange').then(
      (m) => m.LighterExchange,
    ),
  'lighter-robinhood': () =>
    import(/* webpackChunkName: "exchange-lighter" */ '../../exchanges/lighter/lighter.exchange').then(
      (m) => m.LighterExchange,
    ),

  // MEXC
  mexc: () =>
    import(/* webpackChunkName: "exchange-mexc" */ '../../exchanges/mexc/mexc.exchange').then((m) => m.MexcExchange),

  // OKX
  okx: () =>
    import(/* webpackChunkName: "exchange-okx" */ '../../exchanges/okx/okx.exchange').then((m) => m.OKXExchange),

  // OKX Demo
  okxdemo: () =>
    import(/* webpackChunkName: "exchange-okxdemo" */ '../../exchanges/okx/okxdemo.exchange').then(
      (m) => m.OKXDemoExchange,
    ),

  // OKX EU (European entity, separate API host)
  okxeu: () =>
    import(/* webpackChunkName: "exchange-okxeu" */ '../../exchanges/okx/okxeu.exchange').then((m) => m.OKXEUExchange),

  // OKX EU Demo
  okxeudemo: () =>
    import(/* webpackChunkName: "exchange-okxeudemo" */ '../../exchanges/okx/okxeudemo.exchange').then(
      (m) => m.OKXEUDemoExchange,
    ),

  // Paradex (heavy - StarkEx/Starknet dependencies)
  paradex: () =>
    import(/* webpackChunkName: "exchange-paradex" */ '../../exchanges/paradex/paradex.exchange').then(
      (m) => m.ParadexExchange,
    ),

  // Phemex
  phemex: () =>
    import(/* webpackChunkName: "exchange-phemex" */ '../../exchanges/phemex/phemex.exchange').then(
      (m) => m.PhemexExchange,
    ),

  // Polymarket (heavy - ethers for EIP-712 signing)
  polymarket: () =>
    import(/* webpackChunkName: "exchange-polymarket" */ '../../exchanges/polymarket/polymarket.exchange').then(
      (m) => m.PolymarketExchange,
    ),

  // WEEX
  weex: () =>
    import(/* webpackChunkName: "exchange-weex" */ '../../exchanges/weex/weex.exchange').then((m) => m.WeexExchange),

  // WooX
  woo: () =>
    import(/* webpackChunkName: "exchange-woo" */ '../../exchanges/woo/woo.exchange').then((m) => m.WOOXExchange),

  // Orderly Network exchanges
  kodiakfi: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),

  mode: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),

  salsadex: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),

  'tealstreet-dex': () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),

  // TxFlow testnet
  txflow: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),

  what: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),

  woofi: () => import('../../exchanges/rust/rust.exchange').then((m) => m.RustEngineExchange),
};

export const getRegisteredExchangeNames = (): ExchangeName[] => {
  return Object.keys(legacyVenueRegistry) as ExchangeName[];
};

// Kept hosted signing codecs are independent reference oracles, never SDK loaders.
export const hostedSigningReferenceRegistry = {
  coinbase: () => import('../../exchanges/coinbase/coinbase.exchange').then((m) => m.CoinbaseExchange),
  coinbasespot: () => import('../../exchanges/coinbase/coinbasespot.exchange').then((m) => m.CoinbaseSpotExchange),
  kodiakfi: () => import('./orderly/exchanges/kodiakfi/kodiakfi.exchange').then((m) => m.KodiakFIExchange),
  mode: () => import('./orderly/exchanges/mode/mode.exchange').then((m) => m.MODEExchange),
  salsadex: () => import('./orderly/exchanges/salsadex/salsadex.exchange').then((m) => m.SalsaDEXExchange),
  'tealstreet-dex': () => import('./orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange').then((m) => m.TealstreetDEXExchange),
  what: () => import('./orderly/exchanges/what/what.exchange').then((m) => m.WhatExchange),
  woofi: () => import('./orderly/exchanges/woofi/woofi.exchange').then((m) => m.WoofiExchange),
} as Partial<Record<ExchangeName, ExchangeLoader>>;
