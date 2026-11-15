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
const blofin = (store = new DefaultStore()) => {
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
  it.each([
    ['fetch', 'before', 50],
    ['fetch', 'after', 9],
    ['write', 'before', 50],
    ['write', 'after', 9],
  ] as const)('BloFin orders %s confirmation %s a position report by recency', async (source, order, expected) => {
    const { store, exchange } = blofin();
    try {
      // Same-clock observations still have a definite source order.
      vi.spyOn(Date, 'now').mockReturnValue(12345);
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      const get = vi
        .fn()
        .mockResolvedValue({
          data: { code: '0', data: [{ positionSide: 'net', leverage: '9', marginMode: 'cross' }] },
        });
      const post = vi.fn().mockResolvedValue({ data: { code: '0', msg: '' } });
      exchange.xhr = { get, post } as any;
      const confirm = async () => {
        if (source === 'fetch') await exchange.fetchLeverageForSymbol('BTCUSDT');
        else await exchange.setLeverage('BTCUSDT', 9);
        expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 9, sellLeverage: 9 });
      };
      if (order === 'before') await confirm();
      exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
      expect(store.positions[0].leverage).toBe(50);
      exchange.privateWebsocket.handlePositionTopic({ data: [] });
      expect(store.positions).toEqual([]);
      expect(store.positionSettings.BTCUSDT.leverage).toBe(50);
      if (order === 'after') await confirm();
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBe(expected);
      expect(source === 'fetch' ? get : post).toHaveBeenCalledOnce();
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin does not let an older in-flight read supersede a position report', async () => {
    const { store, exchange } = blofin();
    try {
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      let release!: (value: any) => void;
      const get = vi.fn().mockReturnValue(
        new Promise((resolve) => {
          release = resolve;
        }),
      );
      exchange.xhr = { get } as any;
      const pending = exchange.fetchLeverageForSymbol('BTCUSDT');
      await vi.waitFor(() => expect(get).toHaveBeenCalledOnce());
      exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
      exchange.privateWebsocket.handlePositionTopic({ data: [] });
      release({ data: { code: '0', data: [{ positionSide: 'net', leverage: '9', marginMode: 'cross' }] } });
      await pending;
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBe(50);
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin keeps a newer completed read when an older read completes last', async () => {
    const { store, exchange } = blofin();
    try {
      exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
      exchange.privateWebsocket.handlePositionTopic({ data: [] });
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      let release!: (value: any) => void;
      const get = vi
        .fn()
        .mockReturnValueOnce(
          new Promise((resolve) => {
            release = resolve;
          }),
        )
        .mockResolvedValueOnce({
          data: { code: '0', data: [{ positionSide: 'net', leverage: '20', marginMode: 'cross' }] },
        });
      exchange.xhr = { get } as any;
      const older = exchange.fetchLeverageForSymbol('BTCUSDT');
      await vi.waitFor(() => expect(get).toHaveBeenCalledOnce());
      await exchange.fetchLeverageForSymbol('BTCUSDT');
      release({ data: { code: '0', data: [{ positionSide: 'net', leverage: '9', marginMode: 'cross' }] } });
      await older;
      expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(20);
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBe(20);
    } finally {
      exchange.dispose();
    }
  });

  it.each([
    [50, Number.NaN],
    [9, 9],
  ])('BloFin keeps untracked conflicting settings unavailable (%s)', (retained, expected) => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull([position({ leverage: retained })]);
      store.addOrUpdatePositionsFull([]);
      store.update({ fetchedLeverage: { BTCUSDT: { buyLeverage: 9, sellLeverage: 9 } } });
      expect(exchange.mapPositions([{ ...rawBlofin, leverage: '' }] as any)[0].leverage).toBe(expected);
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin keeps a loaded conflict unknown across three real blank handler events', () => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull([position({ leverage: 50 })]);
      store.addOrUpdatePositionsFull([]);
      store.update({ fetchedLeverage: { BTCUSDT: { buyLeverage: 9, sellLeverage: 9 } } });
      for (let event = 1; event <= 3; event++) {
        exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
        expect(store.positions[0].leverage, `blank event ${event}`).toBeNaN();
      }
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin keeps absent leverage unknown across three real handler events', () => {
    const { store, exchange } = blofin();
    try {
      for (let event = 1; event <= 3; event++) {
        exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
        expect(store.positions[0].leverage, `blank event ${event}`).toBeNaN();
      }
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin resolves a loaded conflict with a fresh wire report and preserves it on another blank event', () => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull([position({ leverage: 50 })]);
      store.addOrUpdatePositionsFull([]);
      store.update({ fetchedLeverage: { BTCUSDT: { buyLeverage: 9, sellLeverage: 9 } } });
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBeNaN();
      exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
      expect(store.positions[0].leverage).toBe(50);
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBe(50);
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin does not skip a flat write because an older fetched rail already matches', async () => {
    const { store, exchange } = blofin();
    try {
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      const get = vi
        .fn()
        .mockResolvedValue({
          data: { code: '0', data: [{ positionSide: 'net', leverage: '9', marginMode: 'cross' }] },
        });
      const post = vi.fn().mockResolvedValue({ data: { code: '0', msg: '' } });
      exchange.xhr = { get, post } as any;
      await exchange.fetchLeverageForSymbol('BTCUSDT');
      exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
      exchange.privateWebsocket.handlePositionTopic({ data: [] });
      await exchange.setLeverage('BTCUSDT', 9);
      expect(post).toHaveBeenCalledOnce();
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBe(9);
    } finally {
      exchange.dispose();
    }
  });

  it.each(['fetch', 'write'] as const)('BloFin resolves repeated loaded ambiguity with a fresh %s', async (source) => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull([position({ leverage: 50 })]);
      store.addOrUpdatePositionsFull([]);
      store.update({ fetchedLeverage: { BTCUSDT: { buyLeverage: 9, sellLeverage: 9 } } });
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      const get = vi.fn().mockResolvedValue({
        data: { code: '0', data: [{ positionSide: 'net', leverage: '9', marginMode: 'cross' }] },
      });
      const post = vi.fn().mockResolvedValue({ data: { code: '0', msg: '' } });
      exchange.xhr = { get, post } as any;
      for (let event = 1; event <= 20; event++) {
        exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
        expect(store.positions[0].leverage, `blank event ${event}`).toBeNaN();
        exchange.privateWebsocket.handlePositionTopic({ data: [] });
        expect(store.positions).toEqual([]);
      }
      if (source === 'fetch') await exchange.fetchLeverageForSymbol('BTCUSDT');
      else await exchange.setLeverage('BTCUSDT', 9);
      expect(source === 'fetch' ? get : post).toHaveBeenCalledOnce();
      for (let event = 1; event <= 3; event++) {
        exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
        expect(store.positions[0].leverage).toBe(9);
      }
    } finally {
      exchange.dispose();
    }
  });

  it('BloFin keeps a loaded conflict unknown when an earlier in-flight read completes', async () => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull([position({ leverage: 50 })]);
      store.addOrUpdatePositionsFull([]);
      store.update({ fetchedLeverage: { BTCUSDT: { buyLeverage: 9, sellLeverage: 9 } } });
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      let release!: (value: any) => void;
      const get = vi.fn().mockReturnValue(
        new Promise((resolve) => {
          release = resolve;
        }),
      );
      exchange.xhr = { get } as any;
      const pending = exchange.fetchLeverageForSymbol('BTCUSDT');
      await vi.waitFor(() => expect(get).toHaveBeenCalledOnce());
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBeNaN();
      release({ data: { code: '0', data: [{ positionSide: 'net', leverage: '9', marginMode: 'cross' }] } });
      await pending;
      for (let event = 1; event <= 3; event++) {
        exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
        expect(store.positions[0].leverage).toBeNaN();
      }
      exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBe(50);
    } finally {
      exchange.dispose();
    }
  });

  it.each(['fetch', 'write'] as const)(
    'BloFin drops owner A %s completion after login changes to owner B',
    async (source) => {
      const ownerA = blofin();
      let ownerB: ReturnType<typeof blofin> | undefined;
      try {
        vi.spyOn(ownerA.exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
        vi.spyOn(ownerA.exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
        let release!: (value: any) => void;
        const request = vi.fn().mockReturnValue(
          new Promise((resolve) => {
            release = resolve;
          }),
        );
        ownerA.exchange.xhr = { get: request, post: request } as any;
        const pending =
          source === 'fetch'
            ? ownerA.exchange.fetchLeverageForSymbol('BTCUSDT')
            : ownerA.exchange.setLeverage('BTCUSDT', 9);
        await vi.waitFor(() => expect(request).toHaveBeenCalledOnce());
        ownerA.exchange.dispose();
        ownerB = blofin(ownerA.store);
        ownerB.exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
        ownerB.exchange.privateWebsocket.handlePositionTopic({ data: [] });
        release({ data: { code: '0', data: [{ positionSide: 'net', leverage: '9', marginMode: 'cross' }] } });
        await pending;
        expect(ownerB.store.fetchedLeverage.BTCUSDT).toBeUndefined();
        expect(ownerB.store.positions).toEqual([]);
        ownerB.exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
        expect(ownerB.store.positions[0].leverage).toBe(50);
      } finally {
        if (ownerB) ownerB.exchange.dispose();
        else ownerA.exchange.dispose();
      }
    },
  );

  it.each(['fetch', 'write'] as const)(
    'BloFin drops owner A %s before dispatch when login changes during config await',
    async (source) => {
      const ownerA = blofin();
      let ownerB: ReturnType<typeof blofin> | undefined;
      try {
        let release!: (value: MarginType) => void;
        const margin = vi.spyOn(ownerA.exchange, 'getOrFetchMarginType').mockReturnValue(
          new Promise((resolve) => {
            release = resolve;
          }),
        );
        vi.spyOn(ownerA.exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
        const request = vi.fn();
        ownerA.exchange.xhr = { get: request, post: request } as any;
        const pending =
          source === 'fetch'
            ? ownerA.exchange.fetchLeverageForSymbol('BTCUSDT')
            : ownerA.exchange.setLeverage('BTCUSDT', 9);
        await vi.waitFor(() => expect(margin).toHaveBeenCalledOnce());
        ownerA.exchange.dispose();
        ownerB = blofin(ownerA.store);
        ownerB.exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
        release(MarginType.Cross);
        await pending;
        expect(request).not.toHaveBeenCalled();
        expect(ownerB.store.positions[0].leverage).toBe(50);
        expect(ownerB.store.fetchedLeverage.BTCUSDT).toBeUndefined();
      } finally {
        if (ownerB) ownerB.exchange.dispose();
        else ownerA.exchange.dispose();
      }
    },
  );

  it('BloFin does not recreate a row that closes during a leverage write', async () => {
    const { store, exchange } = blofin();
    try {
      exchange.privateWebsocket.handlePositionTopic({ data: [rawBlofin] as any });
      vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
      vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
      let release!: (value: any) => void;
      const post = vi.fn().mockReturnValue(
        new Promise((resolve) => {
          release = resolve;
        }),
      );
      exchange.xhr = { post } as any;
      const pending = exchange.setLeverage('BTCUSDT', 9);
      await vi.waitFor(() => expect(post).toHaveBeenCalledOnce());
      exchange.privateWebsocket.handlePositionTopic({ data: [] });
      release({ data: { code: '0', msg: '' } });
      await pending;
      expect(store.positions).toEqual([]);
      exchange.privateWebsocket.handlePositionTopic({ data: [{ ...rawBlofin, leverage: '' }] as any });
      expect(store.positions[0].leverage).toBe(9);
    } finally {
      exchange.dispose();
    }
  });

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

  it('BloFin does not revive a closed leverage over an explicitly unknown live value', () => {
    const { store, exchange } = blofin();
    try {
      store.addOrUpdatePositionsFull(exchange.mapPositions([rawBlofin] as any));
      store.updatePosition({ id: 'BTCUSDT' }, { leverage: Number.NaN });
      expect(store.positionSettings.BTCUSDT.leverage).toBe(50);
      expect(exchange.mapPositions([{ ...rawBlofin, leverage: '' }] as any)[0].leverage).toBeNaN();
    } finally {
      exchange.dispose();
    }
  });

  it('Binance does not revive closed settings over explicitly unknown live values', () => {
    const { store, reopen } = binance();
    store.addOrUpdatePositionsFull([position()]);
    store.updatePosition({ id: 'BTCUSDT' }, { leverage: Number.NaN, marginType: undefined, liquidationPrice: null });
    expect(store.positionSettings.BTCUSDT.leverage).toBe(17);
    reopen();
    expect(store.positions[0].leverage).toBeNaN();
    expect(store.positions[0].marginType).toBeUndefined();
  });
});
