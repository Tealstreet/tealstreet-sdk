import { describe, expect, it } from 'vitest';

import {
  mapBitgetFill,
  mapBitgetFills,
  mapBitgetUtaFill,
  mapBitgetUtaFills,
} from '../reference/bitget/bitget.fills';
import { OrderSide, PositionSide } from '../../types';

/** The exact shape measured live on sheet row 36. */
const fill = {
  tradeId: '1450691558581735424',
  symbol: 'TRXUSDT',
  orderId: '1450691558385528833',
  price: '0.31748',
  baseVolume: '31',
  feeDetail: [{ deduction: 'no', feeCoin: 'USDT', totalDeductionFee: null, totalFee: '-0.00590512' }],
  side: 'sell',
  quoteVolume: '9.8418',
  profit: '-0.00031',
  enterPointSource: 'api',
  tradeSide: 'sell_single',
  posMode: 'one_way_mode',
  tradeScope: 'taker',
  cTime: '1781597740788',
};

const deps = {
  resolveSymbol: (s: string) => (s === 'TRXUSDT' || s === 'BTCUSDT' ? s : undefined),
  toTimestamp: (ms: number) => Math.floor(ms / 1000),
};

const utaFill = {
  execId: '131111111111111111',
  execLinkId: '131111111111111111',
  orderId: '131111111111111112',
  clientOid: 'client-1',
  category: 'USDT-FUTURES',
  symbol: 'BTCUSDT',
  orderType: 'market',
  side: 'sell',
  execPrice: '106950.1',
  execQty: '0.01',
  execValue: '1069.501',
  tradeScope: 'taker',
  tradeSide: 'open',
  feeDetail: [{ feeCoin: 'USDT', fee: '0.6417006' }],
  createdTime: '1750141421721',
  updatedTime: '1750141421728',
  execPnl: '-0.002',
  isRPI: 'no',
};

describe('mapBitgetFill', () => {
  it('carries realizedPnl — the field without which bitget cannot fold at all', () => {
    // The previous mapper read orders-history and populated none of this, which is exactly
    // why the venue produced no records despite having a working fills route.
    expect(mapBitgetFill(fill, deps)!.realizedPnl).toBeCloseTo(-0.00031, 12);
  });

  it('sets the per-fill id, which the old order-level mapper never had', () => {
    // Without an id, fills cannot be deduped and a re-fetch double-counts the position.
    expect(mapBitgetFill(fill, deps)!.id).toBe('1450691558581735424');
  });

  it('NEGATES the fee — bitget signs it negative when charged', () => {
    // Canonical `commission` is positive-when-charged. Using Math.abs here instead would
    // silently turn a maker REBATE into a cost, so the negation is deliberate.
    expect(mapBitgetFill(fill, deps)!.commission).toBeCloseTo(0.00590512, 12);
  });

  it('preserves a rebate as a rebate', () => {
    const rebate = { ...fill, feeDetail: [{ feeCoin: 'USDT', totalFee: '0.0001' }] };
    expect(mapBitgetFill(rebate, deps)!.commission).toBeLessThan(0);
  });

  it('SUMS feeDetail rather than reading only the first entry', () => {
    // A fill can be charged in more than one coin; reading [0] under-reports those.
    const multi = {
      ...fill,
      feeDetail: [
        { feeCoin: 'USDT', totalFee: '-0.001' },
        { feeCoin: 'BGB', totalFee: '-0.002' },
      ],
    };
    expect(mapBitgetFill(multi, deps)!.commission).toBeCloseTo(0.003, 12);
  });

  it('reports one-way mode as `both`, never a guessed leg', () => {
    // `sell_single` carries no hedge leg. Inferring Short from the order side would describe
    // a one-way account as hedged and corrupt every hedge census.
    expect(mapBitgetFill(fill, deps)!.positionSide).toBe('both');
  });

  it('reads the hedge leg when the venue reports one', () => {
    expect(mapBitgetFill({ ...fill, tradeSide: 'close_long' }, deps)!.positionSide).toBe(PositionSide.Long);
    expect(mapBitgetFill({ ...fill, tradeSide: 'open_short' }, deps)!.positionSide).toBe(PositionSide.Short);
  });

  it('maps side and maker/taker', () => {
    expect(mapBitgetFill(fill, deps)!.side).toBe(OrderSide.Sell);
    expect(mapBitgetFill(fill, deps)!.maker).toBe(false);
    expect(mapBitgetFill({ ...fill, tradeScope: 'maker' }, deps)!.maker).toBe(true);
  });

  it('drops a row whose symbol cannot be resolved rather than passing the venue id through', () => {
    expect(mapBitgetFill({ ...fill, symbol: 'NOTAMARKET' }, deps)).toBeNull();
  });

  it('drops rows with no usable timestamp, price or size', () => {
    expect(mapBitgetFill({ ...fill, cTime: '0' }, deps)).toBeNull();
    expect(mapBitgetFill({ ...fill, price: '0' }, deps)).toBeNull();
    expect(mapBitgetFill({ ...fill, baseVolume: '0' }, deps)).toBeNull();
  });

  it('skips unmappable rows in a batch without losing the rest', () => {
    expect(mapBitgetFills([fill, { ...fill, symbol: 'NOPE' }, fill], deps)).toHaveLength(2);
  });
});

describe('mapBitgetUtaFill', () => {
  it('maps documented v3 fill fields into an Execution', () => {
    const execution = mapBitgetUtaFill(utaFill, deps)!;

    expect(execution).toEqual(
      expect.objectContaining({
        id: '131111111111111111',
        orderId: '131111111111111112',
        symbol: 'BTCUSDT',
        timestamp: 1750141421,
        price: 106950.1,
        qty: 0.01,
        commission: 0.6417006,
        commissionAsset: 'USDT',
        realizedPnl: -0.002,
        side: OrderSide.Sell,
        positionSide: PositionSide.Short,
        maker: false,
      }),
    );
  });

  it('derives UTA hedge legs from order direction plus open/close semantics', () => {
    expect(mapBitgetUtaFill({ ...utaFill, side: 'buy', tradeSide: 'open' }, deps)!.positionSide).toBe(
      PositionSide.Long,
    );
    expect(mapBitgetUtaFill({ ...utaFill, side: 'sell', tradeSide: 'close' }, deps)!.positionSide).toBe(
      PositionSide.Long,
    );
    expect(mapBitgetUtaFill({ ...utaFill, side: 'buy', tradeSide: 'close' }, deps)!.positionSide).toBe(
      PositionSide.Short,
    );
  });

  it('prefers explicit UTA posSide when the venue includes it', () => {
    expect(mapBitgetUtaFill({ ...utaFill, side: 'buy', tradeSide: 'close', posSide: 'long' }, deps)!.positionSide).toBe(
      PositionSide.Long,
    );
  });

  it('leaves realizedPnl absent when v3 omits execPnl instead of fabricating zero', () => {
    const execution = mapBitgetUtaFill({ ...utaFill, execPnl: undefined }, deps)!;
    expect(execution.realizedPnl).toBeUndefined();
  });

  it('skips unmappable UTA rows without losing the rest', () => {
    expect(mapBitgetUtaFills([utaFill, { ...utaFill, symbol: 'NOPE' }, utaFill], deps)).toHaveLength(2);
  });
});
