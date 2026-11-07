import { describe, expect, it, vi } from 'vitest';

import { CoinbaseSpotExchange } from '../reference/coinbase/coinbasespot.exchange';
import { ENDPOINTS } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';

const makeExchange = () =>
  new CoinbaseSpotExchange(
    'coinbasespot' as any,
    { key: 'api-key', secret: 'api-secret', tealstreetProxy: false, tealstreetTunnel: false } as any,
    new DefaultStore(),
  );

const makeSpotProduct = () => ({
  product_id: 'BTC-USD',
  price_increment: '0.01',
  base_increment: '0.00000001',
  base_min_size: '0.00000001',
  base_max_size: '1000',
  base_currency_id: 'BTC',
  quote_currency_id: 'USD',
  status: 'TRADING',
  price: '100000',
  price_percentage_change_24h: '1',
  volume_24h: '2',
});

describe('Coinbase Spot stays isolated from futures endpoints', () => {
  it('selects a non-INTX portfolio without probing CFM balance support', async () => {
    const exchange = makeExchange();
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          portfolios: [
            { name: 'Perps', uuid: 'intx-portfolio', type: 'INTX', deleted: false },
            { name: 'Default', uuid: 'spot-portfolio', type: 'DEFAULT', deleted: false },
          ],
        },
      })
      .mockResolvedValueOnce({ data: { breakdown: { spot_positions: [] } } });
    exchange.xhr = { get } as any;

    await expect(exchange.findSelectedPortfolio()).resolves.toMatchObject({
      uuid: 'spot-portfolio',
      type: 'DEFAULT',
    });
    expect(get).toHaveBeenCalledTimes(2);
    expect(get).not.toHaveBeenCalledWith(ENDPOINTS.BALANCE_CFM);
    expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.PORTFOLIO_BREAKDOWN, {
      params: { portfolio_uuid: 'spot-portfolio' },
    });
  });

  it('does not re-list portfolios during private startup after spot portfolio selection', async () => {
    const exchange = makeExchange();
    exchange.selectedPortfolio = { name: 'Default', uuid: 'spot-portfolio', type: 'DEFAULT', deleted: false };
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => undefined);
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);
    const findSelectedPortfolio = vi.spyOn(exchange, 'findSelectedPortfolio');

    await exchange._startPrivate();

    expect(findSelectedPortfolio).not.toHaveBeenCalled();
  });

  it('preserves a portfolio-list 429 as a rate-limit failure instead of reporting missing permissions', async () => {
    const exchange = makeExchange();
    const rateLimitError = new Error('GET /api/v3/brokerage/portfolios failed with 429') as Error & {
      response?: { status: number };
    };
    rateLimitError.response = { status: 429 };
    const get = vi.fn().mockRejectedValue(rateLimitError);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    exchange.xhr = { get } as any;

    await expect(exchange.findSelectedPortfolio()).rejects.toThrow('GET /api/v3/brokerage/portfolios failed with 429');
    expect(emitAndLogError.mock.calls[0]?.[0]).toBe(rateLimitError);
    expect(
      emitAndLogError.mock.calls.some(([message]) => message === 'No Coinbase spot portfolio with permissions found'),
    ).toBe(false);
  });

  it('preserves a portfolio-breakdown 429 instead of treating it as a permissions miss', async () => {
    const exchange = makeExchange();
    const rateLimitError = new Error('GET /api/v3/brokerage/portfolios/{portfolio_uuid} failed with 429') as Error & {
      response?: { status: number };
    };
    rateLimitError.response = { status: 429 };
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          portfolios: [{ name: 'Default', uuid: 'spot-portfolio', type: 'DEFAULT', deleted: false }],
        },
      })
      .mockRejectedValueOnce(rateLimitError);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    exchange.xhr = { get } as any;

    await expect(exchange.findSelectedPortfolio()).rejects.toThrow(
      'GET /api/v3/brokerage/portfolios/{portfolio_uuid} failed with 429',
    );
    expect(emitAndLogError.mock.calls[0]?.[0]).toBe(rateLimitError);
    expect(
      emitAndLogError.mock.calls.some(([message]) => message === 'No Coinbase spot portfolio with permissions found'),
    ).toBe(false);
  });

  it('fetches only spot products for ticker search instead of failing the whole market load on CFM 429s', async () => {
    const exchange = makeExchange();
    exchange.selectedPortfolio = { name: 'Default', uuid: 'spot-portfolio', type: 'DEFAULT', deleted: false };
    const get = vi.fn().mockResolvedValue({ data: { products: [makeSpotProduct()] } });
    exchange.xhr = { get } as any;

    const markets = await exchange.fetchMarkets();

    expect(markets.map((market) => market.symbol)).toEqual(['BTC-USD']);
    expect(get).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledWith(ENDPOINTS.MARKETS, { params: {} });
    expect(get).not.toHaveBeenCalledWith(ENDPOINTS.BALANCE_CFM);
  });

  it('fetches spot balances without calling the CFM balance endpoint', async () => {
    const exchange = makeExchange();
    exchange.selectedPortfolio = { name: 'Default', uuid: 'spot-portfolio', type: 'DEFAULT', deleted: false };
    const get = vi.fn().mockResolvedValue({
      data: {
        breakdown: {
          spot_positions: [
            {
              asset: 'BTC',
              total_balance_crypto: 1.5,
              available_to_trade_crypto: 1,
            },
          ],
        },
      },
    });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchBalance()).resolves.toMatchObject({
      currencies: {
        BTC: {
          total: 1,
          free: 1,
          used: 0.5,
          upnl: 0,
        },
      },
    });
    expect(get).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledWith(ENDPOINTS.PORTFOLIO_BREAKDOWN, {
      params: { portfolio_uuid: 'spot-portfolio' },
    });
    expect(get).not.toHaveBeenCalledWith(ENDPOINTS.BALANCE_CFM);
  });
});
