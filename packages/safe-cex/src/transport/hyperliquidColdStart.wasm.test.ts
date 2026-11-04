// @vitest-environment node
import type { EngineEvent, Transport } from '@tealstreet/safe-cex-wasm/host';

import { setImmediate } from 'node:timers';

import { IDBFactory } from 'fake-indexeddb';
import { afterEach, expect, it, vi } from 'vitest';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { prepareRecordedHttp } from '../__tests__/support/managedHttp';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { FrameApplier } from '../store/frameApplier';
import { DefaultStore } from '../store/store.base';

const DEXES = ['xyz', 'flx', 'vntl', 'hyna', 'km', 'abcd', 'cash', 'para', 'mkts', 'io'];
const hosts: EngineHost[] = [];
const transports: Transport[] = [];
afterEach(() => {
  hosts.splice(0).forEach((host) => host.dispose());
  transports.splice(0).forEach((transport) => transport.dispose?.());
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
const flushDatabase = async () => {
  for (let i = 0; i < 12; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
const state = (account = 0) => ({
  assetPositions: [],
  marginSummary: { accountValue: String(1000 + account), totalMarginUsed: '0' },
  time: Date.now(),
});

it.each([0, 8000])(
  'shares twelve catalogue reads across four cold engines (%i ms role response)',
  async (roleLatencyMs) => {
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    vi.setSystemTime(1_000_000);
    vi.stubGlobal('indexedDB', new IDBFactory());
    const { WasmEngine } = await loadNodeEngine();
    const startedAt = Date.now();
    const stores: DefaultStore[] = [];
    const events: EngineEvent[] = [];
    const loadedAt = [0, 0, 0, 0];
    const sends: Array<{ account: number; type: string; dex: string; at: number }> = [];
    for (let account = 0; account < 4; account++) {
      // Separate module state models account worker realms sharing only IndexedDB.
      vi.resetModules();
      const { createEngineTransport } = await import('./engine');
      const key = `0x${String(account + 1).repeat(40)}`;
      const store = new DefaultStore();
      stores.push(store);
      const applier = new FrameApplier(store);
      const raw: Transport = {
        now: () => Date.now(),
        setTimeout: (callback, ms) => setTimeout(callback, ms),
        clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
        prepareHttp: prepareRecordedHttp,
        async request(request) {
          request.signal?.throwIfAborted();
          expect(request.url).toBe('https://api.hyperliquid.xyz/info');
          const { type, dex = '' } = JSON.parse(request.body!);
          sends.push({ account, type, dex, at: Date.now() - startedAt });
          let body: unknown;
          switch (type) {
            case 'metaAndAssetCtxs':
              body = [
                { universe: [{ name: dex ? `${dex}:BTC` : 'BTC', szDecimals: 5, maxLeverage: 20 }] },
                [{ midPx: '100', markPx: '100', oraclePx: '100', prevDayPx: '90' }],
              ];
              break;
            case 'perpDexs':
              body = [null, ...DEXES.map((name) => ({ name }))];
              break;
            case 'userRole':
              if (roleLatencyMs) await new Promise<void>((resolve) => setTimeout(resolve, roleLatencyMs));
              body = { role: 'user' };
              break;
            case 'userAbstraction':
              body = 'disabled';
              break;
            case 'userFees':
              body = { userCrossRate: '0.00045', userAddRate: '0.00015' };
              break;
            case 'maxBuilderFee':
              body = 30;
              break;
            case 'clearinghouseState':
              expect(JSON.parse(request.body!).user).toBe(key);
              body = state(account);
              break;
            case 'frontendOpenOrders':
              expect(JSON.parse(request.body!).user).toBe(key);
              body = [];
              break;
            default:
              throw new Error(`Unexpected startup read ${type}`);
          }
          return { status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify(body)) };
        },
        connect(_url, handlers) {
          let closed = false;
          const deliver = (value: unknown) => {
            if (!closed) handlers.onMessage(JSON.stringify(value));
          };
          queueMicrotask(() => {
            if (!closed) handlers.onOpen();
          });
          return {
            close: () => {
              closed = true;
            },
            send(text) {
              const sent = JSON.parse(text);
              queueMicrotask(() => {
                if (sent.method === 'subscribe') {
                  deliver({ channel: 'subscriptionResponse', data: sent });
                  if (sent.subscription.type === 'clearinghouseState')
                    deliver({
                      channel: 'clearinghouseState',
                      data: { dex: sent.subscription.dex ?? '', clearinghouseState: state(account) },
                    });
                } else if (sent.method === 'ping') deliver({ channel: 'pong' });
              });
            },
          };
        },
      };
      const transport = createEngineTransport('hyperliquid', { key, secret: '' }, raw);
      transports.push(transport);
      const host = new EngineHost(
        new WasmEngine('hyperliquid', JSON.stringify({ key })),
        transport,
        {
          onFrame: (buffer) => applier.apply(new Frame(buffer)),
          onEvent: (event) => events.push(event),
        },
        { ownsEngine: true },
      );
      hosts.push(host);
      host.start();
    }
    for (let elapsed = 0; elapsed < 110_000; elapsed += 100) {
      await flushDatabase();
      await vi.advanceTimersByTimeAsync(100);
      hosts.forEach((host) => host.flush());
      stores.forEach((store, i) => {
        if (!loadedAt[i] && Object.values(store.loaded).every(Boolean)) loadedAt[i] = Date.now() - startedAt;
      });
      const allScopesRead = stores.every(
        (_, account) =>
          new Set(
            sends.filter((row) => row.account === account && row.type === 'frontendOpenOrders').map((row) => row.dex),
          ).size === 11,
      );
      if (allScopesRead && stores.every((store) => Object.values(store.loaded).every(Boolean))) break;
    }
    expect(sends.length).toBeGreaterThan(0);
    expect(stores.map((store) => store.loaded)).toEqual(
      Array.from({ length: 4 }, () => ({ markets: true, tickers: true, balance: true, orders: true, positions: true })),
    );
    expect(Date.now() - startedAt).toBeLessThan(60_000);
    expect(Math.max(...loadedAt)).toBeLessThan(60_000);
    expect(stores.map((store) => store.balance.total)).toEqual([1000, 1001, 1002, 1003]);
    expect(sends.filter((row) => row.type === 'metaAndAssetCtxs' || row.type === 'perpDexs')).toHaveLength(12);
    for (let account = 0; account < 4; account++) {
      const sent = sends.filter((row) => row.account === account);
      expect(sent.filter((row) => row.type === 'userRole')).toHaveLength(1);
      expect(sent.filter((row) => row.type === 'userAbstraction')).toHaveLength(1);
      expect(new Set(sent.filter((row) => row.type === 'frontendOpenOrders').map((row) => row.dex))).toEqual(
        new Set(['', ...DEXES]),
      );
    }
    expect(events.filter((event) => event.level === 'error')).toEqual([]);
  },
);
