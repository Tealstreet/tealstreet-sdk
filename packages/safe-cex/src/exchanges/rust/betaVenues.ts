import type { ExchangeName, ExchangeOptions } from '../../types';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { deriveRemoteSigningPolicy } from './construction';

/** The Rust-only beta build's complete venue policy, including saved aliases. */
export const BETA_VENUES = Object.freeze([
  'binance',
  'binancepm',
  'binancespot',
  'binanceinverse',
  'bybit',
  'bybitspot',
  'bybitinverse',
  'bybitdemo',
  'okx',
  'okxdemo',
  'okxeu',
  'okxeudemo',
  'bitget',
  'bitgetusdc',
  'kraken',
  'krakenspot',
  'hyperliquid',
  'hyperliquidspot',
  'lighter',
  'paradex',
  'bitunix',
  'phemex',
  'bingx',
  'apexomni',
  'blofin',
  'cryptocom',
  'asterdex',
] as const satisfies readonly ExchangeName[]);

export const isBetaVenue = (name: string): name is (typeof BETA_VENUES)[number] =>
  BETA_VENUES.some((venue) => venue === name);

export const assertBetaVenue = (name: string): void => {
  if (!isBetaVenue(name)) {
    const message = `${name} is unsupported in this build`;
    throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
  }
};

/** Unsupported signing modes cannot fall through to a TypeScript venue. */
export const assertBetaAccount = (name: ExchangeName, options: Partial<ExchangeOptions>): void => {
  assertBetaVenue(name);
  if (
    options.publicDataOnly !== true &&
    (options.remoteSigner || options.payloadSigner) &&
    (name === 'paradex' || deriveRemoteSigningPolicy(name)?.enabled !== true)
  ) {
    const message = `${name} delegated signing is unsupported in this build`;
    throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
  }
};
