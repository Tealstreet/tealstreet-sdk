import { describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from './reference/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../store/store.base';
import { ExchangeOptions } from '../types';
import { PositionSide } from '../types';

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
};

const createDeferred = <T>(): Deferred<T> => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((innerResolve) => {
    resolve = innerResolve;
  });
  return { promise, resolve };
};

const createExchange = (): HyperliquidExchange => {
  const options: ExchangeOptions = {
    key: '0xkey',
    secret: '0xsecret',
    applicationId: '0xuser',
    testnet: true,
    tuning: { consoleLogging: false },
  };

  return new HyperliquidExchange('hyperliquid', options, new DefaultStore());
};

const coreMarkets = [
  {
    id: 'BTC',
    symbol: 'BTC-PERP',
    base: 'BTC',
    quote: 'USDC',
    active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: {
      amount: { min: 0.001, max: 1_000_000 },
      leverage: { min: 1, max: 50 },
    },
    info: { hyperliquidId: 0 },
  },
] as any;

const coreTickers = [
  {
    id: 'BTC',
    symbol: 'BTC-PERP',
    bid: 0,
    ask: 0,
    last: 100_000,
    mark: 100_000,
    index: 100_000,
    percentage: 0,
    fundingRate: 0,
    fundingTime: 0,
    volume: 0,
    quoteVolume: 0,
    openInterest: 0,
    info: {},
  },
] as any;

const coreState = (accountValue = 0, totalMarginUsed = 0) => ({
  assetPositions: [],
  marginSummary: {
    accountValue: String(accountValue),
    totalMarginUsed: String(totalMarginUsed),
  },
});

describe('HyperliquidExchange startup sequencing', () => {
  it('adds redacted builder-fee context for rejected order telemetry', () => {
    const exchange = createExchange();
    const mainWallet = '0x1111111111111111111111111111111111111111';
    const signerWallet = '0x2222222222222222222222222222222222222222';
    const vaultAddress = '0x3333333333333333333333333333333333333333';
    (exchange.options as any).applicationId = mainWallet;
    (exchange.options as any).key = signerWallet;
    (exchange.options as any).vault = vaultAddress;
    (exchange as any)._builderFeeApproved = true;
    (exchange as any)._builderFeeLastCheck = {
      approved: true,
      maxFeeRate: '0.01%',
      maxFeeRateTenthsBps: 10,
    };

    const telemetry = exchange.getOrderErrorTelemetry(
      {
        action: {
          type: 'order',
          grouping: 'na',
          orders: [{ a: 0 }],
          builder: { b: '0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d', f: 10 },
        },
      },
      'Builder fee has not been approved.',
    );

    expect(telemetry?.sentryTags).toMatchObject({
      hyperliquid_error_family: 'builder_fee',
      hyperliquid_builder_attached: 'yes',
      hyperliquid_builder_check: 'approved',
      hyperliquid_builder_fee_sufficient: 'yes',
      hyperliquid_has_vault: 'yes',
      hyperliquid_network: 'testnet',
    });
    expect(telemetry?.sentryExtra).toMatchObject({
      hyperliquidBuilderFee: expect.objectContaining({
        builderPayloadPresent: true,
        attachedBuilderFeeTenthsBps: 10,
        checkedMainWallet: '0x111111...111111',
        signerWallet: '0x222222...222222',
        vaultAddress: '0x333333...333333',
      }),
    });
    expect(JSON.stringify(telemetry)).not.toContain(mainWallet);
    expect(JSON.stringify(telemetry)).not.toContain(signerWallet);
    expect(JSON.stringify(telemetry)).not.toContain(vaultAddress);
  });

  it('does not block startup on private REST bootstrap calls', async () => {
    const exchange = createExchange();

    const pendingCoreState = createDeferred<any>();
    const pendingBuilder = createDeferred<void>();

    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange as any, 'resolveUserAbstractionMode').mockResolvedValue(undefined);
    vi.spyOn(exchange as any, 'fetchMarketsAndTickers').mockResolvedValue([coreMarkets, coreTickers]);
    vi.spyOn(exchange as any, 'fetchCorePerpsState').mockReturnValue(pendingCoreState.promise);
    vi.spyOn(exchange as any, 'fetchBuilderFeeApproval').mockReturnValue(pendingBuilder.promise);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchAllHip3Positions').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);

    const publicConnectSpy = vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    const privateConnectSpy = vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => {});

    await exchange.start();

    expect(publicConnectSpy).toHaveBeenCalledTimes(1);
    expect(privateConnectSpy).toHaveBeenCalledTimes(1);
    expect(exchange.store.loaded.markets).toBe(true);
    expect(exchange.store.loaded.tickers).toBe(true);
    expect(exchange.store.loaded.orders).toBe(true);
    expect(exchange.store.loaded.balance).toBe(true);
    expect(exchange.store.balance).toEqual({
      used: 0,
      free: 0,
      total: 0,
      upnl: 0,
    });
    expect(exchange.store.positions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          symbol: 'BTC-PERP',
          side: PositionSide.Long,
          contracts: 0,
        }),
      ]),
    );

    // Cleanup pending promises to avoid cross-test leakage.
    pendingBuilder.resolve();
    pendingCoreState.resolve(coreState());
    await Promise.resolve();
  });

  it('hydrates public data before user abstraction resolution completes', async () => {
    const exchange = createExchange();
    const pendingAbstraction = createDeferred<void>();
    const pendingCoreState = createDeferred<any>();
    const pendingBuilder = createDeferred<void>();

    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange as any, 'resolveUserAbstractionMode').mockReturnValue(pendingAbstraction.promise);
    vi.spyOn(exchange as any, 'fetchMarketsAndTickers').mockResolvedValue([coreMarkets, coreTickers]);
    vi.spyOn(exchange as any, 'fetchCorePerpsState').mockReturnValue(pendingCoreState.promise);
    vi.spyOn(exchange as any, 'fetchBuilderFeeApproval').mockReturnValue(pendingBuilder.promise);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchAllHip3Positions').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);

    const publicConnectSpy = vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    const privateConnectSpy = vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => {});

    const startPromise = exchange.start();
    await Promise.resolve();
    await Promise.resolve();

    expect(publicConnectSpy).toHaveBeenCalledTimes(1);
    expect(privateConnectSpy).not.toHaveBeenCalled();
    expect(exchange.store.loaded.markets).toBe(true);
    expect(exchange.store.loaded.tickers).toBe(true);

    pendingAbstraction.resolve();
    await startPromise;

    expect(privateConnectSpy).toHaveBeenCalledTimes(1);

    pendingBuilder.resolve();
    pendingCoreState.resolve(coreState());
    await Promise.resolve();
  });

  it('applies initial core state atomically after startup store hydration', async () => {
    const exchange = createExchange();
    const deferredCoreState = createDeferred<any>();

    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange as any, 'resolveUserAbstractionMode').mockResolvedValue(undefined);
    vi.spyOn(exchange as any, 'fetchMarketsAndTickers').mockResolvedValue([coreMarkets, coreTickers]);
    vi.spyOn(exchange as any, 'fetchBuilderFeeApproval').mockResolvedValue(undefined);
    vi.spyOn(exchange as any, 'fetchCorePerpsState').mockReturnValue(deferredCoreState.promise);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchAllHip3Positions').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);

    let privateConnectSawMarkets = 0;
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => {
      privateConnectSawMarkets = exchange.store.markets.length;
    });
    const applyCoreStateSpy = vi.spyOn(exchange.privateWebsocket, 'applyCoreClearinghouseState');

    await exchange.start();

    expect(privateConnectSawMarkets).toBeGreaterThan(0);
    expect(exchange.store.balance.total).toBe(0);
    expect(exchange.store.balance.used).toBe(0);
    expect(exchange.store.balance.free).toBe(0);
    expect(exchange.store.balance.upnl).toBe(0);

    const snapshot = coreState(1250, 250);
    deferredCoreState.resolve(snapshot);
    await Promise.resolve();
    await Promise.resolve();

    expect(applyCoreStateSpy).toHaveBeenCalledExactlyOnceWith(snapshot, true);
    expect(exchange.store.balance).toEqual({
      total: 1250,
      used: 250,
      free: 1000,
      upnl: 0,
      info: {
        userAbstractionMode: 'standard',
        source: 'clearinghouseState',
      },
    });
  });

  it('does not apply a deferred core REST snapshot after disposal', async () => {
    const exchange = createExchange();
    const deferredCoreState = createDeferred<any>();

    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange as any, 'resolveUserAbstractionMode').mockResolvedValue(undefined);
    vi.spyOn(exchange as any, 'fetchMarketsAndTickers').mockResolvedValue([coreMarkets, coreTickers]);
    vi.spyOn(exchange as any, 'fetchCorePerpsState').mockReturnValue(deferredCoreState.promise);
    vi.spyOn(exchange as any, 'fetchBuilderFeeApproval').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchAllHip3Positions').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    const applyCoreStateSpy = vi.spyOn(exchange.privateWebsocket, 'applyCoreClearinghouseState');

    await exchange.start();
    exchange.dispose();
    deferredCoreState.resolve(coreState(1250, 250));
    await Promise.resolve();
    await Promise.resolve();

    expect(applyCoreStateSpy).not.toHaveBeenCalled();
    expect(exchange.store.balance).toEqual({ used: 0, free: 0, total: 0, upnl: 0 });
  });

  it('keeps standard-mode core balance math unchanged', () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'standard';
    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 1000,
      totalMarginUsed: 250,
      upnl: 100,
    });

    expect(exchange.store.balance).toEqual({
      total: 900,
      used: 250,
      free: 750,
      upnl: 100,
      info: {
        userAbstractionMode: 'standard',
        source: 'clearinghouseState',
      },
    });
  });

  it('uses aggregate spot collateral as the unified USDC margin slice', () => {
    const exchange = createExchange();

    exchange.store.update({
      markets: coreMarkets as any,
      tickers: coreTickers as any,
    });

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    exchange.handleSpotClearinghouseStateUpdate({
      balances: [
        { coin: 'USDC', total: '1000', hold: '100' },
        { coin: 'BTC', total: '0.01', hold: '0' },
      ],
    });
    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 600,
      totalMarginUsed: 200,
      upnl: 50,
    });

    expect(exchange.store.balance).toEqual({
      total: 2000,
      used: 300,
      free: 1750,
      upnl: 50,
      currencies: {
        BTC: {
          free: 0.01,
          used: 0,
          total: 0.01,
          upnl: 0,
        },
        USDC: {
          free: 1750,
          used: 300,
          total: 2000,
          upnl: 50,
        },
      },
      coins: {
        BTC: {
          base: 'BTC',
          amount: 0.01,
          notional: 1000,
        },
        USDC: {
          base: 'USDC',
          amount: 1000,
          notional: 1000,
        },
      },
      info: {
        userAbstractionMode: 'unifiedAccount',
        source: 'spotClearinghouseState+clearinghouseState',
        spotCollateralCurrencies: {
          BTC: {
            free: 0.01,
            used: 0,
            total: 0.01,
            upnl: 0,
          },
          USDC: {
            free: 900,
            used: 100,
            total: 1000,
            upnl: 0,
          },
        },
      },
    });
  });

  it('uses token 0 post-maintenance availability instead of reconstructing unified free margin', () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 40,
      totalMarginUsed: 50.324,
      upnl: 0,
    });
    exchange.handleSpotClearinghouseStateUpdate({
      balances: [{ coin: 'USDC', token: 0, total: '40', hold: '1.68' }],
      tokenToAvailableAfterMaintenance: [[0, '38.32']],
    });

    expect(exchange.store.balance).toMatchObject({
      total: 40,
      free: 38.32,
      upnl: 0,
      currencies: {
        USDC: {
          total: 40,
          free: 38.32,
          upnl: 0,
        },
      },
      info: {
        spotCollateralCurrencies: {
          USDC: {
            total: 40,
            used: 1.68,
            free: 38.32,
            upnl: 0,
          },
        },
      },
    });
    expect(exchange.store.balance.used).toBeCloseTo(1.68, 10);
    expect(exchange.store.balance.currencies!.USDC.used).toBeCloseTo(1.68, 10);
    expect(exchange.store.balance.free).not.toBe(-12.004);
  });

  it('does not add unified perp PnL twice when spot total already contains it', () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 30,
      totalMarginUsed: 5,
      upnl: -10,
    });
    exchange.handleSpotClearinghouseStateUpdate({
      balances: [{ coin: 'USDC', token: 0, total: '90', hold: '5' }],
      tokenToAvailableAfterMaintenance: [[0, '85']],
    });

    expect(exchange.store.balance).toMatchObject({
      total: 100,
      used: 5,
      free: 85,
      upnl: -10,
      currencies: {
        USDC: {
          total: 100,
          used: 5,
          free: 85,
          upnl: -10,
        },
      },
    });
    expect(exchange.store.balance.total + exchange.store.balance.upnl - exchange.store.balance.used).toBe(85);

    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 29,
      totalMarginUsed: 5,
      upnl: -11,
    });

    expect(exchange.store.balance).toMatchObject({
      total: 100,
      used: 5,
      free: 85,
      upnl: -11,
    });
  });

  it('does not let a field-less spot refresh downgrade authoritative unified availability', () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 40,
      totalMarginUsed: 50.324,
      upnl: 0,
    });
    exchange.handleSpotClearinghouseStateUpdate({
      balances: [{ coin: 'USDC', token: 0, total: '40', hold: '1.68' }],
      tokenToAvailableAfterMaintenance: [[0, '38.32']],
    });

    exchange.handleSpotClearinghouseStateUpdate({
      balances: [{ coin: 'USDC', token: 0, total: '39', hold: '50.324' }],
    });

    expect(exchange.store.balance).toMatchObject({
      total: 40,
      used: expect.closeTo(1.68, 10),
      free: 38.32,
      upnl: 0,
    });
  });

  it('does not let stale REST availability overwrite a newer spotState snapshot', () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 40,
      totalMarginUsed: 50.324,
      upnl: 0,
    });
    exchange.handleSpotClearinghouseStateUpdate(
      {
        balances: [{ coin: 'USDC', token: 0, total: '40', hold: '1.68' }],
        tokenToAvailableAfterMaintenance: [[0, '38.32']],
      },
      'websocket',
    );

    exchange.handleSpotClearinghouseStateUpdate(
      {
        balances: [{ coin: 'USDC', token: 0, total: '25', hold: '5' }],
        tokenToAvailableAfterMaintenance: [[0, '20']],
      },
      'rest',
    );

    expect(exchange.store.balance).toMatchObject({
      total: 40,
      used: expect.closeTo(1.68, 10),
      free: 38.32,
      upnl: 0,
    });
  });

  it('treats an empty unified spot snapshot as authoritative zero collateral', () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    exchange.applyCorePerpsBalanceSnapshot({
      accountValue: 600,
      totalMarginUsed: 200,
      upnl: 50,
    });

    expect(exchange.store.balance).toMatchObject({
      total: 550,
      used: 200,
      free: 400,
      upnl: 50,
      info: {
        userAbstractionMode: 'unifiedAccount',
        source: 'clearinghouseState',
      },
    });

    exchange.handleSpotClearinghouseStateUpdate({ balances: [] });

    expect(exchange.store.balance).toEqual({
      total: 0,
      used: 200,
      free: -150,
      upnl: 50,
      currencies: {
        USDC: {
          total: 0,
          used: 200,
          free: -150,
          upnl: 50,
        },
      },
      coins: {},
      info: {
        userAbstractionMode: 'unifiedAccount',
        source: 'spotClearinghouseState+clearinghouseState',
        spotCollateralCurrencies: {},
      },
    });
  });

  it('keeps ledger-triggered unified spot refreshes cooldown-aware', async () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    const postSpy = vi.spyOn(exchange.infoXhr, 'post').mockResolvedValue({
      data: {
        balances: [{ coin: 'USDC', token: 0, total: '40', hold: '1.68' }],
        tokenToAvailableAfterMaintenance: [[0, '38.32']],
      },
    } as any);
    const stateSpy = vi.spyOn(exchange, 'handleSpotClearinghouseStateUpdate');

    await (exchange as any).refreshUnifiedSpotCollateralBalance(true);
    await (exchange as any).refreshUnifiedSpotCollateralBalance(false);
    await (exchange as any).refreshUnifiedSpotCollateralBalance(true);

    expect(postSpy).toHaveBeenCalledTimes(2);
    expect(stateSpy).toHaveBeenLastCalledWith(
      {
        balances: [{ coin: 'USDC', token: 0, total: '40', hold: '1.68' }],
        tokenToAvailableAfterMaintenance: [[0, '38.32']],
      },
      'rest',
    );
  });

  it('does not force refreshes for non-funding ledger updates', () => {
    const exchange = createExchange();

    (exchange as any).userAbstractionMode = 'unifiedAccount';
    const refreshSpy = vi.spyOn(exchange, 'requestUnifiedSpotCollateralRefresh').mockImplementation(() => {});

    exchange.privateWebsocket.handleUserNonFundingLedgerUpdates({
      nonFundingLedgerUpdates: [],
      isSnapshot: false,
      user: '0xuser',
    });

    expect(refreshSpy).toHaveBeenCalledWith();
  });

  it('uses perp price decimals for HIP-3 market precision', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({
      data: [
        {
          universe: [
            {
              name: 'xyz:FOO',
              szDecimals: 2,
              maxLeverage: 20,
            },
          ],
        },
        [
          {
            midPx: '0.0012345',
            markPx: '0.0012345',
            oraclePx: '0.0012345',
            prevDayPx: '0.0012',
            funding: '0',
            dayNtlVlm: '100',
          },
        ],
      ],
    } as any);

    const { markets } = await (exchange as any).fetchHip3Markets('xyz', 1, null);

    expect(markets[0]).toMatchObject({
      id: 'xyz:FOO',
      symbol: 'XYZ|FOO-PERP',
      precision: {
        amount: 0.01,
        price: 0.0001,
      },
    });
  });

  it('parses irregular HIP-3 orderbook gaps without deriving grouping levels', () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: 'xyz:BRENTOIL',
          symbol: 'XYZ|BRENTOIL-PERP',
          base: 'BRENTOIL',
          quote: 'USDC',
          active: true,
          precision: { amount: 0.01, price: 0.01 },
          limits: {
            amount: { min: 0.01, max: 1_000_000 },
            leverage: { min: 1, max: 20 },
          },
          info: { hyperliquidId: 110049, hip3Dex: 'xyz' },
        },
      ] as any,
    });

    const callback = vi.fn();
    const errorSpy = vi.spyOn(exchange, 'emitAndLogError');
    const publicWebsocket = exchange.publicWebsocket as any;
    publicWebsocket.ws = { readyState: globalThis.WebSocket?.OPEN ?? 1 };
    publicWebsocket.send = vi.fn();

    const unsubscribe = exchange.listenOrderBook('XYZ|BRENTOIL-PERP', callback);
    const [parser] = publicWebsocket.messageHandlers['l2Book.xyz:BRENTOIL'];

    parser({
      coin: 'xyz:BRENTOIL',
      levels: [
        [
          { px: '96.841', sz: '4.92', n: 1 },
          { px: '96.837', sz: '44.22', n: 1 },
        ],
        [
          { px: '96.869', sz: '19.35', n: 1 },
          { px: '96.874', sz: '103.27', n: 1 },
        ],
      ],
      time: Date.now(),
    });

    expect(errorSpy).not.toHaveBeenCalled();
    const orderBook = callback.mock.calls[0][0];
    expect(orderBook.bids).toMatchObject([
      { price: 96.841, amount: 4.92 },
      { price: 96.837, amount: 44.22 },
    ]);
    expect(orderBook.asks).toMatchObject([
      { price: 96.869, amount: 19.35 },
      { price: 96.874, amount: 103.27 },
    ]);
    expect(orderBook.bids[1].total).toBeCloseTo(49.14);
    expect(orderBook.asks[1].total).toBeCloseTo(122.62);

    unsubscribe();
  });
});
