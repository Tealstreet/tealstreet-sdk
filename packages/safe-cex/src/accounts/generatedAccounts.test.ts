import type { ExchangeAccountConfig } from '../generated/ExchangeAccountConfig';
import type { ExchangeOptions } from '../generated/ExchangeOptions';
import { expect, it } from 'vitest';
import { accountToExchangeOptions } from '../createExchangeFromAccount';
import { LocalSignerSetup } from '../exchanges/rust/localSignerSetup';
import { buildExchangeAccountConfig } from './buildExchangeAccountConfig';
import { normalizeExchangeAccountConfig } from './normalizeExchangeAccountConfig';

it('generated saved accounts retain zero timestamps and false metadata through real consumers', () => {
  const account: ExchangeAccountConfig = buildExchangeAccountConfig({ exchange: 'bybit', name: 'fixture',
    apiKey: 'fixture-key', apiSecret: 'fixture-secret', active: false, pinned: false, updatedAt: 0, deletedAt: 0 });
  account.name = 'mutable';
  const normalized: ExchangeAccountConfig = normalizeExchangeAccountConfig(account);
  expect(normalized).toMatchObject({ name: 'mutable', updatedAt: 0, deletedAt: 0, active: false, pinned: false });
  expect(JSON.parse(JSON.stringify(normalized))).toEqual(normalized);
});

it('generated account config maps to the real runtime options with existing credential and toggle policy', () => {
  const account: ExchangeAccountConfig = { exchange: 'bybit', name: 'fixture', apiKey: 'fixture-key',
    apiSecret: 'fixture-secret', passphrase: '', enableSpotMarkets: false, enableOptionMarkets: false };
  const options: ExchangeOptions = accountToExchangeOptions(account);
  expect(options.key).toBe(account.apiKey); expect(options.secret).toBe(account.apiSecret);
  expect(options.enableSpotMarkets).toBeUndefined(); expect(options.enableOptionMarkets).toBeUndefined();
});

it('generated local signer metadata enters the existing private setup capsule', () => {
  const options: ExchangeOptions = { key: 'fixture-key', secret: 'fixture-secret', localSigner: {
    primary: { stableId: 'caller-stable-id', secretEncoding: 'utf8' },
  }, hyperliquidFastBook: false, callerReportsPlacementFailures: false, workerMirror: false,
  engine: 'rust', extra: { rustCommandTimeoutMs: 0, tickInterval: Infinity } };
  const setup = new LocalSignerSetup(options);
  expect(setup.primaryComplete).toBe(true);
  expect(setup.credentialPresence).toEqual({ key: true, secret: true });
  expect(options.extra?.rustCommandTimeoutMs).toBe(0);
  expect(options.extra?.tickInterval).toBe(Infinity);
  setup.dispose();
});
