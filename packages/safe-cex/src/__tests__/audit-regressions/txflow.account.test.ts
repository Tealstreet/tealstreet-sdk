import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TxflowAccount } from '../reference/txflow/txflow.account';
import { DefaultStore } from '../../store/store.base';
import type { Market } from '../../types';

const NOW = 1_700_000_000_000;
const WALLET = '0x0000000000000000000000000000000000000001';
const MARKET: Market = { id: '1', symbol: 'BTC-USDC', base: 'BTC', quote: 'USDC', active: true, precision: { amount: 0.001, price: 0.1 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } } };
const STATE = { time: NOW, marginSummary: { accountValue: '1000', totalMarginUsed: '20', totalRawUsd: '995' }, assetPositions: [{ type: 'oneWay', position: { asset: '1', szi: '2', leverage: { type: 'Cross', value: 10 }, positionValue: '200', unrealizedPnl: '5', marginUsed: '20', entryPx: '97.5', liquidationPx: '10', updateTime: NOW } }] };
const ORDER = { asset: 1, oid: 42, side: 'B', orderType: 'Limit', limitPx: '100', origSz: '2', sz: '1.5', timestamp: NOW, isTrigger: false, reduceOnly: false, cloid: 'caller-id', state: 'Open', tif: 'GTC', children: [] };
function create() {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const emitFill = vi.fn();
  const reportError = vi.fn();
  let fills: unknown[] = [];
  const requestInfo = vi.fn(async <T>(body: Record<string, unknown>): Promise<T> => {
    expect(body.user).toBe(WALLET);
    const response = body.type === 'clearinghouseState' ? structuredClone(STATE) : body.type === 'frontendOpenOrders' ? [ORDER] : body.type === 'userFills' ? { data: fills, hasMore: false } : { coin: '1', user: WALLET, leverage: { type: 'Cross', mode: 'oneWay', value: 10 } };
    return response as T;
  });
  const account = new TxflowAccount({ store, emitFill, reportError, requestInfo: <T>(body: Record<string, unknown>) => requestInfo(body) as Promise<T>, wallet: () => WALLET, marketById: (id) => id === '1' ? MARKET : undefined, disposed: () => false });
  return { account, store, emitFill, reportError, requestInfo, setFills: (rows: unknown[]) => { fills = rows; } };
}
describe('TxFlow account state from documented native payloads', () => {
  beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(NOW); });
  afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });
  it('maps signed size and equity units without zero configuration positions', async () => {
    const { account, store } = create();
    await account.start();
    expect(store.positions[0]).toMatchObject({ contracts: 2, side: 'long', entryPrice: 97.5 });
    expect(store.balance).toMatchObject({ total: 1000, free: 980, used: 20, upnl: 5 });
    expect(store.fetchedLeverage['BTC-USDC']).toMatchObject({ buyLeverage: 10 });
    account.nodeFrame(JSON.stringify({ channel: 'clearinghouseState', data: { ...STATE, assetPositions: [] } }));
    expect(store.positions.filter((p) => p.contracts > 0)).toHaveLength(0);
    expect(store.fetchedLeverage['BTC-USDC'].buyLeverage).toBe(10);
    account.dispose();
  });
  it('fences old REST snapshots behind stream events, terminal orders and disconnects', async () => {
    const { account, store, requestInfo } = create();
    await account.start();
    let release: (value: unknown) => void = () => undefined;
    requestInfo.mockImplementationOnce(<T>() => new Promise<T>((resolve) => { release = resolve as (v: unknown) => void; }));
    const refresh = account.refresh();
    account.nodeFrame(JSON.stringify({ channel: 'clearinghouseState', data: { ...STATE, assetPositions: [{ ...STATE.assetPositions[0], position: { ...STATE.assetPositions[0].position, szi: '3' } }] } }));
    release(STATE);
    await refresh;
    expect(store.positions[0].contracts).toBe(3);
    account.nodeFrame(JSON.stringify({ channel: 'orderUpdates', data: [{ order: ORDER, status: 'Filled', statusTimestamp: NOW + 1000 }] }));
    account.nodeFrame(JSON.stringify({ channel: 'orderUpdates', data: [{ order: ORDER, status: 'Open', statusTimestamp: NOW + 2000 }] }));
    expect(store.orders).toHaveLength(0);
    expect(account.orderIds.get('caller-id')).toBe('42');
    account.disconnected();
    account.dispose();
  });
  it('preserves last good account scope after failed or malformed reconciliation', async () => {
    const { account, store, requestInfo, reportError } = create();
    await account.start();
    requestInfo.mockRejectedValue(new Error('offline'));
    await account.refresh();
    expect(store.balance.total).toBe(1000);
    expect(store.positions[0].contracts).toBe(2);
    expect(store.orders).toHaveLength(1);
    expect(reportError).toHaveBeenCalled();
    account.nodeFrame(JSON.stringify({ channel: 'clearinghouseState', data: { ...STATE, assetPositions: [{ type: 'oneWay', position: { ...STATE.assetPositions[0].position, szi: 'NaN' } }] } }));
    expect(store.positions[0].contracts).toBe(2);
    account.dispose();
  });
  it('emits native execution amount once per tid and suppresses initial/api history', async () => {
    const { account, emitFill, setFills } = create();
    const execution = { tid: 11, oid: 42, instrumentId: 1, px: '102', sz: '0.5', side: 'B', time: NOW + 1000 };
    setFills([{ ...execution, tid: 10, time: NOW - 1000 }]);
    await account.start();
    expect(emitFill).not.toHaveBeenCalled();
    setFills([execution, execution]);
    await account.refresh();
    expect(emitFill).toHaveBeenCalledExactlyOnceWith({ id: '42', symbol: 'BTC-USDC', side: 'buy', price: 102, amount: 0.5 });
    account.disconnected();
    await account.refresh();
    account.apiFrame(JSON.stringify({ channel: 'userFills', data: { fills: [execution], isSnapshot: true } }));
    expect(emitFill).toHaveBeenCalledTimes(1);
    account.dispose();
  });
  it('updates market settings without manufacturing exposure', async () => {
    const { account, store } = create();
    await account.start();
    account.nodeFrame(JSON.stringify({ channel: 'clearinghouseState', data: { ...STATE, assetPositions: [] } }));
    account.nodeFrame(JSON.stringify({ channel: 'userAssetData', data: { user: WALLET, coins: [{ coin: '1', leverage: { type: 'Isolated', value: 7 } }] } }));
    expect(store.positions.filter((p) => p.contracts > 0)).toHaveLength(0);
    expect(account.confirmedSetting('BTC-USDC')).toMatchObject({ buyLeverage: 7, marginType: 'isolated' });
    account.dispose();
  });
  it('emits an execution returned by the initial read when it occurred after startup', async () => {
    const { account, emitFill, setFills } = create();
    setFills([{ tid: 99, oid: 42, instrumentId: 1, px: '123', sz: '0.5', side: 'B', time: NOW + 50 }]);
    await account.start();
    expect(emitFill).toHaveBeenCalledExactlyOnceWith({ id: '42', symbol: 'BTC-USDC', side: 'buy', price: 123, amount: 0.5 });
    account.dispose();
  });
  it('learns flat hedge mode from account settings and preserves directional leverage', async () => {
    const { account, store, requestInfo } = create();
    requestInfo.mockImplementation(async <T>(body: Record<string, unknown>): Promise<T> => {
      const value = body.type === 'clearinghouseState' ? { ...STATE, assetPositions: [] } : body.type === 'frontendOpenOrders' ? [] : body.type === 'userFills' ? { data: [], hasMore: false } : { coin: '1', user: WALLET, leverage: { type: 'Isolated', mode: 'hedge', long_value: 5, short_value: 8 } };
      return value as T;
    });
    await account.start();
    expect(store.options.isHedged).toBe(true);
    expect(store.positions).toHaveLength(0);
    expect(account.confirmedSetting('BTC-USDC')).toMatchObject({ buyLeverage: 5, sellLeverage: 8 });
    account.dispose();
  });

});
