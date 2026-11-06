import { afterEach, describe, expect, it, vi } from 'vitest';
import { HyperliquidSpotExchange } from '../reference/hyperliquid/hyperliquidspot.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide } from '../../types';

const exchanges: HyperliquidSpotExchange[] = [];
afterEach(() => { exchanges.splice(0).forEach((exchange) => exchange.dispose()); vi.restoreAllMocks(); });
const create = () => {
  const exchange = new HyperliquidSpotExchange('hyperliquidspot', {
    key: '', secret: '', applicationId: '0xmain', vault: '0xvault', publicDataOnly: true,
  }, new DefaultStore());
  exchanges.push(exchange);
  return exchange;
};

// https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions
// Documented WsSpotState and WsFill: oid identifies the order, tid the execution.
describe('Hyperliquid spot private protocol contracts', () => {
  it('subscribes to current spot balances and consumed account feeds with vault identity', () => {
    const exchange = create();
    const send = vi.spyOn(exchange.privateWebsocket, 'send').mockImplementation(() => {});
    exchange.privateWebsocket.subscribe();
    expect(send.mock.calls.map(([text]) => JSON.parse(text))).toEqual([
      'notification', 'spotState', 'userFills', 'userNonFundingLedgerUpdates',
    ].map((type) => ({ method: 'subscribe', subscription: { type, user: '0xvault' } })));
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({
      channel: 'spotState', data: { user: '0xvault', spotState: {
        balances: [{ coin: 'USDC', token: 0, hold: '2', total: '10', entryNtl: '0' }],
      } },
    }) } as MessageEvent);
    expect(exchange.store.balance).toMatchObject({ total: 10, free: 8, used: 2 });
  });
  it('preserves execution and native order identities for multiple fills of one order', () => {
    const exchange = create();
    const onFill = vi.fn();
    exchange.emitter.on('fill', onFill);
    vi.spyOn(exchange, 'requestSpotBalanceRefresh').mockImplementation(() => {});
    const fills = [12345, 12346].map((tid) => ({
      coin: 'PURR/USDC', px: '100', sz: '0.5', side: 'B', time: tid, startPosition: '0',
      dir: 'Buy', closedPnl: '0', hash: '0xfixture', oid: 900, crossed: true,
      fee: '0.01', tid, feeToken: 'USDC',
    }));
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({
      channel: 'userFills', data: { user: '0xvault', isSnapshot: true, fills },
    }) } as MessageEvent);
    expect(onFill).not.toHaveBeenCalled();
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({
      channel: 'userFills', data: { user: '0xvault', isSnapshot: false, fills },
    }) } as MessageEvent);
    expect(onFill.mock.calls.map(([fill]) => fill)).toEqual([12345, 12346].map((id) => ({
      id: String(id), orderId: '900', side: OrderSide.Buy, symbol: 'PURR/USDC', price: 100, amount: 0.5,
    })));
  });
});
