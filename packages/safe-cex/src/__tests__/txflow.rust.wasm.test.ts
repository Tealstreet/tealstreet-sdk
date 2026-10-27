vi.mock('../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('./reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../exchanges/rust/betaVenues')>(), true);
});

import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { BaseExchange } from '../exchanges/base';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '../exchanges/registry';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { directTransport } from '../transport/direct';
import { PositionMode } from '../types';
import { prepareRecordedHttp } from './support/managedHttp';

const REST = 'https://rpc.txflow-testnet.xyz';
const PUBLIC_PROXY = 'https://public-cors.invalid';
const WALLET = '0x0000000000000000000000000000000000000001';
// Public disposable official signing-vector key, distinct from the account wallet.
const OPTIONS = { key: '', secret: '0x' + 'ab'.repeat(32), applicationId: WALLET, testnet: true };

/** Documented native envelopes, also used by harness/private-tape-txflow.mjs. */
const recordedTxFlowTransport = () => {
  const startedAt = performance.now();
  const requests: Array<Parameters<Transport['request']>[0]> = [];
  const sockets: Array<{ url: string; channel?: string; sent: unknown[] }> = [];
  const context = {
    markPx: '100',
    oraclePx: '100',
    lastPrice: '100',
    midPx: '100',
    prevDayPx: '100',
    funding: '0.01',
    openInterest: '2',
    dayNtlVlm: '1000',
    dayBaseVlm: '10',
    nextFundingTime: 1_700_003_600_000,
  };
  const responses: Record<string, unknown> = {
    perpMeta: {
      universe: [
        {
          index: 1,
          name: 'BTC-USDC',
          baseCurrency: 'BTC',
          quoteCurrency: 'USDC',
          currencyType: 'usdc',
          priceTick: '0.1',
          basePrecision: '0.001',
          maxLeverage: 50,
          maxLimitOrderSize: 100,
          maxMarketOrderSize: 50,
          delisted: false,
          haltTrading: false,
          onlyIsolated: false,
          useNotionalMargin: false,
        },
      ],
    },
    activeAssetCtx: {
      instrumentId: 1,
      lastPrice: '100',
      prevDayPx: '100',
      markPx: '100',
      oraclePx: '100',
      openInterest: '2',
      dayNtlVlm: '1000',
      coinVlm: '10',
      nodeCtx: context,
    },
    clearinghouseState: {
      user: WALLET,
      time: 1_700_000_000_000,
      assetPositions: [],
      marginSummary: { accountValue: '1000', totalMarginUsed: '0', totalRawUsd: '1000' },
    },
    frontendOpenOrders: [],
    userFills: { data: [], hasMore: false },
    activeAssetData: { user: WALLET, coin: '1', leverage: { type: 'cross', mode: 'oneWay', value: 7 } },
  };
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => 1_700_000_000_000 + performance.now() - startedAt,
    connect: (url, { channel, onOpen }) => {
      const socket = { url, channel, sent: [] as unknown[] };
      sockets.push(socket);
      const timer = setTimeout(onOpen, 0);
      return { send: (text) => socket.sent.push(JSON.parse(text)), close: () => clearTimeout(timer) };
    },
    request: async (request) => {
      requests.push(request);
      const { type } = JSON.parse(request.body!);
      if (!Object.hasOwn(responses, type)) throw new Error(`Unexpected TxFlow read: ${type}`);
      return {
        status: 200,
        body: new TextEncoder().encode(JSON.stringify({ success: true, code: 0, data: responses[type] })),
      };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, requests, sockets };
};

describe('TxFlow production registry through real Wasm', () => {
  let exchange: BaseExchange | undefined;
  afterEach(() => exchange?.dispose());

  it('starts a private wallet with no API key and bridges account settings independently of positions', async () => {
    const tape = recordedTxFlowTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const store = new DefaultStore();
    const Exchange = await exchangeRegistry.txflow();
    exchange = new Exchange(
      'txflow',
      {
        ...OPTIONS,
        transport: { ...directTransport, publicCorsProxy: () => PUBLIC_PROXY },
      },
      store,
    );
    expect(exchange).toBeInstanceOf(RustEngineExchange);
    expect(exchange.isPublicDataOnly).toBe(false);

    await exchange.start();
    await vi.waitFor(() => {
      expect(store.fetchedLeverage['BTC-USDC']).toMatchObject({
        buyLeverage: 7,
        sellLeverage: 7,
        marginType: 'cross',
        positionMode: PositionMode.OneWay,
      });
      expect(tape.sockets).toHaveLength(2);
      expect(tape.sockets.every(({ sent }) => sent.length > 0)).toBe(true);
    });
    expect(store.loaded).toEqual({ markets: true, tickers: true, balance: true, orders: true, positions: true });
    expect(store.marketsBySymbol['BTC-USDC']).toMatchObject({
      id: '1',
      symbol: 'BTC-USDC',
      base: 'BTC',
      quote: 'USDC',
      precision: { price: 0.1, amount: 0.001 },
      limits: { leverage: { max: 50 } },
    });
    expect(store.tickers).toHaveLength(1);
    expect(store.tickers[0]).toMatchObject({
      symbol: 'BTC-USDC',
      last: 100,
      mark: 100,
      openInterest: 200,
      fundingRate: 0.0001,
    });
    expect(store.balance).toMatchObject({ total: 1000, free: 1000, used: 0, upnl: 0 });
    expect(store.positions).toEqual([]);
    expect(store.orders).toEqual([]);
    expect(await exchange.fetchLeverageForSymbol('BTC-USDC')).toMatchObject({
      buyLeverage: 7,
      sellLeverage: 7,
      marginType: 'cross',
    });

    const publicReads = new Set(['perpMeta', 'activeAssetCtx']);
    expect(new Set(tape.requests.map(({ body }) => JSON.parse(body!).type))).toEqual(
      new Set([...publicReads, 'clearinghouseState', 'frontendOpenOrders', 'userFills', 'activeAssetData']),
    );
    for (const request of tape.requests) {
      const body = JSON.parse(request.body!);
      expect(request.method).toBe('POST');
      expect(request.headers['x-api-version']).toBe('2026-08-16');
      if (publicReads.has(body.type)) {
        expect(request.url).toBe(`${PUBLIC_PROXY}/${REST}/info`);
        expect(body).not.toHaveProperty('user');
      } else {
        expect(request.url).toBe(`${REST}/info`);
        expect(body.user).toBe(WALLET);
      }
    }
    expect(tape.sockets.map(({ url, channel }) => ({ url, channel }))).toEqual([
      { url: 'wss://rpc.txflow-testnet.xyz/ws', channel: 'private' },
      { url: 'wss://rpc.txflow-testnet.xyz/api-ws', channel: 'public' },
    ]);
    await vi.waitFor(() => {
      const nodeSubscriptions = tape.sockets[0].sent as Array<{ type?: string; subscription?: { user?: string } }>;
      const privateSubscriptions = nodeSubscriptions.filter(({ subscription }) => subscription?.user);
      expect(privateSubscriptions.map(({ type }) => type)).toEqual([
        'orderUpdates', 'clearinghouseState', 'userAssetData',
      ]);
      expect(privateSubscriptions.every(({ subscription }) => subscription!.user === WALLET)).toBe(true);
    }, { timeout: 3000 });
  });

  it('rejects mainnet startup before HTTP or socket I/O', async () => {
    const tape = recordedTxFlowTransport();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const Exchange = await exchangeRegistry.txflow();
    exchange = new Exchange('txflow', { ...OPTIONS, testnet: false }, new DefaultStore());
    await expect(exchange.start()).rejects.toThrow('TxFlow mainnet is offline; testnet is required');
    expect(tape.requests).toEqual([]);
    expect(tape.sockets).toEqual([]);
  });
});
