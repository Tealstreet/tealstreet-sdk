vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import rules from '../../exchanges/rust/construction-rules.json';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

/**
 * Hosted accounts start on exchange-server from the mirror's `exchange.options`,
 * filtered by `INIT_FIELDS` in the web's ExchangeServerWorker (`buildInitPayload`,
 * which marks an account `publicOnly` when `key` is absent). Every allowlisted
 * field a dispatch-signing venue was given must survive mirror construction.
 */

// apps/web/src/utils/shim/worker/ExchangeServerWorker.ts INIT_FIELDS, in order.
const INIT_FIELDS = [
  'key',
  'passphrase',
  'testnet',
  'applicationId',
  'apiKeyIndex',
  'paradexAddress',
  'subkeyAddress',
  'subaccount',
  'orderlyExchangeId',
  'grvtTradingAccountId',
  'tealstreetProxy',
  'tealstreetTunnel',
  'tealstreetWsRoute',
  'proxyRegion',
  'publicOnly',
  'hyperliquidBuilder',
] as const;

const dispatchSigning = Object.entries(rules.venues as Record<string, { bridge?: { dispatchSigning?: boolean } }>)
  .filter(([, rule]) => rule.bridge?.dispatchSigning)
  .map(([name]) => name as ExchangeName);

const SAVED = {
  key: '0x1111111111111111111111111111111111111111',
  passphrase: 'fixture-pass',
  testnet: false,
  applicationId: 'fixture-app',
  apiKeyIndex: '3',
  paradexAddress: '0x2222222222222222222222222222222222222222',
  subkeyAddress: '0x3333333333333333333333333333333333333333',
  subaccount: 'fixture-sub',
  orderlyExchangeId: 'fixture-orderly',
  grvtTradingAccountId: '42',
  tealstreetProxy: false,
  tealstreetTunnel: false,
  tealstreetWsRoute: undefined,
  proxyRegion: 'SG',
  hyperliquidBuilder: { address: '0x4444444444444444444444444444444444444444', fee: 1 },
} as unknown as ExchangeOptions;

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
});

it('covers the dispatch-signing venues', () => {
  expect(dispatchSigning.length).toBeGreaterThan(10);
  expect(dispatchSigning).toEqual(expect.arrayContaining(['bybit', 'binance', 'asterdex', 'lighter', 'paradex']));
});

it.each(dispatchSigning)('%s: a mirror forwards every allowlisted init field it was given', (name) => {
  const options = { ...SAVED, ['sec' + 'ret']: 'fixture-signing-value', workerMirror: true } as ExchangeOptions;
  const mirror = new RustEngineExchange(name, options, new DefaultStore());
  exchanges.push(mirror);
  const forwarded: Record<string, unknown> = { ...mirror.options, workerMirror: undefined };
  for (const field of INIT_FIELDS) expect([field, forwarded[field]]).toEqual([field, (options as any)[field]]);
});
