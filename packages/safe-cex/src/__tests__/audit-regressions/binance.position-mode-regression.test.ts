import { describe, expect, it, vi } from 'vitest';

import { ENDPOINTS } from '../../exchanges/binance/binance.types';
import { FrameStore } from '../../store/frameStore';
import { PositionMode, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { nativePosition } from '../cutover/nativePosition';
import { applyReferenceFixture } from '../cutover/referenceFixture';
import { BinanceExchange } from '../reference/binance/binance.exchange';

const createExchange = () => {
  const store = new FrameStore();
  const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store };
};

describe('Binance position mode synchronization', () => {
  it('treats already-applied Binance position mode as a local success', async () => {
    const { exchange, store } = createExchange();
    const post = vi.spyOn(exchange.xhr, 'post').mockRejectedValue({
      response: { data: { code: -4059, msg: 'No need to change position side.' } },
    });

    await exchange.changePositionMode(false);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.HEDGE_MODE, { dualSidePosition: 'false' });
    expect(store.options.isHedged).toBe(false);
    expect(exchange.emitAndLogError).not.toHaveBeenCalled();
  });

  it('mirrors worker position mode changes into the main-thread store', () => {
    const { exchange, store } = createExchange();
    applyReferenceFixture(
      store,
      { options: { ...store.options, ['isHedged']: true } },
      store.markets.map((row) => row.symbol),
    );

    applyReferenceFixture(
      store,
      {
        fetchedLeverage: {
          DOGEUSDT: { buyLeverage: 20, sellLeverage: 20, positionMode: PositionMode.Hedged },
        },
        positions: [
          nativePosition({
            id: createPositionId('DOGEUSDT', PositionSide.Long, PositionMode.Hedged),
            symbol: 'DOGEUSDT',
            side: PositionSide.Long,
            contracts: 0,
            positionMode: PositionMode.Hedged,
          }) as any,
        ],
      },
      store.markets.map((row) => row.symbol),
    );

    exchange.afterMethodCalledInWorker(Promise.resolve(), 'changePositionMode', false, 'DOGEUSDT');

    expect(store.options.isHedged).toBe(false);
    expect(store.fetchedLeverage.DOGEUSDT).toEqual({
      buyLeverage: 20,
      sellLeverage: 20,
      positionMode: PositionMode.OneWay,
    });
    expect(store.positions[0]).toMatchObject({
      id: createPositionId('DOGEUSDT', PositionSide.Long, PositionMode.OneWay),
      symbol: 'DOGEUSDT',
      side: PositionSide.Long,
      contracts: 0,
      positionMode: PositionMode.OneWay,
    });
  });
});
