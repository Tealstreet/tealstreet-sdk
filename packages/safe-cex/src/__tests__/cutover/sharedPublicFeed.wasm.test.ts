vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { Candle, ExchangeOptions, Market, OrderBook, PublicDataSource, StoreData } from '../../types';
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
  const sent: Array<{
    channel?: string;
    packet: { type: string; channel: string; product_ids?: string[] };
    verifiedSubscribe: boolean;
  }> = [];
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
          const packet = JSON.parse(text);
          // Verify at the transport boundary with the oracle's fresh clock.
          // Later worker startup must not age a valid JWT past its strict 5s check.
          sent.push({ channel: socket.channel, packet, verifiedSubscribe: verifyCoinbaseSubscribe(packet) });
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
    expect(input.sent.find(({ packet }) => packet.channel === 'user')!.verifiedSubscribe).toBe(true);
    expect(input.failures).toEqual([]);
  }, 15_000);

  it('preserves midpoint additions, changes and clears through a provider, native receiver and main frame store', async () => {
    const source = await exchange({ key: '', secret: '', publicDataOnly: true });
    await source.startup;
    const symbol = 'BTC-USD';
    const tickerIndex = source.account.store.tickers.findIndex((ticker) => ticker.symbol === symbol);
    expect(tickerIndex).toBeGreaterThanOrEqual(0);
    const last = source.account.store.tickers.find((ticker) => ticker.symbol === symbol)!.last;
    // Independent revision-1 ticker delta entering the producer's frame boundary.
    const publishMidpoint = (midpoint: number) => {
      const buffer = new ArrayBuffer(48);
      const view = new DataView(buffer);
      view.setUint32(0, 0x46584353, true);
      view.setUint16(4, 1, true);
      view.setUint16(6, 1, true);
      view.setUint16(16, 4, true);
      view.setUint16(18, 1, true);
      view.setUint32(20, 24, true);
      view.setFloat64(24, tickerIndex, true);
      view.setFloat64(32, 1 << 11, true);
      view.setFloat64(40, midpoint, true);
      (source.account as unknown as { receiveFrame(frame: ArrayBuffer): void }).receiveFrame(buffer);
    };
    publishMidpoint(last + 0.125);
    const provider = new RustPublicFeedProvider(source.account);
    cleanups.push(() => provider.dispose());
    const channel = new MessageChannel();
    provider.attach(channel.port1 as unknown as MessagePort);
    const input = nativeInput(true);
    setRustEngineTransport(() => input.transport);
    const account = new RustEngineExchange(
      'coinbasespot',
      {
        key: '',
        secret: '',
        publicDataOnly: true,
        publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort },
        transport: { ...directTransport, rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }) },
      },
      new DefaultStore(),
    );
    cleanups.push(() => account.dispose());
    const mainStore = new DefaultStore();
    const mainApplier = new FrameApplier(mainStore);
    account.onFrame = (buffer) => mainApplier.apply(new Frame(buffer));
    account.on('error', (message: unknown) => input.failures.push(String(message)));
    await account.start();
    const expectMidpoint = async (midPrice: number | undefined) => {
      await waitFor(() => {
        for (const store of [account.store, mainStore]) {
          const ticker = store.tickers.find((ticker) => ticker.symbol === symbol)!;
          expect(ticker.last).toBe(last);
          expect(ticker.midPrice).toBe(midPrice);
        }
      });
      const snapshot = new DefaultStore();
      new FrameApplier(snapshot).apply(new Frame(account.snapshotFrame()!));
      const ticker = snapshot.tickers.find((ticker) => ticker.symbol === symbol)!;
      expect(ticker.last).toBe(last);
      expect(ticker.midPrice).toBe(midPrice);
    };
    await expectMidpoint(last + 0.125);
    publishMidpoint(last + 0.25);
    await expectMidpoint(last + 0.25);
    publishMidpoint(NaN);
    await expectMidpoint(undefined);
    expect(input.failures).toEqual([]);
    expect(source.input.failures).toEqual([]);
  }, 15_000);

  it('fans one frame-forwarding provider out to two transferred private-worker ports with native snapshots and isolated state', async () => {
    const publicSide = await exchange({ key: '', secret: '', publicDataOnly: true });
    await publicSide.startup;
    const forwardedSourceFrame = vi.fn();
    publicSide.account.onFrame = (frame) => forwardedSourceFrame(structuredClone(frame, { transfer: [frame] }));
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
      expect(input.sent.find(({ packet }) => packet.channel === 'user')!.verifiedSubscribe).toBe(true);
      expect(input.failures).toEqual([]);
      expect({
        balance: worker.store.balance,
        orders: worker.store.orders,
        settings: worker.store.fetchedLeverage,
      }).toEqual(before[index]);
      const mirrored = new DefaultStore();
      const applier = new FrameApplier(mirrored);
      for (const frame of frames) applier.apply(new Frame(frame));
      expect(mirrored.tickersBySymbol['BTC-USD'].last).toBe(777);
      const snapshot = (worker.exchange as RustEngineExchange).snapshotFrame()!;
      const late = new DefaultStore();
      new FrameApplier(late).apply(new Frame(snapshot));
      expect(late.tickersBySymbol['BTC-USD'].last).toBe(777);
      expect(late.balance).toEqual(worker.store.balance);
      expect(worker.store.tickersBySymbol['BTC-USD']).not.toBe(publicSide.account.store.tickersBySymbol['BTC-USD']);
    }
    const books: OrderBook[] = [];
    const candles: Candle[] = [];
    const firstAccount = workers[0].worker.exchange as RustEngineExchange;
    const sourceBookSubscription = vi.spyOn(publicSide.account, 'listenOrderBook');
    const sourceCandleSubscription = vi.spyOn(publicSide.account, 'listenOHLCV');
    const ordinarySourceCallback = vi.fn();
    cleanups.push(publicSide.account.listenOrderBook('BTC-USD', ordinarySourceCallback));
    cleanups.push(
      publicSide.account.listenOHLCV({ symbol: 'BTC-USD', interval: '5m' }, ordinarySourceCallback),
    );
    const stopBook = firstAccount.listenOrderBook('BTC-USD', (book) => books.push(book));
    const stopCandle = firstAccount.listenOHLCV({ symbol: 'BTC-USD', interval: '5m' }, (candle) =>
      candles.push(candle),
    );
    await waitFor(() => {
      expect(sourceBookSubscription).toHaveBeenCalledTimes(2);
      expect(sourceCandleSubscription).toHaveBeenCalledTimes(2);
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
      const applier = new FrameApplier(late);
      const frame = new Frame(firstAccount.snapshotFrame()!);
      applier.apply(frame);
      expect(applier.books(frame)[0]?.orderbook.bids[0]).toMatchObject({ price: 776, amount: 2 });
    });
    await waitFor(() => {
      const applier = new FrameApplier(new DefaultStore());
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
      const applier = new FrameApplier(new DefaultStore());
      const frame = new Frame(sibling.snapshotFrame()!);
      applier.apply(frame);
      expect(applier.books(frame)[0]?.orderbook.bids[0]).toMatchObject({ price: 776, amount: 2 });
    });
    stopSiblingBook();
    expect(books).toEqual([]);
    expect(candles).toEqual([]);
    expect(forwardedSourceFrame).toHaveBeenCalled();
    expect(ordinarySourceCallback).not.toHaveBeenCalled();
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

  it('serves exchange-server in-process externalPublicData to two private accounts across a source restart', async () => {
    // Mirrors services/exchange-server shared-public-data-source.ts: a live view over
    // whichever public exchange the manager currently runs for the venue.
    const first = await exchange({ key: '', secret: '', publicDataOnly: true });
    await first.startup;
    let current = first.account;
    const listeners = new Set<() => void>();
    const stops = new Map<() => void, () => void>();
    const attach = () => {
      for (const [listener, stop] of stops) {
        stop();
        stops.set(listener, current.store.subscribe(listener));
      }
    };
    const source: PublicDataSource = {
      store: {
        subscribe: (callback) => {
          const listener = () => callback(current.store as unknown as StoreData);
          listeners.add(listener);
          stops.set(listener, current.store.subscribe(listener));
          return () => {
            stops.get(listener)?.();
            stops.delete(listener);
            listeners.delete(listener);
          };
        },
        get tickers() {
          return current.store.tickers;
        },
        get markets() {
          return current.store.markets;
        },
        get loaded() {
          return current.store.loaded;
        },
      },
      listenOrderBook: (symbol, callback) => current.listenOrderBook(symbol, callback),
      listenOHLCV: (opts, callback) => current.listenOHLCV(opts, callback),
    };
    const firstSource = current;
    const accounts: Array<Awaited<ReturnType<typeof exchange>>> = [];
    for (let index = 0; index < 2; index++) {
      const side = await exchange({
        ...COINBASE_REPLAY,
        privateOnly: true,
        externalPublicData: { type: 'in-process', source },
      });
      await waitFor(() => expect(side.input.sockets.some((socket) => socket.channel === 'private')).toBe(true));
      side.input.deliver('user');
      await side.startup;
      expect(side.input.sockets.map((socket) => socket.channel)).toEqual(['private']);
      accounts.push(side);
    }
    const sourceBook = vi.spyOn(firstSource, 'listenOrderBook');
    const stopBook = accounts[0].account.listenOrderBook('BTC-USD', () => {});
    await waitFor(() => expect(sourceBook).toHaveBeenCalled());
    stopBook();

    // The manager disposes a stale public exchange and starts a replacement.
    const replacement = await exchange({ key: '', secret: '', publicDataOnly: true });
    await replacement.startup;
    firstSource.dispose();
    current = replacement.account;
    attach();
    replacement.input.setProductPrice(999);
    await waitFor(() => {
      for (const { account } of accounts) expect(account.store.tickersBySymbol['BTC-USD'].last).toBe(999);
    });
    for (const { account, input } of accounts) {
      expect(account.isDisposed).toBe(false);
      expect(input.failures).toEqual([]);
    }
    accounts[0].account.dispose();
    accounts[1].account.dispose();
    await waitFor(() => expect(listeners.size).toBe(0));
  }, 20_000);

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

  it.each([
    ['contract size', (market: Market): Market => ({ ...market, contractSize: (market.contractSize ?? 1) * 2 })],
    ['classification', (market: Market): Market => ({
      ...market,
      info: { ...market.info, symbolType: '', marketRegion: 'US', underlyingTicker: 'AAPL' },
    })],
    ['future terms', (market: Market): Market => ({
      ...market,
      info: { ...market.info, instrumentKind: 'FUTURE', underlying: 'BTC', expiry: 1_800_000_000_000 },
    })],
    ['option terms', (market: Market): Market => ({
      ...market,
      option: {
        underlying: 'BTC',
        expiry: 1_800_000_000_000,
        strike: 60_000,
        right: 'call',
        style: 'european',
        settlement: 'cash',
        settleAsset: 'USDC',
        premiumAsset: 'USDC',
      },
    })],
  ] as const)('fails closed when changed %s cross the SDK frame boundary after readiness', async (_label, change) => {
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
    const before = source.account.store.markets;
    boundary.receiveFrame(
      consumerFrame({
        markets: before.map(change),
      }),
    );
    await waitFor(() => expect(privateSide.account.isDisposed).toBe(true));
    expect(privateSide.input.failures.some((failure) => failure.includes('catalog changed'))).toBe(true);
    expect(privateSide.input.sockets.every((socket) => socket.closed)).toBe(true);
    expect(source.account.isDisposed).toBe(false);
    expect(source.account.store.markets).not.toEqual(before);
    expect(new Frame(source.account.snapshotFrame()!).markets()!.contractSize[0]).toBe(1);
  }, 15_000);

  it('rejects same-ID semantic catalog mismatches before shared account readiness', async () => {
    const source = await exchange({ key: '', secret: '', publicDataOnly: true });
    await source.startup;
    const boundary = source.account as unknown as { receiveFrame(buffer: ArrayBuffer): void };
    boundary.receiveFrame(consumerFrame({
      markets: source.account.store.markets.map((market) => ({
        ...market,
        info: { ...market.info, symbolType: '', marketRegion: 'US', underlyingTicker: 'AAPL' },
      })),
    }));
    const provider = new RustPublicFeedProvider(source.account);
    cleanups.push(() => provider.dispose());
    const channel = new MessageChannel();
    provider.attach(channel.port1 as unknown as MessagePort);
    const receiver = await exchange({
      ...COINBASE_REPLAY,
      privateOnly: true,
      publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort },
    });
    await waitFor(() => expect(receiver.account.isDisposed).toBe(true));
    await expect(receiver.startup).rejects.toThrow('catalog/product mismatch');
    expect(receiver.input.sockets.every((socket) => socket.closed)).toBe(true);
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

  it('settles shared startup when the provider snapshot matches the native REST seed', async () => {
    setWasmEngineLoader(loadNodeEngine);
    const products = JSON.stringify({
      products: [
        {
          product_type: 'SPOT',
          product_id: 'BTC-USD',
          base_currency_id: 'BTC',
          quote_currency_id: 'USD',
          status: 'TRADING',
          base_increment: '0.00000001',
          price_increment: '0.01',
          base_min_size: '0.00000001',
          base_max_size: '10000',
          price: '60000',
          volume_24h: '100',
          price_percentage_change_24h: '9.43%',
        },
      ],
      pagination: { has_next: false },
    });
    const transport: Transport = {
      now: Date.now,
      connect: () => ({ send() {}, close() {} }),
      request: async () => ({ status: 200, body: new TextEncoder().encode(products) }),
      // Isolate the initial snapshot: no later polling or socket event may wake startup.
      setTimeout: () => 0,
      clearTimeout() {},
    };
    setRustEngineTransport(() => transport);
    const options: ExchangeOptions = {
      key: '',
      secret: '',
      publicDataOnly: true,
      transport: { ...directTransport, rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }) },
    };
    const source = new RustEngineExchange('coinbasespot', options, new DefaultStore());
    cleanups.push(() => source.dispose());
    await source.start();
    const provider = new RustPublicFeedProvider(source);
    cleanups.push(() => provider.dispose());
    const channel = new MessageChannel();
    provider.attach(channel.port1 as unknown as MessagePort);
    const receiver = new RustEngineExchange(
      'coinbasespot',
      { ...options, publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort } },
      new DefaultStore(),
    );
    cleanups.push(() => receiver.dispose());
    let ready = false;
    const startup = receiver.start().then(() => {
      ready = true;
    });
    void startup.catch(() => {});
    await vi.waitFor(() => expect(receiver.store.loaded.tickers).toBe(true), { timeout: 5000 });
    expect(receiver.store.tickers).toEqual(source.store.tickers);
    await vi.waitFor(() => expect(ready).toBe(true), { timeout: 1000 });
    await startup;
  }, 15_000);

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
