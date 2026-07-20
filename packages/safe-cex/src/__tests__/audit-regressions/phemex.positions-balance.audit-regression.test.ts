import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../../exchanges/phemex/phemex.exchange';
import { ENDPOINTS } from '../../exchanges/phemex/phemex.types';
import { DefaultStore } from '../../store/store.base';

const createAccountResponse = (currency: 'USDT' | 'USDC', balance: string, used: string, positions: any[]) => ({
  data: {
    code: 0,
    data: {
      account: {
        currency,
        accountBalanceRv: balance,
        totalUsedBalanceRv: used,
      },
      positions,
    },
  },
});

describe('Phemex positions and balance audit regression', () => {
  it('merges USDT and USDC account snapshots before the full position-store replacement', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        { id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', info: { symbol: 'BTCUSDT' } },
        { id: 'BTC/USDC:USDC', symbol: 'BTCUSDC', info: { symbol: 'BTCUSDC' } },
      ] as any,
      tickers: [
        { id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', last: 110 },
        { id: 'BTC/USDC:USDC', symbol: 'BTCUSDC', last: 190 },
      ] as any,
    });
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);
    const get = vi.fn((_endpoint: string, config: { params: { currency: 'USDT' | 'USDC' } }) => {
      if (config.params.currency === 'USDT') {
        return Promise.resolve(
          createAccountResponse('USDT', '100', '10', [
            {
              symbol: 'BTCUSDT',
              size: '2',
              avgEntryPriceRp: '100',
              leverageRr: '10',
              posSide: 'Long',
              posMode: 'Hedged',
              liquidationPriceRp: '50',
            },
          ]),
        );
      }

      return Promise.resolve(
        createAccountResponse('USDC', '50', '5', [
          {
            symbol: 'BTCUSDC',
            size: '3',
            avgEntryPriceRp: '200',
            leverageRr: '10',
            posSide: 'Short',
            posMode: 'Hedged',
            liquidationPriceRp: '250',
          },
        ]),
      );
    });
    exchange.xhr = { get } as any;

    const originalAddOrUpdatePositionsFull = store.addOrUpdatePositionsFull;
    const addOrUpdatePositionsFull = vi.spyOn(store, 'addOrUpdatePositionsFull').mockImplementation((positions) => {
      originalAddOrUpdatePositionsFull(positions);
      exchange.isDisposed = true;
    });

    await exchange.tick();

    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT, { params: { currency: 'USDT' } });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT, { params: { currency: 'USDC' } });
    expect(addOrUpdatePositionsFull).toHaveBeenCalledTimes(1);
    expect(addOrUpdatePositionsFull.mock.calls[0][0].map(({ symbol }) => symbol).sort()).toEqual(['BTCUSDC', 'BTCUSDT']);
    expect(store.positions.map(({ symbol, contracts }) => ({ symbol, contracts })).sort((a, b) => a.symbol.localeCompare(b.symbol))).toEqual([
      { symbol: 'BTCUSDC', contracts: 3 },
      { symbol: 'BTCUSDT', contracts: 2 },
    ]);
    expect(store.balance).toMatchObject({
      total: 150,
      used: 15,
      free: 135,
      upnl: 50,
      currencies: {
        USDT: { total: 100, used: 10, free: 90 },
        USDC: { total: 50, used: 5, free: 45 },
      },
    });
  });

  it('applies a successful USDT snapshot without clearing the last USDC state when USDC polling fails', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        { id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', quote: 'USDT', info: { symbol: 'BTCUSDT', settleCurrency: 'USDT' } },
        { id: 'BTC/USDC:USDC', symbol: 'BTCUSDC', quote: 'USDC', info: { symbol: 'BTCUSDC', settleCurrency: 'USDC' } },
      ] as any,
      tickers: [
        { id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', last: 110 },
        { id: 'BTC/USDC:USDC', symbol: 'BTCUSDC', last: 190 },
      ] as any,
      balance: {
        total: 50,
        used: 5,
        free: 45,
        upnl: 0,
        currencies: { USDC: { total: 50, used: 5, free: 45, upnl: 0 } },
      },
    });
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);
    const previousUsdcPosition = exchange.mapPosition({
      symbol: 'BTCUSDC',
      size: '3',
      avgEntryPriceRp: '200',
      leverageRr: '10',
      posSide: 'Short',
      posMode: 'Hedged',
      liquidationPriceRp: '250',
      currency: 'USDC',
    } as any)!;
    store.addOrUpdatePositionsFull([previousUsdcPosition]);

    const get = vi.fn((_endpoint: string, config: { params: { currency: 'USDT' | 'USDC' } }) => {
      if (config.params.currency === 'USDT') {
        return Promise.resolve(
          createAccountResponse('USDT', '100', '10', [
            {
              symbol: 'BTCUSDT',
              size: '2',
              avgEntryPriceRp: '100',
              leverageRr: '10',
              posSide: 'Long',
              posMode: 'Hedged',
              liquidationPriceRp: '50',
            },
          ]),
        );
      }
      return Promise.reject(new Error('USDC account temporarily unavailable'));
    });
    exchange.xhr = { get } as any;
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    const originalAddOrUpdatePositionsFull = store.addOrUpdatePositionsFull;
    vi.spyOn(store, 'addOrUpdatePositionsFull').mockImplementation((positions) => {
      originalAddOrUpdatePositionsFull(positions);
      exchange.isDisposed = true;
    });

    await exchange.tick();

    expect(store.positions.map(({ symbol, contracts }) => ({ symbol, contracts })).sort((a, b) => a.symbol.localeCompare(b.symbol))).toEqual([
      { symbol: 'BTCUSDC', contracts: 3 },
      { symbol: 'BTCUSDT', contracts: 2 },
    ]);
    expect(store.balance).toMatchObject({
      total: 150,
      used: 15,
      free: 135,
      currencies: {
        USDT: { total: 100, used: 10, free: 90 },
        USDC: { total: 50, used: 5, free: 45 },
      },
    });
  });

  it('keeps the sibling settlement balance when a private account update arrives', () => {
    const store = new DefaultStore();
    store.update({
      balance: {
        total: 150,
        used: 15,
        free: 135,
        upnl: 0,
        currencies: {
          USDT: { total: 100, used: 10, free: 90, upnl: 0, info: { currency: 'USDT' } },
          USDC: { total: 50, used: 5, free: 45, upnl: 0, info: { currency: 'USDC' } },
        },
      },
    });
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        accounts_p: [
          {
            currency: 'USDC',
            accountBalanceRv: '70',
            totalUsedBalanceRv: '7',
          },
        ],
      }),
    } as MessageEvent);

    expect(store.balance).toMatchObject({
      total: 170,
      used: 17,
      free: 153,
      currencies: {
        USDT: { total: 100, used: 10, free: 90 },
        USDC: { total: 70, used: 7, free: 63 },
      },
    });
  });

  it('fails closed for an unavailable settlement account with no prior balance slice', async () => {
    const store = new DefaultStore();
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);
    exchange.xhr = {
      get: vi.fn((_endpoint: string, config: { params: { currency: 'USDT' | 'USDC' } }) =>
        config.params.currency === 'USDT'
          ? Promise.resolve(createAccountResponse('USDT', '100', '10', []))
          : Promise.reject(new Error('USDC account temporarily unavailable')),
      ),
    } as any;
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    const { balance } = await exchange.fetchPositionsAndBalance();

    expect(balance).toMatchObject({
      total: 100,
      used: 10,
      free: 90,
      currencies: {
        USDT: { total: 100, used: 10, free: 90 },
        USDC: { total: 0, used: 0, free: 0 },
      },
    });
  });
});
