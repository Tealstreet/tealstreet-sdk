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
      await input.replayPublicFrames();
      expect(await startup, `${name}: ${JSON.stringify(input.errors)} ${JSON.stringify(errors)}`).toBe(true);
      expect(worker.exchange).toBeInstanceOf(RustEngineExchange);
      expect(worker.store.markets.length, input.source).toBeGreaterThan(0);
      expect(worker.store.tickers.length, input.source).toBeGreaterThan(0);
      expect(worker.store.loaded.markets).toBe(true);
      expect(worker.store.loaded.tickers).toBe(true);
      // Engine input is immediate; the shipping worker exposes store snapshots
      // on its normal 250 ms flush cadence.
      await new Promise((resolve) => setTimeout(resolve, 275));
      expect(input.requests.length).toBeGreaterThan(0);
      expect(input.errors).toEqual([]);
      expect(errors).toEqual([]);

      const before = frames.length;
      worker.sendSnapshot();
      expect(frames.length).toBeGreaterThan(before);
      const lateStore = new DefaultStore();
      new FrameApplier(lateStore).apply(new Frame(frames.at(-1)!));
      expect(lateStore.markets.map((row) => row.id)).toEqual(worker.store.markets.map((row) => row.id));
      expect(lateStore.tickers.map((row) => [row.id, row.last, row.bid, row.ask])).toEqual(
        worker.store.tickers.map((row) => [row.id, row.last, row.bid, row.ask]),
      );
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
      if (name === 'bybitspot' || name === 'coinbasespot')
        expect(lateStore.markets.every((row) => row.isSpot)).toBe(true);
      if (name === 'bitgetinverse' || name === 'bybitinverse')
        expect(lateStore.markets.every((row) => row.isInverse)).toBe(true);
      if (name === 'bitgetusdc') expect(lateStore.markets.every((row) => row.quote === 'USDC')).toBe(true);
    },
    20_000,
  );
});
