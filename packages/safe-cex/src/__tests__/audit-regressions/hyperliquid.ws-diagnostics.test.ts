import { afterEach, describe, expect, it, vi } from 'vitest';

import { HyperliquidPrivateWebSocket } from '../reference/hyperliquid/hyperliquid.ws-private';
import { HyperliquidPublicWebSocket } from '../reference/hyperliquid/hyperliquid.ws-public';
import { setWsDiagnosticsEnabled } from '../../utils/wsDiagnostics';

const WALLET_ADDRESS = '0x1111111111111111111111111111111111111111';

const createPublicParent = () =>
  ({
    isDisposed: false,
    name: 'hyperliquid',
    options: { testnet: false },
    store: {
      markets: [{ id: 'BTC', symbol: 'BTC-PERP', precision: { price: 0.1, amount: 0.001 } }],
      positions: [],
      tickersById: {},
      update: vi.fn(),
      updateTicker: vi.fn(),
    },
    emitter: { emit: vi.fn() },
    emitAndLogError: vi.fn(),
    log: vi.fn(),
    syncBalanceFromLivePositions: vi.fn(),
  }) as any;

const createPrivateParent = () => {
  const parent = {
    isDisposed: false,
    name: 'hyperliquid',
    options: { testnet: false, applicationId: WALLET_ADDRESS },
    hip3DexNames: [],
    marketsLowerCaseMap: new Map(),
    store: {
      markets: [
        { id: 'BTC', symbol: 'BTC-PERP' },
        { id: 'ETH', symbol: 'ETH-PERP' },
        { id: 'PURR/USDC', symbol: 'PURR/USDC', isSpot: true },
        { id: 'dex:ABC', symbol: 'DEX|ABC-PERP' },
      ],
      marketsById: {},
      marketsBySymbol: {},
      orders: [],
      positions: [],
      update: vi.fn(),
      addOrUpdatePositions: vi.fn(),
      addOrUpdatePositionsFull: vi.fn(),
      removeOrder: vi.fn(),
    },
    emitter: { emit: vi.fn() },
    log: vi.fn(),
    _getSymbolConfigurationFromAssetData: vi.fn(() => ({})),
    applyCorePerpsBalanceSnapshot: vi.fn(),
    forgetLimitOrderTimeInForce: vi.fn(),
    getFakePositions: vi.fn(() => []),
    getPlacedLimitOrderTimeInForce: vi.fn(),
    getPendingHip3Order: vi.fn(() => null),
    handleSpotClearinghouseStateUpdate: vi.fn(),
    isOrderCancelled: vi.fn(() => false),
    matchPrePlacement: vi.fn(() => null),
    removePendingHip3Order: vi.fn(),
    requestUnifiedSpotCollateralRefresh: vi.fn(),
    scheduleHip3PositionRefresh: vi.fn(),
    scheduleOrderRefresh: vi.fn(),
    shouldRequestPortfolioMarginSpotState: vi.fn(() => false),
    shouldTrackSpotHoldings: vi.fn(() => false),
    syncBalanceFromLivePositions: vi.fn(),
  };

  return parent as any;
};

const emittedMessages = (emitter: { emit: ReturnType<typeof vi.fn> }, eventName: string): string[] =>
  emitter.emit.mock.calls.filter(([event]) => event === eventName).map(([, message]) => message);

describe('Hyperliquid WebSocket diagnostics', () => {
  afterEach(() => setWsDiagnosticsEnabled(false));

  it('emits public send and receive diagnostics without logging market-data floods', () => {
    setWsDiagnosticsEnabled(true);
    const parent = createPublicParent();
    const websocket = new HyperliquidPublicWebSocket(parent);
    const send = vi.fn();
    (websocket as any).send = send;
    Object.defineProperty(websocket, 'isConnected', { get: () => true, configurable: true });

    const unsubscribe = websocket.listenOHLCV({ symbol: 'BTC-PERP', interval: '1m' }, vi.fn());
    websocket.onMessage({ data: '{"channel":"subscriptionResponse","data":{"method":"subscribe"}}' } as MessageEvent);
    websocket.onMessage({ data: '{"channel":"allMids","data":{"mids":{"BTC":"100"}}}' } as MessageEvent);
    websocket.onMessage({ data: '{"channel":"bbo","data":{"coin":"BTC","bbo":[]}}' } as MessageEvent);

    expect(send).toHaveBeenCalledWith(
      JSON.stringify({ method: 'subscribe', subscription: { type: 'candle', coin: 'BTC', interval: '1m' } }),
    );
    expect(emittedMessages(parent.emitter, 'grafana-pws-sub')).toEqual([
      '[hyperliquid public] {"method":"subscribe","subscription":{"type":"candle","coin":"BTC","interval":"1m"}}',
    ]);
    expect(emittedMessages(parent.emitter, 'grafana-pws')).toEqual([
      '[hyperliquid public] {"channel":"subscriptionResponse","data":{"method":"subscribe"}}',
    ]);

    unsubscribe();
  });

  it('emits private order diagnostics with wallet addresses redacted', () => {
    setWsDiagnosticsEnabled(true);
    const parent = createPrivateParent();
    const websocket = new HyperliquidPrivateWebSocket(parent);
    const send = vi.fn();
    (websocket as any).send = send;

    (websocket as any).subscribeSystemTopics();
    websocket.onMessage({
      data: JSON.stringify({ channel: 'orderUpdates', data: { user: WALLET_ADDRESS, updates: [] } }),
    } as MessageEvent);
    websocket.onMessage({
      data: JSON.stringify({ channel: 'clearinghouseState', data: { user: WALLET_ADDRESS, assetPositions: [] } }),
    } as MessageEvent);

    const sendLogs = emittedMessages(parent.emitter, 'grafana-pws-sub');
    expect(sendLogs.some((message) => message.includes('"type":"orderUpdates"'))).toBe(true);
    expect(sendLogs.every((message) => !message.includes(WALLET_ADDRESS))).toBe(true);
    expect(sendLogs.some((message) => message.includes('[redacted]'))).toBe(true);

    expect(emittedMessages(parent.emitter, 'grafana-pws')).toEqual([
      '[hyperliquid private] {"channel":"orderUpdates","data":{"user":"[redacted]","updates":[]}}',
    ]);
  });

  it('skips noisy private activeAssetData subscription diagnostics', () => {
    const parent = createPrivateParent();
    const websocket = new HyperliquidPrivateWebSocket(parent);
    const send = vi.fn();
    (websocket as any).send = send;

    (websocket as any).subscribeAllActiveAssetData();

    expect(send).toHaveBeenCalledTimes(2);
    expect(emittedMessages(parent.emitter, 'grafana-pws-sub')).toEqual([]);
  });
});
