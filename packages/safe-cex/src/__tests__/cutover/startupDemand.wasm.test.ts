// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { Candle, OrderBook } from '../../types';

import { MessageChannel } from 'node:worker_threads';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustPublicFeedProvider } from '../../exchanges/rust/publicFeed';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { realVenue } from './realVenue';

const cleanups: Array<() => void> = [];
afterEach(() => {
  cleanups
    .splice(0)
    .reverse()
    .forEach((cleanup) => cleanup());
});

function input(gate?: Promise<void>, alterCatalog = false) {
  const recorded = realVenue('bybit');
  cleanups.push(() => recorded.dispose());
  const packets: Array<{ op?: string; args?: string[] }> = [];
  const messages: Array<(text: string) => void> = [];
  const metadataRequested = vi.fn();
  const transport: Transport = {
    ...recorded.transport,
    async request(request) {
      const metadata = new URL(request.url).pathname.endsWith('/instruments-info');
      if (metadata) {
        metadataRequested();
        await gate;
      }
      const response = await recorded.transport.request(request);
      if (metadata && alterCatalog) {
        if (!(response.body instanceof Uint8Array)) throw new Error('Recorded metadata must be bytes');
        const payload = JSON.parse(new TextDecoder().decode(response.body));
        // A different native instrument ID cannot consume this source catalog.
        payload.result.list[0].symbol += '-OTHER';
        return { ...response, body: new TextEncoder().encode(JSON.stringify(payload)) };
      }
      return response;
    },
    connect(url, handlers) {
      if (handlers.channel === 'public') messages.push(handlers.onMessage);
      const socket = recorded.transport.connect(url, handlers);
      return {
        close: () => socket.close(),
        send(text) {
          packets.push(JSON.parse(text));
          socket.send(text);
        },
      };
    },
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  return { recorded, packets, messages, metadataRequested };
}

it('actual native startup replays retained books/candles after delayed metadata and delivers their data', async () => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const f = input(gate);
  const source = new RustEngineExchange('bybit', f.recorded.options, new DefaultStore());
  cleanups.push(() => source.dispose());
  const books: OrderBook[] = [];
  const candles: Candle[] = [];
  source.listenOrderBook('BTCUSDT', (book) => books.push(book));
  const removeBook = source.listenOrderBook('ETHUSDT', () => {});
  removeBook();
  source.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, (candle) => candles.push(candle));
  const removeCandle = source.listenOHLCV({ symbol: 'ETHUSDT', interval: '1m' }, () => {});
  removeCandle();
  const publicRead = source.fetchMarkets();
  await vi.waitFor(() => expect(f.metadataRequested).toHaveBeenCalled(), { timeout: 5000 });
  expect(f.packets).toEqual([]);
  release();
  expect((await publicRead).some(({ symbol }) => symbol === 'BTCUSDT')).toBe(true);
  await source.start();
  await vi.waitFor(() => expect(f.messages.length).toBeGreaterThan(0), { timeout: 5000 });
  await vi.waitFor(() => expect(f.packets.flatMap(({ args }) => args ?? [])).toContain('kline.1.BTCUSDT'), {
    timeout: 5000,
  });
  const topics = f.packets.filter(({ op }) => op === 'subscribe').flatMap(({ args }) => args ?? []);
  expect(topics.filter((topic) => topic.startsWith('orderbook.') && topic.endsWith('.BTCUSDT'))).toHaveLength(1);
  expect(topics.filter((topic) => topic.startsWith('orderbook.') && topic.endsWith('.ETHUSDT'))).toHaveLength(0);
  expect(topics.filter((topic) => topic === 'kline.1.BTCUSDT')).toHaveLength(1);
  expect(topics.filter((topic) => topic === 'kline.1.ETHUSDT')).toHaveLength(0);
  // Documented Bybit v5 public snapshot and kline shapes; native owns parsing/state.
  for (const receive of f.messages) {
    receive(
      JSON.stringify({
        topic: 'orderbook.50.BTCUSDT',
        type: 'snapshot',
        ts: 1700000000000,
        data: { s: 'BTCUSDT', b: [['99', '2']], a: [['101', '3']], u: 1, seq: 1 },
      }),
    );
    receive(
      JSON.stringify({
        topic: 'kline.1.BTCUSDT',
        type: 'snapshot',
        ts: 1700000000000,
        data: [
          {
            start: 1700000000000,
            end: 1700000059999,
            interval: '1',
            open: '100',
            high: '102',
            low: '99',
            close: '101',
            volume: '5',
            turnover: '500',
            confirm: false,
            timestamp: 1700000000000,
          },
        ],
      }),
    );
  }
  await vi.waitFor(
    () => {
      expect(books.at(-1)?.bids[0]).toMatchObject({ price: 99, amount: 2 });
      expect(candles.at(-1)).toMatchObject({ open: 100, close: 101, high: 102, low: 99, volume: 5 });
    },
    { timeout: 5000 },
  );
  expect(f.recorded.errors).toEqual([]);
}, 15_000);

it('an actual native shared-catalog mismatch never acquires provider book or candle demand', async () => {
  const sourceInput = input();
  const source = new RustEngineExchange('bybit', sourceInput.recorded.options, new DefaultStore());
  cleanups.push(() => source.dispose());
  await source.start();
  const provider = new RustPublicFeedProvider(source);
  cleanups.push(() => provider.dispose());
  const bookDemand = vi.spyOn(source, 'listenOrderBookForFeed');
  const candleDemand = vi.spyOn(source, 'listenOHLCVForFeed');
  const channel = new MessageChannel();
  cleanups.push(() => {
    channel.port1.close();
    channel.port2.close();
  });
  provider.attach(channel.port1 as unknown as MessagePort);
  const receiverInput = input(undefined, true);
  const receiver = new RustEngineExchange(
    'bybit',
    {
      ...receiverInput.recorded.options,
      publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort },
    },
    new DefaultStore(),
  );
  cleanups.push(() => receiver.dispose());
  receiver.listenOrderBook('BTCUSDT', () => {});
  receiver.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, () => {});
  await expect(receiver.fetchMarkets()).rejects.toThrow('catalog/product mismatch');
  await new Promise<void>((resolve) => setImmediate(resolve));
  expect(bookDemand).not.toHaveBeenCalled();
  expect(candleDemand).not.toHaveBeenCalled();
  expect(receiver.isDisposed).toBe(true);
  expect(source.isDisposed).toBe(false);
}, 15_000);

it('shared ticker reads wait for the accepted provider snapshot while catalog reads finish earlier', async () => {
  const sourceInput = input();
  const source = new RustEngineExchange('bybit', sourceInput.recorded.options, new DefaultStore());
  cleanups.push(() => source.dispose());
  await source.start();
  for (const receive of sourceInput.messages)
    receive(
      JSON.stringify({
        topic: 'tickers.BTCUSDT',
        type: 'delta',
        ts: 1700000000000,
        data: { symbol: 'BTCUSDT', bid1Price: '17', ask1Price: '18', lastPrice: '17.5' },
      }),
    );
  await vi.waitFor(() => expect(source.store.tickersBySymbol.BTCUSDT?.bid).toBe(17), { timeout: 5000 });
  const provider = new RustPublicFeedProvider(source);
  cleanups.push(() => provider.dispose());
  const upstream = new MessageChannel();
  const downstream = new MessageChannel();
  cleanups.push(() => {
    upstream.port1.close();
    upstream.port2.close();
    downstream.port1.close();
    downstream.port2.close();
  });
  provider.attach(upstream.port1 as unknown as MessagePort);
  let releaseQuotes = false;
  const held: Array<{ op: string }> = [];
  upstream.port2.on('message', (message: { op: string }) => {
    if (message.op === 'publicFeedUpdate' && !releaseQuotes) held.push(message);
    else downstream.port1.postMessage(message);
  });
  downstream.port1.on('message', (message) => upstream.port2.postMessage(message));
  const receiverInput = input();
  const receiver = new RustEngineExchange(
    'bybit',
    {
      ...receiverInput.recorded.options,
      publicFeed: { mode: 'shared', port: downstream.port2 as unknown as MessagePort },
    },
    new DefaultStore(),
  );
  cleanups.push(() => receiver.dispose());
  const catalog = receiver.fetchMarkets();
  let tickersFinished = false;
  const quotes = receiver.fetchTickers().then((rows) => {
    tickersFinished = true;
    return rows;
  });
  expect((await catalog).some(({ symbol }) => symbol === 'BTCUSDT')).toBe(true);
  await vi.waitFor(
    () => {
      expect(receiver.store.loaded.tickers).toBe(true);
      expect(held.length).toBeGreaterThan(0);
    },
    { timeout: 5000 },
  );
  expect(receiver.store.tickersBySymbol.BTCUSDT?.bid).not.toBe(17);
  expect(tickersFinished).toBe(false);
  releaseQuotes = true;
  held.splice(0).forEach((message) => downstream.port1.postMessage(message));
  expect((await quotes).find(({ symbol }) => symbol === 'BTCUSDT')?.bid).toBe(17);
}, 15_000);
