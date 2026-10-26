import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType } from '../../types';

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
      const post = vi.fn().mockResolvedValue({
        data: { retCode: 0, retMsg: 'Request accepted', result: { reasons: [] } },
      });
      const fetchPositions = vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
      exchange.xhr = { post } as any;

      await exchange.changeMarginType(isolated, 'BTCUSDT');

      expect(post).toHaveBeenCalledWith(ENDPOINTS.SET_MARGIN_MODE, { setMarginMode });
      expect(fetchPositions).toHaveBeenCalledWith();
    }
  });

  // Captured from the live venue on 2026-10-09, on an account already in
  // REGULAR_MARGIN: setting REGULAR_MARGIN answered "Request accepted", not
  // "OK". Requiring "OK" threw before marginType was updated or positions
  // were refetched, so the UI kept the old mode until a reload.
  it('treats the live success answer ("Request accepted") as success on both variants', async () => {
    for (const exchange of createVariants()) {
      (exchange as any).marginType = MarginType.Isolated;
      const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
      const fetchPositions = vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
      exchange.xhr = {
        post: vi.fn().mockResolvedValue({ data: { retCode: 0, retMsg: 'Request accepted', result: { reasons: [] } } }),
      } as any;

      await expect(exchange.changeMarginType(false)).resolves.toBeUndefined();
      expect(emitAndLogError).not.toHaveBeenCalled();
      expect((exchange as any).marginType).toBe(MarginType.Cross);
      expect(fetchPositions).toHaveBeenCalledWith();
    }
  });

  it('still fails a zero retCode that carries reasons', async () => {
    const exchange = createVariants()[0];
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: { retCode: 0, retMsg: 'Request accepted', result: { reasons: [{ reasonMsg: 'Open orders exist' }] } },
      }),
    } as any;

    await expect(exchange.changeMarginType(true)).rejects.toThrow('Open orders exist');
    expect(emitAndLogError).toHaveBeenCalledWith('Open orders exist');
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
