import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../exchanges/rust/rust.exchange';
import type { ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { disposePageCandleSources, RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { FrameStore } from '../store/frameStore';
import { consumerFrame } from './cutover/consumerFrame';
import { engineCapabilities } from './fixtures/engineCapabilities';

const exchanges: RustEngineExchange[] = [];
const opts = { symbol: 'BTCUSDT', interval: '15m' as const, from: 1_700_000_000_000, to: 1_700_000_900_000 };
const candles = [{ timestamp: 1_700_000_000, open: 1, high: 2, low: 1, close: 2, volume: 1 }];

type CandleRead = 'answers' | 'venueTimeout' | 'softTimeout';

/**
 * A mirror whose page candle sources are fake engines: `health` decides, per construction,
 * whether the source loads its markets, never does, or fails to start. `candleRead: 'venueTimeout'`
 * issues the candle request one dispatch hop after receipt and fails it when the engine's own
 * 20 s request budget runs out, as a venue that never answers does; an array picks per construction,
 * and a nested array per candle read.
 */
const fixture = (
  health: Array<'ok' | 'late' | 'stuck' | 'fails'>,
  candleRead: CandleRead | Array<CandleRead | CandleRead[]> = 'answers',
) => {
  const built: Array<{ commands: Array<Record<string, unknown>>; freed: ReturnType<typeof vi.fn> }> = [];
  const Constructor = Object.assign(
    function (_venue: string, json: string) {
      const config = JSON.parse(json);
      const page = config.publicDataOnly === true;
      const state = page ? (health[built.length] ?? 'ok') : 'ok';
      const planned = Array.isArray(candleRead) ? (candleRead[built.length] ?? 'answers') : candleRead;
      let candleReads = 0;
      const nextRead = () => (Array.isArray(planned) ? (planned[candleReads++] ?? 'answers') : planned);
      const record = { commands: [] as Array<Record<string, unknown>>, freed: vi.fn() };
      if (page) built.push(record);
      // 'late': the catalogue lands 15 s after start, past the 10 s per-read wait.
      let landed = state === 'ok';
      const frame = () =>
        new Uint8Array(
          consumerFrame({
            markets: [],
            tickers: [],
            loaded: { markets: landed, tickers: landed, balance: true, orders: true, positions: true },
          }),
        );
      const idle = () => '[]';
      const pendingReads = new Map<number, { command: number; stage: 'issue' | 'deadline' }>();
      let timers = 100;
      return {
        construction: () => JSON.stringify({ frameFed: false, publicDataOnly: page }),
        syncHostTime: () => true,
        capabilities: () => engineCapabilities('bybit'),
        start: () => {
          if (state === 'fails') throw new Error('instrument read refused');
          return state === 'late' ? JSON.stringify([{ op: 'setTimer', timer: 1, at: Date.now() + 15_000 }]) : '[]';
        },
        timer: (timer: number) => {
          const read = pendingReads.get(timer);
          if (!read) {
            landed = true;
            return '[]';
          }
          pendingReads.delete(timer);
          if (read.stage === 'issue') {
            pendingReads.set(++timers, { command: read.command, stage: 'deadline' });
            return JSON.stringify([{ op: 'setTimer', timer: timers, at: Date.now() + 20_000 }]);
          }
          const message = 'HTTP request timed out (class=read budget=20000ms elapsed=20000ms)';
          return JSON.stringify([
            { op: 'done', command: read.command, error: message, detail: { message, kind: 'timeout' } },
          ]);
        },
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
          const read = command.op === 'fetchCandles' ? nextRead() : 'answers';
          if (read === 'venueTimeout') {
            pendingReads.set(++timers, { command: command.command, stage: 'issue' });
            return JSON.stringify([{ op: 'setTimer', timer: timers, at: Date.now() + 1 }]);
          }
          // OKX's native soft shape: an attributed timeout, then [] as the answer.
          if (read === 'softTimeout') {
            return JSON.stringify([
              { op: 'commandError', command: command.command, message: 'Request timed out' },
              { op: 'done', command: command.command, result: '[]' },
            ]);
          }
          return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(candles) }]);
        },
      } satisfies WasmEngineLike;
    },
    { frameFedSupported: () => false },
  );
  setWasmEngineLoader(async () => ({ WasmEngine: Constructor as unknown as WasmEngineModule['WasmEngine'] }));
  const mirror = (options: Partial<ExchangeOptions> = {}) => {
    const exchange = new RustEngineExchange(
      'bybit',
      { key: 'k', secret: 's', workerMirror: true, testnet: true, ...options },
      new FrameStore(),
    );
    exchanges.push(exchange);
    exchange.on('callMethod', vi.fn());
    return exchange;
  };
  return { exchange: mirror(), mirror, built };
};

afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  disposePageCandleSources();
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
  expect(built[0].commands.find((command) => command.op === 'fetchCandles')).toMatchObject({ timeoutMs: 25_000 });
});

// TEALSTREET-NEXT-ED8/E74: the read's own 20 s deadline matched the engine's 20 s candle request
// budget, which starts at issuance, so the host's generic "This action timed out after 20000 ms"
// always won and discarded the engine's account of where the request was stuck.
it('a venue that never answers a page candle read fails with the engine timeout, not a bare deadline', async () => {
  vi.useFakeTimers();
  const { exchange } = fixture(['ok'], 'venueTimeout');
  const result = await settle(exchange.fetchOHLCV(opts), 26_000);
  expect(result).toMatchObject({
    error: expect.objectContaining({ message: expect.stringMatching(/^HTTP request timed out \(class=read/) }),
  });
  expect((result as { error: Error }).error.name).not.toBe('CommandTimeoutError');
});

// TEALSTREET-NEXT-ECB/ED8: a source with markets whose candle requests never answered was never
// called broken, so it failed every read at 20 s for hours, and outlived account reloads.
it('a loaded source whose candle reads keep timing out is rebuilt, not reused', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['ok', 'ok'], ['venueTimeout', 'answers']);
  // One timeout is a slow venue: the source is kept.
  expect(await settle(exchange.fetchOHLCV(opts), 26_000)).toHaveProperty('error');
  expect(built[0].freed).not.toHaveBeenCalled();
  expect(await settle(exchange.fetchOHLCV(opts), 26_000)).toHaveProperty('error');
  expect(built[0].freed).toHaveBeenCalled();

  await vi.advanceTimersByTimeAsync(2_000);
  expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: candles });
  expect(built).toHaveLength(2);
});

// TEALSTREET-NEXT-E95: OKX resolves a timed-out candle read as [], which counted as answered,
// so its wedged source was never rebuilt.
it('soft-shape candle timeouts count as unanswered, so the source is rebuilt', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['ok', 'ok'], ['softTimeout', 'answers']);
  expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: [] });
  expect(built[0].freed).not.toHaveBeenCalled();
  expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: [] });
  expect(built[0].freed).toHaveBeenCalled();

  await vi.advanceTimersByTimeAsync(2_000);
  expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: candles });
  expect(built).toHaveLength(2);
});

it('an answered candle read clears the timeout run, so isolated slow reads keep the source', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['ok'], [['venueTimeout', 'answers', 'venueTimeout']]);
  expect(await settle(exchange.fetchOHLCV(opts), 26_000)).toHaveProperty('error');
  expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: candles });
  expect(await settle(exchange.fetchOHLCV(opts), 26_000)).toHaveProperty('error');
  expect(built[0].freed).not.toHaveBeenCalled();
  expect(built).toHaveLength(1);
});

// #3095 review: a slow catalogue was torn down at every 10 s read and never converged.
it('a source whose markets land at 15 s serves the next read itself, not a rebuild', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['late']);
  const first = await settle(exchange.fetchOHLCV(opts), 10_500);
  expect(first).toMatchObject({
    error: expect.objectContaining({ message: expect.stringMatching(/Timed out waiting for native markets/) }),
  });
  expect(built[0].freed).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(5_000);
  expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: candles });
  expect(built).toHaveLength(1);
});

it('a source whose markets never load is kept for a minute, then rebuilt after a backoff', async () => {
  vi.useFakeTimers();
  const { exchange, built } = fixture(['stuck', 'ok']);
  const first = await settle(exchange.fetchOHLCV(opts), 10_500);
  expect(first).toMatchObject({
    error: expect.objectContaining({ message: expect.stringMatching(/Timed out waiting for native markets/) }),
  });
  expect(built[0].freed).not.toHaveBeenCalled();

  // Reads keep their 10 s bound; at a minute without markets the source is called stuck.
  await vi.advanceTimersByTimeAsync(40_000);
  expect(await settle(exchange.fetchOHLCV(opts), 10_500)).toHaveProperty('error');
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

// AGT-299: each mirror used to build its own source, so every account and replacement ran a full
// market discovery against one shared /info budget.
describe('one page candle source per realm and venue configuration', () => {
  it('mirrors of different accounts share one source', async () => {
    vi.useFakeTimers();
    const { exchange, mirror, built } = fixture(['ok']);
    const other = mirror({ key: 'k2', secret: 's2', applicationId: '0xother', subaccount: 'second' });
    expect(await settle(exchange.fetchOHLCV(opts))).toEqual({ value: candles });
    expect(await settle(other.fetchOHLCV(opts))).toEqual({ value: candles });
    expect(built).toHaveLength(1);
    expect(built[0].commands.filter((command) => command.op === 'fetchCandles')).toHaveLength(2);
  });

  it('a different venue configuration gets its own source', async () => {
    vi.useFakeTimers();
    const { exchange, mirror, built } = fixture(['ok', 'ok']);
    const livenet = mirror({ testnet: false });
    await settle(exchange.fetchOHLCV(opts));
    await settle(livenet.fetchOHLCV(opts));
    expect(built).toHaveLength(2);
  });

  it('a replacement mirror reuses the source, which outlives its last holder only briefly', async () => {
    vi.useFakeTimers();
    const { exchange, mirror, built } = fixture(['ok', 'ok']);
    await settle(exchange.fetchOHLCV(opts));
    exchange.dispose();
    expect(built[0].freed).not.toHaveBeenCalled();
    const replacement = mirror();
    expect(await settle(replacement.fetchOHLCV(opts), 1_000)).toEqual({ value: candles });
    expect(built).toHaveLength(1);
    replacement.dispose();
    await vi.advanceTimersByTimeAsync(29_000);
    expect(built[0].freed).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(built[0].freed).toHaveBeenCalled();
    expect(await settle(mirror().fetchOHLCV(opts))).toEqual({ value: candles });
    expect(built).toHaveLength(2);
  });

  it('the rebuild backoff is shared, so another mirror cannot spin a failing venue', async () => {
    vi.useFakeTimers();
    const { exchange, mirror, built } = fixture(['fails', 'ok']);
    expect(await settle(exchange.fetchOHLCV(opts), 100)).toHaveProperty('error');
    expect(await settle(mirror().fetchOHLCV(opts), 0)).toMatchObject({
      error: expect.objectContaining({ message: expect.stringMatching(/recovering/) }),
    });
    expect(built).toHaveLength(1);
  });
});
