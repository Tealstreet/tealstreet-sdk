import type { Market, Ticker, TickerObservedAt } from '../../types';

import { expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import {
  inProcessPortPair,
  RustPublicFeedClient,
  rustPublicFeedIdentity,
  RustPublicFeedProvider,
} from '../../exchanges/rust/publicFeed';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const observationFrame = (times: number[]) => {
  const bytes = new ArrayBuffer(56);
  const view = new DataView(bytes);
  new Uint8Array(bytes, 0, 4).set([83, 67, 88, 70]);
  view.setUint16(4, 1, true);
  view.setUint16(6, 1, true);
  view.setFloat64(8, 20_000, true);
  view.setUint16(16, 24, true);
  view.setUint32(20, 32, true);
  [0, ...times].forEach((value, slot) => view.setFloat64(24 + slot * 8, value, true));
  return bytes;
};

it('a real observation-only frame reaches MessagePort consumers with source time and legacy negotiation', async () => {
  const store = new DefaultStore();
  store.update({
    markets: [{ id: 'BTC', symbol: 'BTC-PERP', contractSize: 1 }] as Market[],
    tickers: [{ id: 'BTC', symbol: 'BTC-PERP', bid: 0, ask: 0, mark: 100, last: 99, index: 98,
      percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0 }],
    loaded: { markets: true, tickers: true, balance: false, orders: false, positions: false },
  });
  const source = new RustEngineExchange('hyperliquid', { key: '', secret: '', publicDataOnly: true }, store);
  Object.defineProperty(source, 'hasStarted', { get: () => true });
  const provider = new RustPublicFeedProvider(source);
  const peers: Array<{
    client: RustPublicFeedClient;
    stop: () => void;
    updates: Array<{ tickers?: Array<{ observedAt?: TickerObservedAt }> }>;
  }> = [];
  try {
    for (const observations of [true, false]) {
      const [port, sourcePort] = inProcessPortPair();
      const stop = provider.attach(sourcePort);
      const updates: Array<{ tickers?: Array<{ observedAt?: TickerObservedAt }> }> = [];
      const client = new RustPublicFeedClient(
        port,
        rustPublicFeedIdentity(source.name, source.options),
        async (update) => {
          updates.push(update);
        },
        (error) => {
          throw error;
        },
        { observations },
      );
      peers.push({ client, stop, updates });
      await client.connect();
    }
    const receive = (source as unknown as { receiveFrame(buffer: ArrayBuffer): void }).receiveFrame;
    receive(observationFrame([1000, 8500, 1000]));
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(store.tickers[0].observedAt).toEqual([1000, 8500, 1000]);
    expect(peers[0].updates.at(-1)?.tickers?.[0].observedAt).toEqual([1000, 8500, 1000]);
    expect(peers[1].updates.at(-1)?.tickers?.[0]).not.toHaveProperty('observedAt');
    receive(observationFrame([1000, 9000, 1000]));
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(peers[0].updates.at(-1)?.tickers?.[0].observedAt).toEqual([1000, 9000, 1000]);
    expect(store.tickers[0].mark).toBe(100);
    expect(new Frame(observationFrame([1000, 9000, 1000])).time).toBe(20_000);
  } finally {
    for (const peer of peers) {
      peer.client.dispose();
      peer.stop();
    }
    provider.dispose();
    source.dispose();
  }
});

it('worker object snapshots retain observation provenance without dating their arrival', () => {
  const store = new DefaultStore();
  const row: Ticker = { id: 'BTC', symbol: 'BTC-PERP', bid: 0, ask: 0, last: 99, mark: 100, index: 0,
    percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0,
    observedAt: [1000, 8500, null] };
  store.update({ tickers: [row] });
  const identity = store.tickers[0];
  store.update({ tickers: [{ ...row, observedAt: [1000, 9000, null] }] }, true);
  expect(store.tickers[0]).toBe(identity);
  expect(store.tickers[0].observedAt).toEqual([1000, 9000, null]);
  const { observedAt: _observedAt, ...legacy } = row;
  store.update({ tickers: [legacy] }, true);
  expect(store.tickers[0]).not.toHaveProperty('observedAt');
});
