// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { Candle, ExchangeOptions, OrderBook } from '../../types';
import type { AppSettingsWorkerType } from '../../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';

import { webcrypto } from 'node:crypto';
import { MessageChannel } from 'node:worker_threads';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { COINBASE_REPLAY, verifyCoinbaseSubscribe } from '../../../../../sdk-rs/harness/coinbase-protocol.mjs';
import { coinbasePrivateTape } from '../../../../../sdk-rs/harness/private-tape-coinbase.mjs';
import { Tape } from '../../../../../sdk-rs/harness/tape.mjs';
import { venueOf } from '../../../../../sdk-rs/harness/venues.mjs';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustPublicFeedProvider } from '../../exchanges/rust/publicFeed';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { virtualClock } from '../../utils/virtualClock';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { setMainAcceptsFrames } from '../../webworker/frameForwarding';
import { consumerFrame } from './consumerFrame';

vi.mock('../../utils/virtualClock', async (original) => {
  const actual = await original<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: async () => {} } };
});

const previousWindowCrypto = Object.getOwnPropertyDescriptor(window, 'crypto');

function nativeInput(publicOnly = false) {
  // SDK setup supplies a minimal window in Node. Native getrandom sees that
  // window first, so expose Node's real cryptographic RNG for JWT nonces.
  Object.defineProperty(window, 'crypto', { configurable: true, value: webcrypto });
  const records = coinbasePrivateTape({ profile: 'spot', venue: 'coinbasespot' }).records as Array<{
    type: string;
    text?: string;
    [key: string]: unknown;
  }>;
  for (const row of [...records])
    if (row.type === 'http' && String(row.url).includes('/brokerage/products'))
      records.push({ ...row, url: String(row.url).replace('/brokerage/products', '/brokerage/market/products') });
  const tape = new Tape(records, venueOf('coinbasespot'), !publicOnly, 2000, 'rust', [], COINBASE_REPLAY);
  const sockets: Array<{
    url: string;
    channel?: string;
    closed: boolean;
    opened: boolean;
    pending: string[];
    onMessage(text: string): void;
  }> = [];
  const requests: string[] = [];
  const sent: Array<{ channel?: string; packet: { type: string; channel: string; product_ids?: string[] } }> = [];
  const failures: string[] = [];
  let replacementPrice: number | undefined;
  const transport: Transport = {
    now: Date.now,
    connect(url, handlers) {
      const socket = {
        url,
        channel: handlers.channel,
        closed: false,
        opened: false,
        pending: [] as string[],
        onMessage: handlers.onMessage,
      };
      sockets.push(socket);
      setTimeout(() => {
        if (socket.closed) return;
        socket.opened = true;
        handlers.onOpen();
        for (const text of socket.pending.splice(0)) socket.onMessage(text);
      }, 0);
      return {
        send(text) {
          sent.push({ channel: socket.channel, packet: JSON.parse(text) });
        },
        close() {
          socket.closed = true;
        },
      };
    },
    async request({ method, url, body, headers }) {
      requests.push(`${method} ${url}`);
      let response;
      try {
        response = tape.respond(method, url, body, headers);
      } catch (error) {
        failures.push(String(error));
        throw error;
      }
      if (response.status === 599) {
        failures.push(`Untaped ${method} ${url}`);
        throw new Error(`Untaped ${method} ${url}`);
      }
      let payload = response.body;
      if (replacementPrice !== undefined && new URL(url).pathname.endsWith('/products')) {
        const decoded = JSON.parse(payload);
        decoded.products = decoded.products.map((product: { product_id: string; base_currency_id: string }) => ({
          ...product,
          ...(replacementPrice !== undefined ? { price: String(replacementPrice) } : {}),
        }));
        payload = JSON.stringify(decoded);
      }
      return { status: response.status, headers: response.headers, body: new TextEncoder().encode(payload) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  return {
    transport,
    sockets,
    requests,
    sent,
    failures,
    setProductPrice(price: number) {
      replacementPrice = price;
    },
    deliver(channel: string) {
      const index = records.findIndex((record) => record.type === 'ws' && JSON.parse(record.text!).channel === channel);
      if (index < 0) throw new Error(`Missing documented ${channel}`);
      // The fixture's first user snapshot follows a private heartbeat. Preserve
      // both sequence numbers instead of fabricating a new initial sequence.
      const frames =
        channel === 'user'
          ? records
              .slice(0, index + 1)
              .filter((record) => record.type === 'ws' && String(record.url).includes('-ws-user.'))
          : [records[index]];
      for (const socket of sockets.filter(
        (candidate) =>
          !candidate.closed && (channel === 'user' ? candidate.channel === 'private' : candidate.channel === 'public'),
      ))
        for (const frame of frames) {
          if (socket.opened) socket.onMessage(frame.text!);
          else socket.pending.push(frame.text!);
        }
    },
    sendPublic(data: unknown) {
      for (const socket of sockets.filter((candidate) => !candidate.closed && candidate.channel === 'public'))
        socket.onMessage(JSON.stringify(data));
    },
  };
}

const waitFor = (assertion: () => unknown) => vi.waitFor(assertion, { timeout: 5000 });

describe('native public-feed ownership through real private workers', () => {
  const cleanups: Array<() => void> = [];
  afterEach(() => {
    for (const cleanup of cleanups.splice(0).reverse()) cleanup();
    virtualClock.stop();
    setMainAcceptsFrames(false);
    if (previousWindowCrypto) Object.defineProperty(window, 'crypto', previousWindowCrypto);
    else Reflect.deleteProperty(window, 'crypto');
  });
  const exchange = async (options: ExchangeOptions, input = nativeInput(Boolean(options.publicDataOnly))) => {
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => input.transport);
    const account = new RustEngineExchange(
      'coinbasespot',
      {
        ...options,
        transport: { ...directTransport, rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }) },
      },
      new DefaultStore(),
    );
    account.on('error', (message: unknown) => input.failures.push(String(message)));
    cleanups.push(() => account.dispose());
    const startup = account.start();
    // Observe rejection immediately, even in explicit early-disposal tests.
    void startup.catch((error) => input.failures.push(String(error)));
    return { account, startup, input };
  };

  it('privateOnly suppresses public sockets while private user data still loads', async () => {
    const { account, input, startup } = await exchange({ ...COINBASE_REPLAY, privateOnly: true });
    await waitFor(() =>
      expect(
        input.sockets.some((socket) => socket.channel === 'private'),
        JSON.stringify({ requests: input.requests, failures: input.failures, sockets: input.sockets }),
      ).toBe(true),
    );
    input.deliver('user');
    await startup;
    expect(input.sockets.map((socket) => socket.channel)).toEqual(['private']);
    expect(account.store.loaded).toEqual({
      markets: true,
      tickers: true,
      balance: true,
      orders: true,
      positions: true,
    });
    expect(account.store.balance.total).toBe(0);
    expect(account.store.balance.currencies?.BTC).toMatchObject({ total: 1.5, free: 1 });
    expect(input.requests.some((request) => request.includes('/products'))).toBe(true);
    expect(verifyCoinbaseSubscribe(input.sent.find(({ packet }) => packet.channel === 'user')!.packet)).toBe(true);
    expect(input.failures).toEqual([]);
  }, 15_000);

  it('fans one provider out to two transferred private-worker ports with native snapshots and isolated state', async () => {
    const publicSide = await exchange({ key: '', secret: '', publicDataOnly: true });
    await publicSide.startup;
    publicSide.account.setActiveTickerSymbols(['BTC-USD']);
    const provider = new RustPublicFeedProvider(publicSide.account);
    cleanups.push(() => provider.dispose());
    const workers: Array<{ worker: ExchangeWorker; input: ReturnType<typeof nativeInput>; frames: ArrayBuffer[] }> = [];
    setMainAcceptsFrames(true);
    for (let index = 0; index < 2; index++) {
      const channel = new MessageChannel();
      provider.attach(channel.port1 as unknown as MessagePort);
      const options = structuredClone(
        {
          ...COINBASE_REPLAY,
          engine: 'rust' as const,
          privateOnly: true,
          publicFeed: { mode: 'shared' as const, port: channel.port2 },
        },
        { transfer: [channel.port2] },
      ) as unknown as ExchangeOptions;
      const input = nativeInput();
      setRustEngineTransport(() => input.transport);
      const frames: ArrayBuffer[] = [];
      const ctx = {
        postMessage(message: { op: string; data: unknown }, transfer?: ArrayBuffer[]) {
          const copy = structuredClone(message, transfer ? { transfer } : undefined);
          if (copy.op === 'frame') frames.push(copy.data as ArrayBuffer);
          if (copy.op === 'error') input.failures.push(JSON.stringify(copy.data));
        },
      };
      const worker = new ExchangeWorker(
        ctx as unknown as Worker,
        `private-${index}`,
        'coinbasespot',
        {
          ...options,
          transport: { ...directTransport, rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }) },
        },
        {} as AppSettingsWorkerType,
        {} as ExchangeMetadataWorkerType,
        'shared-feed-test',
      );
      cleanups.push(() => worker.dispose());
      const startup = worker.start();
      await waitFor(() =>
        expect(
          input.sockets.some((socket) => socket.channel === 'private'),
          JSON.stringify({ requests: input.requests, failures: input.failures, sockets: input.sockets }),
        ).toBe(true),
      );
      input.deliver('user');
      expect(await startup).toBe(true);
      if (index === 0) worker.exchange!.setActiveTickerSymbols(['BTC-USD']);
      workers.push({ worker, input, frames });
    }
    await waitFor(() => expect(publicSide.account.getActiveTickerSymbols()).toEqual([]));
    workers[1].worker.exchange!.setActiveTickerSymbols(['BTC-USD']);
    await waitFor(() => expect(publicSide.account.getActiveTickerSymbols()).toEqual(['BTC-USD']));
    workers[0].worker.exchange!.setActiveTickerSymbols([]);
    await waitFor(() => expect(publicSide.account.getActiveTickerSymbols()).toEqual([]));
    const before = workers.map(({ worker }) =>
      structuredClone({
        balance: worker.store.balance,
        orders: worker.store.orders,
        settings: worker.store.fetchedLeverage,
      }),
    );
    publicSide.input.setProductPrice(777);
    await waitFor(() => {
      expect(publicSide.account.store.tickersBySymbol['BTC-USD'].last).toBe(777);
      for (const { worker } of workers) expect(worker.store.tickersBySymbol['BTC-USD'].last).toBe(777);
    });
    for (const [index, { worker, input, frames }] of workers.entries()) {
      expect(input.sockets.map((socket) => socket.channel)).toEqual(['private']);
      expect(verifyCoinbaseSubscribe(input.sent.find(({ packet }) => packet.channel === 'user')!.packet)).toBe(true);
      expect(input.failures).toEqual([]);
      expect({
        balance: worker.store.balance,
        orders: worker.store.orders,
        settings: worker.store.fetchedLeverage,
      }).toEqual(before[index]);
      const mirrored = new DefaultStore();
      const applier = new FrameApplier(mirrored, true);
      for (const frame of frames) applier.apply(new Frame(frame));
      expect(mirrored.tickersBySymbol['BTC-USD'].last).toBe(777);
      const snapshot = (worker.exchange as RustEngineExchange).snapshotFrame()!;
      const late = new DefaultStore();
      new FrameApplier(late, true).apply(new Frame(snapshot));
      expect(late.tickersBySymbol['BTC-USD'].last).toBe(777);
      expect(late.balance).toEqual(worker.store.balance);
      expect(worker.store.tickersBySymbol['BTC-USD']).not.toBe(publicSide.account.store.tickersBySymbol['BTC-USD']);
    }
    const books: OrderBook[] = [];
    const candles: Candle[] = [];
    const firstAccount = workers[0].worker.exchange as RustEngineExchange;
    const stopBook = firstAccount.listenOrderBook('BTC-USD', (book) => books.push(book));
    const stopCandle = firstAccount.listenOHLCV({ symbol: 'BTC-USD', interval: '5m' }, (candle) =>
      candles.push(candle),
    );
    await waitFor(() => {
      const subscriptions = publicSide.input.sent
        .filter(({ packet }) => packet.type === 'subscribe')
        .map(({ packet }) => packet.channel);
      expect(subscriptions).toContain('level2');
      expect(subscriptions).toContain('candles');
    });
    publicSide.input.sendPublic({
      channel: 'l2_data',
      sequence_num: 0,
      events: [
        {
          type: 'snapshot',
          product_id: 'BTC-USD',
          updates: [
            { side: 'bid', price_level: '776', new_quantity: '2' },
            { side: 'offer', price_level: '778', new_quantity: '3' },
          ],
        },
      ],
    });
    publicSide.input.sendPublic({
      channel: 'candles',
      sequence_num: 1,
      events: [
        {
          type: 'update',
          candles: [
            {
              product_id: 'BTC-USD',
              start: '1765805100',
              open: '770',
              high: '780',
              low: '760',
              close: '777',
              volume: '5',
            },
          ],
        },
      ],
    });
    // Frame-forwarding workers route callbacks on main; inspect the native frames too.
    await waitFor(() => {
      const late = new DefaultStore();
      const applier = new FrameApplier(late, true);
      const frame = new Frame(firstAccount.snapshotFrame()!);
      applier.apply(frame);
      expect(applier.books(frame)[0]?.orderbook.bids[0]).toMatchObject({ price: 776, amount: 2 });
    });
    await waitFor(() => {
      const applier = new FrameApplier(new DefaultStore(), true);
      const found = workers[0].frames.some((buffer) => {
        const frame = new Frame(buffer);
        applier.apply(frame);
        return applier.candles(frame).some(({ candle }) => candle.close === 777 && candle.volume === 5);
      });
      expect(found).toBe(true);
    });
    // The sibling joins after the sole venue book event. The provider must seed
    // it from the source's native snapshot without waiting for another update.
    const sibling = workers[1].worker.exchange as RustEngineExchange;
    const stopSiblingBook = sibling.listenOrderBook('BTC-USD', () => {});
    await waitFor(() => {
      const applier = new FrameApplier(new DefaultStore(), true);
      const frame = new Frame(sibling.snapshotFrame()!);
      applier.apply(frame);
      expect(applier.books(frame)[0]?.orderbook.bids[0]).toMatchObject({ price: 776, amount: 2 });
    });
    stopSiblingBook();
    expect(books).toEqual([]);
    expect(candles).toEqual([]);
    stopBook();
    stopCandle();
    // One account leaving must not close the provider or its sibling.
    workers[0].worker.dispose();
    await waitFor(() => expect(publicSide.account.getActiveTickerSymbols()).toEqual(['BTC-USD']));
    publicSide.input.setProductPrice(888);
    await waitFor(() => expect(workers[1].worker.store.tickersBySymbol['BTC-USD'].last).toBe(888));
    expect(publicSide.input.sockets.every((socket) => !socket.closed)).toBe(true);
    provider.dispose();
    await waitFor(() => expect(workers[1].worker.exchange?.isDisposed).toBe(true));
    expect(workers[1].input.sockets.every((socket) => socket.closed)).toBe(true);
    await expect((workers[1].worker.exchange as RustEngineExchange).fetchPublicTrades('BTC-USD')).rejects.toThrow(
      /disposed/,
    );
  }, 15_000);

  it('rejects mismatched shared feed before private I/O and disposal settles handshake', async () => {
    const publicSide = await exchange({ key: '', secret: '', publicDataOnly: true });
    await publicSide.startup;
    const provider = new RustPublicFeedProvider(publicSide.account);
    cleanups.push(() => provider.dispose());
    const channel = new MessageChannel();
    provider.attach(channel.port1 as unknown as MessagePort);
    const bad = await exchange({
      ...COINBASE_REPLAY,
      testnet: true,
      publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort },
    });
    await expect(bad.startup).rejects.toThrow('mismatch');
    expect(bad.input.requests).toEqual([]);
    expect(bad.input.sockets).toEqual([]);
    const pending = new MessageChannel();
    cleanups.push(() => pending.port1.close());
    const waiting = await exchange({
      ...COINBASE_REPLAY,
      publicFeed: { mode: 'shared', port: pending.port2 as unknown as MessagePort },
    });
    waiting.account.dispose();
    await expect(waiting.startup).rejects.toThrow(/disposed/);
  });

  it('fails closed when a changed catalog crosses the SDK frame boundary after readiness', async () => {
    const source = await exchange({ key: '', secret: '', publicDataOnly: true });
    await source.startup;
    const provider = new RustPublicFeedProvider(source.account);
    cleanups.push(() => provider.dispose());
    const channel = new MessageChannel();
    provider.attach(channel.port1 as unknown as MessagePort);
    const privateSide = await exchange({
      ...COINBASE_REPLAY,
      privateOnly: true,
      publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort },
    });
    await waitFor(() => expect(privateSide.input.sockets.some((socket) => socket.channel === 'private')).toBe(true));
    privateSide.input.deliver('user');
    await privateSide.startup;
    // Coinbase polling changes quotes, not its catalog. This separate SDK
    // control-boundary case injects an independently encoded MARKETS frame;
    // startup and the account remain real WASM, with no claimed venue refresh.
    const boundary = source.account as unknown as { receiveFrame(buffer: ArrayBuffer): void };
    boundary.receiveFrame(
      consumerFrame({
        markets: source.account.store.markets.map((market) => ({
          ...market,
          contractSize: (market.contractSize ?? 1) * 2,
        })),
      }),
    );
    await waitFor(() => expect(privateSide.account.isDisposed).toBe(true));
    expect(privateSide.input.failures.some((failure) => failure.includes('catalog changed'))).toBe(true);
    expect(privateSide.input.sockets.every((socket) => socket.closed)).toBe(true);
    expect(source.account.isDisposed).toBe(false);
    expect(source.account.store.marketsBySymbol['BTC-USD'].contractSize).toBe(2);
    expect(new Frame(source.account.snapshotFrame()!).markets()!.contractSize[0]).toBe(1);
  }, 15_000);

  it('requires a loaded direct provider', () => {
    const unloaded = new RustEngineExchange(
      'coinbasespot',
      { key: '', secret: '', publicDataOnly: true },
      new DefaultStore(),
    );
    cleanups.push(() => unloaded.dispose());
    expect(() => new RustPublicFeedProvider(unloaded)).toThrow('started, loaded');
  });

  it('rejects legacy TS providers and contradictory direct policy', async () => {
    const legacy = await exchange({
      ...COINBASE_REPLAY,
      externalPublicData: { type: 'websocket', url: 'wss://invalid.test', exchangeId: 'coinbasespot' },
    });
    await expect(legacy.startup).rejects.toThrow('use publicFeed');
    expect(legacy.input.requests).toEqual([]);
    const conflicting = await exchange({ ...COINBASE_REPLAY, privateOnly: true, publicFeed: { mode: 'direct' } });
    await expect(conflicting.startup).rejects.toThrow('conflicts');
  });
});
