import type { ExchangeName } from '../types';

// Exchanges where a secondary, read-only API key is NOT a coherent concept:
//  - wallet / private-key / on-chain signed venues (Ethereum/Cosmos/StarkEx/ZK)
//    and the Orderly ed25519 DEX family: a single keypair, no server-managed
//    key+secret with a read-only permission scope.
//  - session-login venues (breakoutprop=DXtrade session token):
//    auth is a shared per-session token injected on every request, so a second
//    client cannot authenticate as a distinct read-only key.
// There is nothing to put in a "secondary read-only key" field for these. Every
// other exchange uses standard per-request HMAC/ed25519/JWT key+secret and can opt in.
// AsterDEX is deliberately NOT excluded: its V3 agent model issues a distinct
// read-only agent wallet (secondary_apiKey = agent address, secondary_apiSecret =
// its private key) sharing the main wallet (applicationId) — a real second read credential.
export const exchangesWithoutSecondaryKey: ExchangeName[] = [
  'apexomni',
  'breakoutprop',
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
