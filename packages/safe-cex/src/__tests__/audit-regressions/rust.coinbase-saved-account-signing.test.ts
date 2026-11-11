import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeAccountConfig, ExchangeOptions, RemoteSigner } from '../../types';

import { expect, it, vi } from 'vitest';

import { accountToExchangeOptions } from '../../createExchangeFromAccount';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';

class BindingProbe implements WasmEngineLike {
  readonly options: unknown;
  constructor(_venue: string, json: string) {
    this.options = JSON.parse(json);
  }
  capabilities = () => '{}';
  start = () => '[]';
  command = () => '[]';
  takeFrame = () => undefined;
  snapshotFrame = () => new Uint8Array();
  wsOpen = () => '[]';
  wsMessage = () => '[]';
  wsClose = () => '[]';
  wsRouteExhausted = () => '[]';
  httpStart = () => '[]';
  httpChunk = () => '[]';
  httpEnd = () => '[]';
  httpFailed = () => '[]';
  timer = () => '[]';
}

const venues: Array<ExchangeAccountConfig['exchange']> = ['coinbase', 'coinbasespot'];
const key = ['organizations', 'fixture', 'apiKeys', 'fixture'].join('/');

it.each(venues)('%s carries a saved Ed25519 algorithm into delegated construction', (name) => {
  const account: ExchangeAccountConfig = {
    exchange: name,
    name: 'saved fixture',
    apiKey: key,
    apiSecret: '',
    coinbaseJwtAlgorithm: 'edDsa',
  };
  const remoteSigner: RemoteSigner = {
    sign: vi.fn<RemoteSigner['sign']>(),
    isAvailable: () => true,
    onAvailabilityChange: () => () => {},
  };
  const options = accountToExchangeOptions(account, { remoteSigner });
  expect(options.coinbaseJwtAlgorithm).toBe('edDsa');
  const setup = new DelegatedSignerSetup(name, options);
  try {
    const engine = setup.construct(BindingProbe, name, '{}');
    expect(engine).toBeInstanceOf(BindingProbe);
    if (!(engine instanceof BindingProbe)) throw new Error('Missing construction probe');
    expect(engine.options).toMatchObject({
      coinbaseSigning: { keyName: key, algorithm: 'edDsa' },
      remoteSigning: true,
    });
    expect(remoteSigner.sign).not.toHaveBeenCalled();
  } finally {
    setup.dispose();
  }
});

it.each(venues)('%s preserves algorithm omission and explicit overrides', (exchange) => {
  const account: ExchangeAccountConfig = { exchange, name: 'fixture', apiKey: key, apiSecret: '' };
  expect(accountToExchangeOptions(account)).not.toHaveProperty('coinbaseJwtAlgorithm');
  const overrides: Partial<ExchangeOptions> = { coinbaseJwtAlgorithm: 'es256' };
  expect(accountToExchangeOptions({ ...account, coinbaseJwtAlgorithm: 'edDsa' }, overrides)).toHaveProperty(
    'coinbaseJwtAlgorithm',
    'es256',
  );
});
