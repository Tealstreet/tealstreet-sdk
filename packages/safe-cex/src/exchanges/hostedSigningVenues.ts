import type { ExchangeName, ExchangeOptions } from '../types';
import type { BaseExchange } from './base';

/**
 * A hosted account: no local secret, signed by the browser through `remoteSigner`.
 * Paradex and Polymarket keep their signing key in `key`, and Lighter runs read-only
 * without one, as TS does, so none of them is keyless here.
 */
export const isKeylessRemoteSigned = (
  name: ExchangeName,
  options: Partial<Pick<ExchangeOptions, 'secret' | 'remoteSigner' | 'auth'>>,
) =>
  Boolean(options.remoteSigner) &&
  !options.secret &&
  !(name === 'asterdex' && options.auth?.privateKey) &&
  !['lighter', 'lighter-robinhood', 'paradex', 'polymarket'].includes(name);

/**
 * TS venues kept only for hosted remote signing, for names whose registry default is the
 * Rust engine. The engine cannot sign through a `remoteSigner` (docs/plans/RUST_REMOTE_SIGNING.md),
 * so a keyless hosted account runs here, and a browser signer for these names delegates here.
 */
export const hostedSigningVenues: Partial<Record<ExchangeName, () => Promise<typeof BaseExchange>>> = {
  coinbase: () =>
    import(/* webpackChunkName: "exchange-coinbase" */ '../__tests__/reference/coinbase/coinbase.exchange').then((m) => m.CoinbaseExchange),
  coinbasespot: () =>
    import(/* webpackChunkName: "exchange-coinbasespot" */ '../__tests__/reference/coinbase/coinbasespot.exchange').then(
      (m) => m.CoinbaseSpotExchange,
    ),
  kodiakfi: () =>
    import(/* webpackChunkName: "exchange-kodiakfi" */ '../__tests__/reference/orderly/exchanges/kodiakfi/kodiakfi.exchange').then(
      (m) => m.KodiakFIExchange,
    ),
  mode: () =>
    import(/* webpackChunkName: "exchange-mode" */ '../__tests__/reference/orderly/exchanges/mode/mode.exchange').then((m) => m.MODEExchange),
  salsadex: () =>
    import(/* webpackChunkName: "exchange-salsadex" */ '../__tests__/reference/orderly/exchanges/salsadex/salsadex.exchange').then(
      (m) => m.SalsaDEXExchange,
    ),
  'tealstreet-dex': () =>
    import(
      /* webpackChunkName: "exchange-tealstreet-dex" */ '../__tests__/reference/orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange'
    ).then((m) => m.TealstreetDEXExchange),
  what: () =>
    import(/* webpackChunkName: "exchange-what" */ '../__tests__/reference/orderly/exchanges/what/what.exchange').then((m) => m.WhatExchange),
  woofi: () =>
    import(/* webpackChunkName: "exchange-woofi" */ '../__tests__/reference/orderly/exchanges/woofi/woofi.exchange').then(
      (m) => m.WoofiExchange,
    ),
};
