import { describe, expect, it, vi } from 'vitest';

import { PolymarketExchange } from '../reference/polymarket/polymarket.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../../types';

const PRIVATE_KEY = `0x${'11'.repeat(32)}`;
const SYMBOL = 'safe-market-Yes';
const TOKEN_ID = '111';

function createExchange(store = new DefaultStore()) {
  const exchange = new PolymarketExchange(
    'polymarket',
    {
      key: PRIVATE_KEY,
      publicDataOnly: true,
      polymarket: {
        apiKey: 'api-key',
        apiSecret: 'c2VjcmV0',
        apiPassphrase: 'passphrase',
      },
    } as any,
    store,
  );
  store.update({
    markets: [
      {
        id: TOKEN_ID,
        symbol: SYMBOL,
        base: 'safe-market',
        quote: 'pUSD',
        active: true,
        precision: { amount: 2, price: 2 },
        limits: { amount: { min: 5, max: 1_000_000 }, leverage: { min: 1, max: 1 } },
        contractSize: 1,
        info: { tickSize: 0.01, negRisk: false },
      },
    ],
  });
  (exchange as any).symbolToToken.set(SYMBOL, TOKEN_ID);
  (exchange as any).tokenToSymbol.set(TOKEN_ID, SYMBOL);
  return exchange;
}

const limitOrder = {
  symbol: SYMBOL,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 5,
  price: 0.42,
};

describe('Polymarket private-state safety', () => {
  it('rethrows private read failures instead of manufacturing empty or zero snapshots', async () => {
    const exchange = createExchange();
    const failure = new Error('transient upstream failure');
    (exchange as any).api = {
      getPositions: vi.fn().mockRejectedValue(failure),
      getOpenOrders: vi.fn().mockRejectedValue(failure),
      getBalanceAllowance: vi.fn().mockRejectedValue(failure),
    };

    await expect(exchange.fetchAndSetPositions()).rejects.toBe(failure);
    await expect(exchange.fetchOrders()).rejects.toBe(failure);
    await expect((exchange as any).fetchBalance()).rejects.toBe(failure);
    exchange.dispose();
  });

  it('persists the authoritative position refresh triggered by a user-channel trade', async () => {
    const store = new DefaultStore();
    const exchange = createExchange(store);
    const refreshedPosition = {
      id: `${SYMBOL}-long-oneway`,
      symbol: SYMBOL,
      side: PositionSide.Long,
      entryPrice: 0.4,
      notional: 3,
      leverage: 1,
      unrealizedPnl: 1,
      contracts: 5,
      liquidationPrice: 0,
      positionMode: PositionMode.OneWay,
      info: { tokenId: TOKEN_ID },
    };
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([refreshedPosition]);

    (exchange.privateWebsocket as any).handleTradeUpdate({
      id: 'trade-1',
      asset_id: TOKEN_ID,
      side: 'BUY',
      price: '0.6',
      size: '5',
    });

    await vi.waitFor(() => expect(store.positions).toEqual([refreshedPosition]));
    expect(exchange.fetchPositions).toHaveBeenCalledOnce();
    exchange.dispose();
  });
});

describe('Polymarket placement safety', () => {
  it.each([
    OrderType.Market,
    OrderType.Conditional,
    OrderType.StopLoss,
    OrderType.TakeProfit,
    OrderType.TrailingStopLoss,
  ])('rejects unsupported %s orders before signing or posting', async (type) => {
    const exchange = createExchange();
    const postOrder = vi.fn();
    const emitAndLogError = vi
      .spyOn(exchange, 'emitAndLogError')
      .mockImplementation(() => undefined);
    const signTypedData = vi.spyOn((exchange as any).wallet, 'signTypedData');
    (exchange as any).api = { postOrder };

    const message = `Polymarket does not support ${type} orders in safe-cex`;
    await expect(exchange.placeOrder({ ...limitOrder, type })).rejects.toThrow(
      message,
    );
    expect(emitAndLogError).toHaveBeenCalledOnce();
    expect(emitAndLogError).toHaveBeenCalledWith(message);
    expect(signTypedData).not.toHaveBeenCalled();
    expect(postOrder).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it.each([
    [{ triggerPrice: 0.5 }, 'trigger, bracket, or close-only'],
    [{ stopLoss: 0.2 }, 'trigger, bracket, or close-only'],
    [{ takeProfit: 0.8 }, 'trigger, bracket, or close-only'],
    [{ reduceOnly: true }, 'trigger, bracket, or close-only'],
    [{ close: true }, 'trigger, bracket, or close-only'],
    [{ timeInForce: OrderTimeInForce.PostOnly }, OrderTimeInForce.PostOnly],
  ])('rejects unsupported limit modifiers before posting', async (extra, message) => {
    const exchange = createExchange();
    const postOrder = vi.fn();
    (exchange as any).api = { postOrder };

    await expect(exchange.placeOrder({ ...limitOrder, ...extra } as any)).rejects.toThrow(message);
    expect(postOrder).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it.each([
    [{ price: undefined }, 'require a finite price'],
    [{ price: Number.NaN }, 'require a finite price'],
    [{ price: 0 }, 'must be between'],
    [{ price: 1 }, 'must be between'],
    [{ amount: 0 }, 'require a positive finite amount'],
    [{ amount: Number.POSITIVE_INFINITY }, 'require a positive finite amount'],
    [{ amount: 4 }, 'must be at least 5'],
  ])('rejects malformed limit orders before posting', async (update, message) => {
    const exchange = createExchange();
    const postOrder = vi.fn();
    (exchange as any).api = { postOrder };

    await expect(exchange.placeOrder({ ...limitOrder, ...update } as any)).rejects.toThrow(message);
    expect(postOrder).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it('posts a validated plain limit order as GTC', async () => {
    const exchange = createExchange();
    const postOrder = vi.fn().mockResolvedValue({ success: true, orderID: 'order-1' });
    (exchange as any).api = { postOrder };

    await expect(exchange.placeOrder(limitOrder)).resolves.toEqual(['order-1']);
    expect(postOrder).toHaveBeenCalledWith(expect.objectContaining({ tokenId: TOKEN_ID }), 'GTC');
    exchange.dispose();
  });

  it('emits the API rejection before throwing it', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi
      .spyOn(exchange, 'emitAndLogError')
      .mockImplementation(() => undefined);
    (exchange as any).api = {
      postOrder: vi.fn().mockResolvedValue({ success: false, errorMsg: 'order rejected' }),
    };

    await expect(exchange.placeOrder(limitOrder)).rejects.toThrow('order rejected');
    expect(emitAndLogError).toHaveBeenCalledWith('order rejected');
    exchange.dispose();
  });
});
