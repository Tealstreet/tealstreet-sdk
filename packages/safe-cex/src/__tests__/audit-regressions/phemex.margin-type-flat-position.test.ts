import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../reference/phemex/phemex.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../../types';

// A flat ADAUSDT row as accountPositions actually returns it: size 0, but
// carrying the leverage and margin fields the leverage endpoint needs.
const flatCrossRow = {
  symbol: 'ADAUSDT',
  side: 'None',
  posSide: 'Long',
  posMode: 'OneWay',
  size: '0',
  avgEntryPriceRp: '0',
  leverageRr: '-10',
  crossMargin: true,
  liquidationPriceRp: '0',
};

const flatIsolatedRow = {
  ...flatCrossRow,
  leverageRr: '10',
  crossMargin: false,
};

const flatHedgedRow = {
  ...flatIsolatedRow,
  posSide: 'Long',
  posMode: 'Hedged',
};

const buildExchange = (positionRows = [flatCrossRow]) => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'ADAUSDT',
        symbol: 'ADAUSDT',
        info: { symbol: 'ADAUSDT' },
        precision: { amount: 1, price: 0.0001 },
        limits: { amount: { min: 1, max: 100000 }, leverage: { min: 1, max: 100 } },
      },
    ] as any,
    tickers: [{ id: 'ADAUSDT', symbol: 'ADAUSDT', last: 0.2 }] as any,
  });
  const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);
  const put = vi.fn().mockResolvedValue({ data: { code: 0, msg: '' } });
  // The store stays empty: loaded.positions is false on the hosted-mode mirror,
  // and mapPosition drops size-0 rows anyway, so a flat symbol is never in it.
  const get = vi.fn().mockResolvedValue({ data: { data: { positions: positionRows } } });
  exchange.xhr = { put, get } as any;
  return { exchange, put, get };
};

describe('audit regression: Phemex margin type on a flat symbol', () => {
  it('switches margin type without an open position', async () => {
    // Both guards used to raise "Position ADAUSDT not found" before issuing any
    // request, because getOrFetchPositionsForSymbol returns [] whenever
    // store.loaded.positions is false. The margin switch simply never happened.
    const { exchange, put } = buildExchange();

    await exchange.changeMarginType(false, 'ADAUSDT');

    expect(put).toHaveBeenCalledTimes(1);
    const [, , config] = put.mock.calls[0];
    // Phemex has no margin-mode endpoint: negative leverageRr IS cross margin.
    expect(config.params.leverageRr).toBeLessThan(0);
  });

  it('sends a positive leverage when switching to isolated', async () => {
    const { exchange, put } = buildExchange();

    await exchange.changeMarginType(true, 'ADAUSDT');

    const [, , config] = put.mock.calls[0];
    expect(config.params.leverageRr).toBeGreaterThan(0);
  });

  it('preserves cross margin when only the leverage changes', async () => {
    // setLeverage must not flip the account to isolated as a side effect. With
    // the position unreadable it fell through to a positive leverageRr, which
    // silently switched a cross account to isolated.
    const { exchange, put } = buildExchange();

    await exchange.setLeverage('ADAUSDT', 25);

    const [, , config] = put.mock.calls[0];
    expect(config.params.leverageRr).toBe(-25);
  });

  it('records the applied leverage for a flat symbol', async () => {
    // mapPosition drops size-0 rows, so a leverage change on a flat symbol
    // leaves nothing in store.positions and the place-order % slider falls back
    // to 1x — order sizes then come out `leverage` times too small.
    const { exchange } = buildExchange();

    await exchange.setLeverage('ADAUSDT', 10);

    expect(exchange.store.fetchedLeverage.ADAUSDT).toMatchObject({
      buyLeverage: 10,
      sellLeverage: 10,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
    });
  });

  it('does not record leverage when the exchange rejects the change', async () => {
    const { exchange, put } = buildExchange();
    put.mockResolvedValue({ data: { code: 11001, msg: 'rejected' } });
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(exchange.setLeverage('ADAUSDT', 25)).rejects.toThrow();

    expect(exchange.store.fetchedLeverage.ADAUSDT).toBeUndefined();
  });

  it('records the confirmed position mode for a flat symbol', async () => {
    const { exchange, put } = buildExchange([flatHedgedRow]);

    await exchange.changePositionMode(true, 'ADAUSDT');

    expect(put).toHaveBeenCalledTimes(1);
    expect(put.mock.calls[0][2].params.targetPosMode).toBe('Hedged');
    expect(exchange.store.fetchedLeverage.ADAUSDT).toMatchObject({
      buyLeverage: 10,
      sellLeverage: 10,
      marginType: MarginType.Isolated,
      positionMode: PositionMode.Hedged,
    });
  });

  it('reads margin mode off the raw row, which mapPosition would have dropped', async () => {
    // mapPosition returns undefined for size-0 rows on purpose, so the leverage
    // context has to read accountPositions directly.
    const { exchange } = buildExchange();

    expect(exchange.mapPosition(flatCrossRow as any)).toBeUndefined();

    const context = await (exchange as any).resolveLeverageContext('ADAUSDT');
    expect(context).toMatchObject({ marginType: MarginType.Cross, longLeverage: 10 });
  });

  it('prefers the current raw row over stale stored margin metadata', async () => {
    const { exchange } = buildExchange([flatIsolatedRow]);
    exchange.store.addOrUpdatePositions([
      {
        id: 'ADAUSDT-long',
        symbol: 'ADAUSDT',
        side: PositionSide.Long,
        contracts: 1,
        leverage: 10,
        marginType: MarginType.Cross,
        positionMode: PositionMode.OneWay,
      } as any,
    ]);

    const context = await (exchange as any).resolveLeverageContext('ADAUSDT');

    expect(context).toMatchObject({ marginType: MarginType.Isolated, longLeverage: 10 });
  });
});
