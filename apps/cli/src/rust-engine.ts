import type { VenueFamily } from '@tealstreet/safe-cex-wasm/venues';

import { readFileSync } from 'node:fs';

import { initializeWebEngine } from '@tealstreet/safe-cex-wasm/engine-cache';
import { venueFamily } from '@tealstreet/safe-cex-wasm/venues';
import { setWasmEngineLoader } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

import apexomniAsset from '../../../sdk-rs/hosts/wasm/pkg/web/apexomni/safe_cex_wasm_bg.wasm' with { type: 'file' };
import asterdexAsset from '../../../sdk-rs/hosts/wasm/pkg/web/asterdex/safe_cex_wasm_bg.wasm' with { type: 'file' };
import backpackAsset from '../../../sdk-rs/hosts/wasm/pkg/web/backpack/safe_cex_wasm_bg.wasm' with { type: 'file' };
import binanceAsset from '../../../sdk-rs/hosts/wasm/pkg/web/binance/safe_cex_wasm_bg.wasm' with { type: 'file' };
import bingxAsset from '../../../sdk-rs/hosts/wasm/pkg/web/bingx/safe_cex_wasm_bg.wasm' with { type: 'file' };
import bitgetAsset from '../../../sdk-rs/hosts/wasm/pkg/web/bitget/safe_cex_wasm_bg.wasm' with { type: 'file' };
import bitrueAsset from '../../../sdk-rs/hosts/wasm/pkg/web/bitrue/safe_cex_wasm_bg.wasm' with { type: 'file' };
import blofinAsset from '../../../sdk-rs/hosts/wasm/pkg/web/blofin/safe_cex_wasm_bg.wasm' with { type: 'file' };
import breakoutpropAsset from '../../../sdk-rs/hosts/wasm/pkg/web/breakoutprop/safe_cex_wasm_bg.wasm' with { type: 'file' };
import bybitAsset from '../../../sdk-rs/hosts/wasm/pkg/web/bybit/safe_cex_wasm_bg.wasm' with { type: 'file' };
import coinbaseAsset from '../../../sdk-rs/hosts/wasm/pkg/web/coinbase/safe_cex_wasm_bg.wasm' with { type: 'file' };
import cryptocomAsset from '../../../sdk-rs/hosts/wasm/pkg/web/cryptocom/safe_cex_wasm_bg.wasm' with { type: 'file' };
import grvtAsset from '../../../sdk-rs/hosts/wasm/pkg/web/grvt/safe_cex_wasm_bg.wasm' with { type: 'file' };
import hyperliquidAsset from '../../../sdk-rs/hosts/wasm/pkg/web/hyperliquid/safe_cex_wasm_bg.wasm' with { type: 'file' };
import krakenAsset from '../../../sdk-rs/hosts/wasm/pkg/web/kraken/safe_cex_wasm_bg.wasm' with { type: 'file' };
import kucoinAsset from '../../../sdk-rs/hosts/wasm/pkg/web/kucoin/safe_cex_wasm_bg.wasm' with { type: 'file' };
import kucoinspotAsset from '../../../sdk-rs/hosts/wasm/pkg/web/kucoinspot/safe_cex_wasm_bg.wasm' with { type: 'file' };
import lighterAsset from '../../../sdk-rs/hosts/wasm/pkg/web/lighter/safe_cex_wasm_bg.wasm' with { type: 'file' };
import mexcAsset from '../../../sdk-rs/hosts/wasm/pkg/web/mexc/safe_cex_wasm_bg.wasm' with { type: 'file' };
import okxAsset from '../../../sdk-rs/hosts/wasm/pkg/web/okx/safe_cex_wasm_bg.wasm' with { type: 'file' };
import orderlyAsset from '../../../sdk-rs/hosts/wasm/pkg/web/orderly/safe_cex_wasm_bg.wasm' with { type: 'file' };
import paradexAsset from '../../../sdk-rs/hosts/wasm/pkg/web/paradex/safe_cex_wasm_bg.wasm' with { type: 'file' };
import phemexAsset from '../../../sdk-rs/hosts/wasm/pkg/web/phemex/safe_cex_wasm_bg.wasm' with { type: 'file' };
import polymarketAsset from '../../../sdk-rs/hosts/wasm/pkg/web/polymarket/safe_cex_wasm_bg.wasm' with { type: 'file' };
import txflowAsset from '../../../sdk-rs/hosts/wasm/pkg/web/txflow/safe_cex_wasm_bg.wasm' with { type: 'file' };
import weexAsset from '../../../sdk-rs/hosts/wasm/pkg/web/weex/safe_cex_wasm_bg.wasm' with { type: 'file' };
import wooAsset from '../../../sdk-rs/hosts/wasm/pkg/web/woo/safe_cex_wasm_bg.wasm' with { type: 'file' };

// Literal imports let Bun embed every family while initializing only the selected one.
const imports = {
  apexomni: () => import('@tealstreet/safe-cex-wasm/web/apexomni'),
  asterdex: () => import('@tealstreet/safe-cex-wasm/web/asterdex'),
  backpack: () => import('@tealstreet/safe-cex-wasm/web/backpack'),
  binance: () => import('@tealstreet/safe-cex-wasm/web/binance'),
  bingx: () => import('@tealstreet/safe-cex-wasm/web/bingx'),
  bitget: () => import('@tealstreet/safe-cex-wasm/web/bitget'),
  bitrue: () => import('@tealstreet/safe-cex-wasm/web/bitrue'),
  blofin: () => import('@tealstreet/safe-cex-wasm/web/blofin'),
  breakoutprop: () => import('@tealstreet/safe-cex-wasm/web/breakoutprop'),
  bybit: () => import('@tealstreet/safe-cex-wasm/web/bybit'),
  coinbase: () => import('@tealstreet/safe-cex-wasm/web/coinbase'),
  cryptocom: () => import('@tealstreet/safe-cex-wasm/web/cryptocom'),
  grvt: () => import('@tealstreet/safe-cex-wasm/web/grvt'),
  hyperliquid: () => import('@tealstreet/safe-cex-wasm/web/hyperliquid'),
  kraken: () => import('@tealstreet/safe-cex-wasm/web/kraken'),
  kucoin: () => import('@tealstreet/safe-cex-wasm/web/kucoin'),
  kucoinspot: () => import('@tealstreet/safe-cex-wasm/web/kucoinspot'),
  lighter: () => import('@tealstreet/safe-cex-wasm/web/lighter'),
  mexc: () => import('@tealstreet/safe-cex-wasm/web/mexc'),
  okx: () => import('@tealstreet/safe-cex-wasm/web/okx'),
  orderly: () => import('@tealstreet/safe-cex-wasm/web/orderly'),
  paradex: () => import('@tealstreet/safe-cex-wasm/web/paradex'),
  phemex: () => import('@tealstreet/safe-cex-wasm/web/phemex'),
  polymarket: () => import('@tealstreet/safe-cex-wasm/web/polymarket'),
  txflow: () => import('@tealstreet/safe-cex-wasm/web/txflow'),
  weex: () => import('@tealstreet/safe-cex-wasm/web/weex'),
  woo: () => import('@tealstreet/safe-cex-wasm/web/woo'),
} satisfies Record<VenueFamily, () => Promise<unknown>>;

const assets: Record<VenueFamily, unknown> = {
  apexomni: apexomniAsset,
  asterdex: asterdexAsset,
  backpack: backpackAsset,
  binance: binanceAsset,
  bingx: bingxAsset,
  bitget: bitgetAsset,
  bitrue: bitrueAsset,
  blofin: blofinAsset,
  breakoutprop: breakoutpropAsset,
  bybit: bybitAsset,
  coinbase: coinbaseAsset,
  cryptocom: cryptocomAsset,
  grvt: grvtAsset,
  hyperliquid: hyperliquidAsset,
  kraken: krakenAsset,
  kucoin: kucoinAsset,
  kucoinspot: kucoinspotAsset,
  lighter: lighterAsset,
  mexc: mexcAsset,
  okx: okxAsset,
  orderly: orderlyAsset,
  paradex: paradexAsset,
  phemex: phemexAsset,
  polymarket: polymarketAsset,
  txflow: txflowAsset,
  weex: weexAsset,
  woo: wooAsset,
};

// Bun reads embedded paths directly, without extraction or workspace resolution.
export const loadBundledRustEngine = (venue: string) => {
  const family = venueFamily(venue);
  return initializeWebEngine(family, async () => {
    const engine = await imports[family]();
    // Bun file imports are paths; generated declarations describe raw WASM exports.
    await engine.default({ module_or_path: readFileSync(assets[family] as string) });
    return engine;
  });
};

setWasmEngineLoader(loadBundledRustEngine);
