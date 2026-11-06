vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';

const ACCOUNT = {
  accountType: 'UNIFIED',
  totalEquity: '1010.5',
  totalWalletBalance: '1000.5',
  coin: [{ coin: 'USDT', equity: '1010.5', marginCollateral: true, collateralSwitch: true }],
};

type Boundary = {
  command: (input: unknown, options?: unknown) => Promise<unknown>;
  syncBalanceInfo: () => Promise<void>;
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
      await boundary.syncBalanceInfo();
      expect(store.balance.info).toEqual({ stale: true });
      await boundary.syncBalanceInfo();
      expect(store.balance.info).toEqual(ACCOUNT);
      expect(store.balance.total).toBe(3);
      const update = vi.spyOn(store, 'update');
      await boundary.syncBalanceInfo();
      await boundary.syncBalanceInfo();
      expect(update).not.toHaveBeenCalled();
      expect(store.balance.info).toEqual(ACCOUNT);
      expect(command).toHaveBeenCalledWith({ op: 'fetchBalanceInfo' }, { report: false });
    } finally {
      exchange.dispose();
    }
  },
);

it('frame-forwarding workers send balance.info to main when it changes, since frames carry none', () => {
  const worker = new ExchangeWorker(
    { postMessage: vi.fn() } as unknown as Worker,
    'account',
    'bybit',
    { key: '', secret: '' },
    {} as never,
    {} as never,
    'session',
  );
  const derive = (worker as unknown as { _derivedStorePayload: (data: unknown, force?: boolean) => Record<string, unknown> })
    ._derivedStorePayload;
  const store = new DefaultStore();
  try {
    expect(derive({ balance: { ...store.balance } })).not.toHaveProperty('balance');
    const withInfo = { balance: { used: 1, free: 2, total: 3, upnl: 0, info: ACCOUNT } };
    expect(derive(withInfo).balance).toEqual(withInfo.balance);
    expect(derive(withInfo)).not.toHaveProperty('balance');
    expect(derive(withInfo, true).balance).toEqual(withInfo.balance);
    const changed = { balance: { ...withInfo.balance, info: { ...ACCOUNT, accountType: 'CONTRACT' } } };
    expect(derive(changed).balance).toEqual(changed.balance);
  } finally {
    worker.dispose();
  }
});
