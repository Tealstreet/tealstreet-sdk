import { describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { BASE_URL, BASE_WS_URL, ENDPOINTS } from '../../exchanges/binance/binance.types';
import { BASE_FAPI_URL, BASE_PAPI_URL, BASE_WS_URL as PM_BASE_WS_URL } from '../../exchanges/binance/binancepm.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../../types';

const market = {
  id: 'BTC/USDT:USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 125 } },
};

describe('Binance USD-M V3 account contract', () => {
  it('uses the current V3 account, balance, and position-risk endpoints', () => {
    expect(ENDPOINTS.ACCOUNT).toBe('/fapi/v3/account');
    expect(ENDPOINTS.BALANCE).toBe('/fapi/v3/balance');
    expect(ENDPOINTS.POSITION_RISK).toBe('/fapi/v3/positionRisk');
    expect(ENDPOINTS.SYMBOL_CONFIG).toBe('/fapi/v1/symbolConfig');
  });

  it('uses the documented USD-M demo REST and split websocket hosts', () => {
    expect(BASE_URL.testnet).toBe('https://demo-fapi.binance.com');
    expect(BASE_WS_URL).toMatchObject({
      public: { testnet: 'wss://demo-fstream.binance.com/public/ws' },
      market: { testnet: 'wss://demo-fstream.binance.com/market/ws' },
      private: { testnet: 'wss://demo-fstream.binance.com/private/ws' },
    });
    expect(BASE_FAPI_URL.testnet).toBe('https://demo-fapi.binance.com');
    expect(BASE_PAPI_URL.testnet).toBe('https://demo-fapi.binance.com');
    expect(PM_BASE_WS_URL.public.testnet).toBe('wss://demo-fstream.binance.com/public/ws');
    expect(PM_BASE_WS_URL.market.testnet).toBe('wss://demo-fstream.binance.com/market/ws');
    expect(PM_BASE_WS_URL.private.testnet).toBe('wss://demo-fstream.binance.com/pm/ws');
  });

  it('joins V3 position risk with symbol config and synthesizes omitted zero positions', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [market, { ...market, id: 'ETH/USDT:USDT', symbol: 'ETHUSDT', base: 'ETH' }],
      options: { isHedged: false },
    });
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);
    vi.spyOn(exchange.xhr, 'get').mockImplementation(async (url: string) => {
      if (url === ENDPOINTS.POSITION_RISK) {
        return {
          data: [
            {
              symbol: 'BTCUSDT',
              positionSide: 'BOTH',
              positionAmt: '-0.01',
              entryPrice: '50000',
              markPrice: '49000',
              unRealizedProfit: '10',
              liquidationPrice: '70000',
              isolatedMargin: '0',
              notional: '-490',
              isolatedWallet: '0',
              positionInitialMargin: '49',
              updateTime: 1,
            },
          ],
        } as any;
      }
      if (url === ENDPOINTS.SYMBOL_CONFIG) {
        return {
          data: [
            { symbol: 'BTCUSDT', marginType: 'CROSSED', leverage: 10 },
            { symbol: 'ETHUSDT', marginType: 'ISOLATED', leverage: 5 },
          ],
        } as any;
      }
      throw new Error(`unexpected endpoint ${url}`);
    });

    const positions = await exchange.fetchPositions();

    expect(positions).toHaveLength(2);
    expect(positions.find((position) => position.side === PositionSide.Short)).toMatchObject({
      contracts: 0.01,
      notional: 490,
      leverage: 10,
      marginSize: 49,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
    });
    expect(positions.find((position) => position.symbol === 'ETHUSDT')).toMatchObject({
      contracts: 0,
      leverage: 5,
      marginType: MarginType.Isolated,
    });
  });

  it('does not overwrite rich V3 position-risk state with compact account rows', async () => {
    const store = new DefaultStore();
    const richPosition = {
      id: 'BTCUSDT:short:oneway',
      symbol: 'BTCUSDT',
      side: PositionSide.Short,
      entryPrice: 50_000,
      contracts: 0.01,
      notional: 490,
      leverage: 10,
      unrealizedPnl: 10,
      liquidationPrice: 70_000,
    };
    store.update({ positions: [richPosition] });
    const exchange = new BinanceExchange('binance', { key: 'key', secret: 'secret' }, store);
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: {
        totalWalletBalance: '100',
        availableBalance: '75',
        totalUnrealizedProfit: '10',
        totalInitialMargin: '25',
        assets: [{ asset: 'USDT', walletBalance: '100', availableBalance: '75', unrealizedProfit: '10', initialMargin: '25' }],
        positions: [{ symbol: 'BTCUSDT', positionSide: 'BOTH', positionAmt: '-0.01', unrealizedProfit: '10' }],
      },
    } as any);

    const result = await exchange.fetchPositionsAndBalance();

    expect(result.positions).toBe(store.positions);
    expect(result.positions[0]).toEqual(richPosition);
    expect(result.balance).toMatchObject({ total: 100, free: 75, used: 25, upnl: 10 });
  });
});
