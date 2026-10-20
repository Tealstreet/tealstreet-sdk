import type { Market, Ticker } from '../../types';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { HyperliquidSpotExchange } from '../../exchanges/hyperliquid/hyperliquidspot.exchange';
import { KrakenSpotExchange } from '../../exchanges/kraken/krakenspot.exchange';
import { KrakenSpotPublicWebsocket } from '../../exchanges/kraken/krakenspot.ws-public';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const exchanges: Array<HyperliquidSpotExchange | KrakenSpotExchange> = [];
afterEach(() => { exchanges.splice(0).forEach((exchange) => exchange.dispose()); vi.restoreAllMocks(); });
const market = (changes: Partial<Market> = {}): Market => ({
  id: 'BTC/USD', symbol: 'XBT/USD', base: 'BTC', quote: 'USD', active: true,
  isSpot: true, precision: { amount: 0.00001, price: 0.1 },
  limits: { amount: { min: 0.00001, max: 1_000_000 }, leverage: { min: 1, max: 1 } },
  ...changes,
});
const ticker = (row: Market): Ticker => ({
  id: row.id, symbol: row.symbol, bid: 99, ask: 101, last: 100, mark: 100, index: 100,
  percentage: 0, volume: 0, quoteVolume: 0, fundingRate: 0, fundingTime: 0, openInterest: 0,
});

describe('documented spot public payloads and native placement acknowledgements', () => {
  it('maps Kraken WS v2 BTC back to REST XBT and removes levels falling beyond subscribed depth', () => {
    const store = new DefaultStore();
    const row = market();
    store.update({ markets: [row], tickers: [ticker(row)] });
    const exchange = new KrakenSpotExchange('krakenspot', { key: '', secret: '', publicDataOnly: true }, store);
    exchanges.push(exchange);
    const socket = new KrakenSpotPublicWebsocket(exchange);
    exchange.publicWebsocket = socket;
    Object.defineProperty(socket, 'isConnected', { get: () => true });
    const send = vi.spyOn(socket, 'send').mockImplementation(() => {});
    const books: Array<{ bids: Array<{ price: number }> }> = [];
    socket.listenOrderBook('XBT/USD', (book) => books.push(book));
    expect(JSON.parse(send.mock.calls[0][0]).params.symbol).toEqual(['BTC/USD']);
    const deliver = (channel: string, type: string, data: object) => socket.onMessage({ data: JSON.stringify({ channel, type, data: [data] }) } as MessageEvent);
    deliver('ticker', 'update', { symbol: 'BTC/USD', last: 110 });
    expect(store.tickersBySymbol['XBT/USD'].last).toBe(110);
    deliver('book', 'snapshot', {
      symbol: 'BTC/USD', bids: Array.from({ length: 100 }, (_, i) => ({ price: 100 - i, qty: 1 })), asks: [{ price: 120, qty: 1 }],
    });
    deliver('book', 'update', { symbol: 'BTC/USD', bids: [{ price: 101, qty: 1 }], asks: [] });
    deliver('book', 'update', { symbol: 'BTC/USD', bids: [{ price: 101, qty: 0 }], asks: [] });
    expect(books.at(-1)!.bids).toHaveLength(99);
    expect(books.at(-1)!.bids.some((level) => level.price === 1)).toBe(false);
  });

  it('loads Hyperliquid spot contexts without coin using documented universe order', async () => {
    const exchange = new HyperliquidSpotExchange('hyperliquidspot', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    exchanges.push(exchange);
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: [
      { tokens: [{ name: 'PURR', index: 1, szDecimals: 0 }], universe: [{ name: 'PURR/USDC', index: 0, tokens: [1, 0] }] },
      [{ midPx: '0.209265', markPx: '0.14', prevDayPx: '0.20432', dayNtlVlm: '8906.0' }],
    ] });
    const [markets, tickers] = await (exchange as unknown as {
      fetchMarketsAndTickers(): Promise<[Market[], Ticker[]]>;
    }).fetchMarketsAndTickers();
    expect(markets.map((row) => [row.id, row.symbol])).toEqual([['PURR/USDC', 'PURR/USDC']]);
    expect(tickers[0].last).toBe(0.209265);
  });

  it('values Kraken XXBT holdings through the normalized market identity and REST-facing XBT symbol', async () => {
    const store = new DefaultStore();
    const row = market();
    store.update({ markets: [row], tickers: [ticker(row)] });
    const exchange = new KrakenSpotExchange('krakenspot', { key: '', secret: '', publicDataOnly: true }, store);
    exchanges.push(exchange);
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: { error: [], result: { XXBT: '2', ZUSD: '5' } } });
    await expect(exchange.fetchBalance()).resolves.toMatchObject({ total: 205, free: 205, currencies: { BTC: { total: 2 } } });
  });

  it.each(['resting', 'filled'] as const)('returns string IDs from Hyperliquid spot %s acknowledgements', async (status) => {
    const row = market({ id: '@1', symbol: 'HFUN/USDC', base: 'HFUN', quote: 'USDC', info: { hyperliquidId: 10001 } });
    const store = new DefaultStore();
    store.update({ markets: [row], tickers: [ticker(row)] });
    const exchange = new HyperliquidSpotExchange('hyperliquidspot', { key: '', secret: `0x${'11'.repeat(32)}` }, store);
    exchanges.push(exchange);
    vi.spyOn(exchange, 'requestSpotBalanceRefresh').mockImplementation(() => {});
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: { status: 'ok', response: { data: { statuses: [{ [status]: { oid: 42 } }] } } } });
    await expect(exchange.placeOrder({ symbol: row.symbol, side: OrderSide.Buy, type: OrderType.Market, amount: 1 })).resolves.toEqual(['42']);
  });
});
