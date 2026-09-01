import { describe, expect, it, vi } from 'vitest';

import type { CLIPlatformAdapter, CLISession } from '@tealstreet/cli-core';
import type { CoreAdapter } from '@tealstreet/script-api-core';
import type { OrderBook } from '@tealstreet/safe-cex/src/types';

import { OEMS_READ_METHODS, OEMS_WRITE_METHODS } from '@tealstreet/oems-core';

import type { ExchangeManager } from './exchange-manager';
import { createStandaloneCoreAdapter } from './script-adapter';
import { ScriptOrderbookCache } from './script-orderbook-cache';

const buildPlatformAdapter = (): CLIPlatformAdapter =>
  ({
    listAccounts: () => [],
    getAccount: () => undefined,
    addAccount: () => {},
    updateAccount: () => false,
    removeAccount: () => false,
    setDefaultAccount: () => false,
    getDefaultSymbol: () => undefined,
    setDefaultSymbol: () => {},
    getActiveAccounts: () => [],
    createExchange: async () => {
      throw new Error('not implemented');
    },
    startExchange: async () => {},
    stopExchange: async () => {},
    print: () => {},
    printError: () => {},
    printSuccess: () => {},
    printWarning: () => {},
    printInfo: () => {},
  }) as CLIPlatformAdapter;

describe('the OEMS slice is servable here', () => {
  // The web half of this lives in
  // `apps/web/src/components/aiOrders/oemsSurface.test.ts`. This is the CLI
  // half, and it exists because the slice is only meaningful while every host
  // in the venn diagram actually serves it — see the venn table in
  // `packages/script-api-core/CLAUDE.md`. Mobile is the third host and cannot
  // be asserted from this repo; `MobileCoreAdapter` throws on every write by
  // design, which is documented there rather than pinned here.
  it('every OEMS method is a key of the CoreAdapter type', () => {
    // The runtime assertions below are nearly vacuous on their own — the
    // factory returns an object literal annotated `CoreAdapter`, so tsc
    // already guarantees the keys exist. THIS is the assertion that bites: it
    // fails to COMPILE if the slice names a method the contract dropped.
    const writes: readonly (keyof CoreAdapter)[] = OEMS_WRITE_METHODS;
    const reads: readonly (keyof CoreAdapter)[] = OEMS_READ_METHODS;
    expect(writes.length + reads.length).toBe(OEMS_WRITE_METHODS.length + OEMS_READ_METHODS.length);
  });

  it('every OEMS write method is a callable member of the CLI adapter', () => {
    const adapter = createStandaloneCoreAdapter(
      { getExchange: () => null } as unknown as ExchangeManager,
      { currentExchange: null, currentAccountName: 'acct', currentSymbol: 'BTCUSDT' },
      buildPlatformAdapter(),
    );
    for (const method of OEMS_WRITE_METHODS) {
      expect(typeof adapter[method]).toBe('function');
    }
  });

  it('every OEMS read method is a callable member of the CLI adapter', () => {
    const adapter = createStandaloneCoreAdapter(
      { getExchange: () => null } as unknown as ExchangeManager,
      { currentExchange: null, currentAccountName: 'acct', currentSymbol: 'BTCUSDT' },
      buildPlatformAdapter(),
    );
    for (const method of OEMS_READ_METHODS) {
      expect(typeof adapter[method]).toBe('function');
    }
  });
});

describe('createStandaloneCoreAdapter', () => {
  it('fails script runCommand calls that cli-core does not handle', async () => {
    const exchangeManager = {
      getExchange: () => null,
    } as unknown as ExchangeManager;
    const session: CLISession = {
      currentExchange: null,
      currentAccountName: 'acct',
      currentSymbol: 'BTCUSDT',
    };

    const adapter = createStandaloneCoreAdapter(exchangeManager, session, buildPlatformAdapter());
    const result = await adapter.runCommand('script:missing', { account: 'acct', depth: 1 });

    expect(result).toEqual({
      ok: false,
      capturedIds: [],
      output: [],
      error: 'Unknown command: script:missing',
    });
  });

  it('distinguishes unsupported candles from an empty candle result', async () => {
    const session: CLISession = {
      currentExchange: null,
      currentAccountName: 'acct',
      currentSymbol: 'BTCUSDT',
    };

    const unsupported = createStandaloneCoreAdapter(
      {
        getExchange: () => ({}),
      } as unknown as ExchangeManager,
      session,
      buildPlatformAdapter(),
    );
    await expect(unsupported.getCandles('BTCUSDT', '1m', { account: 'acct' })).rejects.toThrow(
      "getCandles: account 'acct' does not support OHLCV candles",
    );

    const supported = createStandaloneCoreAdapter(
      {
        getExchange: () => ({ fetchOHLCV: async () => [] }),
      } as unknown as ExchangeManager,
      session,
      buildPlatformAdapter(),
    );
    await expect(supported.getCandles('BTCUSDT', '1m', { account: 'acct' })).resolves.toEqual([]);
  });

  it('backs getOrderbook with the exchange orderbook subscription', async () => {
    const unsubscribe = vi.fn();
    const listenOrderBook = vi.fn((_symbol: string, callback: (book: OrderBook) => void) => {
      callback({
        bids: [
          { price: 100, amount: 1, total: 1 },
          { price: 99, amount: 2, total: 3 },
        ],
        asks: [
          { price: 101, amount: 1.5, total: 1.5 },
          { price: 102, amount: 2.5, total: 4 },
        ],
      });
      return unsubscribe;
    });
    const session: CLISession = {
      currentExchange: null,
      currentAccountName: 'acct',
      currentSymbol: 'BTCUSDT',
    };
    const adapter = createStandaloneCoreAdapter(
      {
        getExchange: () => ({ listenOrderBook }),
      } as unknown as ExchangeManager,
      session,
      buildPlatformAdapter(),
    );

    await expect(adapter.getOrderbook('BTCUSDT', { account: 'acct', depth: 1 })).resolves.toEqual({
      bids: [{ price: 100, amount: 1, total: 1 }],
      asks: [{ price: 101, amount: 1.5, total: 1.5 }],
    });
    expect(listenOrderBook).toHaveBeenCalledWith('BTCUSDT', expect.any(Function));
  });

  it('rejects unsupported orderbook subscriptions instead of returning a stub', async () => {
    const session: CLISession = {
      currentExchange: null,
      currentAccountName: 'acct',
      currentSymbol: 'BTCUSDT',
    };
    const adapter = createStandaloneCoreAdapter(
      {
        getExchange: () => ({}),
      } as unknown as ExchangeManager,
      session,
      buildPlatformAdapter(),
    );

    await expect(adapter.getOrderbook('BTCUSDT', { account: 'acct' })).rejects.toThrow(
      "getOrderbook: account 'acct' does not support orderbook subscriptions",
    );
  });

  it('rebuilds orderbook subscriptions when the account exchange is replaced', async () => {
    const unsubscribeFirst = vi.fn();
    const unsubscribeSecond = vi.fn();
    let exchange = {
      listenOrderBook: vi.fn((_symbol: string, callback: (book: OrderBook) => void) => {
        callback({ bids: [{ price: 100, amount: 1, total: 1 }], asks: [{ price: 101, amount: 1, total: 1 }] });
        return unsubscribeFirst;
      }),
    };
    const cache = new ScriptOrderbookCache(
      {
        getExchange: () => exchange,
      } as unknown as ExchangeManager,
      { firstSnapshotTimeoutMs: 1, idleTtlMs: 60_000 },
    );

    await expect(cache.get('BTCUSDT', { account: 'acct' })).resolves.toEqual({
      bids: [{ price: 100, amount: 1, total: 1 }],
      asks: [{ price: 101, amount: 1, total: 1 }],
    });

    exchange = {
      listenOrderBook: vi.fn((_symbol: string, callback: (book: OrderBook) => void) => {
        callback({ bids: [{ price: 200, amount: 2, total: 2 }], asks: [{ price: 201, amount: 2, total: 2 }] });
        return unsubscribeSecond;
      }),
    };

    await expect(cache.get('BTCUSDT', { account: 'acct' })).resolves.toEqual({
      bids: [{ price: 200, amount: 2, total: 2 }],
      asks: [{ price: 201, amount: 2, total: 2 }],
    });
    expect(unsubscribeFirst).toHaveBeenCalledTimes(1);

    cache.disposeAll();
    expect(unsubscribeSecond).toHaveBeenCalledTimes(1);
  });
});
