vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName, Market, Position } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { PositionSide } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

describe('Rust leverage command side boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

  it.each([false, true])(
    'includes settings-only symbols without duplicating live hedge writes (live=%s)',
    async (live) => {
      const store = new DefaultStore();
      const closed = [
        { id: 'ETH-long', symbol: 'ETHUSDT', side: PositionSide.Long, leverage: 5 },
        { id: 'ETH-short', symbol: 'ETHUSDT', side: PositionSide.Short, leverage: 7 },
      ] as Position[];
      const positions = live
        ? ([
            { id: 'BTC-long', symbol: 'BTCUSDT', side: PositionSide.Long, leverage: 3 },
            { id: 'BTC-short', symbol: 'BTCUSDT', side: PositionSide.Short, leverage: 4 },
          ] as Position[])
        : [];
      store.addOrUpdatePositionsFull([...closed, ...positions]);
      store.addOrUpdatePositionsFull(positions);
      const exchange = new RustEngineExchange('bingx', { key: '', secret: '' }, store);
      exchanges.push(exchange);
      const command = vi.fn().mockResolvedValue(null);
      Object.defineProperty(exchange, 'command', { value: command });
      Object.defineProperty(exchange, 'fetchPositions', { value: async () => store.positions });
      await exchange.setAllLeverage(8);
      expect(command.mock.calls.map(([body]) => body)).toEqual([
        ...positions.map(({ symbol, side }) => ({ op: 'setLeverage', symbol, side, leverage: 8 })),
        { op: 'setLeverage', symbol: 'ETHUSDT', leverage: 8 },
      ]);
      expect(store.positions).toEqual(positions);
    },
  );

  it('keeps the account-wide capability on its single native write path', async () => {
    const store = new DefaultStore();
    store.addOrUpdatePositionsFull([
      { id: 'ETH-long', symbol: 'ETHUSDT', side: PositionSide.Long, leverage: 5 },
    ] as Position[]);
    store.addOrUpdatePositionsFull([]);
    const exchange = new RustEngineExchange('bingx', { key: '', secret: '' }, store);
    exchanges.push(exchange);
    adoptEngineCapabilities(exchange, { accountLeverage: true });
    const command = vi.fn().mockResolvedValue(null);
    const fetchPositions = vi.spyOn(exchange, 'fetchPositions');
    Object.defineProperty(exchange, 'command', { value: command });
    await exchange.setAllLeverage(8);
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'setLeverage', symbol: '', leverage: 8 });
    expect(fetchPositions).not.toHaveBeenCalled();
  });

  it.each(['read', 'write'] as const)(
    'drops queued settings writes when login switches during the %s await',
    async (stage) => {
      const store = new DefaultStore();
      store.addOrUpdatePositionsFull([
        { id: 'ETH-long', symbol: 'ETHUSDT', side: PositionSide.Long, leverage: 5 },
        { id: 'SOL-long', symbol: 'SOLUSDT', side: PositionSide.Long, leverage: 7 },
      ] as Position[]);
      store.addOrUpdatePositionsFull([]);
      const ownerA = new RustEngineExchange('bingx', { key: '', secret: '' }, store);
      exchanges.push(ownerA);
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      const read = vi.fn(async () => {
        if (stage === 'read') await held;
        return store.positions;
      });
      const commandA = vi.fn(async () => {
        if (stage === 'write') await held;
        return null;
      });
      Object.defineProperty(ownerA, 'command', { value: commandA });
      Object.defineProperty(ownerA, 'fetchPositions', { value: read });
      const pending = ownerA.setAllLeverage(8);
      await vi.waitFor(() => expect(stage === 'read' ? read : commandA).toHaveBeenCalledOnce());
      ownerA.dispose();
      store.reset();
      store.addOrUpdatePositionsFull([
        { id: 'XRP-long', symbol: 'XRPUSDT', side: PositionSide.Long, leverage: 3 },
      ] as Position[]);
      store.addOrUpdatePositionsFull([]);
      const ownerB = new RustEngineExchange('bingx', { key: '', secret: '' }, store);
      exchanges.push(ownerB);
      const commandB = vi.fn();
      Object.defineProperty(ownerB, 'command', { value: commandB });
      release();
      await pending;
      expect(commandA).toHaveBeenCalledTimes(stage === 'read' ? 0 : 1);
      if (stage === 'write')
        expect(commandA).toHaveBeenCalledWith({ op: 'setLeverage', symbol: 'ETHUSDT', leverage: 8 });
      expect(commandB).not.toHaveBeenCalled();
      expect(Object.keys(ownerB.store.positionSettings)).toEqual(['XRP-long']);
      expect(ownerB.store.positionSettings['XRP-long'].leverage).toBe(3);
      expect(ownerB.store.positions).toEqual([]);
    },
  );

  // Venues answering leverage reads from their published rails. BingX, WEEX and
  // Bitget read the venue; their crates test per-leg reads and write fencing.
  const independentVenues: ExchangeName[] = ['mexc', 'phemex'];
  it.each(independentVenues)('keeps confirmed %s leg settings when leverage is read', async (name) => {
    const store = new DefaultStore();
    const market: Market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      info: {},
    };
    store.update({ markets: [market], fetchedLeverage: { BTCUSDT: { buyLeverage: 5, sellLeverage: 7 } } });
    const exchange = new RustEngineExchange(name, { key: '', secret: '' }, store);
    exchanges.push(exchange);
    adoptEngineCapabilities(exchange);
    await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toMatchObject({
      buyLeverage: 5,
      sellLeverage: 7,
    });
    expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 5, sellLeverage: 7 });
  });

  it.each([undefined, PositionSide.Long, PositionSide.Short, '' as PositionSide])(
    'forwards the requested side %s unchanged',
    async (side) => {
      const exchange = new RustEngineExchange('bitget', { key: '', secret: '' }, new DefaultStore());
      exchanges.push(exchange);
      const command = vi.fn().mockResolvedValue(null);
      Object.defineProperty(exchange, 'command', { value: command });
      await exchange.setLeverage('BTCUSDT', 8, side);
      expect(command).toHaveBeenCalledExactlyOnceWith({
        op: 'setLeverage',
        symbol: 'BTCUSDT',
        leverage: 8,
        ...(side !== undefined ? { side } : {}),
      });
    },
  );

  it.each([undefined, 'BTCUSDT'])('forwards position mode scope %s', async (symbol) => {
    const exchange = new RustEngineExchange('phemex', { key: '', secret: '' }, new DefaultStore());
    exchanges.push(exchange);
    const command = vi.fn().mockResolvedValue(null);
    Object.defineProperty(exchange, 'command', { value: command });
    await exchange.changePositionMode(true, symbol);
    expect(command).toHaveBeenCalledExactlyOnceWith({
      op: 'setPositionMode',
      hedged: true,
      ...(symbol !== undefined ? { symbol } : {}),
    });
  });

  it('sets leverage for both open hedge legs without dropping their sides', async () => {
    const store = new DefaultStore();
    store.update({
      positions: [
        { id: 'long', symbol: 'BTCUSDT', side: PositionSide.Long },
        { id: 'short', symbol: 'BTCUSDT', side: PositionSide.Short },
      ] as Position[],
    });
    const exchange = new RustEngineExchange('bingx', { key: '', secret: '' }, store);
    exchanges.push(exchange);
    const command = vi.fn().mockResolvedValue(null);
    Object.defineProperty(exchange, 'command', { value: command });
    // Positions come from the authoritative native read; this test pins the legs' sides.
    Object.defineProperty(exchange, 'fetchPositions', { value: async () => store.positions });
    await exchange.setAllLeverage(8);
    expect(command.mock.calls.map(([body]) => body)).toEqual([
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 8, side: PositionSide.Long },
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 8, side: PositionSide.Short },
    ]);
  });
  it.each([false, true])('checks market membership after native readiness (market remains=%s)', async (remains) => {
    const store = new DefaultStore();
    const market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      info: {},
    };
    store.update({ markets: [market] });
    const exchange = new RustEngineExchange('bybit', { key: 'read-test', secret: 'read-secret' }, store);
    exchanges.push(exchange);
    adoptEngineCapabilities(exchange);
    const ready = Promise.withResolvers<void>();
    Object.defineProperty(exchange, 'untilNativeData', { value: () => ready.promise });
    const command = vi.fn().mockResolvedValue({ leverage: 5 });
    Object.defineProperty(exchange, 'command', { value: command });
    const pending = exchange.updateSymbolMetadata('BTCUSDT');
    if (!remains) store.update({ markets: [] });
    ready.resolve();
    await pending;
    expect(command).toHaveBeenCalledTimes(remains ? 1 : 0);
  });
});
