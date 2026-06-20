import type { ExchangeName } from '../types';

// Exchanges where a secondary, read-only API key is NOT a coherent concept:
//  - wallet / private-key / on-chain signed venues (Ethereum/Cosmos/StarkEx/ZK)
//    and the Orderly ed25519 DEX family: a single keypair, no server-managed
//    key+secret with a read-only permission scope.
//  - session-login venues (breakoutprop=DXtrade session token, btcc=MD5 login):
//    auth is a shared per-session token injected on every request, so a second
//    client cannot authenticate as a distinct read-only key (and btcc issues all
//    REST via this.xhr.request(), which the read-routing Proxy does not intercept).
// There is nothing to put in a "secondary read-only key" field for these. Every
// other exchange uses standard per-request HMAC/ed25519/JWT key+secret and can opt in.
export const exchangesWithoutSecondaryKey: ExchangeName[] = [
  'apexomni',
  'asterdex',
  'breakoutprop',
  'btcc',
  'dydx',
  'grvt',
  'hyperliquid',
  'hyperliquidspot',
  'lighter',
  'paradex',
  'polymarket',
  'woofi',
  'kodiakfi',
  'mode',
  'salsadex',
  'tealstreet-dex',
  'what',
];

export const exchangeSupportsSecondaryKey = (exchange: ExchangeName): boolean =>
  !exchangesWithoutSecondaryKey.includes(exchange);
