import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { BybitInverseExchange } from '../../exchanges/bybit/bybitinverse.exchange';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { DefaultStore } from '../../store/store.base';

const createVariants = () => [
  new BybitExchange('bybit', { key: 'key', secret: 'secret' }, new DefaultStore()),
  new BybitInverseExchange('bybitinverse', { key: 'key', secret: 'secret' }, new DefaultStore()),
];

describe('Bybit UTA account-wide margin mode', () => {
  it.each([
    [true, 'ISOLATED_MARGIN'],
    [false, 'REGULAR_MARGIN'],
  ] as const)('maps isolated=%s to %s without per-symbol payload fields', async (isolated, setMarginMode) => {
    for (const exchange of createVariants()) {
      const post = vi.fn().mockResolvedValue({ data: { retCode: 0, retMsg: 'OK', result: { reasons: [] } } });
      const fetchPositions = vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
      exchange.xhr = { post } as any;

      await exchange.changeMarginType(isolated, 'BTCUSDT');

      expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_MARGIN_MODE, { setMarginMode });
      expect(fetchPositions).toHaveBeenCalledWith();
    }
  });

  it('surfaces Bybit reason details when the account-level switch is rejected', async () => {
    const exchange = createVariants()[0];
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: {
          retCode: 3400045,
          retMsg: 'Set margin mode failed',
          result: { reasons: [{ reasonCode: '3400000', reasonMsg: 'Equity must meet the account minimum' }] },
        },
      }),
    } as any;

    await expect(exchange.changeMarginType(true)).rejects.toThrow('Equity must meet the account minimum');
    expect(emitAndLogError).toHaveBeenCalledWith('Equity must meet the account minimum');
  });
});
