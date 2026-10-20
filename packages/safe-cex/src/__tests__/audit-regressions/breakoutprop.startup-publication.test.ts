import type { Market, OrderBook, Ticker } from '../../types';
import { AxiosError } from 'axios';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BreakoutpropExchange } from '../../exchanges/breakoutprop/breakoutprop.exchange';
import { DefaultStore } from '../../store/store.base';

const symbol = 'ETHUSD';
const market: Market = { id: symbol, symbol, base: 'ETH', quote: 'USD', active: true,
  precision: { amount: 0.01, price: 0.01 }, limits: { amount: { min: 0.01, max: 100 }, leverage: { min: 1, max: 5 } } };
const ticker = (bid = 100): Ticker => ({ id: symbol, symbol, bid, ask: bid + 1, last: bid, mark: bid, index: bid,
  percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0 });
const users = { userDetails: [{ accounts: ['00002', '00003'].map((id) => ({ account: `default:${id}`, accountStatus: 'FULL_TRADING' })) }] };
const deferred = <T>() => {
  let resolve!: (value: T) => void, reject!: (error: Error) => void;
  const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
};
const running: BreakoutpropExchange[] = [];
const setup = () => {
  const exchange = new BreakoutpropExchange('breakoutprop', {
    key: 'user', secret: 'password', publicDataOnly: true, subaccount: '00002', extra: { tickInterval: Infinity },
  }, new DefaultStore());
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  running.push(exchange); return exchange;
};
const session = (exchange: BreakoutpropExchange) => {
  exchange.sessionToken = 'session'; exchange.sessionTokenExpiresAt = Date.now() + 60_000;
};
const startup = (exchange: BreakoutpropExchange) => {
  vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue([market]);
  return vi.spyOn(exchange, 'fetchTickers').mockResolvedValue([ticker()]);
};
beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(1_800_000_000_000); });
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks();
});

describe('Breakout startup discovery and publication', () => {
  it('uses five actual requests in the first 5-RPS window and publishes initial quotes to existing books', async () => {
    const exchange = setup(), begin = Date.now(), sent: Array<[string, number]> = [];
    exchange.xhr.defaults.adapter = async (config) => {
      const path = config.url!; sent.push([path, Date.now() - begin]);
      const data = path === '/login' ? { sessionToken: 'session', timeout: '00:30:00' }
        : path === '/users/user' ? users
          : path === 'instruments/query' ? { instruments: [{ symbol, priceIncrement: 0.01, lotSize: 1 }] }
            : path.endsWith('/instruments/query') ? { instrumentDetails: [{ symbol, tradingStatus: 'FULL', minOrderSize: 0.01,
              minOrderSizeIncrement: 0.01, maxOrderSize: { value: 100 }, marginRate: { tiers: [{ value: 0.2 }] } }] }
              : path === 'marketdata' ? { events: [{ symbol, type: 'Quote', bid: 100, ask: 101 }] }
                : (() => { throw new Error(`Unexpected ${path}`); })();
      return { data, config, headers: {}, status: 200, statusText: 'OK' };
    };
    const books: OrderBook[] = [];
    exchange.listenOrderBook(symbol, (book) => books.push(structuredClone(book)));
    let ready = false;
    const started = exchange.start().then(() => { ready = true; });
    await vi.advanceTimersByTimeAsync(100);
    expect(sent.filter(([path]) => path === '/users/user'), JSON.stringify(sent)).toHaveLength(1);
    expect(sent).toHaveLength(5); expect(sent.every(([, time]) => time < 1000)).toBe(true);
    expect(ready).toBe(true); await started;
    expect(books).toEqual([{ bids: [{ price: 100, amount: 1, total: 1 }], asks: [{ price: 101, amount: 1, total: 1 }] }]);
  });

  it('shares a failed discovery and releases it for retry', async () => {
    const exchange = setup(); session(exchange);
    const pending = deferred<{ data: typeof users }>();
    const get = vi.spyOn(exchange.xhr, 'get').mockImplementation(() => pending.promise);
    const result = Promise.allSettled([exchange.getAccountCode(), exchange.getAccountCode()]);
    await vi.advanceTimersByTimeAsync(0); expect(get).toHaveBeenCalledTimes(1);
    pending.reject(new Error('discovery unavailable'));
    expect((await result).map((row) => row.status)).toEqual(['rejected', 'rejected']);
    get.mockResolvedValue({ data: users });
    await expect(exchange.getAccountCode()).resolves.toBe('default:00002'); expect(get).toHaveBeenCalledTimes(2);
  });

  it('cannot commit an obsolete selection or clear a newer pending lookup', async () => {
    const exchange = setup(); session(exchange);
    const old = deferred<{ data: typeof users }>(), next = deferred<{ data: typeof users }>();
    const get = vi.spyOn(exchange.xhr, 'get').mockImplementationOnce(() => old.promise).mockImplementation(() => next.promise);
    const first = exchange.getAccountCode().catch((error) => error);
    await vi.advanceTimersByTimeAsync(0); exchange.options = { ...exchange.options, subaccount: '00003' };
    const second = exchange.getAccountCode(); await vi.advanceTimersByTimeAsync(0);
    old.resolve({ data: users }); expect(await first).toBeInstanceOf(Error); expect(exchange.accountCode).toBeNull();
    const third = exchange.getAccountCode(); await vi.advanceTimersByTimeAsync(0); expect(get).toHaveBeenCalledTimes(2);
    next.resolve({ data: users });
    expect(await Promise.all([second, third])).toEqual(['default:00003', 'default:00003']); expect(exchange.accountCode).toBe('default:00003');
  });

  it('rejects a superseded lookup when selection changes away and back before its reply', async () => {
    const exchange = setup(); session(exchange);
    const a = deferred<{ data: typeof users }>(), b = deferred<{ data: typeof users }>(), fresh = deferred<{ data: typeof users }>();
    const get = vi.spyOn(exchange.xhr, 'get').mockImplementationOnce(() => a.promise).mockImplementationOnce(() => b.promise).mockImplementation(() => fresh.promise);
    const first = exchange.getAccountCode().catch((error) => error); await vi.advanceTimersByTimeAsync(0);
    exchange.options = { ...exchange.options, subaccount: '00003' };
    const second = exchange.getAccountCode().catch((error) => error); await vi.advanceTimersByTimeAsync(0);
    exchange.options = { ...exchange.options, subaccount: '00002' };
    const third = exchange.getAccountCode(); await vi.advanceTimersByTimeAsync(0);
    a.resolve({ data: users }); b.resolve({ data: users });
    expect(await first).toBeInstanceOf(Error); expect(await second).toBeInstanceOf(Error);
    expect(exchange.accountCode).toBeNull();
    const fourth = exchange.getAccountCode(); await vi.advanceTimersByTimeAsync(0);
    expect(get).toHaveBeenCalledTimes(3);
    fresh.resolve({ data: users });
    expect(await Promise.all([third, fourth])).toEqual(['default:00002', 'default:00002']);
  });

  it('rejects disposed discovery without committing or requesting again', async () => {
    const exchange = setup(); session(exchange);
    const pending = deferred<{ data: typeof users }>();
    const get = vi.spyOn(exchange.xhr, 'get').mockImplementation(() => pending.promise);
    const result = exchange.getAccountCode().catch((error) => error); await vi.advanceTimersByTimeAsync(0);
    exchange.dispose(); pending.resolve({ data: users });
    expect(await result).toBeInstanceOf(Error); expect(exchange.accountCode).toBeNull();
    await expect(exchange.getAccountCode()).rejects.toThrow(/disposed/i); expect(get).toHaveBeenCalledTimes(1);
  });

  it('preserves transparent session refresh during a shared lookup', async () => {
    const exchange = setup(); session(exchange); let reads = 0, logins = 0;
    exchange.xhr.defaults.adapter = async (config) => {
      if (config.url === '/login') { logins++; return { data: { sessionToken: 'fresh', timeout: '00:30:00' }, config, headers: {}, status: 200, statusText: 'OK' }; }
      reads++;
      if (reads === 1) throw new AxiosError('unauthorized', 'ERR_BAD_REQUEST', config, undefined, { config, data: {}, headers: {}, status: 401, statusText: 'Unauthorized' });
      return { data: users, config, headers: {}, status: 200, statusText: 'OK' };
    };
    const result = Promise.all([exchange.getAccountCode(), exchange.getAccountCode()]); await vi.advanceTimersByTimeAsync(100);
    expect(await result).toEqual(['default:00002', 'default:00002']); expect(reads).toBe(2); expect(logins).toBe(1);
    expect(exchange.sessionToken).toBe('fresh');
  });

  it('seeds existing listeners only and preserves later book and candle polling', async () => {
    const exchange = setup(), fetch = startup(exchange), removed = vi.fn(), late = vi.fn(), candle = vi.fn();
    let changed = false, unsubscribe = () => {};
    const existing = vi.fn(() => { if (!changed) { changed = true; unsubscribe(); exchange.listenOrderBook(symbol, late); } });
    exchange.listenOrderBook(symbol, existing); unsubscribe = exchange.listenOrderBook(symbol, removed);
    exchange.listenOHLCV({ symbol, interval: '1m' }, candle);
    await exchange.start(); expect(existing).toHaveBeenCalledTimes(1);
    expect(removed).not.toHaveBeenCalled(); expect(late).not.toHaveBeenCalled(); expect(candle).not.toHaveBeenCalled();
    fetch.mockResolvedValue([ticker(102)]); await exchange.tick();
    expect(existing).toHaveBeenCalledTimes(2); expect(late).toHaveBeenCalledTimes(1); expect(candle).toHaveBeenCalledTimes(1);
  });

  it('does not seed a late listener registered by the initial store update', async () => {
    const exchange = setup(); startup(exchange); const late = vi.fn(), existing = vi.fn();
    exchange.listenOrderBook(symbol, existing); let subscribed = false;
    exchange.store.subscribe((data) => { if (data.loaded.tickers && !subscribed) { subscribed = true; exchange.listenOrderBook(symbol, late); } });
    await exchange.start(); expect(existing).toHaveBeenCalledTimes(1); expect(late).not.toHaveBeenCalled();
  });

  it('stops initial dispatch when an earlier listener disposes the exchange', async () => {
    const exchange = setup(); startup(exchange); const later = vi.fn();
    exchange.listenOrderBook(symbol, () => exchange.dispose()); exchange.listenOrderBook(symbol, later);
    await exchange.start(); expect(exchange.isDisposed).toBe(true); expect(later).not.toHaveBeenCalled();
  });

  it('does not arm a poll or mark account state loaded when the initial store notification disposes it', async () => {
    const exchange = setup(); startup(exchange);
    exchange.options.extra!.tickInterval = 2000;
    exchange.store.subscribe((data) => { if (data.loaded.tickers) exchange.dispose(); });
    await exchange.start();
    expect(exchange.store.loaded.balance).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });

  it.each(['dispose', 'selection'] as const)('ignores initial quotes after %s changes ownership', async (change) => {
    const exchange = setup(); startup(exchange); const pending = deferred<Ticker[]>();
    vi.mocked(exchange.fetchTickers).mockReturnValue(pending.promise); const listener = vi.fn(); exchange.listenOrderBook(symbol, listener);
    const result = exchange.start(); await vi.advanceTimersByTimeAsync(0);
    if (change === 'dispose') exchange.dispose(); else exchange.options = { ...exchange.options, subaccount: '00003' };
    pending.resolve([ticker()]); await result; expect(listener).not.toHaveBeenCalled(); expect(exchange.store.tickers).toEqual([]);
  });

  it('does not present cached fallback quotes as a successful initial response', async () => {
    const exchange = setup(); startup(exchange); exchange.store.update({ tickers: [ticker()] });
    vi.mocked(exchange.fetchTickers).mockResolvedValue(exchange.store.tickers);
    const listener = vi.fn(); exchange.listenOrderBook(symbol, listener); await exchange.start(); expect(listener).not.toHaveBeenCalled();
  });
});
