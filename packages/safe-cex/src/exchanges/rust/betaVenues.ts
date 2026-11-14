import type { ExchangeName, ExchangeOptions } from '../../types';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';

import { exchangeMetadata } from '../../exchangeMetadata';
import { unsupportedVenueError } from '../../unsupportedVenue';
import { deriveRemoteSigningPolicy } from './construction';
import { delegatedCallbacksWellFormed, snapshotDelegatedSigningOptions } from './delegatedSigningSnapshot';
import { hasPrimaryKey, resolveEffectivePublicDataOnly } from './localSignerSetup';

/** The Rust-only beta build's complete venue policy, including saved aliases. */
export const BETA_VENUES = Object.freeze([
  'binance',
  'binancepm',
  'binanceoptions',
  'binancespot',
  'binanceinverse',
  'bybit',
  'bybitspot',
  'bybitinverse',
  'bybitdemo',
  'hyrotrader',
  'okx',
  'okxdemo',
  'okxeu',
  'okxeudemo',
  'bitget',
  'bitgetusdc',
  'bitgetinverse',
  'kraken',
  'krakenspot',
  'hyperliquid',
  'hyperliquidspot',
  'lighter',
  // Robinhood shares Lighter's beta family and existing Hosted signing policy.
  'lighter-robinhood',
  'paradex',
  'bitunix',
  'phemex',
  'bingx',
  'apexomni',
  'blofin',
  'cryptocom',
  'asterdex',
  'mexc',
  'woo',
  'kucoin',
  'kucoinspot',
  'coinbase',
  'coinbasespot',
  'grvt',
  'kodiakfi',
  'mode',
  'what',
  'woofi',
  'backpack',
  // restored by #3502; #3317 dropped a live venue without a product decision.
  'bitrue',
] as const satisfies readonly ExchangeName[]);

export const isBetaVenue = (name: string): name is (typeof BETA_VENUES)[number] =>
  BETA_VENUES.some((venue) => venue === name);

export const assertBetaVenue: (name: string) => asserts name is (typeof BETA_VENUES)[number] = (name) => {
  if (!isBetaVenue(name)) {
    throw unsupportedVenueError(`${name} is unsupported in this build`);
  }
};

/** Unsupported signing modes cannot fall through to a TypeScript venue. */
export const assertBetaAccount = (name: ExchangeName, options: Partial<ExchangeOptions>): void => {
  assertBetaVenue(name);
  options = snapshotDelegatedSigningOptions(name, options);
  // For admitted venues, unused callbacks on a public account are not custody.
  const publicDataOnly = resolveEffectivePublicDataOnly(name, options);
  // Configured means present, whatever its value: the same predicate as the resolver and setup.
  const delegated = !publicDataOnly && (options.remoteSigner !== undefined || options.payloadSigner !== undefined);
  const disabled = deriveRemoteSigningPolicy(name)?.enabled !== true;
  // A keyed caller's present-but-invalid callback refuses whatever mode the rules resolve; only
  // an explicit public request ignores unused callback contents. DelegatedSignerSetup validates
  // private accounts of enabled families itself; admission covers what it never sees.
  const configured = options.remoteSigner !== undefined || options.payloadSigner !== undefined;
  const keyedPublic = !delegated && configured && options.publicDataOnly !== true && hasPrimaryKey(name, options);
  if (((delegated && disabled) || keyedPublic) && !delegatedCallbacksWellFormed(options)) {
    const message = 'Invalid delegated signer setup';
    throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
  }
  if (delegated && disabled) {
    const prettyName = exchangeMetadata[name].prettyName;
    const message = options.remoteSigner !== undefined
      ? `${prettyName} isn't available on the Hosted Server yet; turn Hosted Server off to trade it with your API key in this browser`
      : `${prettyName} delegated signing is unsupported in this build`;
    throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
  }
};
