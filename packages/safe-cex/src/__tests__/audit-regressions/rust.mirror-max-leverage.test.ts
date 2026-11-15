import type { ExchangeOptions, Market } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

const symbol = 'E-BTC-USDT';
const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

function mirror(signingMode: ExchangeOptions['signingMode'], publicDataOnly = false) {
  const constructed = vi.fn(() => { throw new Error('A mirror must not construct an account engine'); });
  setWasmEngineLoader(async () => ({
    WasmEngine: Object.assign(constructed, { frameFedSupported: () => false }) as never,
  }));
  const store = new DefaultStore();
  store.update({ markets: [{
    id: symbol, symbol, base: 'BTC', quote: 'USDT', active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } }, info: {},
  } satisfies Market] });
  const exchange = new RustEngineExchange('bitrue', {
    key: 'synthetic-key', secret: 'synthetic-secret', workerMirror: true, signingMode, publicDataOnly,
  }, store);
  exchanges.push(exchange);
  let generation = 1;
  exchange.setWorkerOwner(() => ({ ownerId: 'synthetic-owner', generation }));
  adoptEngineCapabilities(exchange);
  const calls: Array<{ requestId: string; method: string; args: unknown[] }> = [];
  let pending: (typeof calls)[number] | undefined;
  let deferred = false;
  let result = 125;
  const reply = (request: (typeof calls)[number]) => exchange.emitter.emit('callMethod:done', {
    requestId: request.requestId, success: true, result,
  });
  const bridge = (request: (typeof calls)[number]) => {
    calls.push(request);
    if (deferred) pending = request;
    else reply(request);
  };
  exchange.on('callMethod', bridge);
  return {
    exchange, store, calls, constructed,
    result(value: number) { result = value; },
    defer() { deferred = true; },
    retire() { generation++; },
    finish() { if (!pending) throw new Error('Expected a pending source read'); reply(pending); },
  };
}

it.each(['local', 'cosigned', 'delegated', undefined] as const)(
  'a Bitrue mirror reads the source maximum without a page engine (mode=%s)', async (mode) => {
    const fixture = mirror(mode);
    await expect(fixture.exchange.fetchSymbolMaxLeverage(symbol)).resolves.toBe(125);
    expect(fixture.calls.map(({ method, args }) => ({ method, args })))
      .toEqual([{ method: 'fetchSymbolMaxLeverage', args: [symbol] }]);
    expect(fixture.constructed).not.toHaveBeenCalled();
    expect(fixture.store.fetchedLeverage[symbol]).toBeUndefined();
  },
);

it('keeps a public-only maximum on the mirrored market without a private source read', async () => {
  const fixture = mirror('local', true);
  await expect(fixture.exchange.fetchSymbolMaxLeverage(symbol)).resolves.toBe(100);
  expect(fixture.calls).toEqual([]);
  expect(fixture.constructed).not.toHaveBeenCalled();
});

it.each([0, Number.NaN, Number.POSITIVE_INFINITY])('refuses an invalid source maximum %s', async (value) => {
  const fixture = mirror('local');
  fixture.result(value);
  await expect(fixture.exchange.fetchSymbolMaxLeverage(symbol)).rejects.toThrow('Invalid Bitrue maximum leverage');
  expect(fixture.calls).toHaveLength(1);
  expect(fixture.constructed).not.toHaveBeenCalled();
});

it.each(['local', 'cosigned'] as const)('retires a pending maximum read when its owner changes (%s)', async (mode) => {
  const fixture = mirror(mode);
  fixture.defer();
  const pending = fixture.exchange.fetchSymbolMaxLeverage(symbol).then(
    (value) => ({ value }),
    (error: unknown) => ({ error }),
  );
  await vi.waitFor(() => expect(fixture.calls).toHaveLength(1));
  fixture.retire();
  fixture.finish();
  expect(await pending).toMatchObject({ error: { code: 'WORKER_STOPPED' } });
  expect(fixture.calls).toHaveLength(1);
});
