import { describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { ENDPOINTS } from '../reference/bitrue/bitrue.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, Market, PositionMode, PositionSide } from '../../types';

const MARKET: Market = {
  id: 'E-BTC-USDT',
  symbol: 'E-BTC-USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: {
    amount: { min: 0.001, max: 1_000 },
    leverage: { min: 1, max: 100 },
  },
};

const accountWithLeverage = (leverageLevel: number) =>
  ({
    code: '0',
    data: {
      account: [
        {
          marginCoin: 'USDT',
          positionVos: [
            {
              contractName: MARKET.symbol,
              positions: [
                {
                  positionType: 1,
                  side: 'BUY',
                  volume: 0.01,
                  openPrice: 60_000,
                  positionBalance: 600,
                  leverageLevel,
                  forceLiquidationPrice: 0,
                  reducePrice: 0,
                },
              ],
            },
          ],
        },
      ],
    },
  }) as any;

describe('audit regression: Bitrue leverage read probe', () => {
  it('reads account metadata without placing an order or changing leverage', async () => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
    const placeOrder = vi.fn();
    const setLeverage = vi.fn();
    const accountRead = vi.fn().mockResolvedValue({ data: accountWithLeverage(17) });
    exchange.unlimitedXHR = { post: placeOrder } as any;
    exchange.xhr = { get: accountRead, post: setLeverage } as any;

    await expect(exchange.fetchPositionMetadata([MARKET.symbol])).resolves.toEqual([
      expect.objectContaining({ symbol: MARKET.symbol, leverage: 17, marginType: MarginType.Cross }),
    ]);

    expect(accountRead).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT);
    expect(placeOrder).not.toHaveBeenCalled();
    expect(setLeverage).not.toHaveBeenCalled();
  });

  it('uses the account snapshot without placing an order or changing leverage', async () => {
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
    const placeOrder = vi.fn();
    const setLeverage = vi.fn();
    exchange.unlimitedXHR = { post: placeOrder } as any;
    exchange.xhr = { post: setLeverage } as any;

    const positions = await exchange.parsePositions(accountWithLeverage(17));

    expect(placeOrder).not.toHaveBeenCalled();
    expect(setLeverage).not.toHaveBeenCalled();
    expect(positions).toHaveLength(1);
    expect(positions[0].leverage).toBe(17);
  });

  it('keeps the current position snapshot when account data is malformed', async () => {
    const store = new DefaultStore();
    const currentPosition = {
      ...accountWithLeverage(11).data.account[0].positionVos[0].positions[0],
    };
    store.update({
      markets: [MARKET],
      positions: [
        {
          id: `${MARKET.symbol}-long-one-way`,
          symbol: MARKET.symbol,
          side: PositionSide.Long,
          positionMode: PositionMode.OneWay,
          marginType: MarginType.Cross,
          leverage: 11,
          entryPrice: currentPosition.openPrice,
          notional: currentPosition.positionBalance,
          unrealizedPnl: 0,
          contracts: currentPosition.volume,
          liquidationPrice: 0,
        },
      ] as any,
    });
    const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
    const placeOrder = vi.fn();
    const setLeverage = vi.fn();
    exchange.unlimitedXHR = { post: placeOrder } as any;
    exchange.xhr = { post: setLeverage } as any;

    await expect(exchange.parsePositions({ data: { account: null } } as any)).resolves.toEqual(store.positions);
    expect(placeOrder).not.toHaveBeenCalled();
    expect(setLeverage).not.toHaveBeenCalled();
  });

  it.each([
    ['missing positionVos', { marginCoin: 'USDT' }],
    ['null positionVos', { marginCoin: 'USDT', positionVos: null }],
    ['null position list', { marginCoin: 'USDT', positionVos: [{ contractName: MARKET.symbol, positions: null }] }],
    [
      'invalid position',
      { marginCoin: 'USDT', positionVos: [{ contractName: MARKET.symbol, positions: [{ side: 'BUY', volume: 'nope' }] }] },
    ],
  ])('does not turn %s into a clearing position snapshot', async (_name, account) => {
    const store = new DefaultStore();
    store.update({
      markets: [MARKET],
      positions: [
        {
          id: `${MARKET.symbol}-long-one-way`,
          symbol: MARKET.symbol,
          side: PositionSide.Long,
          positionMode: PositionMode.OneWay,
          marginType: MarginType.Cross,
          leverage: 11,
          entryPrice: 60_000,
          notional: 600,
          unrealizedPnl: 0,
          contracts: 0.01,
          liquidationPrice: 0,
        },
      ] as any,
    });
    const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);

    await exchange.parsePositions(accountWithLeverage(17));
    await expect(exchange.parsePositions({ code: '0', data: { account: [account] } } as any)).resolves.toEqual(store.positions);
    await expect(exchange.fetchPositionMetadata([MARKET.symbol])).resolves.toEqual([
      expect.objectContaining({ symbol: MARKET.symbol, leverage: 17 }),
    ]);
  });

  it('accepts documented empty position arrays and clears positions', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [MARKET],
      positions: [
        {
          id: `${MARKET.symbol}-long-one-way`,
          symbol: MARKET.symbol,
          side: PositionSide.Long,
          positionMode: PositionMode.OneWay,
          marginType: MarginType.Cross,
          leverage: 11,
          entryPrice: 60_000,
          notional: 600,
          unrealizedPnl: 0,
          contracts: 0.01,
          liquidationPrice: 0,
        },
      ] as any,
    });
    const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);

    await expect(exchange.parsePositions({ code: '0', data: { account: [{ marginCoin: 'USDT', positionVos: [] }] } } as any)).resolves.toEqual([]);
  });

  it('never passes a malformed snapshot as an empty full-store update', async () => {
    const store = new DefaultStore();
    const currentPosition = {
      id: `${MARKET.symbol}-long-one-way`,
      symbol: MARKET.symbol,
      side: PositionSide.Long,
      positionMode: PositionMode.OneWay,
      marginType: MarginType.Cross,
      leverage: 11,
      entryPrice: 60_000,
      notional: 600,
      unrealizedPnl: 0,
      contracts: 0.01,
      liquidationPrice: 0,
    };
    store.update({ markets: [MARKET], positions: [currentPosition] as any });
    const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { code: '0', data: { account: [{ marginCoin: 'USDT', positionVos: null }] } } }) } as any;
    const addOrUpdatePositionsFull = vi.spyOn(store, 'addOrUpdatePositionsFull').mockImplementation((positions) => {
      exchange.dispose();
      return positions;
    });

    await exchange.tick();

    expect(addOrUpdatePositionsFull).toHaveBeenCalledWith([currentPosition]);
    expect(addOrUpdatePositionsFull).not.toHaveBeenCalledWith([]);
  });

  it('keeps the current position snapshot when the account read fails', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [MARKET],
      positions: [
        {
          id: `${MARKET.symbol}-long-one-way`,
          symbol: MARKET.symbol,
          side: PositionSide.Long,
          positionMode: PositionMode.OneWay,
          marginType: MarginType.Cross,
          leverage: 11,
          entryPrice: 60_000,
          notional: 600,
          unrealizedPnl: 0,
          contracts: 0.01,
          liquidationPrice: 0,
        },
      ] as any,
    });
    const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
    const placeOrder = vi.fn();
    const setLeverage = vi.fn();
    const accountRead = vi.fn().mockRejectedValue(new Error('network interrupted'));
    exchange.unlimitedXHR = { post: placeOrder } as any;
    exchange.xhr = { get: accountRead, post: setLeverage } as any;

    const result = await exchange.fetchBalanceAndPositions();

    expect(accountRead).toHaveBeenCalledTimes(1);
    expect(result.positions).toEqual(store.positions);
    expect(placeOrder).not.toHaveBeenCalled();
    expect(setLeverage).not.toHaveBeenCalled();
  });
});
