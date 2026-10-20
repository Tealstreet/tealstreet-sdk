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
    import(/* webpackChunkName: "exchange-apexomni" */ './apexomni/apexomni.exchange').then((m) => m.ApexOmniExchange),

  // AsterDEX
  asterdex: () =>
    import(/* webpackChunkName: "exchange-asterdex" */ './asterdex/asterdex.exchange').then((m) => m.AsterDEXExchange),

  // Backpack
  backpack: () =>
    import(/* webpackChunkName: "exchange-backpack" */ './backpack/backpack.exchange').then((m) => m.BackpackExchange),

  // Binance
  binance: () =>
    import(/* webpackChunkName: "exchange-binance" */ './binance/binance.exchange').then((m) => m.BinanceExchange),

  // Binance Options (European options, eapi)
  binanceoptions: () =>
    import(/* webpackChunkName: "exchange-binanceoptions" */ './binance/binanceoptions.exchange').then(
      (m) => m.BinanceOptionsExchange,
    ),

  // Binance Inverse (COIN-M Futures)
  binanceinverse: () =>
    import(/* webpackChunkName: "exchange-binanceinverse" */ './binance/binanceinverse.exchange').then(
      (m) => m.BinanceInverseExchange,
    ),

  // Binance PM (Portfolio Margin)
  binancepm: () =>
    import(/* webpackChunkName: "exchange-binancepm" */ './binance/binancepm.exchange').then(
      (m) => m.BinancePMExchange,
    ),

  // Binance Spot
  binancespot: () =>
    import(/* webpackChunkName: "exchange-binancespot" */ './binance/binancespot.exchange').then(
      (m) => m.BinanceSpotExchange,
    ),

  // BingX
  bingx: () => import(/* webpackChunkName: "exchange-bingx" */ './bingx/bingx.exchange').then((m) => m.BingxExchange),

  // Bitget
  bitget: () =>
    import(/* webpackChunkName: "exchange-bitget" */ './bitget/bitget.exchange').then((m) => m.BitgetExchange),

  // Bitget Inverse
  bitgetinverse: () =>
    import(/* webpackChunkName: "exchange-bitgetinverse" */ './bitget/bitgetinverse.exchange').then(
      (m) => m.BitgetInverseExchange,
    ),

  // Bitget USDC
  bitgetusdc: () =>
    import(/* webpackChunkName: "exchange-bitgetusdc" */ './bitget/bitgetusdc.exchange').then(
      (m) => m.BitgetUSDCExchange,
    ),

  // Bitrue
  bitrue: () =>
    import(/* webpackChunkName: "exchange-bitrue" */ './bitrue/bitrue.exchange').then((m) => m.BitrueExchange),

  // Bitunix
  bitunix: () =>
    import(/* webpackChunkName: "exchange-bitunix" */ './bitunix/bitunix.exchange').then((m) => m.BitunixExchange),

  // BloFin
  blofin: () =>
    import(/* webpackChunkName: "exchange-blofin" */ './blofin/blofin.exchange').then((m) => m.BlofinExchange),

  // Breakoutprop
  breakoutprop: () =>
    import(/* webpackChunkName: "exchange-breakoutprop" */ './breakoutprop/breakoutprop.exchange').then(
      (m) => m.BreakoutpropExchange,
    ),

  // Bybit
  bybit: () => import(/* webpackChunkName: "exchange-bybit" */ './bybit/bybit.exchange').then((m) => m.BybitExchange),

  // Bybit Demo
  bybitdemo: () =>
    import(/* webpackChunkName: "exchange-bybitdemo" */ './bybit/bybitdemo.exchange').then((m) => m.BybitDemoExchange),

  // HyroTrader
  hyrotrader: () =>
    import(/* webpackChunkName: "exchange-hyrotrader" */ './bybit/hyrotrader.exchange').then(
      (m) => m.HyrotraderExchange,
    ),

  // Bybit Inverse
  bybitinverse: () =>
    import(/* webpackChunkName: "exchange-bybitinverse" */ './bybit/bybitinverse.exchange').then(
      (m) => m.BybitInverseExchange,
    ),

  // Bybit Spot
  bybitspot: () =>
    import(/* webpackChunkName: "exchange-bybitspot" */ './bybit/bybitspot.exchange').then((m) => m.BybitSpotExchange),

  // Coinbase
  coinbase: () =>
    import(/* webpackChunkName: "exchange-coinbase" */ './coinbase/coinbase.exchange').then((m) => m.CoinbaseExchange),

  // Coinbase Spot
  coinbasespot: () =>
    import(/* webpackChunkName: "exchange-coinbasespot" */ './coinbase/coinbasespot.exchange').then(
      (m) => m.CoinbaseSpotExchange,
    ),

  // Crypto.com
  cryptocom: () =>
    import(/* webpackChunkName: "exchange-cryptocom" */ './cryptocom/cryptocom.exchange').then(
      (m) => m.CryptocomExchange,
    ),

  // GRVT (heavy - ethers + SDK dependencies)
  grvt: () => import(/* webpackChunkName: "exchange-grvt" */ './grvt/grvt.exchange').then((m) => m.GrvtExchange),

  // Hyperliquid
  hyperliquid: () =>
    import(/* webpackChunkName: "exchange-hyperliquid" */ './hyperliquid/hyperliquid.exchange').then(
      (m) => m.HyperliquidExchange,
    ),

  // Hyperliquid Spot
  hyperliquidspot: () =>
    import(/* webpackChunkName: "exchange-hyperliquidspot" */ './hyperliquid/hyperliquidspot.exchange').then(
      (m) => m.HyperliquidSpotExchange,
    ),

  // Kraken
  kraken: () =>
    import(/* webpackChunkName: "exchange-kraken" */ './kraken/kraken.exchange').then((m) => m.KrakenExchange),

  // Kraken Spot
  krakenspot: () =>
    import(/* webpackChunkName: "exchange-krakenspot" */ './kraken/krakenspot.exchange').then(
      (m) => m.KrakenSpotExchange,
    ),

  // KuCoin
  kucoin: () =>
    import(/* webpackChunkName: "exchange-kucoin" */ './kucoin/kucoin.exchange').then((m) => m.KucoinExchange),

  // KuCoin Spot
  kucoinspot: () =>
    import(/* webpackChunkName: "exchange-kucoinspot" */ './kucoin/kucoinspot.exchange').then(
      (m) => m.KucoinSpotExchange,
    ),

  // Lighter (heavy - WASM crypto dependencies)
  lighter: () =>
    import(/* webpackChunkName: "exchange-lighter" */ './lighter/lighter.exchange').then((m) => m.LighterExchange),
  'lighter-robinhood': () =>
    import(/* webpackChunkName: "exchange-lighter" */ './lighter/lighter.exchange').then((m) => m.LighterExchange),

  // MEXC
  mexc: () => import(/* webpackChunkName: "exchange-mexc" */ './mexc/mexc.exchange').then((m) => m.MexcExchange),

  // OKX
  okx: () => import(/* webpackChunkName: "exchange-okx" */ './okx/okx.exchange').then((m) => m.OKXExchange),

  // OKX Demo
  okxdemo: () =>
    import(/* webpackChunkName: "exchange-okxdemo" */ './okx/okxdemo.exchange').then((m) => m.OKXDemoExchange),

  // OKX EU (European entity, separate API host)
  okxeu: () => import(/* webpackChunkName: "exchange-okxeu" */ './okx/okxeu.exchange').then((m) => m.OKXEUExchange),

  // OKX EU Demo
  okxeudemo: () =>
    import(/* webpackChunkName: "exchange-okxeudemo" */ './okx/okxeudemo.exchange').then(
      (m) => m.OKXEUDemoExchange,
    ),

  // Paradex (heavy - StarkEx/Starknet dependencies)
  paradex: () =>
    import(/* webpackChunkName: "exchange-paradex" */ './paradex/paradex.exchange').then((m) => m.ParadexExchange),

  // Phemex
  phemex: () =>
    import(/* webpackChunkName: "exchange-phemex" */ './phemex/phemex.exchange').then((m) => m.PhemexExchange),

  // Polymarket (heavy - ethers for EIP-712 signing)
  polymarket: () =>
    import(/* webpackChunkName: "exchange-polymarket" */ './polymarket/polymarket.exchange').then(
      (m) => m.PolymarketExchange,
    ),

  // WEEX
  weex: () => import(/* webpackChunkName: "exchange-weex" */ './weex/weex.exchange').then((m) => m.WeexExchange),

  // WooX
  woo: () => import(/* webpackChunkName: "exchange-woo" */ './woo/woo.exchange').then((m) => m.WOOXExchange),

  // Orderly Network exchanges
  kodiakfi: () =>
    import(/* webpackChunkName: "exchange-kodiakfi" */ './orderly/exchanges/kodiakfi/kodiakfi.exchange').then(
      (m) => m.KodiakFIExchange,
    ),

  mode: () =>
    import(/* webpackChunkName: "exchange-mode" */ './orderly/exchanges/mode/mode.exchange').then(
      (m) => m.MODEExchange,
    ),

  salsadex: () =>
    import(/* webpackChunkName: "exchange-salsadex" */ './orderly/exchanges/salsadex/salsadex.exchange').then(
      (m) => m.SalsaDEXExchange,
    ),

  'tealstreet-dex': () =>
    import(
      /* webpackChunkName: "exchange-tealstreet-dex" */ './orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange'
    ).then((m) => m.TealstreetDEXExchange),

  // TxFlow (Phase 0 scaffold only)
  txflow: () =>
    import(/* webpackChunkName: "exchange-txflow" */ './txflow/txflow.exchange').then((m) => m.TxFlowExchange),

  what: () =>
    import(/* webpackChunkName: "exchange-what" */ './orderly/exchanges/what/what.exchange').then(
      (m) => m.WhatExchange,
    ),

  woofi: () =>
    import(/* webpackChunkName: "exchange-woofi" */ './orderly/exchanges/woofi/woofi.exchange').then(
      (m) => m.WoofiExchange,
    ),
};

/**
 * Get all registered exchange names
 */
export const getRegisteredExchangeNames = (): ExchangeName[] => {
  return Object.keys(exchangeRegistry) as ExchangeName[];
};
