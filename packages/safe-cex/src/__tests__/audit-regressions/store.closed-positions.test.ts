import { describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';
import { MarginType, Position, PositionMode, PositionSide, Writable } from '../../types';
import { DisplayedPositionsPostprocessor } from '../../webworker/postProcessors/DisplayedPositionsPostprocessor';
import { buildWriteContext } from '../../writer/writeContext';

const position = (changes: Partial<Position> = {}): Position => ({
  id: 'BTC-long',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  contracts: 2,
  entryPrice: 100,
  notional: 200,
  unrealizedPnl: 3,
  leverage: 7,
  liquidationPrice: 60,
  marginSize: 28,
  marginType: MarginType.Isolated,
  positionMode: PositionMode.Hedged,
  ...changes,
});

describe('closed positions leave settings, never manufactured exposure', () => {
  it.each(['single', 'batch'] as const)('restores omitted settings on an incremental %s reopen', (method) => {
    const store = new DefaultStore();
    store.addOrUpdatePositionsFull([
      position(),
      position({ id: 'BTC-short', side: PositionSide.Short, marginType: MarginType.Cross }),
    ]);
    store.addOrUpdatePositionsFull([]);
    const row: Writable<Position> = position({ contracts: 4, entryPrice: 120, leverage: 9, liquidationPrice: null });
    delete row.marginType;
    delete row.positionMode;
    const notify = vi.fn();
    store.subscribe(notify);
    const version = store.updateIds.positions;
    if (method === 'single') store.addOrUpdatePosition(row);
    else store.addOrUpdatePositions([row]);
    expect(store.positions).toEqual([{ ...row, marginType: MarginType.Isolated, positionMode: PositionMode.Hedged }]);
    expect(store.updateIds.positions).toBe(version + 1);
    expect(notify).toHaveBeenCalledOnce();
    store.addOrUpdatePositionsFull([]);
    expect(store.positions).toEqual([]);
    expect(store.positionSettings['BTC-long']).toEqual({
      id: 'BTC-long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      leverage: 9,
      marginType: MarginType.Isolated,
      positionMode: PositionMode.Hedged,
    });
    expect(store.positionSettings['BTC-short'].marginType).toBe(MarginType.Cross);
  });

  it.each(['single', 'batch'] as const)('keeps incoming settings and explicit unknowns on a %s reopen', (method) => {
    for (const supplied of [
      { leverage: 11, marginType: MarginType.Cross, positionMode: PositionMode.OneWay },
      { leverage: Number.NaN, marginType: undefined, positionMode: undefined },
    ]) {
      const store = new DefaultStore();
      store.addOrUpdatePositionsFull([position()]);
      store.addOrUpdatePositionsFull([]);
      const row = position({ ...supplied, liquidationPrice: null });
      if (method === 'single') store.addOrUpdatePosition(row);
      else store.addOrUpdatePositions([row]);
      expect(store.positions).toEqual([row]);
      store.addOrUpdatePositionsFull([]);
      expect(store.positionSettings['BTC-long']).toEqual(store.createEmptyPositionFrom(row));
    }
  });

  it('removes omitted IDs on partial and empty full snapshots and notifies once', () => {
    const store = new DefaultStore();
    store.addOrUpdatePositions([position(), position({ id: 'ETH-short', symbol: 'ETHUSDT' })]);
    const notify = vi.fn();
    store.subscribe(notify);
    const version = store.updateIds.positions;
    store.addOrUpdatePositionsFull([position({ contracts: 1 })]);
    expect(store.positions.map(({ id }) => id)).toEqual(['BTC-long']);
    expect(store.updateIds.positions).toBe(version + 1);
    expect(notify).toHaveBeenCalledTimes(1);
    store.addOrUpdatePositionsFull([]);
    store.addOrUpdatePositionsFull([]);
    expect(store.positions).toEqual([]);
    expect(store.serialize().positions).toEqual([]);
  });

  it('projects only actual identity and settings from a closed row', () => {
    const store = new DefaultStore();
    expect(store.createEmptyPositionFrom(position({ realizedPnl: 9, extra: { contractsNative: 5 } }))).toEqual({
      id: 'BTC-long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      leverage: 7,
      marginType: MarginType.Isolated,
      positionMode: PositionMode.Hedged,
    });
  });

  it('preserves independent hedge and margin settings through serialization, reopening and reset', () => {
    const store = new DefaultStore();
    store.addOrUpdatePositions([
      position(),
      position({ id: 'BTC-short-cross', side: PositionSide.Short, leverage: 13, marginType: MarginType.Cross }),
    ]);
    store.addOrUpdatePositionsFull([]);
    const output = JSON.parse(JSON.stringify(store.serialize()));
    expect(output.positions).toEqual([]);
    expect(output.positionSettings).toEqual({
      'BTC-long': {
        id: 'BTC-long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        leverage: 7,
        marginType: MarginType.Isolated,
        positionMode: PositionMode.Hedged,
      },
      'BTC-short-cross': {
        id: 'BTC-short-cross',
        symbol: 'BTCUSDT',
        side: PositionSide.Short,
        leverage: 13,
        marginType: MarginType.Cross,
        positionMode: PositionMode.Hedged,
      },
    });
    const receiving = new DefaultStore();
    receiving.update(output, true);
    const row: Writable<Position> = position();
    delete row.marginType;
    delete row.positionMode;
    receiving.addOrUpdatePositionsFull([row]);
    expect(receiving.positions[0]).toMatchObject({
      marginType: MarginType.Isolated,
      positionMode: PositionMode.Hedged,
    });
    receiving.reset();
    expect(receiving.serialize().positionSettings).toEqual({});
  });

  it('keeps a producer-reported zero-size row unchanged instead of treating it as an omission', () => {
    const store = new DefaultStore();
    const row = position({ contracts: 0, entryPrice: 100, unrealizedPnl: 3, liquidationPrice: null });
    store.addOrUpdatePositionsFull([row]);
    expect(store.positions).toEqual([row]);
  });

  it('ships no closed rows to display or writer context while retaining the actual margin mode', () => {
    const store = new DefaultStore();
    store.addOrUpdatePositionsFull([position()]);
    store.addOrUpdatePositionsFull([]);
    const output = store.serialize();
    new DisplayedPositionsPostprocessor('account', 'blofin', { key: '', secret: '' }).process(output);
    const receiving = new DefaultStore();
    receiving.update(structuredClone(output), true);
    expect(receiving.positions).toEqual([]);
    expect(receiving.displayedPositions).toEqual([]);
    const context = buildWriteContext(receiving, { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' });
    expect(context.positions).toBeUndefined();
    expect(context.settings?.BTCUSDT.marginMode).toBe(MarginType.Isolated);
    const accountContext = buildWriteContext(receiving, { op: 'cancelSymbolOrders', symbol: '' });
    expect(accountContext.positions).toBeUndefined();
    expect(accountContext.settings?.BTCUSDT.marginMode).toBe(MarginType.Isolated);
  });

  it('sets leverage for known flat symbols as well as live symbols without duplicating hedge sides', async () => {
    const store = new DefaultStore();
    store.addOrUpdatePositionsFull([
      position(),
      position({ id: 'BTC-short', side: PositionSide.Short }),
      position({ id: 'ETH-long', symbol: 'ETHUSDT' }),
    ]);
    store.addOrUpdatePositionsFull([position({ id: 'ETH-long', symbol: 'ETHUSDT' })]);
    const exchange = new BaseExchange('blofin', { key: '', secret: '' }, store);
    exchange.setLeverage = vi.fn().mockResolvedValue(undefined);
    await exchange.setAllLeverage(9);
    expect(exchange.setLeverage).toHaveBeenCalledTimes(2);
    expect(exchange.setLeverage).toHaveBeenCalledWith('BTCUSDT', 9);
    expect(exchange.setLeverage).toHaveBeenCalledWith('ETHUSDT', 9);
    exchange.dispose();
  });

  it('retires queued settings work when login switches during the first leverage await', async () => {
    const store = new DefaultStore();
    store.addOrUpdatePositionsFull([position(), position({ id: 'ETH-long', symbol: 'ETHUSDT' })]);
    store.addOrUpdatePositionsFull([]);
    const ownerA = new BaseExchange('blofin', { key: '', secret: '' }, store);
    const ownerB = new BaseExchange('blofin', { key: '', secret: '' }, new DefaultStore());
    let release!: () => void;
    ownerA.setLeverage = vi.fn().mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    ownerB.setLeverage = vi.fn().mockResolvedValue(undefined);
    let signedIn = ownerA;
    const pending = signedIn.setAllLeverage(9);
    await vi.waitFor(() => expect(ownerA.setLeverage).toHaveBeenCalledTimes(1));
    ownerA.dispose();
    signedIn = ownerB;
    release();
    await pending;
    expect(ownerA.setLeverage).toHaveBeenCalledTimes(1);
    expect(signedIn.setLeverage).not.toHaveBeenCalled();
    expect(signedIn.store.positionSettings).toEqual({});
    ownerB.dispose();
  });
});
