import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinancePMExchange } from '../../exchanges/binance/binancepm.exchange';
import { BinancePMPrivateWebsocket } from '../../exchanges/binance/binancepm.ws-private';
import { DefaultStore } from '../../store/store.base';
import { PositionSide } from '../../types';

describe('Binance portfolio-margin WS position signs', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('keeps short contracts and notional non-negative like the REST mapper', () => {
    vi.useFakeTimers();
    const store = new DefaultStore();
    store.update({
      positions: [
        {
          id: 'BTCUSDT:short',
          symbol: 'BTCUSDT',
          side: PositionSide.Short,
          entryPrice: 50_000,
          contracts: 0.01,
          notional: 500,
          leverage: 10,
          unrealizedPnl: 0,
          liquidationPrice: 0,
        },
      ],
    });
    const exchange = new BinancePMExchange('binancepm', { key: 'key', secret: 'secret' }, store);
    const websocket = new BinancePMPrivateWebsocket(exchange);

    websocket.handleAccountEvents([
      {
        e: 'ACCOUNT_UPDATE',
        a: {
          P: [{ s: 'BTCUSDT', pa: '-0.006', ep: '84803.13', up: '0.604', ps: 'BOTH' }],
        },
      },
    ]);

    expect(store.positions[0]).toMatchObject({ side: PositionSide.Short, contracts: 0.006 });
    expect(store.positions[0].notional).toBeCloseTo(508.81878);
  });
});
