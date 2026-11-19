import { expect, it } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from '../cutover/nativeFixture';

const account = {
  accountType: 'UNIFIED',
  totalEquity: '1010.5',
  totalWalletBalance: '1000.5',
  coin: [{ coin: 'USDT', equity: '1010.5', marginCollateral: true, collateralSwitch: true }],
};

it.each(['bybit', 'bybitdemo', 'hyrotrader', 'bybitinverse', 'bybitspot'])(
  '%s wallet metadata crosses only native balance.info sections and survives independent balance updates',
  () => {
    const store = new FrameStore();
    store.ingest(nativeFixture({ balance: { used: 1, free: 2, total: 3, upnl: 0, info: account } }));
    expect(store.balance.info).toEqual(account);
    const info = store.balance.info;
    store.ingest(nativeFixture({ balance: { used: 2, free: 3, total: 5, upnl: 0 } }));
    expect(store.balance.info).toBe(info);
    expect(store.balance.total).toBe(5);
    store.ingest(nativeFixture({ balance: { ...store.balance, info: { ...account, accountType: 'CONTRACT' } } }));
    expect(store.balance.info.accountType).toBe('CONTRACT');
  },
);

it('clears native wallet metadata with an explicit null', () => {
  const store = new FrameStore();
  store.ingest(nativeFixture({ balance: { used: 0, free: 1, total: 1, upnl: 0, info: account } }));
  store.ingest(nativeFixture({ balance: { ...store.balance, info: null } }));
  expect(store.balance.info).toBeNull();
});
