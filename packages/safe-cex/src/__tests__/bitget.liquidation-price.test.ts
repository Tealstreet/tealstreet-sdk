import { describe, expect, it, vi } from 'vitest';
import axios from 'axios';

import { BitgetExchange } from './reference/bitget/bitget.exchange';
import { ENDPOINTS } from '../exchanges/bitget/bitget.types';
import { DefaultStore } from '../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../types';
import { createPositionId } from '../utils/createPositionId';

const createAPIMock = vi.hoisted(() => vi.fn());

vi.mock('./reference/bitget/bitget.api', () => ({
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

/**
 * Route `xhr.get` by URL so the shared account-type probe (`ACCOUNT_SETTINGS_V3`,
 * added in commit `4417d66f86`) short-circuits to Classic. A blanket
 * `mockResolvedValue` would otherwise return the position payload for the probe
 * too, flip `isUta` on, and reroute `fetchOpenPositions` into the UTA branch
 * (which parses `data.data.list`) — the test's response has no `list`, so the
 * function returns an empty array and `positions[0]` is undefined.
 */
const stubClassicWithPositionResponse = (exchange: BitgetExchange, positionData: unknown) => {
  (exchange.xhr.get as any).mockImplementation(async (url: string) => {
    if (url === ENDPOINTS.ACCOUNT_SETTINGS_V3) {
      return { data: { code: '40084', msg: 'Classic Account mode' } };
    }
    if (url === ENDPOINTS.POSITIONS_V2) {
      return { data: { code: '00000', data: positionData } };
    }
    return { data: { code: '00000', data: [] } };
  });
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
    stubClassicWithPositionResponse(exchange, [rawPosition('')]);

    const positions = await exchange.fetchOpenPositions();

    expect(positions[0].liquidationPrice).toBe(45000);
  });

  it('treats explicit zero liquidation price as authoritative', async () => {
    const { exchange, store } = createExchange();
    seedExistingPosition(store, 45000);
    stubClassicWithPositionResponse(exchange, [rawPosition('0')]);

    const positions = await exchange.fetchOpenPositions();

    expect(positions[0].liquidationPrice).toBe(0);
  });
});
