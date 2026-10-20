import type { ExchangeName } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const ACCOUNT = {
  accountType: 'UNIFIED',
  totalEquity: '1010.5',
  totalWalletBalance: '1000.5',
  coin: [{ coin: 'USDT', equity: '1010.5', marginCollateral: true, collateralSwitch: true }],
};

type Boundary = {
  command: (input: unknown, options?: unknown) => Promise<unknown>;
  syncBybitBalanceInfo: () => Promise<void>;
};

it.each<ExchangeName>(['bybit', 'bybitdemo', 'hyrotrader', 'bybitinverse', 'bybitspot'])(
  '%s exposes the raw wallet account as balance.info, replacing it like TS',
  async (name) => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange(name, { key: 'k', secret: 's' }, store);
    const boundary = exchange as unknown as Boundary;
    const command = vi
      .spyOn(boundary, 'command')
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(ACCOUNT)
      .mockResolvedValueOnce({ ...ACCOUNT })
      .mockRejectedValueOnce(new Error('engine busy'));
    try {
      store.update({ balance: { used: 1, free: 2, total: 3, upnl: 0, info: { stale: true } } });
      await boundary.syncBybitBalanceInfo();
      expect(store.balance.info).toEqual({ stale: true });
      await boundary.syncBybitBalanceInfo();
      expect(store.balance.info).toEqual(ACCOUNT);
      expect(store.balance.total).toBe(3);
      const update = vi.spyOn(store, 'update');
      await boundary.syncBybitBalanceInfo();
      await boundary.syncBybitBalanceInfo();
      expect(update).not.toHaveBeenCalled();
      expect(store.balance.info).toEqual(ACCOUNT);
      expect(command).toHaveBeenCalledWith({ op: 'fetchBalanceInfo' }, { report: false });
    } finally {
      exchange.dispose();
    }
  },
);
