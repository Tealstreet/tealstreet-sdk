import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { consumerFrame } from './cutover/consumerFrame';
import { engineCapabilities } from './fixtures/engineCapabilities';

const exchanges: RustEngineExchange[] = [];
const opts = { symbol: 'BTCUSDT', interval: '15m' as const, from: 1_700_000_000_000, to: 1_700_000_900_000 };
const candles = [{ timestamp: 1_700_000_000, open: 1, high: 2, low: 1, close: 2, volume: 1 }];

/**
 * A mirror whose page candle sources are fake engines: `health` decides, per construction,
 * whether the source loads its markets, never does, or fails to start.
 */
const fixture = (health: Array<'ok' | 'stuck' | 'fails'>) => {
  const built: Array<{ commands: Array<Record<string, unknown>>; freed: ReturnType<typeof vi.fn> }> = [];
  const Constructor = Object.assign(
    function (_venue: string, json: string) {
      const config = JSON.parse(json);
      const page = config.publicDataOnly === true;
      const state = page ? (health[built.length] ?? 'ok') : 'ok';
      const record = { commands: [] as Array<Record<string, unknown>>, freed: vi.fn() };
      if (page) built.push(record);
      const frame = () =>
        new Uint8Array(
          consumerFrame({
            markets: [],
            tickers: [],
            loaded: { markets: state === 'ok', tickers: state === 'ok', balance: true, orders: true, positions: true },
          }),
        );
      const idle = () => '[]';
      return {
        construction: () => JSON.stringify({ frameFed: false, publicDataOnly: page }),
        capabilities: () => engineCapabilities('bybit'),
        start: () => {
          if (state === 'fails') throw new Error('instrument read refused');
          return '[]';
        },
        timer: idle,
        wsOpen: idle,
        wsMessage: idle,
        wsClose: idle,
        wsRouteExhausted: idle,
        httpStart: idle,
        httpChunk: idle,
        httpEnd: idle,
        httpFailed: idle,
        takeFrame: frame,
        snapshotFrame: frame,
        applyFrame: idle,
        free: record.freed,
        command: (json: string) => {
          const command = JSON.parse(json);
          record.commands.push(command);
          return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(candles) }]);
        },
      } satisfies WasmEngineLike;
    },
    { frameFedSupported: () => false },
  );
  setWasmEngineLoader(async () => ({ WasmEngine: Constructor as unknown as WasmEngineModule['WasmEngine'] }));
  const exchange = new RustEngineExchange(
    'bybit',
    { key: 'k', secret: 's', workerMirror: true, testnet: true },
    new DefaultStore(),
  );
  exchanges.push(exchange);
  exchange.on('callMethod', vi.fn());
  return { exchange, built };
};

afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  vi.useRealTimers();
});

/** Settles `read` while fake time runs past every bound it may wait on. */
const settle = async <T>(read: Promise<T>, ms = 15_000) => {
  const outcome = read.then(
    (value) => ({ value }),
    (error: Error) => ({ error }),
  );
  await vi.advanceTimersByTimeAsync(ms);
  return outcome;
};

it('a page candle read is bounded: a short markets wait and a candle command deadline', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['ok']);
  const result = await settle(exchange.fetchOHLCV(opts));
  expect(result).toEqual({ value: candles });
  expect(built).toHaveLength(1);
  expect(built[0].commands.find((command) => command.op === 'fetchCandles')).toMatchObject({ timeoutMs: 20_000 });
});

it('a source whose markets never load fails within 10 s and is rebuilt after a backoff', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['stuck', 'ok']);
  const first = await settle(exchange.fetchOHLCV(opts), 10_500);
  expect(first).toMatchObject({
    error: expect.objectContaining({ message: expect.stringMatching(/Timed out waiting for native markets/) }),
  });
  expect(built[0].freed).toHaveBeenCalled();

  // Within the backoff the chart sees a visible failure, and nothing is rebuilt.
  const early = await settle(exchange.fetchOHLCV(opts), 0);
  expect(early).toMatchObject({ error: expect.objectContaining({ message: expect.stringMatching(/recovering/) }) });
  expect(built).toHaveLength(1);

  await vi.advanceTimersByTimeAsync(2_000);
  const healed = await settle(exchange.fetchOHLCV(opts));
  expect(healed).toEqual({ value: candles });
  expect(built).toHaveLength(2);
});

it('a source that fails to start is replaced, and backoff doubles while the venue keeps failing', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['fails', 'fails', 'ok']);
  expect(await settle(exchange.fetchOHLCV(opts), 100)).toHaveProperty('error');
  await vi.advanceTimersByTimeAsync(2_000);
  expect(await settle(exchange.fetchOHLCV(opts), 100)).toHaveProperty('error');
  expect(built).toHaveLength(2);
  // The second rebuild waits 4 s, not 2 s.
  await vi.advanceTimersByTimeAsync(2_000);
  expect(await settle(exchange.fetchOHLCV(opts), 0)).toMatchObject({
    error: expect.objectContaining({ message: expect.stringMatching(/recovering/) }),
  });
  await vi.advanceTimersByTimeAsync(2_000);
  expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: candles });
  expect(built).toHaveLength(3);
});
