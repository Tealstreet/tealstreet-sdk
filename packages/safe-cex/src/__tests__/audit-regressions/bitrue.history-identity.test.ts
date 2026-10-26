import { afterEach, describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market } from '../../types';

const market: Market = {
  id: 'E-BTC-USDT',
  symbol: 'E-BTC-USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  contractSize: 0.001,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
  info: { multiplier: 0.001 },
};
const row = {
  tradeId: '1558124009467904992',
  bidId: '1558124009467904993',
  askId: '1558124009467904994',
  contractName: market.symbol,
  side: 'BUY',
  price: 60000,
  qty: 3,
  amount: 180,
  fee: '-0.1',
  ctime: 1700000000123,
};
const make = (rows: unknown[]) => {
  vi.useFakeTimers();
  const store = new DefaultStore();
  store.update({ markets: [market] });
  const exchange = new BitrueExchange('bitrue', { key: 'key', secret: 'secret' }, store);
  vi.spyOn(exchange as any, 'emitAndLogError').mockImplementation(() => undefined);
  (exchange as any).xhr.get = vi.fn().mockResolvedValue({ data: { code: '0', data: rows } });
  return exchange;
};
afterEach(() => vi.useRealTimers());
describe('Bitrue execution history identity and completeness', () => {
  it.each([undefined, null, '', 0, -1, {}, true, 9007199254740992])(
    'rejects unresolved or coerced own order ID %j',
    async (bidId) => {
      const exchange = make([{ ...row, bidId }]);
      await expect(exchange.fetchHistoricalFills({ symbol: market.symbol })).rejects.toThrow(
        'Malformed Bitrue order ID',
      );
    },
  );
  it.each([undefined, null, '', 0, {}, true, 9007199254740992])(
    'rejects malformed execution ID %j',
    async (tradeId) => {
      const exchange = make([{ ...row, tradeId }]);
      await expect(exchange.fetchHistoricalFills({ symbol: market.symbol })).rejects.toThrow(
        'Malformed Bitrue trade ID',
      );
    },
  );
  it('retains exact native identity and follows the owning side only', async () => {
    const exchange = make([{ ...row, side: 'SELL' }]);
    const fills = await exchange.fetchHistoricalFills({ symbol: market.symbol });
    expect(fills).toHaveLength(1);
    expect(fills[0]).toMatchObject({ id: row.tradeId, orderId: row.askId, qty: 0.003 });
  });
  it('reports a full filtered page as incomplete instead of claiming an empty history', async () => {
    const exchange = make([row]);
    await expect(exchange.fetchHistoricalFills({ symbol: market.symbol, orderId: '999', limit: 1 })).rejects.toThrow(
      'INCOMPLETE:',
    );
  });
});
