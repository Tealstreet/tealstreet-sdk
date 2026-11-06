import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../reference/phemex/phemex.exchange';
import { ENDPOINTS } from '../../exchanges/phemex/phemex.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, PositionMode, PositionSide } from '../../types';

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
  it.each([
    [20, 30],
    [-20, -30],
    [20, -30],
    [-20, 30],
  ])('keeps REST and WS balances equivalent with USDT/USDC UPnL %s/%s', async (usdtPnl, usdcPnl) => {
    const store = new DefaultStore();
    store.update({
      markets: [
        { id: 'BTCUSDT', symbol: 'BTCUSDT', quote: 'USDT', info: { settleCurrency: 'USDT' } },
        { id: 'ETHUSDC', symbol: 'ETHUSDC', quote: 'USDC', info: { settleCurrency: 'USDC' } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 100 },
        { symbol: 'ETHUSDC', last: 200 },
      ],
    } as any);
    const exchange = new PhemexExchange(
      'phemex',
      {
        key: 'test',
        secret: 'test',
        extra: { tickInterval: Infinity },
      } as any,
      store,
    );
    const rows = [
      { symbol: 'BTCUSDT', currency: 'USDT', size: '2', avgEntryPriceRp: String(100 - usdtPnl / 2) },
      { symbol: 'ETHUSDC', currency: 'USDC', size: '3', avgEntryPriceRp: String(200 - usdcPnl / 3) },
    ].map((row) => ({
      ...row,
      side: 'Buy',
      posSide: 'Long',
      posMode: 'Hedged',
      leverageRr: '10',
      liquidationPriceRp: '0',
    }));
    let usdtBalance = 1000;
    let failUsdc = false;
    exchange.xhr = {
      get: vi.fn((_endpoint: string, { params: { currency } }: { params: { currency: 'USDT' | 'USDC' } }) => {
        if (currency === 'USDC' && failUsdc) return Promise.reject(new Error('USDC unavailable'));
        return Promise.resolve(
          createAccountResponse(
            currency,
            String(currency === 'USDT' ? usdtBalance : 200),
            currency === 'USDT' ? '100' : '20',
            rows.filter((row) => row.currency === currency),
          ),
        );
      }),
    } as any;
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    const expected = {
      total: 1200 - usdtPnl - usdcPnl,
      free: 1080,
      used: 120,
      upnl: usdtPnl + usdcPnl,
      currencies: {
        USDT: { total: 1000 - usdtPnl, free: 900, used: 100, upnl: usdtPnl },
        USDC: { total: 200 - usdcPnl, free: 180, used: 20, upnl: usdcPnl },
      },
    };
    try {
      const snapshot = await exchange.fetchPositionsAndBalance();
      expect(snapshot.balance).toMatchObject(expected);
      expect(
        exchange.privateWebsocket.commitAccountSnapshot(
          exchange.privateWebsocket.accountStateRevision,
          snapshot.balance,
          snapshot.positions,
        ),
      ).toBe(true);
      exchange.privateWebsocket.onMessage({
        data: JSON.stringify({
          positions_p: rows,
          accounts_p: [
            { currency: 'USDT', accountBalanceRv: '1000', totalUsedBalanceRv: '100' },
            { currency: 'USDC', accountBalanceRv: '200', totalUsedBalanceRv: '20' },
          ],
        }),
      } as MessageEvent);
      expect(store.balance).toMatchObject(expected);
      // A new authoritative read after a push must not change the convention.
      await exchange.tick();
      expect(store.balance).toMatchObject(expected);
      usdtBalance = 1100;
      failUsdc = true;
      await exchange.tick();
      expect(store.balance).toMatchObject({
        ...expected,
        total: expected.total + 100,
        free: expected.free + 100,
        currencies: {
          USDT: { ...expected.currencies.USDT, total: 1100 - usdtPnl, free: 1000 },
          USDC: expected.currencies.USDC,
        },
      });
      expect(store.positions.map((position) => position.contracts).sort()).toEqual([2, 3]);
    } finally {
      exchange.dispose();
    }
  });

  it('maps one-way Buy positions from lowercase side and ignores zero-size rows', () => {
    const store = new DefaultStore();
    store.update({
      markets: [{ id: 'ADA/USDT:USDT', symbol: 'ADAUSDT', info: { symbol: 'ADAUSDT' } }] as any,
      tickers: [{ id: 'ADA/USDT:USDT', symbol: 'ADAUSDT', last: 0.189 }] as any,
    });
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);

    const position = exchange.mapPosition({
      symbol: 'ADAUSDT',
      side: 'Buy',
      posSide: 'Merged',
      posMode: 'OneWay',
      size: '53.02',
      avgEntryPriceRp: '0.1887',
      leverageRr: '10',
      liquidationPriceRp: '0.1',
    } as any);

    expect(position).toMatchObject({
      symbol: 'ADAUSDT',
      side: PositionSide.Long,
      positionMode: PositionMode.OneWay,
      contracts: 53.02,
    });
    expect(
      exchange.mapPosition({
        symbol: 'ADAUSDT',
        side: 'Buy',
        posSide: 'Merged',
        posMode: 'OneWay',
        size: '0',
        avgEntryPriceRp: '0.1887',
        leverageRr: '10',
        liquidationPriceRp: '0.1',
      } as any),
    ).toBeUndefined();
  });

  it('maps nonzero positions from raw mark price when the ticker cache is missing', () => {
    const store = new DefaultStore();
    store.update({
      markets: [{ id: 'ADA/USDT:USDT', symbol: 'ADAUSDT', info: { symbol: 'ADAUSDT' } }] as any,
      tickers: [],
    });
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);

    const position = exchange.mapPosition({
      symbol: 'ADAUSDT',
      side: 'Buy',
      posSide: 'Merged',
      posMode: 'OneWay',
      size: '106.02',
      avgEntryPriceRp: '0.188749962',
      markPriceRp: '0.1887',
      leverageRr: '-10',
      liquidationPriceRp: '0.0001',
    } as any);

    expect(position).toMatchObject({
      symbol: 'ADAUSDT',
      side: PositionSide.Long,
      contracts: 106.02,
      positionMode: PositionMode.OneWay,
    });
    expect(position?.unrealizedPnl).toBeCloseTo((0.1887 - 0.188749962) * 106.02);
  });

  it('maps one-way Sell positions from symbol-scoped recovery even when market indexes lag', () => {
    const store = new DefaultStore();
    store.update({ markets: [], tickers: [] });
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);

    const position = exchange.mapPosition({
      symbol: 'ADAUSDT',
      side: 'Sell',
      posSide: 'Merged',
      posMode: 'OneWay',
      size: '52.91',
      avgEntryPrice: '0.189',
      avgEntryPriceRp: '0.189',
      markPriceRp: '0.1891',
      leverageRr: '-10',
      liquidationPriceRp: '1.8436',
    } as any);

    expect(position).toMatchObject({
      id: 'ADAUSDT',
      symbol: 'ADAUSDT',
      side: PositionSide.Short,
      positionMode: PositionMode.OneWay,
      contracts: 52.91,
      entryPrice: 0.189,
      leverage: 10,
    });
    expect(position?.unrealizedPnl).toBeCloseTo((0.189 - 0.1891) * 52.91);
  });

  it('uses zero-size raw one-way position rows to choose merged order position side', async () => {
    const store = new DefaultStore();
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);
    const get = vi.fn().mockResolvedValue(
      createAccountResponse('USDT', '88', '0', [
        {
          symbol: 'ADAUSDT',
          side: 'None',
          posSide: 'Merged',
          posMode: 'OneWay',
          size: '0',
          avgEntryPriceRp: '0',
          markPriceRp: '0.1885',
          leverageRr: '-10',
          liquidationPriceRp: '0',
        },
      ]),
    );
    exchange.xhr = { get } as any;

    await expect(
      (exchange as any).getOrderPositionSide({
        symbol: 'ADAUSDT',
        side: OrderSide.Buy,
        type: OrderType.Market,
        reduceOnly: false,
      }),
    ).resolves.toBe('Merged');
    expect(get).toHaveBeenCalledWith(ENDPOINTS.POSITIONS, {
      params: { currency: 'USDT', symbol: 'ADAUSDT' },
    });
  });

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
    expect(addOrUpdatePositionsFull.mock.calls[0][0].map(({ symbol }) => symbol).sort()).toEqual([
      'BTCUSDC',
      'BTCUSDT',
    ]);
    expect(
      store.positions
        .map(({ symbol, contracts }) => ({ symbol, contracts }))
        .sort((a, b) => a.symbol.localeCompare(b.symbol)),
    ).toEqual([
      { symbol: 'BTCUSDC', contracts: 3 },
      { symbol: 'BTCUSDT', contracts: 2 },
    ]);
    expect(store.balance).toMatchObject({
      total: 100,
      used: 15,
      free: 135,
      upnl: 50,
      currencies: {
        USDT: { total: 80, used: 10, free: 90 },
        USDC: { total: 20, used: 5, free: 45 },
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

    expect(
      store.positions
        .map(({ symbol, contracts }) => ({ symbol, contracts }))
        .sort((a, b) => a.symbol.localeCompare(b.symbol)),
    ).toEqual([
      { symbol: 'BTCUSDC', contracts: 3 },
      { symbol: 'BTCUSDT', contracts: 2 },
    ]);
    expect(store.balance).toMatchObject({
      total: 130,
      used: 15,
      free: 135,
      currencies: {
        USDT: { total: 80, used: 10, free: 90 },
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
