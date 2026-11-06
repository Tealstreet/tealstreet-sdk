import type { Market, Ticker } from '../../types';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { HyperliquidSpotExchange } from '../reference/hyperliquid/hyperliquidspot.exchange';
import { KrakenSpotExchange } from '../reference/kraken/krakenspot.exchange';
import { KrakenSpotPublicWebsocket } from '../reference/kraken/krakenspot.ws-public';
import { KrakenSpotPrivateWebsocket } from '../reference/kraken/krakenspot.ws-private';
import { normalizeAsset } from '../reference/kraken/krakenspot.types';
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
  it.each([[4, 0.0001], [5, 0.00001], [8, 0.00000001]])('retains the exact Kraken decimal step for %i places', async (decimals, step) => {
    const exchange = new KrakenSpotExchange('krakenspot', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    exchanges.push(exchange);
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { error: [], result: {
      XXBTZUSD: { wsname: 'XBT/USD', pair_decimals: decimals, lot_decimals: decimals, ordermin: '0.001', status: 'online' },
    } } });
    const markets = await exchange.fetchMarkets();
    expect(markets[0].precision).toEqual({ price: step, amount: step });
  });
  it('normalizes Kraken XDG asset aliases to accepted WS v2 DOGE symbols', () => {
    expect(normalizeAsset('XDG')).toBe('DOGE');
    expect(normalizeAsset('XXDG')).toBe('DOGE');
    const row = market({ id: 'DOGE/USD', symbol: 'XDG/USD', base: 'DOGE' });
    const store = new DefaultStore();
    store.update({ markets: [row], tickers: [ticker(row)] });
    const exchange = new KrakenSpotExchange('krakenspot', { key: '', secret: '', publicDataOnly: true }, store);
    exchanges.push(exchange);
    const socket = new KrakenSpotPublicWebsocket(exchange);
    exchange.publicWebsocket = socket;
    Object.defineProperty(socket, 'isConnected', { get: () => true });
    const send = vi.spyOn(socket, 'send').mockImplementation(() => {});
    socket.listenOrderBook('XDG/USD', () => {});
    expect(JSON.parse(send.mock.calls[0][0]).params.symbol).toEqual(['DOGE/USD']);
    socket.onMessage({ data: JSON.stringify({ channel: 'ticker', type: 'update', data: [{ symbol: 'DOGE/USD', last: 0.1 }] }) } as MessageEvent);
    expect(store.tickersBySymbol['XDG/USD'].last).toBe(0.1);
  });
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
      { tokens: [{ name: 'USDC', index: 0, szDecimals: 8 }, { name: 'PURR', index: 1, szDecimals: 0 }], universe: [{ name: 'PURR/USDC', index: 0, tokens: [1, 0] }] },
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

  it('never joins mixed or differently sized Hyperliquid spot contexts positionally', async () => {
    const exchange = new HyperliquidSpotExchange('hyperliquidspot', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    exchanges.push(exchange);
    const post = vi.spyOn(exchange.xhr, 'post');
    const meta = { tokens: [{ name: 'USDC', index: 0, szDecimals: 8 }, { name: 'PURR', index: 1, szDecimals: 0 }], universe: [{ name: 'PURR/USDC', index: 0, tokens: [1, 0] }] };
    const read = () => (exchange as unknown as { fetchMarketsAndTickers(): Promise<[Market[], Ticker[]]> }).fetchMarketsAndTickers();
    post.mockResolvedValue({ data: [meta, [{ midPx: '900' }, { coin: '@other', midPx: '800' }]] });
    expect(await read()).toEqual([[], []]);
    post.mockResolvedValue({ data: [meta, [{ midPx: '900' }, { midPx: '800' }]] });
    expect(await read()).toEqual([[], []]);
  });

  it('keeps Hyperliquid native pairs distinct when their base token is shared', async () => {
    const exchange = new HyperliquidSpotExchange('hyperliquidspot', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    exchanges.push(exchange);
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: [
      { tokens: [{ name: 'USDC', index: 0, szDecimals: 8 }, { name: 'HYPE', index: 150, szDecimals: 2 }, { name: 'USDH', index: 360, szDecimals: 8 }],
        universe: [{ name: '@107', index: 107, tokens: [150, 0] }, { name: '@232', index: 232, tokens: [150, 360] }] },
      [{ coin: '@232', midPx: '80', markPx: '80' }, { coin: '@107', midPx: '79', markPx: '79' }],
    ] });
    const [markets, tickers] = await (exchange as unknown as { fetchMarketsAndTickers(): Promise<[Market[], Ticker[]]> }).fetchMarketsAndTickers();
    expect(markets.map(({ id, symbol, quote }) => ({ id, symbol, quote }))).toEqual([
      { id: '@107', symbol: 'HYPE/USDC', quote: 'USDC' }, { id: '@232', symbol: 'HYPE/USDH', quote: 'USDH' },
    ]);
    expect(tickers.map(({ symbol, last }) => [symbol, last])).toEqual([['HYPE/USDC', 79], ['HYPE/USDH', 80]]);
  });

  it('converts Hyperliquid quote units to USD, prefers direct USDC and preserves unpriced holdings', () => {
    const store = new DefaultStore();
    const quoted = market({ id: '@1', symbol: 'FOO/BAR', base: 'FOO', quote: 'BAR', info: { tokens: [1, 2] } });
    const conversion = market({ id: '@2', symbol: 'BAR/USDC', base: 'BAR', quote: 'USDC', info: { tokens: [2, 0] } });
    const rows = [quoted, conversion];
    store.update({ markets: rows, tickers: rows.map((row, i) => ({ ...ticker(row), last: i ? 2 : 3, mark: i ? 2 : 3 })) });
    const exchange = new HyperliquidSpotExchange('hyperliquidspot', { key: '', secret: '', publicDataOnly: true }, store);
    exchanges.push(exchange);
    const balances = [{ coin: 'FOO', token: 1, total: '2', hold: '1' }, { coin: 'UNPRICED', total: '7', hold: '0' }];
    exchange.applySpotBalances(balances);
    expect(store.balance).toMatchObject({ total: 12, used: 6, free: 6, coins: { UNPRICED: { amount: 7, notional: 0 } } });
    const direct = market({ id: '@3', symbol: 'FOO/USDC', base: 'FOO', quote: 'USDC', info: { tokens: [1, 0] } });
    store.update({ markets: [...rows, direct], tickers: [...store.tickers, { ...ticker(direct), last: 8, mark: 8 }] });
    exchange.applySpotBalances(balances);
    expect(store.balance).toMatchObject({ total: 16, used: 8, free: 8 });
  });

  it('never values a Hyperliquid token through a cyclic path, including native balance aliases', () => {
    const rows = [
      market({ id: '@1', symbol: 'A/B', base: 'A', quote: 'B', info: { tokens: [1, 2] } }),
      market({ id: '@2', symbol: 'A/USDH', base: 'A', quote: 'USDH', info: { tokens: [1, 3] } }),
      market({ id: '@3', symbol: 'B/A', base: 'B', quote: 'A', info: { tokens: [2, 1] } }),
    ];
    const store = new DefaultStore();
    store.update({ markets: rows, tickers: rows.map((row, i) => ({ ...ticker(row), mark: [2, 3, 4][i], last: [2, 3, 4][i] })) });
    const exchange = new HyperliquidSpotExchange('hyperliquidspot', { key: '', secret: '', publicDataOnly: true }, store);
    exchanges.push(exchange);
    for (const coin of ['A', 'NATIVE_A_ALIAS']) {
      exchange.applySpotBalances([{ coin, token: 1, total: '1', hold: '0' }]);
      expect(store.balance.total).toBe(3);
    }
    store.update({ markets: [rows[0], rows[2]], tickers: [store.tickers[0], store.tickers[2]] });
    exchange.applySpotBalances([{ coin: 'A', token: 1, total: '1', hold: '0' }]);
    expect(store.balance).toMatchObject({ total: 0, coins: { A: { amount: 1, notional: 0 } } });
  });

  it.each([
    { actual_price: 91, price: 5, price_type: 'pct' },
    { price: 91, price_type: 'static' },
  ])('retains Kraken v2 trigger prices and SDK symbols from current native payloads: %j', (triggers) => {
    const store = new DefaultStore();
    store.update({ markets: [market()] });
    const exchange = new KrakenSpotExchange('krakenspot', { key: '', secret: '', publicDataOnly: true }, store);
    exchanges.push(exchange);
    const socket = new KrakenSpotPrivateWebsocket(exchange);
    exchange.privateWebsocket = socket;
    const order = (socket as unknown as { parseOrder(item: object): { price: number; symbol: string } }).parseOrder({
      order_id: 'O1', symbol: 'BTC/USD', order_type: 'stop-loss', side: 'sell',
      order_qty: 1, cum_qty: 0, triggers,
    });
    expect(order).toMatchObject({ price: 91, symbol: 'XBT/USD' });
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
