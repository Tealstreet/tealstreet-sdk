vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { MessageChannel } from 'node:worker_threads';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustPublicFeedProvider } from '../../exchanges/rust/publicFeed';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { prepareRecordedHttp } from '../support/managedHttp';

/** Documented `metaAndAssetCtxs` shape: BTC's mid and mark are both 97,500.5. */
const META =
  '[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"97500.5","markPx":"97500.5","oraclePx":"97500","prevDayPx":"95000","dayNtlVlm":"195001","openInterest":"2","funding":"0.0001"}]]';
const TRADES = '{"method":"subscribe","subscription":{"type":"trades","coin":"BTC"}}';

function hyperliquid() {
  const sockets: Array<{ sent: string[]; onMessage(text: string): void }> = [];
  const transport: Transport = {
    now: Date.now,
    prepareHttp: prepareRecordedHttp,
    connect(_url, handlers) {
      const socket = { sent: [] as string[], onMessage: handlers.onMessage };
      sockets.push(socket);
      setTimeout(() => handlers.onOpen(), 0);
      return { send: (text) => socket.sent.push(text), close() {} };
    },
    async request({ body }) {
      const type = JSON.parse(String(body ?? '{}')).type;
      const payload = type === 'metaAndAssetCtxs' ? META : type === 'perpDexs' ? '[null]' : '[]';
      return { status: 200, headers: '', body: new TextEncoder().encode(payload) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  return { transport, sockets };
}

const waitFor = (assertion: () => unknown) => vi.waitFor(assertion, { timeout: 5000 });

describe('Hyperliquid trade-sourced last through the shared public feed', () => {
  const cleanups: Array<() => void> = [];
  afterEach(() => {
    for (const cleanup of cleanups.splice(0).reverse()) cleanup();
  });
  const exchange = (venue: ReturnType<typeof hyperliquid>, publicFeed?: { mode: 'shared'; port: MessagePort }) => {
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => venue.transport);
    const account = new RustEngineExchange(
      'hyperliquid',
      {
        key: '',
        secret: '',
        publicDataOnly: true,
        ...(publicFeed && { publicFeed }),
        transport: { ...directTransport, rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }) },
      },
      new DefaultStore(),
    );
    cleanups.push(() => account.dispose());
    return account;
  };

  it("a shared account's book makes the source stream trades, and its last follows them", async () => {
    const venue = hyperliquid();
    const source = exchange(venue);
    await source.start();
    const provider = new RustPublicFeedProvider(source);
    cleanups.push(() => provider.dispose());
    const channel = new MessageChannel();
    provider.attach(channel.port1 as unknown as MessagePort);
    const accountVenue = hyperliquid();
    const account = exchange(accountVenue, { mode: 'shared', port: channel.port2 as unknown as MessagePort });
    await account.start();
    expect(venue.sockets.flatMap((socket) => socket.sent)).not.toContain(TRADES);

    const stop = account.listenOrderBook('BTC-PERP', () => {});
    await waitFor(() => expect(venue.sockets.flatMap((socket) => socket.sent)).toContain(TRADES));
    for (const socket of venue.sockets)
      socket.onMessage(
        '{"channel":"trades","data":[{"coin":"BTC","side":"B","px":"97520","sz":"0.1","time":2000,"hash":"0x0","tid":1}]}',
      );
    await waitFor(() => {
      const ticker = account.store.tickers.find((ticker) => ticker.symbol === 'BTC-PERP')!;
      expect(ticker.last).toBe(97520);
      expect(ticker.midPrice).toBe(97500.5);
      expect(ticker.mark).toBe(97500.5);
    });
    expect(accountVenue.sockets.flatMap((socket) => socket.sent).join()).not.toContain('"trades"');

    stop();
    await waitFor(() =>
      expect(venue.sockets.flatMap((socket) => socket.sent)).toContain(TRADES.replace('subscribe', 'unsubscribe')),
    );
  }, 15_000);
});
