import { afterEach, describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { DefaultStore } from '../../store/store.base';
import { PositionSide } from '../../types';

const createExchange = (includeNonUsdtCurrency = false) => {
  const store = new DefaultStore();
  store.update({
    balance: {
      total: 100,
      free: 80,
      used: 20,
      upnl: 2,
      info: { retained: true },
      currencies: {
        USDT: { total: 100, free: 80, used: 20, upnl: 2 },
        ...(includeNonUsdtCurrency ? { BUSD: { total: 5, free: 5, used: 0, upnl: 0 } } : {}),
      },
    },
    positions: [
      {
        id: 'BTCUSDT-long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        contracts: 1,
        entryPrice: 100,
        notional: 102,
        unrealizedPnl: 2,
        realizedPnl: 0,
      },
    ] as any,
  });
  const exchange = new AsterDEXExchange('asterdex', { key: 'test-key', secret: 'test-secret' }, store);
  return { exchange, store, websocket: exchange.privateWebsocket };
};

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('audit regression: AsterDEX private balance events', () => {
  it('maps ACCOUNT_UPDATE collateral fields to canonical balance fields without root asset keys', () => {
    const { store, websocket } = createExchange();

    websocket.onMessage({
      data: JSON.stringify({
        e: 'ACCOUNT_UPDATE',
        E: 100,
        a: {
          B: [
            { a: 'USDT', wb: '120', cw: '90' },
          ],
          P: [{ s: 'BTCUSDT', pa: '1', ps: 'BOTH', ep: '100', up: '5', cr: '0' }],
        },
      }),
    } as MessageEvent);

    expect(store.balance).toMatchObject({
      total: 120,
      free: 90,
      used: 30,
      upnl: 5,
      info: { retained: true },
      currencies: {
        USDT: { total: 120, free: 90, used: 30, upnl: 5 },
      },
    });
    expect((store.balance as any).USDT).toBeUndefined();
    expect((store.balance as any).BUSD).toBeUndefined();
  });

  it('orders ACCOUNT_UPDATE by numeric E and fails closed on missing or malformed event times', () => {
    const { store, websocket } = createExchange();

    websocket.onMessage({
      data: JSON.stringify({ e: 'ACCOUNT_UPDATE', E: 200, a: { B: [{ a: 'USDT', wb: '120', cw: '90' }] } }),
    } as MessageEvent);
    const lastValidBalance = store.balance;

    websocket.onMessage({
      data: JSON.stringify({ e: 'ACCOUNT_UPDATE', E: 199, a: { B: [{ a: 'USDT', wb: '10', cw: '5' }] } }),
    } as MessageEvent);
    websocket.onMessage({
      data: JSON.stringify({ e: 'ACCOUNT_UPDATE', a: { B: [{ a: 'USDT', wb: '10', cw: '5' }] } }),
    } as MessageEvent);
    websocket.onMessage({
      data: JSON.stringify({ e: 'ACCOUNT_UPDATE', E: '201', a: { B: [{ a: 'USDT', wb: '10', cw: '5' }] } }),
    } as MessageEvent);

    expect(store.balance).toBe(lastValidBalance);
    expect(store.balance).toMatchObject({ total: 120, free: 90, used: 30 });
  });

  it('fails closed on malformed balance numbers and keeps the last valid account state', () => {
    const { store, websocket } = createExchange();
    const lastValidBalance = store.balance;

    websocket.onMessage({
      data: JSON.stringify({ e: 'ACCOUNT_UPDATE', E: 100, a: { B: [{ a: 'USDT', wb: '100junk', cw: '50' }] } }),
    } as MessageEvent);

    expect(store.balance).toBe(lastValidBalance);
  });

  it('preserves multi-asset root totals until an ordered authoritative reconciliation succeeds', async () => {
    vi.useFakeTimers();
    const { exchange, store, websocket } = createExchange();
    let resolveFirstReconciliation: ((response: any) => void) | undefined;
    exchange.primaryAPI.get = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirstReconciliation = resolve;
          }),
      )
      .mockResolvedValueOnce({
        data: {
          totalWalletBalance: '222',
          availableBalance: '200',
          totalUnrealizedProfit: '7',
          totalInitialMargin: '20',
          totalOpenOrderInitialMargin: '2',
        },
      });

    websocket.onMessage({
      data: JSON.stringify({ e: 'ACCOUNT_UPDATE', E: 100, a: { B: [{ a: 'BUSD', wb: '8', cw: '6' }] } }),
    } as MessageEvent);
    expect(store.balance).toMatchObject({ total: 100, free: 80, used: 20, upnl: 2 });
    expect(store.balance.currencies?.BUSD).toMatchObject({ total: 8, free: 6, used: 2 });

    vi.advanceTimersByTime(1_000);
    await Promise.resolve();
    expect(exchange.primaryAPI.get).toHaveBeenCalledTimes(1);

    websocket.onMessage({
      data: JSON.stringify({ e: 'ACCOUNT_UPDATE', E: 101, a: { B: [{ a: 'BUSD', wb: '9', cw: '7' }] } }),
    } as MessageEvent);
    resolveFirstReconciliation?.({
      data: {
        totalWalletBalance: '999',
        availableBalance: '998',
        totalUnrealizedProfit: '0',
        totalInitialMargin: '1',
        totalOpenOrderInitialMargin: '0',
      },
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(store.balance).toMatchObject({ total: 100, free: 80, used: 20, upnl: 2 });

    await vi.advanceTimersByTimeAsync(1_000);
    expect(exchange.primaryAPI.get).toHaveBeenCalledTimes(2);
    expect(store.balance).toMatchObject({ total: 222, free: 200, used: 22, upnl: 7 });
  });
});
