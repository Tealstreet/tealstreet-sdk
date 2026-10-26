import { describe, expect, it, vi } from 'vitest';

import { CryptocomExchange } from '../reference/cryptocom/cryptocom.exchange';
import { CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS } from '../reference/cryptocom/cryptocom.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';

const createExchange = () => new CryptocomExchange('cryptocom', { key: 'key', secret: 'secret' }, new DefaultStore());

describe('audit regression: Crypto.com WebSocket protocol', () => {
  it('waits for the documented rate-limit reset before reconnect auth and subscriptions', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const publicSend = vi.fn();
    const privateSend = vi.fn();
    exchange.publicWebsocket.ws = { readyState: WebSocket.OPEN, send: publicSend } as unknown as WebSocket;
    exchange.privateWebsocket.ws = { readyState: WebSocket.OPEN, send: privateSend } as unknown as WebSocket;
    exchange.publicWebsocket.subscribeToTicker('BTCUSD-PERP');
    publicSend.mockClear();

    exchange.publicWebsocket.onOpen();
    exchange.privateWebsocket.onOpen();
    expect(publicSend).not.toHaveBeenCalled();
    expect(privateSend).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(999);
    expect(publicSend).not.toHaveBeenCalled();
    expect(privateSend).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(JSON.parse(publicSend.mock.calls[0][0])).toMatchObject({ method: 'subscribe' });
    expect(JSON.parse(privateSend.mock.calls[0][0])).toMatchObject({ method: 'public/auth' });
    vi.useRealTimers();
  });

  it('processes every order and position in documented batched private frames', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: ['BTC', 'ETH'].map((base) => ({
        id: `${base}USD-PERP`,
        symbol: `${base}USD-PERP`,
        base,
        quote: 'USD',
        active: true,
        precision: { amount: 0.001, price: 0.5 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
      })),
    });

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        code: 0,
        result: {
          channel: 'user.order',
          subscription: 'user.order',
          data: ['BTC', 'ETH'].map((base, index) => ({
            order_id: `order-${index}`,
            order_type: 'LIMIT',
            time_in_force: 'GOOD_TILL_CANCEL',
            side: 'BUY',
            exec_inst: index === 1 ? ['REDUCE_ONLY'] : [],
            quantity: '1',
            limit_price: index === 0 ? '60000' : '3000',
            status: 'ACTIVE',
            instrument_name: `${base}USD-PERP`,
            create_time: 1,
          })),
        },
      }),
    } as MessageEvent);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        code: 0,
        result: {
          channel: 'user.positions',
          subscription: 'user.positions',
          data: [
            {
              instrument_name: 'BTCUSD-PERP',
              type: 'PERPETUAL_SWAP',
              quantity: '0.5',
              cost: '30000',
              open_position_pnl: '250',
              leverage: '12',
              liquidation_price: '54000',
              mark_price: '60500',
            },
            {
              instrument_name: 'ETHUSD-PERP',
              type: 'PERPETUAL_SWAP',
              quantity: '-2',
              cost: '-6000',
              open_position_pnl: '100',
              target_leverage: '8',
              liquidation_price: '3500',
              mark_price: '2950',
              isolation_id: 'isolated-1',
              isolation_type: 'ISOLATED_MARGIN',
            },
          ],
        },
      }),
    } as MessageEvent);

    await vi.waitFor(() => expect(exchange.store.positions).toHaveLength(2));
    expect(exchange.store.orders).toHaveLength(2);
    expect(exchange.store.orders[1]).toMatchObject({ id: 'order-1', reduceOnly: true });
    expect(exchange.privateWebsocket.messageHandlersStats['user.order']).toBeDefined();
    expect(exchange.privateWebsocket.messageHandlersStats['user.positions']).toBeDefined();
    expect(exchange.store.positions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ symbol: 'BTCUSD-PERP', leverage: 12, liquidationPrice: 54_000 }),
        expect.objectContaining({
          symbol: 'ETHUSD-PERP',
          leverage: 8,
          liquidationPrice: 3_500,
          marginType: MarginType.Isolated,
          info: { markPrice: 2_950, isolationId: 'isolated-1' },
        }),
      ]),
    );
  });

  it('applies user.balance summaries immediately instead of waiting for REST polling', () => {
    const exchange = createExchange();

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        code: 0,
        result: {
          channel: 'user.balance',
          subscription: 'user.balance',
          data: [
            {
              instrument_name: 'USD',
              total_available_balance: '38.32',
              total_margin_balance: '50',
              total_initial_margin: '11.68',
              total_maintenance_margin: '5',
              total_cash_balance: '45',
              total_collateral_value: '50',
              total_session_unrealized_pnl: '5',
              total_session_realized_pnl: '0',
              total_position_cost: '100',
              position_balances: [
                {
                  instrument_name: 'USD',
                  quantity: '50',
                  reserved_qty: '11.68',
                  collateral_amount: '50',
                  collateral_eligible: true,
                  haircut: '0',
                  market_value: '50',
                  max_withdrawal_balance: '38.32',
                  hourly_interest_rate: '0',
                },
              ],
            },
          ],
        },
      }),
    } as MessageEvent);

    expect(exchange.store.balance).toMatchObject({
      total: 50,
      free: 38.32,
      used: 11.68,
      upnl: 5,
      currencies: { USD: { total: 50, free: 38.32, used: 11.68 } },
    });
  });

  it('rebuilds the default full-snapshot book so disappeared levels are removed', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const callback = vi.fn();
    exchange.store.update({
      markets: [
        {
          id: 'BTCUSD-PERP',
          symbol: 'BTCUSD-PERP',
          base: 'BTC',
          quote: 'USD',
          active: true,
          precision: { amount: 0.001, price: 0.5 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
        },
      ],
    });
    exchange.publicWebsocket.ws = { readyState: WebSocket.OPEN, send: vi.fn() } as unknown as WebSocket;
    exchange.listenOrderBook('BTCUSD-PERP', callback);

    const frame = (bids: string[][], asks: string[][]) => ({
      data: JSON.stringify({
        code: 0,
        result: {
          channel: 'book',
          subscription: 'book.BTCUSD-PERP.50',
          instrument_name: 'BTCUSD-PERP',
          data: [{ bids, asks, t: 1, tt: 1 }],
        },
      }),
    });

    exchange.publicWebsocket.onMessage(frame([['100', '1', '1'], ['99', '2', '1']], [['101', '3', '1']]) as MessageEvent);
    exchange.publicWebsocket.onMessage(frame([], [['102', '5', '1']]) as MessageEvent);
    await vi.advanceTimersByTimeAsync(100);

    const latest = callback.mock.calls.at(-1)![0];
    expect(latest.bids).toEqual([]);
    expect(latest.asks).toEqual([{ price: 102, amount: 5, total: 5 }]);
    vi.useRealTimers();
  });

  it.each(['publicWebsocket', 'privateWebsocket'] as const)(
    'immediately echoes mandatory heartbeats on %s',
    (socketName) => {
      const websocket = createExchange()[socketName];
      const send = vi.fn();
      websocket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;

      websocket.onMessage({
        data: JSON.stringify({ id: 1_750_000_000_123, method: 'public/heartbeat', code: 0 }),
      } as MessageEvent);

      expect(send).toHaveBeenCalledWith(
        JSON.stringify({ id: 1_750_000_000_123, method: 'public/respond-heartbeat' }),
      );
    },
  );

  it('uses synchronized time on public and private subscription commands', () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_750_000_000_123);
    const exchange = createExchange();
    const publicSend = vi.fn();
    const privateSend = vi.fn();
    exchange.publicWebsocket.ws = { readyState: WebSocket.OPEN, send: publicSend } as unknown as WebSocket;
    exchange.privateWebsocket.ws = { readyState: WebSocket.OPEN, send: privateSend } as unknown as WebSocket;

    exchange.publicWebsocket.subscribeToTicker('BTCUSD-PERP');
    exchange.privateWebsocket.subscribeToPositionUpdates();

    expect(JSON.parse(publicSend.mock.calls[0][0])).toMatchObject({ nonce: 1_750_000_000_123 });
    // The unauthenticated private socket records the subscription without
    // sending it; authenticate to flush the synchronized subscribe request.
    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({ id: 1, method: 'public/auth', code: 0 }),
    } as MessageEvent);
    expect(JSON.parse(privateSend.mock.calls[0][0])).toMatchObject({ nonce: 1_750_000_000_123 });
  });

  it.each(['publicWebsocket', 'privateWebsocket'] as const)(
    'does not reconnect quiet %s sessions at the base 20s threshold',
    (socketName) => {
      const websocket = createExchange()[socketName];
      const reconnect = vi.spyOn(websocket, 'reconnect').mockResolvedValue();

      websocket.lastReceived = Date.now() - 21_000;
      websocket.checkLastReceived();
      expect(reconnect).not.toHaveBeenCalled();

      websocket.lastReceived = Date.now() - CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS - 1;
      websocket.checkLastReceived();
      expect(reconnect).toHaveBeenCalledTimes(1);
    },
  );

  it('maps daily candles to the case-sensitive wire interval and dispatches by subscription', () => {
    const exchange = createExchange();
    const callback = vi.fn();
    const send = vi.fn();
    exchange.publicWebsocket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;

    exchange.listenOHLCV({ symbol: 'BTCUSD-PERP', interval: '1d' }, callback);

    expect(JSON.parse(send.mock.calls[0][0])).toMatchObject({
      method: 'subscribe',
      params: { channels: ['candlestick.1D.BTCUSD-PERP'] },
    });

    exchange.publicWebsocket.onMessage({
      data: JSON.stringify({
        code: 0,
        result: {
          channel: 'candlestick',
          subscription: 'candlestick.1D.BTCUSD-PERP',
          instrument_name: 'BTCUSD-PERP',
          data: [{ t: 1, o: '1', h: '2', l: '0.5', c: '1.5', v: '10' }],
        },
      }),
    } as MessageEvent);

    expect(callback).toHaveBeenCalledWith({ timestamp: 1, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 });
  });

  it('rejects unsupported safe-cex candle intervals before subscribing', () => {
    const exchange = createExchange();
    exchange.publicWebsocket.ws = { readyState: WebSocket.OPEN, send: vi.fn() } as unknown as WebSocket;

    expect(() => exchange.listenOHLCV({ symbol: 'BTCUSD-PERP', interval: '3m' }, vi.fn())).toThrow(
      'Unsupported Crypto.com candlestick interval: 3m',
    );
  });
});
