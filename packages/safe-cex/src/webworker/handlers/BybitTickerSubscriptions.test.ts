import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../../__tests__/reference/bybit/bybit.exchange';
import { BybitInverseExchange } from '../../__tests__/reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../../__tests__/reference/bybit/bybitspot.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market } from '../../types';

const makeOptions = (activeTickerSymbols: string[] = []) => ({
  key: '',
  secret: '',
  tuning: {
    activeTickerSymbols,
  },
});

const seedMarkets = (store: DefaultStore, markets: Market[]) => {
  store.update({
    markets,
    loaded: { ...store.loaded, markets: true },
  });
};

const mockConnectedSend = (ws: { send: (message: string) => void }, connected: boolean = true) => {
  Object.defineProperty(ws, 'isConnected', {
    configurable: true,
    get: () => connected,
  });
  return vi.spyOn(ws, 'send').mockImplementation(() => {});
};

describe('Bybit ticker subscription symbol filtering', () => {
  it('filters Bybit linear symbols to active ticker hints', () => {
    const store = new DefaultStore();
    seedMarkets(store, [
      { symbol: 'BTCUSDT', quote: 'USDT' } as Market,
      { symbol: 'ETHUSDT', quote: 'USDT' } as Market,
      { symbol: 'BTCUSD', quote: 'USD' } as Market,
    ]);

    const exchange = new BybitExchange('bybit', makeOptions(['BTCUSDT']), store);

    expect(exchange.getTickerSymbolsForCategory('linear')).toEqual(['BTCUSDT']);
  });

  it('falls back to full category list when hints do not match any Bybit symbols', () => {
    const store = new DefaultStore();
    seedMarkets(store, [
      { symbol: 'BTCUSDT', quote: 'USDT' } as Market,
      { symbol: 'ETHUSDT', quote: 'USDT' } as Market,
    ]);

    const exchange = new BybitExchange('bybit', makeOptions(['NOT_REAL']), store);

    expect(exchange.getTickerSymbolsForCategory('linear')).toEqual(['BTCUSDT', 'ETHUSDT']);
  });

  it('filters Bybit spot symbols to active ticker hints', () => {
    const store = new DefaultStore();
    seedMarkets(store, [
      { symbol: 'BTCUSDT', quote: 'USDT' } as Market,
      { symbol: 'ETHUSDT', quote: 'USDT' } as Market,
    ]);

    const exchange = new BybitSpotExchange('bybitspot', makeOptions(['ETHUSDT']), store);

    expect(exchange.getTickerSymbolsForPublicStream()).toEqual(['ETHUSDT']);
  });

  it('filters Bybit inverse symbols to active ticker hints', () => {
    const store = new DefaultStore();
    seedMarkets(store, [
      { symbol: 'BTCUSD', quote: 'USD' } as Market,
      { symbol: 'ETHUSD', quote: 'USD' } as Market,
      { symbol: 'BTCUSDT', quote: 'USDT' } as Market,
    ]);

    const exchange = new BybitInverseExchange('bybitinverse', makeOptions(['ETHUSD']), store);

    expect(exchange.getTickerSymbolsForPublicStream()).toEqual(['ETHUSD']);
  });

  it('Bybit linear does not dynamically update ticker topics (subscribes to all on connect)', () => {
    const store = new DefaultStore();
    seedMarkets(store, [
      { symbol: 'BTCUSDT', quote: 'USDT' } as Market,
      { symbol: 'ETHUSDT', quote: 'USDT' } as Market,
    ]);
    const exchange = new BybitExchange('bybit', makeOptions(['BTCUSDT']), store);
    const ws = exchange.publicWebsocket;
    ws.topics.tickers = ['tickers.BTCUSDT'];

    const sendSpy = mockConnectedSend(ws);

    exchange.setActiveTickerSymbols(['BTCUSDT', 'ETHUSDT']);

    // Linear subscribes to all markets on connect, so setActiveTickerSymbols is a no-op
    expect(ws.topics.tickers).toEqual(['tickers.BTCUSDT']);
    expect(sendSpy).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it('adds Bybit spot ticker topics at runtime', () => {
    const store = new DefaultStore();
    seedMarkets(store, [
      { symbol: 'BTCUSDT', quote: 'USDT' } as Market,
      { symbol: 'ETHUSDT', quote: 'USDT' } as Market,
    ]);
    const exchange = new BybitSpotExchange('bybitspot', makeOptions(['BTCUSDT']), store);
    const ws = exchange.publicWebsocket;
    ws.topics.tickers = ['tickers.BTCUSDT'];

    const sendSpy = mockConnectedSend(ws);

    exchange.setActiveTickerSymbols(['BTCUSDT', 'ETHUSDT']);

    expect(ws.topics.tickers).toEqual(['tickers.BTCUSDT', 'tickers.ETHUSDT']);
    expect(sendSpy).toHaveBeenCalledWith(JSON.stringify({ op: 'subscribe', args: ['tickers.ETHUSDT'] }));
    exchange.dispose();
  });

  it('adds Bybit inverse ticker topics at runtime', () => {
    const store = new DefaultStore();
    seedMarkets(store, [{ symbol: 'BTCUSD', quote: 'USD' } as Market, { symbol: 'ETHUSD', quote: 'USD' } as Market]);
    const exchange = new BybitInverseExchange('bybitinverse', makeOptions(['BTCUSD']), store);
    const ws = exchange.publicInverseWebsocket;
    ws.topics.tickers = ['tickers.BTCUSD'];

    const sendSpy = mockConnectedSend(ws);

    exchange.setActiveTickerSymbols(['BTCUSD', 'ETHUSD']);

    expect(ws.topics.tickers).toEqual(['tickers.BTCUSD', 'tickers.ETHUSD']);
    expect(sendSpy).toHaveBeenCalledWith(JSON.stringify({ op: 'subscribe', args: ['tickers.ETHUSD'] }));
    exchange.dispose();
  });

  it('Bybit linear ignores setActiveTickerSymbols while disconnected', () => {
    const store = new DefaultStore();
    seedMarkets(store, [
      { symbol: 'BTCUSDT', quote: 'USDT' } as Market,
      { symbol: 'ETHUSDT', quote: 'USDT' } as Market,
    ]);
    const exchange = new BybitExchange('bybit', makeOptions(['BTCUSDT']), store);
    const ws = exchange.publicWebsocket;
    ws.topics.tickers = ['tickers.BTCUSDT'];

    const sendSpy = mockConnectedSend(ws, false);

    exchange.setActiveTickerSymbols(['BTCUSDT', 'ETHUSDT']);

    // Linear doesn't react to activeTickerSymbols changes
    expect(ws.topics.tickers).toEqual(['tickers.BTCUSDT']);
    expect(sendSpy).not.toHaveBeenCalled();
    exchange.dispose();
  });
});
