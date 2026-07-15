import { describe, expect, it, vi } from 'vitest';

import { PolymarketAPI } from '../../exchanges/polymarket/polymarket.api';
import { ENDPOINTS } from '../../exchanges/polymarket/polymarket.constants';

const credentials = { apiKey: 'api-key', secret: 'c2VjcmV0', passphrase: 'pass' };

const createApi = () =>
  new PolymarketAPI(
    {} as any,
    { name: 'polymarket', log: vi.fn() } as any,
    () => credentials,
    () => '0x1111111111111111111111111111111111111111',
  );

describe('Polymarket official REST contracts', () => {
  it('uses the current official private paths', () => {
    expect(ENDPOINTS).toMatchObject({
      OPEN_ORDERS: '/data/orders',
      GET_ORDER: '/data/order/',
      TRADES: '/data/trades',
      CANCEL: '/order',
      CANCEL_ORDERS: '/orders',
      BALANCE_ALLOWANCE: '/balance-allowance',
    });
  });

  it('paginates open orders and uses exact cancellation body shapes', async () => {
    const api = createApi();
    const get = vi
      .fn()
      .mockResolvedValueOnce({ data: { data: [{ id: 'one' }], next_cursor: 'cursor-2' } })
      .mockResolvedValueOnce({ data: { data: [{ id: 'two' }], next_cursor: 'LTE=' } });
    const deleteRequest = vi.fn().mockResolvedValue({ data: {} });
    (api as any).xhr = { get, delete: deleteRequest };

    await expect(api.getOpenOrders()).resolves.toEqual([{ id: 'one' }, { id: 'two' }]);
    expect(get).toHaveBeenNthCalledWith(1, '/data/orders', {
      params: { next_cursor: 'MA==' },
    });
    expect(get).toHaveBeenNthCalledWith(2, '/data/orders', {
      params: { next_cursor: 'cursor-2' },
    });

    await api.cancelOrder('order-1');
    await api.cancelOrders(['order-2', 'order-3']);
    expect(deleteRequest).toHaveBeenNthCalledWith(1, '/order', { data: { orderID: 'order-1' } });
    expect(deleteRequest).toHaveBeenNthCalledWith(2, '/orders', {
      data: ['order-2', 'order-3'],
    });
  });

  it('queries pUSD collateral balance with signature context', async () => {
    const api = createApi();
    const get = vi.fn().mockResolvedValue({ data: { balance: '1', allowance: '2' } });
    (api as any).xhr = { get };

    await api.getBalanceAllowance(3);

    expect(get).toHaveBeenCalledWith('/balance-allowance', {
      params: { asset_type: 'COLLATERAL', signature_type: 3 },
    });
  });

  it('serializes the official V2 order envelope before request signing', async () => {
    const api = createApi();
    const post = vi.fn().mockResolvedValue({ data: { success: true, orderID: 'id' } });
    (api as any).xhr = { post };

    await api.postOrder(
      {
        salt: '123',
        maker: '0x1111111111111111111111111111111111111111',
        signer: '0x1111111111111111111111111111111111111111',
        tokenId: '456',
        makerAmount: '1000000',
        takerAmount: '2000000',
        side: 'BUY',
        signatureType: 0,
        timestamp: '1700000000000',
        expiration: '0',
        metadata: `0x${'00'.repeat(32)}`,
        builder: `0x${'00'.repeat(32)}`,
        signature: `0x${'11'.repeat(65)}`,
      },
      'GTC',
    );

    expect(post).toHaveBeenCalledWith('/order', {
      deferExec: false,
      postOnly: false,
      order: expect.objectContaining({
        salt: 123,
        timestamp: '1700000000000',
        metadata: `0x${'00'.repeat(32)}`,
        builder: `0x${'00'.repeat(32)}`,
      }),
      owner: 'api-key',
      orderType: 'GTC',
    });
  });

  it('paginates Gamma and Data API collections with their documented limits', async () => {
    const api = createApi();
    const firstGammaPage = Array.from({ length: 500 }, (_, index) => ({ id: `${index}` }));
    const gammaGet = vi
      .fn()
      .mockResolvedValueOnce({ data: firstGammaPage })
      .mockResolvedValueOnce({ data: [{ id: 'last' }] });
    const firstPositionsPage = Array.from({ length: 500 }, (_, index) => ({ asset: `${index}` }));
    const dataGet = vi
      .fn()
      .mockResolvedValueOnce({ data: firstPositionsPage })
      .mockResolvedValueOnce({ data: [{ asset: 'last' }] });
    (api as any).gammaXhr = { get: gammaGet };
    (api as any).dataXhr = { get: dataGet };

    await expect(api.getGammaMarkets({ active: true, closed: false })).resolves.toHaveLength(501);
    expect(gammaGet).toHaveBeenNthCalledWith(2, '/markets', {
      params: { active: true, closed: false, limit: 500, offset: 500 },
    });

    await expect(
      api.getPositions('0x1111111111111111111111111111111111111111'),
    ).resolves.toHaveLength(501);
    expect(dataGet).toHaveBeenNthCalledWith(2, '/positions', {
      params: {
        user: '0x1111111111111111111111111111111111111111',
        limit: 500,
        offset: 500,
        sizeThreshold: 0,
      },
    });
  });
});
