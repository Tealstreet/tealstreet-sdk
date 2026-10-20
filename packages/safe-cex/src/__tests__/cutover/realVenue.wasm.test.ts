// @vitest-environment node
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import {
  RUST_VENUES,
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { setMainAcceptsFrames } from '../../webworker/frameForwarding';
import { realVenue } from './realVenue';

// Clock synchronization is host policy. Supplying a local server-time source
// keeps the real worker lifecycle without its unrelated live Bybit time read.
vi.mock('../../utils/virtualClock', async (original) => {
  const actual = await original<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: async () => {} } };
});

describe('every shipping Rust venue through the real worker frame bridge', () => {
  let worker: ExchangeWorker | undefined;
  let input: ReturnType<typeof realVenue> | undefined;
  const pendingTimers = new Set<ReturnType<typeof setTimeout>>();
  afterEach(() => {
    worker?.dispose();
    input?.dispose();
    virtualClock.stop();
    for (const handle of pendingTimers) clearTimeout(handle);
    pendingTimers.clear();
    setMainAcceptsFrames(false);
    vi.restoreAllMocks();
  });

  it.each(Object.keys(RUST_VENUES) as ExchangeName[])(
    '%s starts natively and sends a complete late-consumer snapshot',
    async (name) => {
      const nativeTimeout = globalThis.setTimeout;
      vi.spyOn(globalThis, 'setTimeout').mockImplementation(((...args: Parameters<typeof setTimeout>) => {
        const handle = nativeTimeout(...args);
        pendingTimers.add(handle);
        return handle;
      }) as typeof setTimeout);
      input = realVenue(name);
      setWasmEngineLoader(loadNodeEngine);
      setRustEngineTransport(() => input!.transport);
      setMainAcceptsFrames(true);
      const frames: ArrayBuffer[] = [];
      const storePayloads: Record<string, unknown>[] = [];
      const errors: unknown[] = [];
      const ctx = {
        postMessage(message: { op: string; data: unknown }, transfer?: ArrayBuffer[]) {
          // Exercise the transfer boundary: the worker's original buffer detaches.
          const copy = structuredClone(message, transfer ? { transfer } : undefined);
          if (copy.op === 'frame') frames.push(copy.data as ArrayBuffer);
          if (copy.op === 'store') storePayloads.push(copy.data as Record<string, unknown>);
          if (copy.op === 'error') errors.push(copy.data);
        },
      };
      worker = new ExchangeWorker(
        ctx as unknown as Worker,
        `cutover-${name}`,
        name,
        input.options,
        {} as AppSettingsWorkerType,
        {} as ExchangeMetadataWorkerType,
        'cutover-session',
      );
      const startup = worker.start();
      if (name === 'hyperliquid') {
        // Paced catalog discovery finishes before this venue creates its socket.
        expect(await startup).toBe(true);
      }
      if (name === 'polymarket' || name === 'paradex') {
        // Subscribe the books represented by the captured input. Polymarket
        // opens its socket lazily; Paradex must match the taped book ACKs.
        expect(await startup).toBe(true);
        if (name === 'polymarket') expect(input.connectedSocketUrls).toEqual([]);
        else expect(input.publicBookSymbols).toEqual(['BTC-USD-PERP', 'ETH-USD-PERP']);
        expect(input.publicBookSymbols.length).toBeGreaterThan(0);
        for (const symbol of input.publicBookSymbols)
          worker.handlePayload({ op: 'subscribeOrderBook', data: { symbol, subId: `cutover-${symbol}` } });
      }
      let framesBeforeReplay = frames.length;
      if (input.polling) {
        // DXtrade market reads require a documented test login even in public
        // mode. Observe a natural post-startup HTTP poll, never invent a WS.
        expect(await startup).toBe(true);
        expect(input.breakoutQuotes.length).toBeGreaterThan(1);
        const startupPolls = input.publicPollResponses.length;
        expect(startupPolls).toBeGreaterThan(0);
        const startupQuotes = worker.store.tickers.map(({ symbol, bid, ask }) => ({ symbol, bid, ask }));
        framesBeforeReplay = frames.length;
        await vi.waitFor(
          () => {
            expect(input!.publicPollResponses.length).toBeGreaterThan(startupPolls);
            expect(worker!.store.tickers.map(({ symbol, bid, ask }) => ({ symbol, bid, ask }))).toEqual(
              input!.publicPollResponses.at(-1)!.quotes,
            );
            expect(worker!.store.tickers.map(({ symbol, bid, ask }) => ({ symbol, bid, ask }))).not.toEqual(
              startupQuotes,
            );
          },
          { timeout: 7000, interval: 25 },
        );
        // Natural polling may skip intermediate fixture timestamps. The exact
        // changed quote response must still be one of the recorded inputs.
        expect(input.breakoutQuotes).toContainEqual(input.publicPollResponses.at(-1)!.quotes);
      } else {
        const delivered = await input.replayPublicFrames();
        expect(delivered, `${name}: no fixture input reached a matching native socket`).toBeGreaterThan(0);
        expect(input.replayedSocketUrls.size).toBeGreaterThan(0);
      }
      expect(await startup, `${name}: ${JSON.stringify(input.errors)} ${JSON.stringify(errors)}`).toBe(true);
      expect(worker.exchange).toBeInstanceOf(RustEngineExchange);
      expect(worker.store.markets.length, input.source).toBeGreaterThan(0);
      expect(worker.store.tickers.length, input.source).toBeGreaterThan(0);
      expect(worker.store.loaded.markets).toBe(true);
      expect(worker.store.loaded.tickers).toBe(true);
      // Engine input is immediate; the shipping worker exposes store snapshots
      // on its normal 250 ms flush cadence.
      await new Promise((resolve) => setTimeout(resolve, 275));
      if (input.polling) {
        expect(input.connectedSocketUrls).toEqual([]);
        expect(input.replayedSocketUrls.size).toBe(0);
        expect(input.requests.some((request) => request.endsWith('/login'))).toBe(true);
        expect(input.requests.filter((request) => request.endsWith('/marketdata')).length).toBeGreaterThan(1);
      }
      for (const url of input.connectedSocketUrls) expect(new URL(url).protocol).toBe('wss:');
      for (const url of input.canonicalConnectedSocketUrls)
        expect(input.fixtureSocketUrls.has(url), `${name}: untaped native socket ${url}`).toBe(true);
      for (const url of input.replayedSocketUrls) expect(input.fixtureSocketUrls.has(url), url).toBe(true);
      // Public input and startup may interleave: require a real native public
      // table/delta frame before requesting the later full snapshot. A fixture
      // need not change an existing quote to prove startup and frame delivery.
      expect(
        frames.slice(framesBeforeReplay).some((buffer) => {
          const frame = new Frame(buffer);
          return (frame.tickers()?.count ?? 0) > 0 || (frame.tickerDeltas()?.length ?? 0) > 0;
        }),
        `${name}: no native public ticker frame during replay/startup`,
      ).toBe(true);
      expect(input.requests.length).toBeGreaterThan(0);
      expect(input.errors).toEqual([]);
      expect(errors).toEqual([]);

      const before = frames.length;
      worker.sendSnapshot();
      expect(frames.length).toBeGreaterThan(before);
      const lateStore = new DefaultStore();
      new FrameApplier(lateStore).apply(new Frame(frames.at(-1)!));
      // A late HTTP reply may update native state just after the periodic
      // flush. Wait for the real store mirror to catch up with the snapshot;
      // this is bounded eventual consistency, not an artificial local flush.
      await vi.waitFor(
        () => {
          expect(lateStore.tickers.map((row) => [row.id, row.last, row.bid, row.ask])).toEqual(
            worker!.store.tickers.map((row) => [row.id, row.last, row.bid, row.ask]),
          );
        },
        { timeout: 3000, interval: 25 },
      );
      expect(lateStore.markets.map((row) => row.id)).toEqual(worker.store.markets.map((row) => row.id));
      expect(lateStore.tickers.some((row) => row.last > 0 || row.bid > 0 || row.ask > 0)).toBe(true);
      expect(lateStore.loaded.markets).toBe(true);
      expect(lateStore.loaded.tickers).toBe(true);
      // Frame mode carries venue collections exactly once; derived worker fields
      // continue over the normal store payload alongside the transferred frame.
      expect(storePayloads.length).toBeGreaterThan(0);
      for (const payload of storePayloads) {
        for (const key of ['markets', 'tickers', 'orders', 'positions', 'balance'])
          expect(payload).not.toHaveProperty(key);
      }
      if (['bybitspot', 'coinbasespot', 'binancespot', 'kucoinspot', 'hyperliquidspot', 'krakenspot'].includes(name))
        expect(lateStore.markets.every((row) => row.isSpot)).toBe(true);
      if (name === 'hyperliquidspot' || name === 'krakenspot') {
        expect(input.spotMarkets.length).toBeGreaterThan(0);
        for (const store of [worker.store, lateStore]) {
          expect(store.markets.map(({ id, symbol, base, quote }) => ({ id, symbol, base, quote }))).toEqual(input.spotMarkets);
          expect(store.markets.every((row) => !row.isInverse && !row.isPerpetual && row.limits.leverage.max === 1)).toBe(true);
          expect(store.positions.every((row) => row.contracts === 0)).toBe(true);
        }
      }
      if (name === 'bitgetinverse' || name === 'bybitinverse' || name === 'binanceinverse')
        expect(lateStore.markets.every((row) => row.isInverse)).toBe(true);
      if (name === 'bitgetusdc') expect(lateStore.markets.every((row) => row.quote === 'USDC')).toBe(true);
      if (name === 'binancespot' || name === 'binanceinverse') {
        expect(input.binanceMarkets.length).toBeGreaterThan(0);
        for (const store of [worker.store, lateStore]) {
          expect(
            store.markets.map(({ id, symbol, base, quote, active, contractSize }) => ({
              id,
              symbol,
              base,
              quote,
              active,
              ...(name === 'binanceinverse' ? { contractSize } : {}),
            })),
          ).toEqual(input.binanceMarkets);
          if (name === 'binanceinverse') {
            expect(store.markets.every((row) => row.isInverse && !row.isSpot && row.quote === 'USD')).toBe(true);
            expect(store.markets.find((row) => row.symbol === 'BTCUSD_PERP')?.contractSize).toBe(100);
          } else expect(store.markets.every((row) => row.isSpot && !row.isInverse)).toBe(true);
        }
      }
      if (name === 'grvt') {
        expect(input.grvtMarkets.length).toBeGreaterThan(0);
        for (const store of [worker.store, lateStore]) {
          expect(
            store.markets.map(({ id, symbol, base, quote, precision }) => ({ id, symbol, base, quote, precision })),
          ).toEqual(input.grvtMarkets);
          expect(store.markets.every((row) => row.isPerpetual && row.symbol.endsWith('_PERP'))).toBe(true);
        }
      }
      if (name === 'polymarket') {
        expect(input.predictionMarkets).toHaveLength(4);
        expect(new Set(input.predictionMarkets.map((row) => row.info.negRisk))).toEqual(new Set([false, true]));
        for (const store of [worker.store, lateStore]) {
          expect(store.markets.map(({ id, symbol, info }) => ({ id, symbol, info }))).toEqual(input.predictionMarkets);
          expect(
            store.markets.every(
              (row) => row.quote === 'pUSD' && row.limits.leverage.min === 1 && row.limits.leverage.max === 1,
            ),
          ).toBe(true);
        }
        const books = frames.slice(framesBeforeReplay).flatMap((buffer) => [...new Frame(buffer).books()]);
        for (const symbol of input.publicBookSymbols)
          expect(
            books.some(
              (book) =>
                lateStore.markets[book.market]?.symbol === symbol && book.bids.length > 0 && book.asks.length > 0,
            ),
          ).toBe(true);
      }
    },
    20_000,
  );
});
