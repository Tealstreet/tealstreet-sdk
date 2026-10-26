import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BackpackExchange } from '../reference/backpack/backpack.exchange';
import { DefaultStore } from '../../store/store.base';
import { Balance, ExchangeOptions, MarginType, PositionMode, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';

const position = {
  id: createPositionId('BTC_USDC_PERP', PositionSide.Long, PositionMode.Hedged),
  symbol: 'BTC_USDC_PERP',
  side: PositionSide.Long,
  entryPrice: 100,
  notional: 100,
  leverage: 5,
  unrealizedPnl: 2,
  contracts: 1,
  liquidationPrice: 80,
  lastUpdate: 1,
  positionMode: PositionMode.Hedged,
  marginType: MarginType.Cross,
};

const balance: Balance = { free: 120, used: 30, total: 150, upnl: 2 };

describe('Backpack fill reconciliation audit regression', () => {
  let store: DefaultStore;
  let exchange: BackpackExchange;

  beforeEach(() => {
    store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC_USDC_PERP',
          symbol: 'BTC_USDC_PERP',
          base: 'BTC',
          quote: 'USDC',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
          contractSize: 1,
          takerFee: 0,
          makerFee: 0,
        },
      ],
    });
    exchange = new BackpackExchange('backpack', { key: '', secret: '', testnet: false } as ExchangeOptions, store);
  });

  afterEach(() => {
    exchange.dispose();
    vi.useRealTimers();
  });

  it('commits the throttled post-fill position and balance snapshots', async () => {
    exchange.fetchPositions = vi.fn().mockResolvedValue([position]);
    exchange.fetchBalance = vi.fn().mockResolvedValue(balance);

    await (exchange.privateWebsocket as any).executePositionFetch();
    await (exchange.privateWebsocket as any).executeBalanceFetch();

    expect(store.positions).toEqual([position]);
    expect(store.balance).toEqual(balance);
  });

  it('preserves the current store when a post-fill refresh fails', async () => {
    store.update({ positions: [position], balance });
    exchange.fetchPositions = vi.fn().mockRejectedValue(new Error('temporary position outage'));
    exchange.fetchBalance = vi.fn().mockRejectedValue(new Error('temporary balance outage'));

    await (exchange.privateWebsocket as any).executePositionFetch();
    await (exchange.privateWebsocket as any).executeBalanceFetch();

    expect(store.positions).toEqual([position]);
    expect(store.balance).toEqual(balance);
  });

  it('rejects transient order and position reads instead of returning clearing snapshots', async () => {
    store.update({ positions: [position] });
    exchange.xhr = {
      get: vi.fn().mockRejectedValue(new Error('temporary proxy outage')),
    } as any;

    await expect(exchange.fetchOrders()).rejects.toThrow('temporary proxy outage');
    await expect(exchange.fetchPositions()).rejects.toThrow('temporary proxy outage');
    expect(store.positions).toEqual([position]);
  });

  it('evicts the stale side when a signed net position flips direction', async () => {
    store.update({ positions: [position] });

    await (exchange.privateWebsocket as any).handlePositionUpdate({
      i: 1,
      s: 'BTC_USDC_PERP',
      b: '101',
      M: '100',
      n: '-202',
      Q: '2',
      q: '-2',
      P: '3',
      p: '0',
      l: '130',
      m: '0.05',
      f: '0.2',
      E: 1_723_456_789_000_000,
    });

    expect(store.positions).toHaveLength(1);
    expect(store.positions[0]).toMatchObject({
      id: createPositionId('BTC_USDC_PERP', PositionSide.Short, PositionMode.Hedged),
      side: PositionSide.Short,
      contracts: 2,
    });
  });

  it('clears both internal sides for a zero signed net position', async () => {
    const shortPosition = {
      ...position,
      id: createPositionId('BTC_USDC_PERP', PositionSide.Short, PositionMode.Hedged),
      side: PositionSide.Short,
    };
    store.update({ positions: [position, shortPosition] });

    await (exchange.privateWebsocket as any).handlePositionUpdate({
      i: 1,
      s: 'BTC_USDC_PERP',
      b: '0',
      M: '100',
      n: '0',
      Q: '0',
      q: '0',
      P: '0',
      p: '0',
      l: '0',
      m: '0.05',
      f: '0.2',
      E: 1_723_456_789_000_000,
    });

    expect(store.positions).toEqual([]);
  });

  it('bounds order reconciliation during sustained WebSocket activity', async () => {
    vi.useFakeTimers();
    exchange.fetchOrders = vi.fn().mockResolvedValue([]);

    (exchange.privateWebsocket as any).throttledFetchOrders();
    await vi.advanceTimersByTimeAsync(9_000);
    (exchange.privateWebsocket as any).throttledFetchOrders();
    await vi.advanceTimersByTimeAsync(1_000);

    expect(exchange.fetchOrders).toHaveBeenCalledTimes(1);
  });
});
