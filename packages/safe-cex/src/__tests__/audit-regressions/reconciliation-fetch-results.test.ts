import type { Balance, Order } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { applyReferenceFixture } from '../cutover/referenceFixture';
import { ApexOmniExchange } from '../reference/apexomni/apexomni.exchange';
import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { LighterExchange } from '../reference/lighter/lighter.exchange';
import { ParadexExchange } from '../reference/paradex/paradex.exchange';

const balance: Balance = { total: 100, free: 90, used: 10, upnl: 0, info: {} };
const orders = [{ id: 'order-1' }] as Order[];

const createApex = () =>
  new ApexOmniExchange(
    'apexomni',
    { key: 'key', secret: 'secret', passphrase: 'passphrase', omniKeySeed: 'seed' } as any,
    new FrameStore(),
  );

const createLighter = () =>
  new LighterExchange(
    'lighter',
    { key: 'key', secret: 'secret', apiKeyIndex: '1', applicationId: '123' } as any,
    new FrameStore(),
  );

const createParadex = () =>
  new ParadexExchange('paradex', { publicDataOnly: true, tuning: { consoleLogging: false } } as any, new FrameStore());

const createHyperliquid = () =>
  new HyperliquidExchange(
    'hyperliquid',
    { key: '0xkey', secret: '0xsecret', applicationId: '0xuser', tuning: { consoleLogging: false } },
    new FrameStore(),
  );

afterEach(() => vi.restoreAllMocks());

describe('reconciliation fetch result ownership', () => {
  it('routes Hyperliquid fire-and-forget reconciliation through its named legacy refresh wrapper', async () => {
    const exchange = createHyperliquid();
    const fetchOrders = vi.spyOn(exchange, 'fetchOrders').mockResolvedValue(orders);

    await expect((exchange as any).refreshOrders()).resolves.toBeUndefined();

    expect(fetchOrders).toHaveBeenCalledOnce();
  });

  it('keeps ApeX fetchOrders pure and commits successful private snapshots in the caller', async () => {
    const exchange = createApex();
    const commitOrders = (() => {
      const observed = vi.fn<(value: typeof exchange.store.orders) => void>();
      exchange.store.subscribe('orders', (value) => observed(value));
      return observed;
    })();
    vi.spyOn(exchange, 'mapOrder').mockReturnValue(orders);
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { data: [{ id: 'order-1' }] } }) } as any;

    await expect(exchange.fetchOrders()).resolves.toEqual(orders);
    expect(commitOrders).not.toHaveBeenCalled();

    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue(orders);
    vi.spyOn(exchange, 'fetchBalance').mockResolvedValue(balance);
    await exchange.fetchAndSetPrivateSnapshotHelper();

    expect(commitOrders).toHaveBeenCalledWith(orders);
  });

  it('leaves ApeX orders unchanged when reconciliation fails or resolves after disposal', async () => {
    const exchange = createApex();
    const commitOrders = (() => {
      const observed = vi.fn<(value: typeof exchange.store.orders) => void>();
      exchange.store.subscribe('orders', (value) => observed(value));
      return observed;
    })();
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchOrders')
      .mockRejectedValueOnce(new Error('temporary failure'))
      .mockResolvedValueOnce(orders);
    vi.spyOn(exchange, 'fetchBalance').mockResolvedValue(balance);

    await exchange.fetchAndSetPrivateSnapshotHelper();
    exchange.isDisposed = true;
    await exchange.fetchAndSetPrivateSnapshotHelper();

    expect(commitOrders).not.toHaveBeenCalled();
  });

  it('keeps Lighter fetchOrders pure and commits successful private snapshots in the caller', async () => {
    const exchange = createLighter();
    const commitOrders = (() => {
      const observed = vi.fn<(value: typeof exchange.store.orders) => void>();
      exchange.store.subscribe('orders', (value) => observed(value));
      return observed;
    })();
    (exchange as any).accountIndex = 123n;
    (exchange as any).getPrivateWsAuthToken = vi.fn().mockResolvedValue('token');
    (exchange as any).lighterApiClient = { get: vi.fn().mockResolvedValue({ orders: [] }) };

    await expect(exchange.fetchOrders()).resolves.toEqual([]);
    expect(commitOrders).not.toHaveBeenCalled();

    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue(orders);
    vi.spyOn(exchange, 'fetchBalance').mockResolvedValue(balance);
    await exchange.fetchAndSetPrivateSnapshotHelper();

    expect(commitOrders).toHaveBeenCalledWith(orders);
  });

  it('keeps Lighter private snapshots alive when order auth cannot find the account', async () => {
    const exchange = createLighter();
    applyReferenceFixture(
      exchange.store,
      { orders },
      exchange.store.markets.map((row) => row.symbol),
    );
    (exchange as any).accountIndex = 123n;
    (exchange as any).getPrivateWsAuthToken = vi.fn().mockResolvedValue('token');
    (exchange as any).lighterApiClient = {
      get: vi.fn().mockRejectedValue({
        message: 'Request failed with status code 401',
        response: {
          data: {
            code: 20013,
            message: 'invalid auth: couldnt find account',
          },
        },
      }),
    };

    await expect(exchange.fetchOrders()).resolves.toEqual(orders);
  });

  it('leaves Lighter orders unchanged when reconciliation fails or resolves after disposal', async () => {
    const exchange = createLighter();
    const commitOrders = (() => {
      const observed = vi.fn<(value: typeof exchange.store.orders) => void>();
      exchange.store.subscribe('orders', (value) => observed(value));
      return observed;
    })();
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchOrders')
      .mockRejectedValueOnce(new Error('temporary failure'))
      .mockResolvedValueOnce(orders);
    vi.spyOn(exchange, 'fetchBalance').mockResolvedValue(balance);

    await exchange.fetchAndSetPrivateSnapshotHelper();
    exchange.isDisposed = true;
    await exchange.fetchAndSetPrivateSnapshotHelper();

    expect(commitOrders).not.toHaveBeenCalled();
  });

  it('commits Paradex WS-triggered position fetches only after a successful live response', async () => {
    const exchange = createParadex();
    const positions = [{ id: 'BTC-USD-PERP-long' }] as any;
    const commitPositions = (() => {
      const observed = vi.fn<(value: typeof exchange.store.positions) => void>();
      exchange.store.subscribe('positions', (value) => observed(value));
      return observed;
    })();
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValueOnce(positions).mockRejectedValueOnce(new Error('temporary'));

    await (exchange.privateWebsocket as any).executePositionFetch();
    await (exchange.privateWebsocket as any).executePositionFetch();

    expect(commitPositions).toHaveBeenCalledOnce();
    expect(commitPositions).toHaveBeenCalledWith(positions);
  });

  it('does not commit a late Paradex position response after disposal', async () => {
    const exchange = createParadex();
    const commitPositions = (() => {
      const observed = vi.fn<(value: typeof exchange.store.positions) => void>();
      exchange.store.subscribe('positions', (value) => observed(value));
      return observed;
    })();
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([{ id: 'BTC-USD-PERP-long' }] as any);
    exchange.isDisposed = true;

    await (exchange.privateWebsocket as any).executePositionFetch();

    expect(commitPositions).not.toHaveBeenCalled();
  });
});
