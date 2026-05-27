import { describe, expect, it, vi } from 'vitest';
import axios from 'axios';

import { BitgetExchange } from '../exchanges/bitget/bitget.exchange';
import { DefaultStore } from '../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../types';
import { createPositionId } from '../utils/createPositionId';

const createAPIMock = vi.hoisted(() => vi.fn());

vi.mock('../exchanges/bitget/bitget.api', () => ({
  createAPI: (...args: any[]) => createAPIMock(...args),
}));

const createExchange = () => {
  const store = new DefaultStore();
  createAPIMock.mockReturnValue(axios.create());
  const exchange = new BitgetExchange(
    'bitget' as any,
    {
      key: 'k',
      secret: 's',
      passphrase: 'p',
    } as any,
    store,
  );
  exchange.xhr = { get: vi.fn() } as any;
  return { exchange, store };
};

const seedExistingPosition = (store: DefaultStore, liquidationPrice: number) => {
  store.update({
    positions: [
      {
        id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.OneWay),
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 50000,
        notional: 50000,
        leverage: 10,
        unrealizedPnl: 0,
        contracts: 1,
        liquidationPrice,
        marginType: MarginType.Cross,
        positionMode: PositionMode.OneWay,
        info: {},
      },
    ],
  });
};

const rawPosition = (liquidationPrice: string) => ({
  marginCoin: 'USDT',
  symbol: 'BTCUSDT',
  holdSide: 'long',
  openDelegateSize: '0',
  marginSize: '5000',
  available: '1',
  locked: '0',
  total: '1',
  leverage: '10',
  achievedProfits: '0',
  openPriceAvg: '50000',
  marginMode: 'crossed',
  posMode: 'one_way_mode',
  unrealizedPL: '0',
  liquidationPrice,
  keepMarginRate: '0.004',
  markPrice: '50000',
  breakEvenPrice: '50000',
  totalFee: '0',
  deductedFee: '0',
  takeProfit: '',
  stopLoss: '',
  takeProfitId: '',
  stopLossId: '',
  marginRatio: '0',
  assetMode: 'single',
  cTime: '0',
  uTime: '0',
});

describe('Bitget liquidation price mapping', () => {
  it('preserves the previous liquidation price when REST sends a blank value', async () => {
    const { exchange, store } = createExchange();
    seedExistingPosition(store, 45000);
    (exchange.xhr.get as any).mockResolvedValue({
      data: {
        code: '00000',
        data: [rawPosition('')],
      },
    });

    const positions = await exchange.fetchOpenPositions();

    expect(positions[0].liquidationPrice).toBe(45000);
  });

  it('treats explicit zero liquidation price as authoritative', async () => {
    const { exchange, store } = createExchange();
    seedExistingPosition(store, 45000);
    (exchange.xhr.get as any).mockResolvedValue({
      data: {
        code: '00000',
        data: [rawPosition('0')],
      },
    });

    const positions = await exchange.fetchOpenPositions();

    expect(positions[0].liquidationPrice).toBe(0);
  });
});
