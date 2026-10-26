/**
 * Exchange Registry
 *
 * Maps exchange names to their dynamic import functions.
 * This enables lazy-loading of exchange implementations to reduce initial bundle size.
 *
 * Each exchange is loaded only when createExchange() is called for that exchange,
 * keeping the initial bundle small regardless of how many exchanges are supported.
 */

import type { ExchangeName } from '../types';
import type { BaseExchange } from './base';

type ExchangeLoader = () => Promise<typeof BaseExchange>;

/**
 * Registry mapping exchange names to their dynamic import functions.
 * Uses webpack magic comments for named chunks to improve caching and debugging.
 */
export const exchangeRegistry: Record<ExchangeName, ExchangeLoader> = {
  // Apex Omni
  apexomni: () =>
    import(/* webpackChunkName: "exchange-apexomni" */ '../__tests__/reference/apexomni/apexomni.exchange').then((m) => m.ApexOmniExchange),

  // AsterDEX
  asterdex: () =>
    import(/* webpackChunkName: "exchange-asterdex" */ '../__tests__/reference/asterdex/asterdex.exchange').then((m) => m.AsterDEXExchange),

  // Backpack
  backpack: () =>
    import(/* webpackChunkName: "exchange-backpack" */ '../__tests__/reference/backpack/backpack.exchange').then((m) => m.BackpackExchange),

  // Binance
  binance: () =>
    import(/* webpackChunkName: "exchange-binance" */ '../__tests__/reference/binance/binance.exchange').then((m) => m.BinanceExchange),

  // Binance Options (European options, eapi)
  binanceoptions: () =>
    import(/* webpackChunkName: "exchange-binanceoptions" */ '../__tests__/reference/binance/binanceoptions.exchange').then(
      (m) => m.BinanceOptionsExchange,
    ),

  // Binance Inverse (COIN-M Futures)
  binanceinverse: () =>
    import(/* webpackChunkName: "exchange-binanceinverse" */ '../__tests__/reference/binance/binanceinverse.exchange').then(
      (m) => m.BinanceInverseExchange,
    ),

  // Binance PM (Portfolio Margin)
  binancepm: () =>
    import(/* webpackChunkName: "exchange-binancepm" */ '../__tests__/reference/binance/binancepm.exchange').then(
      (m) => m.BinancePMExchange,
    ),

  // Binance Spot
  binancespot: () =>
    import(/* webpackChunkName: "exchange-binancespot" */ '../__tests__/reference/binance/binancespot.exchange').then(
      (m) => m.BinanceSpotExchange,
    ),

  // BingX
  bingx: () => import(/* webpackChunkName: "exchange-bingx" */ '../__tests__/reference/bingx/bingx.exchange').then((m) => m.BingxExchange),

  // Bitget
  bitget: () =>
    import(/* webpackChunkName: "exchange-bitget" */ '../__tests__/reference/bitget/bitget.exchange').then((m) => m.BitgetExchange),

  // Bitget Inverse
  bitgetinverse: () =>
    import(/* webpackChunkName: "exchange-bitgetinverse" */ '../__tests__/reference/bitget/bitgetinverse.exchange').then(
      (m) => m.BitgetInverseExchange,
    ),

  // Bitget USDC
  bitgetusdc: () =>
    import(/* webpackChunkName: "exchange-bitgetusdc" */ '../__tests__/reference/bitget/bitgetusdc.exchange').then(
      (m) => m.BitgetUSDCExchange,
    ),

  // Bitrue
  bitrue: () =>
    import(/* webpackChunkName: "exchange-bitrue" */ '../__tests__/reference/bitrue/bitrue.exchange').then((m) => m.BitrueExchange),

  // Bitunix
  bitunix: () =>
    import(/* webpackChunkName: "exchange-bitunix" */ '../__tests__/reference/bitunix/bitunix.exchange').then((m) => m.BitunixExchange),

  // BloFin
  blofin: () =>
    import(/* webpackChunkName: "exchange-blofin" */ '../__tests__/reference/blofin/blofin.exchange').then((m) => m.BlofinExchange),

  // Breakoutprop
  breakoutprop: () =>
    import(/* webpackChunkName: "exchange-breakoutprop" */ '../__tests__/reference/breakoutprop/breakoutprop.exchange').then(
      (m) => m.BreakoutpropExchange,
    ),

  // Bybit
  bybit: () => import(/* webpackChunkName: "exchange-bybit" */ '../__tests__/reference/bybit/bybit.exchange').then((m) => m.BybitExchange),

  // Bybit Demo
  bybitdemo: () =>
    import(/* webpackChunkName: "exchange-bybitdemo" */ '../__tests__/reference/bybit/bybitdemo.exchange').then((m) => m.BybitDemoExchange),

  // HyroTrader
  hyrotrader: () =>
    import(/* webpackChunkName: "exchange-hyrotrader" */ '../__tests__/reference/bybit/hyrotrader.exchange').then(
      (m) => m.HyrotraderExchange,
    ),

  // Bybit Inverse
  bybitinverse: () =>
    import(/* webpackChunkName: "exchange-bybitinverse" */ '../__tests__/reference/bybit/bybitinverse.exchange').then(
      (m) => m.BybitInverseExchange,
    ),

  // Bybit Spot
  bybitspot: () =>
    import(/* webpackChunkName: "exchange-bybitspot" */ '../__tests__/reference/bybit/bybitspot.exchange').then((m) => m.BybitSpotExchange),

  // Coinbase Advanced Trade and saved spot-only accounts
  coinbase: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),
  coinbasespot: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),

  // Crypto.com
  cryptocom: () =>
    import(/* webpackChunkName: "exchange-cryptocom" */ '../__tests__/reference/cryptocom/cryptocom.exchange').then(
      (m) => m.CryptocomExchange,
    ),

  // GRVT (heavy - ethers + SDK dependencies)
  grvt: () => import(/* webpackChunkName: "exchange-grvt" */ '../__tests__/reference/grvt/grvt.exchange').then((m) => m.GrvtExchange),

  // Hyperliquid
  hyperliquid: () =>
    import(/* webpackChunkName: "exchange-hyperliquid" */ '../__tests__/reference/hyperliquid/hyperliquid.exchange').then(
      (m) => m.HyperliquidExchange,
    ),

  // Hyperliquid Spot
  hyperliquidspot: () =>
    import(/* webpackChunkName: "exchange-hyperliquidspot" */ '../__tests__/reference/hyperliquid/hyperliquidspot.exchange').then(
      (m) => m.HyperliquidSpotExchange,
    ),

  // Kraken
  kraken: () =>
    import(/* webpackChunkName: "exchange-kraken" */ '../__tests__/reference/kraken/kraken.exchange').then((m) => m.KrakenExchange),

  // Kraken Spot
  krakenspot: () =>
    import(/* webpackChunkName: "exchange-krakenspot" */ '../__tests__/reference/kraken/krakenspot.exchange').then(
      (m) => m.KrakenSpotExchange,
    ),

  // KuCoin
  kucoin: () =>
    import(/* webpackChunkName: "exchange-kucoin" */ '../__tests__/reference/kucoin/kucoin.exchange').then((m) => m.KucoinExchange),

  // KuCoin Spot
  kucoinspot: () =>
    import(/* webpackChunkName: "exchange-kucoinspot" */ '../__tests__/reference/kucoin/kucoinspot.exchange').then(
      (m) => m.KucoinSpotExchange,
    ),

  // Lighter (heavy - WASM crypto dependencies)
  lighter: () =>
    import(/* webpackChunkName: "exchange-lighter" */ '../__tests__/reference/lighter/lighter.exchange').then((m) => m.LighterExchange),
  'lighter-robinhood': () =>
    import(/* webpackChunkName: "exchange-lighter" */ '../__tests__/reference/lighter/lighter.exchange').then((m) => m.LighterExchange),

  // MEXC
  mexc: () => import(/* webpackChunkName: "exchange-mexc" */ '../__tests__/reference/mexc/mexc.exchange').then((m) => m.MexcExchange),

  // OKX
  okx: () => import(/* webpackChunkName: "exchange-okx" */ '../__tests__/reference/okx/okx.exchange').then((m) => m.OKXExchange),

  // OKX Demo
  okxdemo: () =>
    import(/* webpackChunkName: "exchange-okxdemo" */ '../__tests__/reference/okx/okxdemo.exchange').then((m) => m.OKXDemoExchange),

  // OKX EU (European entity, separate API host)
  okxeu: () => import(/* webpackChunkName: "exchange-okxeu" */ '../__tests__/reference/okx/okxeu.exchange').then((m) => m.OKXEUExchange),

  // OKX EU Demo
  okxeudemo: () =>
    import(/* webpackChunkName: "exchange-okxeudemo" */ '../__tests__/reference/okx/okxeudemo.exchange').then(
      (m) => m.OKXEUDemoExchange,
    ),

  // Paradex (heavy - StarkEx/Starknet dependencies)
  paradex: () =>
    import(/* webpackChunkName: "exchange-paradex" */ '../__tests__/reference/paradex/paradex.exchange').then((m) => m.ParadexExchange),

  // Phemex
  phemex: () =>
    import(/* webpackChunkName: "exchange-phemex" */ '../__tests__/reference/phemex/phemex.exchange').then((m) => m.PhemexExchange),

  // Polymarket (heavy - ethers for EIP-712 signing)
  polymarket: () =>
    import(/* webpackChunkName: "exchange-polymarket" */ '../__tests__/reference/polymarket/polymarket.exchange').then(
      (m) => m.PolymarketExchange,
    ),

  // WEEX
  weex: () => import(/* webpackChunkName: "exchange-weex" */ '../__tests__/reference/weex/weex.exchange').then((m) => m.WeexExchange),

  // WooX
  woo: () => import(/* webpackChunkName: "exchange-woo" */ '../__tests__/reference/woo/woo.exchange').then((m) => m.WOOXExchange),

  // Orderly Network exchanges
  kodiakfi: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),

  mode: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),

  salsadex: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),

  'tealstreet-dex': () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),

  // TxFlow testnet
  txflow: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),

  what: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),

  woofi: () => import('./rust/rust.exchange').then((m) => m.RustEngineExchange),
};

/**
 * Get all registered exchange names
 */
export const getRegisteredExchangeNames = (): ExchangeName[] => {
  return Object.keys(exchangeRegistry) as ExchangeName[];
};
