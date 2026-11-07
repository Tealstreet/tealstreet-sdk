import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { OrderFillEvent } from '../../types';

import { expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { OrderSide } from '../../types';

it.each([false, true])(
  'delivers a quantified Spot cancel ACK racing execution once to worker and main stores (reused=%s)',
  async (reused) => {
    let privateMessage: ((text: string) => void) | undefined;
    let releaseCancel: (() => void) | undefined;
    const workerFills: OrderFillEvent[] = [];
    const mainFills: OrderFillEvent[] = [];
    const errors: unknown[] = [];
    const raw = (value: unknown) => ({
      status: 200,
      headers: '',
      body: new TextEncoder().encode(JSON.stringify(value)),
    });
    const order = {
      symbol: 'BTCUSDT',
      orderId: 12,
      clientOrderId: 'maker',
      type: 'LIMIT',
      side: 'BUY',
      price: '9000',
      origQty: '7',
      executedQty: '2',
      status: 'PARTIALLY_FILLED',
      timeInForce: 'GTC',
    };
    const transport: Transport = {
      prepareHttp: prepareRecordedHttp,
      now: () => 1_700_000_000_000,
      connect: (url, handlers) => {
        if (url.includes('/ws-api/')) privateMessage = handlers.onMessage;
        queueMicrotask(handlers.onOpen);
        return {
          close() {},
          send: (text) => {
            if (!url.includes('/ws-api/')) return;
            const message = JSON.parse(text);
            queueMicrotask(() =>
              handlers.onMessage(
                JSON.stringify({
                  id: message.id,
                  status: 200,
                  result:
                    message.method === 'userDataStream.subscribe.signature'
                      ? { subscriptionId: 7 }
                      : [{ subscriptionId: 7 }],
                }),
              ),
            );
          },
        };
      },
      request: async (request) => {
        const path = new URL(request.url).pathname;
        expect(path.startsWith('/api/v3/')).toBe(true);
        if (path.endsWith('/exchangeInfo'))
          return raw({
            symbols: ['BTC', 'ETH'].map((base) => ({
              symbol: `${base}USDT`,
              status: 'TRADING',
              baseAsset: base,
              quoteAsset: 'USDT',
              baseAssetPrecision: 8,
              quoteAssetPrecision: 8,
              filters: [
                { filterType: 'PRICE_FILTER', tickSize: '0.1' },
                { filterType: 'LOT_SIZE', stepSize: '1', minQty: '1', maxQty: '100' },
              ],
            })),
          });
        if (path.endsWith('/24hr'))
          return raw(
            ['BTC', 'ETH'].map((base) => ({
              symbol: `${base}USDT`,
              lastPrice: '10000',
              bidPrice: '9999',
              askPrice: '10001',
              priceChangePercent: '0',
              volume: '1',
              quoteVolume: '10000',
            })),
          );
        if (path.endsWith('/account')) return raw({ balances: [] });
        if (path.endsWith('/openOrders') && request.method === 'DELETE') {
          return new Promise<ReturnType<typeof raw>>((resolve) => {
            releaseCancel = () =>
              resolve(
                raw([
                  {
                    symbol: 'BTCUSDT',
                    orderId: 12,
                    origClientOrderId: 'maker',
                    clientOrderId: 'cancel-request',
                    origQty: '7',
                    executedQty: '3',
                    status: 'CANCELED',
                  },
                ]),
              );
          });
        }
        if (path.endsWith('/openOrders')) return raw([order]);
        throw new Error(`Unexpected Spot request ${request.method} ${path}`);
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    };
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => transport);
    const exchange = new RustEngineExchange('binancespot', { key: 'key', secret: 'secret' }, new DefaultStore());
    const main = new BaseExchange('binancespot', { key: '', secret: '' }, new DefaultStore());
    const message = (event: unknown) => privateMessage!(JSON.stringify({ subscriptionId: 7, event }));
    exchange.on('fill', (fill: OrderFillEvent) => workerFills.push(fill));
    main.on('fill', (fill: OrderFillEvent) => mainFills.push(fill));
    exchange.on('error', (error: unknown) => errors.push(error));
    try {
      await exchange.start();
      main.applyWorkerFrame(exchange.snapshotFrame()!);
      exchange.onFrame = (frame) => main.applyWorkerFrame(frame.slice(0));
      const cancellation = exchange.cancelSymbolOrders('BTCUSDT');
      await vi.waitFor(() => expect(releaseCancel).toBeDefined());
      if (reused) {
        message({
          e: 'executionReport',
          i: 44,
          s: 'ETHUSDT',
          c: 'maker',
          S: 'SELL',
          o: 'LIMIT',
          X: 'NEW',
          x: 'NEW',
          p: '100',
          L: '0',
          q: '9',
          z: '0',
          l: '0',
        });
        await vi.waitFor(() => expect(exchange.store.orders.find((row) => row.id === 'maker')?.symbol).toBe('ETHUSDT'));
      }
      releaseCancel!();
      await cancellation;
      await vi.waitFor(() =>
        expect(main.store.orders.find((row) => row.id === 'maker')).toMatchObject(
          reused ? { symbol: 'ETHUSDT', amount: 9, filled: 0 } : { symbol: 'BTCUSDT', filled: 3, remaining: 4 },
        ),
      );
      expect(workerFills).toEqual([]);
      expect(mainFills).toEqual([]);
      const execution = {
        e: 'executionReport',
        i: 12,
        t: 100,
        s: 'BTCUSDT',
        c: 'maker',
        S: 'BUY',
        o: 'LIMIT',
        X: 'PARTIALLY_FILLED',
        x: 'TRADE',
        p: '9000',
        L: '9100',
        q: '7',
        z: '3',
        l: '1',
      };
      message(execution);
      message(execution);
      await vi.waitFor(() => {
        expect(workerFills).toHaveLength(1);
        expect(mainFills).toHaveLength(1);
      });
    const expected = [{ id: 'maker', orderId: 'maker', symbol: 'BTCUSDT', side: OrderSide.Buy, price: 9100, amount: 1 }];
      expect(workerFills).toEqual(expected);
      expect(mainFills).toEqual(expected);
      if (reused) {
        expect(exchange.store.orders.find((row) => row.id === 'maker')).toMatchObject({
          symbol: 'ETHUSDT',
          amount: 9,
          filled: 0,
        });
        expect(main.store.orders.find((row) => row.id === 'maker')).toMatchObject({
          symbol: 'ETHUSDT',
          amount: 9,
          filled: 0,
        });
      }
      expect(errors).toEqual([]);
    } finally {
      exchange.dispose();
      main.dispose();
    }
  },
);
