import type { ExchangeOptions, Market } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from '../cutover/nativeFixture';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

const symbol = 'E-BTC-USDT';
const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

function mirror(signingMode: ExchangeOptions['signingMode'], publicDataOnly = false, workerMirror = true) {
  const constructed = vi.fn(() => {
    throw new Error('A mirror must not construct an account engine');
  });
  setWasmEngineLoader(async () => ({
    WasmEngine: Object.assign(constructed, { frameFedSupported: () => false }) as never,
  }));
  const store = new FrameStore();
  store.apply(
    nativeFixture(
      {
        markets: [
          {
            id: symbol,
            symbol,
            base: 'BTC',
            quote: 'USDT',
            active: true,
            precision: { amount: 0.001, price: 0.1 },
            limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
            info: {},
          } satisfies Market,
        ],
      },
      store.markets.map((row) => row.symbol),
    ),
  );
  const exchange = new RustEngineExchange(
    'bitrue',
    {
      key: 'synthetic-key',
      secret: 'synthetic-secret',
      workerMirror,
      signingMode,
      publicDataOnly,
    },
    store,
  );
  exchanges.push(exchange);
  let generation = 1;
  exchange.setWorkerOwner(() => ({ ownerId: 'synthetic-owner', generation }));
  adoptEngineCapabilities(exchange);
  const calls: Array<{ requestId: string; method: string; args: unknown[] }> = [];
  let pending: (typeof calls)[number] | undefined;
  let deferred = false;
  let result = 125;
  const reply = (request: (typeof calls)[number]) =>
    exchange.emitter.emit('callMethod:done', {
      requestId: request.requestId,
      success: true,
      result,
    });
  const bridge = (request: (typeof calls)[number]) => {
    calls.push(request);
    if (deferred) pending = request;
    else reply(request);
  };
  exchange.on('callMethod', bridge);
  return {
    exchange,
    store,
    calls,
    constructed,
    attach() {
      exchange.on('callMethod', bridge);
    },
    detach() {
      exchange.off('callMethod', bridge);
    },
    result(value: number) {
      result = value;
    },
    defer() {
      deferred = true;
    },
    retire() {
      generation++;
    },
    finish() {
      if (!pending) throw new Error('Expected a pending source read');
      reply(pending);
    },
  };
}

it.each(['local', 'cosigned', 'delegated', undefined] as const)(
  'a Bitrue mirror reads the source maximum without a page engine (mode=%s)',
  async (mode) => {
    const fixture = mirror(mode);
    await expect(fixture.exchange.fetchSymbolMaxLeverage(symbol)).resolves.toBe(125);
    expect(fixture.calls.map(({ method, args }) => ({ method, args }))).toEqual([
      { method: 'fetchSymbolMaxLeverage', args: [symbol] },
    ]);
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

function deferCapabilities(exchange: RustEngineExchange) {
  let release!: () => void;
  const ready = new Promise<void>((resolve) => {
    release = resolve;
  });
  const boundary = exchange as unknown as {
    capabilitiesFor(method: string): unknown;
    command(command: { op: string; symbol: string }): Promise<number>;
  };
  const original = boundary.capabilitiesFor.bind(exchange);
  vi.spyOn(boundary, 'capabilitiesFor').mockImplementation((method) => ready.then(() => original(method)));
  const native = vi.spyOn(boundary, 'command').mockResolvedValue(125);
  const pending = exchange.fetchSymbolMaxLeverage(symbol).then(
    (value) => ({ value }),
    (error: unknown) => ({ error }),
  );
  return { release, native, pending };
}

it('keeps the initiating native route when a legacy bridge attaches during capability readiness', async () => {
  const fixture = mirror('local', false, false);
  fixture.detach();
  const held = deferCapabilities(fixture.exchange);
  fixture.attach();
  held.release();
  expect(await held.pending).toEqual({ value: 125 });
  expect(held.native).toHaveBeenCalledOnce();
  expect(fixture.calls).toEqual([]);
});

it('refuses an old native read before a successor owner bridge or native engine can receive it', async () => {
  const fixture = mirror('local', false, false);
  fixture.detach();
  const held = deferCapabilities(fixture.exchange);
  fixture.retire();
  fixture.attach();
  held.release();
  expect(await held.pending).toMatchObject({ error: { code: 'WORKER_STOPPED' } });
  expect(held.native).not.toHaveBeenCalled();
  expect(fixture.calls).toEqual([]);
});

it('refuses a removed initiating bridge before a native read during capability readiness', async () => {
  const fixture = mirror('local', false, false);
  const held = deferCapabilities(fixture.exchange);
  fixture.detach();
  held.release();
  expect(await held.pending).toMatchObject({ error: { code: 'WORKER_STOPPED' } });
  expect(held.native).not.toHaveBeenCalled();
  expect(fixture.calls).toEqual([]);
});
