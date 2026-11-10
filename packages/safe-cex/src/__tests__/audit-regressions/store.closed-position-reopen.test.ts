import type { Market, Position } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { BinancePrivateWebsocket } from '../reference/binance/binance.ws-private';
import { BlofinExchange } from '../reference/blofin/blofin.exchange';

vi.mock('../reference/blofin/blofin.api', () => ({ createAPI: () => ({}) }));

const market: Market = {
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.1, price: 0.1 },
  limits: { amount: { min: 0.0001, max: 20000 }, leverage: { min: 1, max: 150 } },
  contractSize: 0.001,
};
const rawBlofin = {
  instId: 'BTC-USDT',
  positions: '200',
  positionSide: 'net',
  markPrice: '50000',
  averagePrice: '49000',
  unrealizedPnl: '10',
  leverage: '50',
  marginMode: 'cross',
  liquidationPrice: '30000',
};
const blofin = () => {
  const store = new DefaultStore();
  store.update({ markets: [market] });
  const exchange = new BlofinExchange('blofin', { key: '', secret: '', passphrase: '' }, store);
  return { store, exchange };
};
const binance = () => {
  const store = new DefaultStore();
  const ws = new BinancePrivateWebsocket({ store, markPositionStateUpdatedByWebsocket: vi.fn() } as any);
  const repair = vi.spyOn(ws as any, 'throttledFetchPositions').mockImplementation(() => undefined);
  const reopen = (ps = 'BOTH', pa = '0.2') =>
    ws.handleAccountEvents([{ a: { P: [{ s: 'BTCUSDT', ps, pa, ep: '50000', up: '10' }] } }]);
  return { store, reopen, repair };
};
const position = (changes: Partial<Position> = {}): Position => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  positionMode: PositionMode.OneWay,
  leverage: 17,
  marginType: MarginType.Isolated,
  contracts: 0.1,
  entryPrice: 49000,
  notional: 4900,
  unrealizedPnl: 3,
  liquidationPrice: 30000,
  ...changes,
});

describe('retained adapters reopen without fabricated settings', () => {
  it('BloFin prefers acknowledged flat-account leverage to closed settings', async () => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull(exchange.mapPositions([rawBlofin] as any));
      store.addOrUpdatePositionsFull([]);
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      const post = vi.fn().mockResolvedValue({ data: { code: '0', msg: '' } });
      exchange.xhr = { post } as any;
      await exchange.setLeverage('BTCUSDT', 9);
      expect(post).toHaveBeenCalledOnce();
      expect(store.positions).toEqual([]);
      expect(store.positionSettings.BTCUSDT.leverage).toBe(50);
      expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 9, sellLeverage: 9 });
      expect(exchange.mapPositions([{ ...rawBlofin, leverage: '' }] as any)[0].leverage).toBe(9);
      expect(exchange.mapPositions([{ ...rawBlofin, leverage: '12' }] as any)[0].leverage).toBe(12);
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin keeps absent leverage unavailable when no source knows it', () => {
    const { exchange } = blofin();
    try {
      expect(exchange.mapPositions([{ ...rawBlofin, leverage: '' }] as any)[0].leverage).toBeNaN();
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin ignores an in-flight fetched setting and keeps the matching closed setting', () => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull(exchange.mapPositions([rawBlofin] as any));
      store.addOrUpdatePositionsFull([]);
      store.update({ fetchedLeverage: { BTCUSDT: { buyLeverage: 9, sellLeverage: 9, isFetching: true } } });
      expect(exchange.mapPositions([{ ...rawBlofin, leverage: '' }] as any)[0].leverage).toBe(50);
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin does not promote a rejected leverage change into confirmed settings', async () => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull(exchange.mapPositions([rawBlofin] as any));
      store.addOrUpdatePositionsFull([]);
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
      exchange.xhr = { post: vi.fn().mockResolvedValue({ data: { code: '1', msg: 'synthetic rejection' } }) } as any;
      await expect(exchange.setLeverage('BTCUSDT', 9)).rejects.toThrow('synthetic rejection');
      expect(store.fetchedLeverage.BTCUSDT).toBeUndefined();
      expect(exchange.mapPositions([{ ...rawBlofin, leverage: '' }] as any)[0].leverage).toBe(50);
    } finally {
      exchange.dispose();
    }
  });

  it.each([
    ['BOTH', '0.2', PositionSide.Long, PositionMode.OneWay],
    ['LONG', '0.2', PositionSide.Long, PositionMode.Hedged],
    ['SHORT', '-0.2', PositionSide.Short, PositionMode.Hedged],
  ] as const)('Binance restores exact-ID settings on %s stream reopen', (ps, pa, side, mode) => {
    const { store, reopen, repair } = binance();
    const id = createPositionId('BTCUSDT', side, mode);
    store.addOrUpdatePositionsFull([position({ id, side, positionMode: mode })]);
    store.addOrUpdatePositionsFull([]);
    expect(store.positions).toEqual([]);
    reopen(ps, pa);
    expect(store.positions).toHaveLength(1);
    expect(store.positions[0]).toMatchObject({
      id,
      side,
      positionMode: mode,
      contracts: 0.2,
      leverage: 17,
      marginType: MarginType.Isolated,
      liquidationPrice: null,
    });
    expect(repair).toHaveBeenCalledOnce();
  });

  it('Binance prefers confirmed side-specific settings to the closed cache', () => {
    const { store, reopen } = binance();
    store.addOrUpdatePositionsFull([position({ side: PositionSide.Short })]);
    store.addOrUpdatePositionsFull([]);
    store.update({
      fetchedLeverage: {
        BTCUSDT: { buyLeverage: 9, sellLeverage: 13, marginType: MarginType.Cross },
      },
    });
    reopen('BOTH', '-0.2');
    expect(store.positions[0]).toMatchObject({ side: PositionSide.Short, leverage: 13, marginType: MarginType.Cross });
  });

  it('Binance keeps unreported settings unknown instead of inventing zero/Cross', () => {
    const { store, reopen } = binance();
    reopen();
    expect(store.positions).toHaveLength(1);
    expect(store.positions[0].leverage).toBeNaN();
    expect(store.positions[0].marginType).toBeUndefined();
    expect(store.positions[0].liquidationPrice).toBeNull();
  });

  it('Binance does not borrow settings from the opposite hedge leg', () => {
    const { store, reopen } = binance();
    store.addOrUpdatePositionsFull([
      position({
        id: 'BTCUSDT:long',
        positionMode: PositionMode.Hedged,
      }),
    ]);
    store.addOrUpdatePositionsFull([]);
    reopen('SHORT', '-0.2');
    expect(store.positions[0].id).toBe('BTCUSDT:short');
    expect(store.positions[0].leverage).toBeNaN();
    expect(store.positions[0].marginType).toBeUndefined();
  });
});
