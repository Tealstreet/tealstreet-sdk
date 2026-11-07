import { beforeEach, describe, expect, it, vi } from 'vitest';

import { HyperliquidPrivateWebSocket } from './reference/hyperliquid/hyperliquid.ws-private';
import { PositionMode, PositionSide } from '../types';

const hasSeenFillMock = vi.fn();
const markFillSeenMock = vi.fn();

vi.mock('./reference/hyperliquid/hyperliquid.cross-cache', () => ({
  hasSeenFill: (...args: any[]) => hasSeenFillMock(...args),
  markFillSeen: (...args: any[]) => markFillSeenMock(...args),
}));

const createParentMock = () => {
  const market = {
    id: 'BTC',
    symbol: 'BTC-PERP',
    info: {},
  };

  const store = {
    marketsById: { BTC: market },
    marketsBySymbol: { 'BTC-PERP': market },
    positions: [],
    update: vi.fn(),
    addOrUpdatePosition: vi.fn(),
    addOrUpdatePositions: vi.fn(),
    addOrUpdatePositionsFull: vi.fn(),
  };

  const parent = {
    name: 'hyperliquid',
    isDisposed: false,
    options: { testnet: false } as Record<string, unknown>,
    store,
    hip3DexNames: [],
    marketsLowerCaseMap: new Map<string, typeof market>(),
    emitter: { emit: vi.fn() },
    log: vi.fn(),
    _getSymbolConfigurationFromAssetData: vi.fn(() => ({})),
    getFakePositions: vi.fn(() => []),
    scheduleHip3PositionRefresh: vi.fn(),
    syncBalanceFromLivePositions: vi.fn(),
    shouldUseSpotCollateralBalance: vi.fn(() => false),
    shouldRequestPortfolioMarginSpotState: vi.fn(() => false),
    handleSpotClearinghouseStateUpdate: vi.fn(),
    // Spot state is subscribed for unified/PM accounts AND for standard
    // accounts that opted into spot markets, so the stub needs both. Derived
    // exactly as the exchange derives it, so a test that flips either input
    // gets the real behaviour instead of a hardcoded answer.
    shouldTrackSpotHoldings: vi.fn(
      (): boolean => parent.shouldUseSpotCollateralBalance() || Boolean(parent.options.enableSpotMarkets),
    ),
  };

  return parent;
};

const createSampleFill = () => ({
  coin: 'BTC',
  px: '100000',
  sz: '0.25',
  side: 'B',
  time: 1_700_000_000_000,
  startPosition: '0.25',
  dir: 'Close Long',
  closedPnl: '12.5',
  hash: '0xabc',
  oid: 123,
  crossed: false,
  fee: '0.5',
  tid: 456,
});

const createSampleClearinghouseState = () => ({
  assetPositions: [
    {
      type: 'oneWay',
      position: {
        coin: 'BTC',
        szi: '0.25',
        entryPx: '99000',
        positionValue: '24750',
        leverage: { value: '10', type: 'cross' },
        unrealizedPnl: '3.25',
        liquidationPx: '80000',
      },
    },
  ],
});

describe('HyperliquidPrivateWebSocket realized PnL behavior', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    hasSeenFillMock.mockReturnValue(false);
  });

  it('subscribes to core clearinghouseState on primary system topics', () => {
    const parent = createParentMock();
    (parent as any).options = { ...parent.options, applicationId: '0xuser' };
    const ws = new HyperliquidPrivateWebSocket(parent as any);
    const sendMock = vi.fn();

    (ws as any).send = sendMock;
    (ws as any).subscribeSystemTopics();

    const payloads = sendMock.mock.calls.map((args: any[]) => JSON.parse(args[0]));
    const hasCoreClearinghouseSubscription = payloads.some(
      (payload: any) =>
        payload?.subscription?.type === 'clearinghouseState' &&
        payload?.subscription?.user === '0xuser' &&
        !payload?.subscription?.dex,
    );

    expect(hasCoreClearinghouseSubscription).toBe(true);
    expect(payloads.some((payload: any) => payload?.subscription?.type === 'webData2')).toBe(false);
  });

  it('does not send rejected webData2 subscriptions for HIP-3 DEXes', () => {
    const parent = createParentMock();
    (parent as any).options = { ...parent.options, applicationId: '0xuser' };
    (parent as any).hip3DexNames = ['vntls'];
    const ws = new HyperliquidPrivateWebSocket(parent as any);
    const sendMock = vi.fn();

    (ws as any).send = sendMock;
    (ws as any).subscribeSystemTopics();

    const payloads = sendMock.mock.calls.map((args: any[]) => JSON.parse(args[0]));
    expect(payloads.some((payload: any) => payload?.subscription?.type === 'webData2')).toBe(false);
    expect(
      payloads.some(
        (payload: any) =>
          payload?.subscription?.type === 'clearinghouseState' && payload?.subscription?.dex === 'vntls',
      ),
    ).toBe(true);
  });

  it('subscribes to spotState and unwraps its nested unified-account payload', () => {
    const parent = createParentMock();
    (parent as any).options = { ...parent.options, applicationId: '0xuser' };
    (parent as any).shouldUseSpotCollateralBalance.mockReturnValue(true);
    (parent as any).shouldTrackSpotHoldings.mockReturnValue(true);
    const ws = new HyperliquidPrivateWebSocket(parent as any);
    const sendMock = vi.fn();

    (ws as any).send = sendMock;
    (ws as any).subscribeSystemTopics();

    const payloads = sendMock.mock.calls.map((args: any[]) => JSON.parse(args[0]));
    expect(payloads).toContainEqual({
      method: 'subscribe',
      subscription: { type: 'spotState', user: '0xuser' },
    });
    expect(payloads.some((payload: any) => payload?.subscription?.type === 'spotClearinghouseState')).toBe(false);

    const spotState = {
      balances: [{ coin: 'USDC', token: 0, total: '100', hold: '0' }],
      tokenToAvailableAfterMaintenance: [[0, '38.32']],
    };
    (ws as any).processMessage({
      channel: 'spotState',
      data: { user: '0xuser', spotState },
    });

    expect(parent.handleSpotClearinghouseStateUpdate).toHaveBeenCalledWith(spotState, 'websocket');
  });

  it('requests portfolio-margin spotState for portfolio-margin accounts', () => {
    const parent = createParentMock();
    (parent as any).options = { ...parent.options, applicationId: '0xuser' };
    (parent as any).shouldUseSpotCollateralBalance.mockReturnValue(true);
    (parent as any).shouldTrackSpotHoldings.mockReturnValue(true);
    (parent as any).shouldRequestPortfolioMarginSpotState.mockReturnValue(true);
    const ws = new HyperliquidPrivateWebSocket(parent as any);
    const sendMock = vi.fn();

    (ws as any).send = sendMock;
    (ws as any).subscribeSystemTopics();

    const payloads = sendMock.mock.calls.map((args: any[]) => JSON.parse(args[0]));
    expect(payloads).toContainEqual({
      method: 'subscribe',
      subscription: { type: 'spotState', user: '0xuser', isPortfolioMargin: true },
    });
  });

  it('accepts the legacy spotClearinghouseState channel during migration', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);
    const spotState = {
      balances: [{ coin: 'USDC', token: 0, total: '40', hold: '1.68' }],
      tokenToAvailableAfterMaintenance: [[0, '38.32']],
    };

    (ws as any).processMessage({ channel: 'spotClearinghouseState', data: spotState });

    expect(parent.handleSpotClearinghouseStateUpdate).toHaveBeenCalledWith(spotState, 'websocket');
  });

  it('unwraps the live core clearinghouseState subscription payload', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    // The live subscription frame wraps the state and represents core perps
    // with an empty dex string rather than omitting dex.
    (ws as any).processMessage({
      channel: 'clearinghouseState',
      data: {
        user: '0xuser',
        dex: '',
        clearinghouseState: createSampleClearinghouseState(),
      },
    });

    expect(parent.store.addOrUpdatePositions).toHaveBeenCalledWith([
      expect.objectContaining({
        symbol: 'BTC-PERP',
        contracts: 0.25,
        side: PositionSide.Long,
      }),
    ]);
    expect(parent.syncBalanceFromLivePositions).toHaveBeenCalledTimes(1);
  });

  it('logs a sanitized Hyperliquid server error', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    (ws as any).processMessage({
      channel: 'error',
      data: 'Error parsing {"subscription":{"type":"webData2","user":"0xprivate"}}',
    });

    expect(parent.log).toHaveBeenCalledWith(
      'Hyperliquid WebSocket server error: rejected webData2 subscription',
      'warning',
    );
    expect(parent.log).not.toHaveBeenCalledWith(expect.stringContaining('0xprivate'), expect.anything());
  });

  it('authoritatively clears stale core positions while preserving HIP-3 positions', () => {
    const parent = createParentMock();
    const hip3Market = {
      id: 'vntls:GAS',
      symbol: 'VNTLS|GAS-PERP',
      info: { hip3Dex: 'vntls' },
    };
    (parent.store as any).marketsById['vntls:GAS'] = hip3Market;
    (parent.store as any).marketsBySymbol['VNTLS|GAS-PERP'] = hip3Market;
    (parent.store as any).positions = [
      { id: 'BTC-PERP', symbol: 'BTC-PERP', contracts: 1 },
      { id: 'VNTLS|GAS-PERP', symbol: 'VNTLS|GAS-PERP', contracts: 2 },
    ];
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.applyCoreClearinghouseState({ assetPositions: [] }, true);

    expect(parent.store.addOrUpdatePositionsFull).toHaveBeenCalledWith([
      expect.objectContaining({ id: 'VNTLS|GAS-PERP', contracts: 2 }),
    ]);
  });

  it('does not let an older REST snapshot overwrite a newer core WebSocket state', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.applyCoreClearinghouseState({ ...createSampleClearinghouseState(), time: 200 });
    (parent.store.addOrUpdatePositionsFull as any).mockClear();

    ws.applyCoreClearinghouseState({ assetPositions: [], time: 100 }, true);

    expect(parent.store.addOrUpdatePositionsFull).not.toHaveBeenCalled();
  });

  it('routes userEvents fill payloads through fill handling', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    (ws as any).processMessage({
      channel: 'userEvents',
      data: {
        user: '0xuser',
        isSnapshot: false,
        fills: [createSampleFill()],
      },
    });

    expect(markFillSeenMock).toHaveBeenCalledWith('456');
    expect(parent.emitter.emit).toHaveBeenCalledWith(
      'fill',
      expect.objectContaining({
        id: '456',
        orderId: '123',
        symbol: 'BTC-PERP',
      }),
    );
  });

  it('applies optimistic core position reconciliation even for deduped fills', () => {
    hasSeenFillMock.mockReturnValue(true);
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.handleFills({
      fills: [createSampleFill() as any],
      isSnapshot: false,
      user: '0xuser',
    });

    expect(parent.store.addOrUpdatePosition).toHaveBeenCalledTimes(1);
    expect(parent.store.addOrUpdatePosition).toHaveBeenCalledWith(
      expect.objectContaining({
        symbol: 'BTC-PERP',
        contracts: 0.5,
        side: PositionSide.Long,
      }),
    );
    expect(markFillSeenMock).not.toHaveBeenCalled();
    expect(parent.emitter.emit).not.toHaveBeenCalled();
  });

  it('emits fill events with net realizedPnl and fee for close fills', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.handleFills({
      fills: [createSampleFill() as any],
      isSnapshot: false,
      user: '0xuser',
    });

    expect(markFillSeenMock).toHaveBeenCalledWith('456');
    expect(parent.emitter.emit).toHaveBeenCalledTimes(1);
    expect(parent.emitter.emit).toHaveBeenCalledWith(
      'fill',
      expect.objectContaining({
        id: '456',
        orderId: '123',
        symbol: 'BTC-PERP',
        side: 'buy',
        price: 100000,
        amount: 0.25,
        realizedPnl: 12,
        fee: 0.5,
      }),
    );
  });

  it('does not emit events for snapshot fills', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.handleFills({
      fills: [createSampleFill() as any],
      isSnapshot: true,
      user: '0xuser',
    });

    expect(markFillSeenMock).not.toHaveBeenCalled();
    expect(parent.emitter.emit).not.toHaveBeenCalled();
  });

  it('seeds realizedPnl from snapshot fills and projects it to positions', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.handleFills({
      fills: [createSampleFill() as any],
      isSnapshot: true,
      user: '0xuser',
    });

    ws.applyCoreClearinghouseState(createSampleClearinghouseState());

    expect(parent.store.addOrUpdatePositions).toHaveBeenCalledTimes(1);
    const firstUpdate = (parent.store.addOrUpdatePositions as any).mock.calls[0][0];

    expect(firstUpdate).toHaveLength(1);
    expect(firstUpdate[0]).toEqual(
      expect.objectContaining({
        symbol: 'BTC-PERP',
        unrealizedPnl: 3.25,
        contracts: 0.25,
        realizedPnl: 12,
      }),
    );
    expect(parent.syncBalanceFromLivePositions).toHaveBeenCalledTimes(1);
  });

  it('accumulates realizedPnl across close fills for same side/symbol', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.handleFills({
      fills: [createSampleFill() as any],
      isSnapshot: false,
      user: '0xuser',
    });

    ws.handleFills({
      fills: [
        {
          ...createSampleFill(),
          tid: 457,
          closedPnl: '-1.5',
          fee: '0.5',
        } as any,
      ],
      isSnapshot: false,
      user: '0xuser',
    });

    ws.applyCoreClearinghouseState(createSampleClearinghouseState());

    expect(parent.store.addOrUpdatePositions).toHaveBeenCalledTimes(1);
    const firstUpdate = (parent.store.addOrUpdatePositions as any).mock.calls[0][0];

    expect(firstUpdate).toHaveLength(1);
    expect(firstUpdate[0]).toEqual(
      expect.objectContaining({
        symbol: 'BTC-PERP',
        unrealizedPnl: 3.25,
        contracts: 0.25,
        realizedPnl: 10,
      }),
    );
  });

  it('defaults missing HIP-3 position type to one-way IDs', () => {
    const parent = createParentMock();
    const hip3Market = {
      id: 'vntls:GAS',
      symbol: 'VNTLS|GAS-PERP',
      info: { hip3Dex: 'vntls' },
    };
    (parent.store as any).marketsById['vntls:GAS'] = hip3Market;
    (parent.store as any).marketsBySymbol['VNTLS|GAS-PERP'] = hip3Market;
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.handleHip3ClearinghouseState('vntls', {
      assetPositions: [
        {
          position: {
            coin: 'vntls:GAS',
            szi: '10',
            entryPx: '1.59',
            positionValue: '15.9',
            leverage: { value: '5', type: 'cross' },
            unrealizedPnl: '0.01',
            liquidationPx: '0',
          },
        },
      ],
    } as any);

    expect(parent.store.addOrUpdatePositionsFull).toHaveBeenCalledTimes(1);
    const positions = (parent.store.addOrUpdatePositionsFull as any).mock.calls[0][0];
    expect(positions).toHaveLength(1);
    expect(positions[0]).toEqual(
      expect.objectContaining({
        id: 'VNTLS|GAS-PERP',
        symbol: 'VNTLS|GAS-PERP',
        side: PositionSide.Long,
        contracts: 10,
        positionMode: PositionMode.OneWay,
      }),
    );
  });

  it('schedules debounced HIP-3 REST position refresh on live HIP-3 fills', () => {
    const parent = createParentMock();
    const ws = new HyperliquidPrivateWebSocket(parent as any);

    ws.handleFills({
      fills: [
        {
          ...createSampleFill(),
          coin: 'vntls:GAS',
          tid: 999,
        } as any,
      ],
      isSnapshot: false,
      user: '0xuser',
    });

    expect(parent.scheduleHip3PositionRefresh).toHaveBeenCalledTimes(1);
    const dexes = (parent.scheduleHip3PositionRefresh as any).mock.calls[0][0] as Set<string>;
    expect(Array.from(dexes)).toEqual(['vntls']);
  });
});
