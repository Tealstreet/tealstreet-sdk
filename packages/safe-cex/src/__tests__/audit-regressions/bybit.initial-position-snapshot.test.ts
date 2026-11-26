import type { StoreData } from '../../generated/StoreData';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { MarginType, Position, PositionMode, PositionSide } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { applyReferenceFixture } from '../cutover/referenceFixture';
import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';

const createPosition = (symbol: string): Position => ({
  id: `${symbol}:long:oneway`,
  symbol,
  side: PositionSide.Long,
  entryPrice: 50_000,
  notional: 5_000,
  leverage: 10,
  unrealizedPnl: 25,
  realizedPnl: 0,
  contracts: 0.1,
  liquidationPrice: 45_000,
  positionMode: PositionMode.OneWay,
  marginType: MarginType.Cross,
  info: {},
  lastUpdate: 9_900,
});

describe('Bybit initial position snapshot reconciliation', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('preserves a recent websocket position when the startup REST snapshot is stale empty', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(10_000);

    const variants = [
      {
        exchange: new BybitExchange('bybit', { key: 'key', secret: 'secret' }, new FrameStore()),
        symbol: 'BTCUSDT',
      },
      {
        exchange: new BybitInverseExchange('bybitinverse', { key: 'key', secret: 'secret' }, new FrameStore()),
        symbol: 'BTCUSD',
      },
    ];

    for (const { exchange, symbol } of variants) {
      const websocketPosition = createPosition(symbol);
      (() => {
        const incoming: StoreData['positions'] = [websocketPosition];
        return applyReferenceFixture(
          exchange.store,
          {
            positions: incoming.map((row) => ({
              ...exchange.store.positions.find((old) => old.id === row.id),
              ...row,
            })),
          },
          exchange.store.markets.map((row) => row.symbol),
        );
      })();
      exchange.lastWebsocketPositionsUpdate = 10_000;
      vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
      vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
      vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue({} as never);
      vi.spyOn(exchange, 'formatBalance').mockReturnValue({ free: 900, total: 1_000, used: 100, upnl: 25 });

      await exchange.fetchOrdersPositionsAndBalance();

      expect(exchange.store.positions).toEqual([websocketPosition]);
      expect(exchange.store.loaded.positions).toBe(true);
    }
  });
});
