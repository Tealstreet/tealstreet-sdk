// @vitest-environment node
import type { EngineEvent, HttpPacket, Transport } from '@tealstreet/safe-cex-wasm/host';

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
afterEach(async () => {
  hosts.splice(0).forEach((host) => host.dispose());
  transports.splice(0).forEach((transport) => transport.dispose?.());
  await flushDatabase();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
let pendingDatabaseTasks = 0;
const flushDatabase = async () => {
  // Settle fake IDB's real tasks before advancing virtual deadlines; a fixed
  // number of turns imposes artificial storage latency on a large read wave.
  let quietTurns = 0;
  for (let i = 0; i < 10_000; i++) {
    await new Promise<void>((resolve) => setImmediate(resolve));
    quietTurns = pendingDatabaseTasks === 0 ? quietTurns + 1 : 0;
    if (quietTurns === 2) return;
  }
  throw new Error('IndexedDB task queue did not settle');
};
const state = (account = 0, dex = '') => ({
  assetPositions: [
    {
      position: {
        coin: dex ? `${dex}:BTC` : 'BTC',
        szi: '2',
        entryPx: '100',
        positionValue: '200',
        unrealizedPnl: '0',
        liquidationPx: '50',
        marginUsed: '20',
        leverage: { type: 'cross', value: 10 },
      },
    },
  ],
  marginSummary: { accountValue: String(1000 + account), totalMarginUsed: '0' },
  time: Date.now(),
});

it.each([0, 8000])(
  'four cold engines yield to an explicit user amend and complete every private scope (%i ms role response)',
  async (roleLatencyMs) => {
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    vi.setSystemTime(1_000_000);
    vi.stubGlobal('indexedDB', new IDBFactory());
    pendingDatabaseTasks = 0;
    vi.stubGlobal('setImmediate', (callback: (...args: unknown[]) => void, ...args: unknown[]) => {
      pendingDatabaseTasks += 1;
      return setImmediate(() => {
        try {
          callback(...args);
        } finally {
          pendingDatabaseTasks -= 1;
        }
      });
    });
    const { WasmEngine } = await loadNodeEngine();
    const startedAt = Date.now();
    const stores: DefaultStore[] = [];
    const events: EngineEvent[] = [];
    const loadedAt = [0, 0, 0, 0];
    const connectCounts = [0, 0, 0, 0];
    const sends: Array<{
      account: number;
      type: string;
      dex: string;
      at: number;
      readClass?: HttpPacket['readClass'];
    }> = [];
    const completed: typeof sends = [];
    const deliveries: Array<(frame: unknown) => void> = [];
    let amendAt: number | undefined;
    let amendIndex = 0;
    let amendResult: unknown;
    let amendError: unknown;
    for (let account = 0; account < 4; account++) {
      // Separate module state models account worker realms sharing only IndexedDB.
      vi.resetModules();
      const { createEngineTransport } = await import('./engine');
      const key = account === 0 ? '0x7e5f4552091a69125d5dfcb7b8c2659029395bdf' : `0x${String(account + 1).repeat(40)}`;
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
          if (request.url.endsWith('/exchange')) {
            const { action } = JSON.parse(request.body!);
            sends.push({
              account,
              type: action.type,
              dex: '',
              at: Date.now() - startedAt,
              readClass: request.readClass,
            });
            return {
              status: 200,
              headers: '',
              body: new TextEncoder().encode(
                JSON.stringify({
                  status: 'ok',
                  response: {
                    type: action.type,
                    data: { statuses: action.type === 'cancel' ? ['success'] : [{ resting: { oid: 10 } }] },
                  },
                }),
              ),
            };
          }
          expect(request.url).toBe('https://api.hyperliquid.xyz/info');
          const { type, dex = '' } = JSON.parse(request.body!);
          sends.push({ account, type, dex, at: Date.now() - startedAt, readClass: request.readClass });
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
              body = state(account, dex);
              break;
            case 'orderStatus':
              body = {
                status: 'order',
                order: {
                  order: { coin: 'BTC', oid: 9, side: 'A', limitPx: '80', origSz: '1', sz: '1' },
                  status: 'open',
                },
              };
              break;
            case 'frontendOpenOrders':
              expect(JSON.parse(request.body!).user).toBe(key);
              body = [
                {
                  coin: dex ? `${dex}:BTC` : 'BTC',
                  oid: 10_000 + account * 100 + ['', ...DEXES].indexOf(dex),
                  side: 'B',
                  limitPx: '90',
                  origSz: '0.1',
                  sz: '0.1',
                  orderType: 'Limit',
                  tif: 'Gtc',
                },
              ];
              break;
            default:
              throw new Error(`Unexpected startup read ${type}`);
          }
          request.signal?.throwIfAborted();
          completed.push({ account, type, dex, at: Date.now() - startedAt });
          return { status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify(body)) };
        },
        connect(_url, handlers) {
          connectCounts[account] += 1;
          let closed = false;
          const deliver = (value: unknown) => {
            if (!closed) handlers.onMessage(JSON.stringify(value));
          };
          deliveries[account] = deliver;
          let publicTimer: ReturnType<typeof setTimeout>;
          const publicPulse = () => {
            if (closed) return;
            deliver({ channel: 'allMids', data: { mids: { BTC: '100' } } });
            publicTimer = setTimeout(publicPulse, 5000);
          };
          queueMicrotask(() => {
            if (!closed) handlers.onOpen();
            publicPulse();
          });
          return {
            close: () => {
              closed = true;
              clearTimeout(publicTimer);
            },
            send(text) {
              const sent = JSON.parse(text);
              queueMicrotask(() => {
                if (sent.method === 'subscribe') {
                  deliver({ channel: 'subscriptionResponse', data: sent });
                  if (sent.subscription.type === 'clearinghouseState')
                    deliver({
                      channel: 'clearinghouseState',
                      data: {
                        dex: sent.subscription.dex ?? '',
                        clearinghouseState: state(account, sent.subscription.dex ?? ''),
                      },
                    });
                } else if (sent.method === 'ping') {
                  deliver({ channel: 'pong' });
                  deliver({ channel: 'allMids', data: { mids: { BTC: '100' } } });
                }
              });
            },
          };
        },
      };
      const transport = createEngineTransport('hyperliquid', { key, secret: '' }, raw);
      transports.push(transport);
      const host = new EngineHost(
        new WasmEngine(
          'hyperliquid',
          JSON.stringify({ key, ...(account === 0 ? { secret: '0x' + '0'.repeat(63) + '1' } : {}) }),
        ),
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
      if (elapsed === 20_000) {
        expect(completed.filter((row) => row.type === 'frontendOpenOrders').length).toBeLessThan(44);
        deliveries[0]({
          channel: 'orderUpdates',
          data: [
            {
              order: {
                coin: 'BTC',
                oid: 9,
                side: 'A',
                limitPx: '80',
                triggerPx: '80',
                origSz: '1',
                sz: '1',
                reduceOnly: true,
                orderType: 'Stop Market',
              },
              status: 'open',
            },
          ],
        });
        amendAt = Date.now() - startedAt;
        amendIndex = sends.length;
        void hosts[0].command({ op: 'amendOrder', id: '9', price: 75 }).then(
          (value) => {
            amendResult = value;
          },
          (error) => {
            amendError = error;
          },
        );
      }
      await vi.advanceTimersByTimeAsync(100);
      hosts.forEach((host) => host.flush());
      stores.forEach((store, i) => {
        if (!loadedAt[i] && Object.values(store.loaded).every(Boolean)) loadedAt[i] = Date.now() - startedAt;
      });
      const allScopesRead = stores.every((_, account) =>
        ['clearinghouseState', 'frontendOpenOrders'].every(
          (type) =>
            new Set(completed.filter((row) => row.account === account && row.type === type).map((row) => row.dex))
              .size === 11,
        ),
      );
      const ancillaryComplete = ['userFees', 'maxBuilderFee'].every(
        (type) => completed.filter((row) => row.type === type).length === 4,
      );
      if (allScopesRead && ancillaryComplete && stores.every((store) => Object.values(store.loaded).every(Boolean)))
        break;
    }
    expect(connectCounts).toEqual([1, 1, 1, 1]);
    const nextRead = sends[amendIndex];
    expect(nextRead?.type).toBe('orderStatus');
    expect(nextRead?.readClass).toBe('critical');
    expect(nextRead!.at - amendAt!).toBeLessThan(1500);
    expect(amendError).toBeUndefined();
    expect(amendResult).toBeDefined();
    expect(sends.length).toBeGreaterThan(0);
    expect(stores.map((store) => store.loaded)).toEqual(
      Array.from({ length: 4 }, () => ({ markets: true, tickers: true, balance: true, orders: true, positions: true })),
    );
    // The initial financial census may spend the critical reserve; fees and builder may not.
    for (let account = 0; account < 4; account++) {
      for (const type of ['clearinghouseState', 'frontendOpenOrders']) {
        for (const dex of ['', ...DEXES]) {
          const first = sends.find((row) => row.account === account && row.type === type && row.dex === dex)!;
          expect(first.readClass, `${account}/${type}/${dex}`).toBe('reconciliation');
          const reply = completed.find((row) => row.account === account && row.type === type && row.dex === dex)!;
          expect(reply.at, `${account}/${type}/${dex}`).toBeLessThan(60_000);
          const symbol = dex ? `${dex.toUpperCase()}|BTC-PERP` : 'BTC-PERP';
          if (type === 'clearinghouseState')
            expect(stores[account].positions.some((row) => row.symbol === symbol && row.contracts === 2)).toBe(true);
          else
            expect(
              stores[account].orders.some(
                (row) =>
                  row.symbol === symbol && row.id === String(10_000 + account * 100 + ['', ...DEXES].indexOf(dex)),
              ),
            ).toBe(true);
        }
      }
    }
    for (const row of sends.filter(
      (row) =>
        ![
          'clearinghouseState',
          'frontendOpenOrders',
          'userRole',
          'userAbstraction',
          'orderStatus',
          'cancel',
          'order',
        ].includes(row.type),
    ))
      expect(row.readClass).toBe('normal');
    expect(Math.max(...loadedAt)).toBeLessThan(roleLatencyMs + 10_000);
    expect(Date.now() - startedAt).toBeLessThan(60_000);
    expect(Math.max(...loadedAt)).toBeLessThan(60_000);
    expect(stores.map((store) => store.balance.total)).toEqual([1000, 1001, 1002, 1003]);
    expect(sends.filter((row) => row.type === 'metaAndAssetCtxs' || row.type === 'perpDexs')).toHaveLength(12);
    for (let account = 0; account < 4; account++) {
      const sent = sends.filter((row) => row.account === account);
      expect(sent.filter((row) => row.type === 'userRole')).toHaveLength(1);
      expect(sent.filter((row) => row.type === 'userAbstraction')).toHaveLength(1);
      for (const type of ['userFees', 'maxBuilderFee'])
        expect(completed.filter((row) => row.account === account && row.type === type)).toHaveLength(1);
      for (const type of ['clearinghouseState', 'frontendOpenOrders'])
        expect(
          new Set(completed.filter((row) => row.account === account && row.type === type).map((row) => row.dex)),
        ).toEqual(new Set(['', ...DEXES]));
    }
    expect(events.filter((event) => event.code === 'request.failed' || event.level === 'error')).toEqual([]);
  },
);
