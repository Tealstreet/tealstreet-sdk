import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import '../transport/registerTealstreetTransport';

import { WS_MIN_RECONNECTION_DELAY } from '../config/config';
import { resolveLighterDeploymentProfile } from '../exchanges/lighter/lighter.types';
import { LighterPrivateWebsocket } from './reference/lighter/lighter.ws-private';
import { resolveExchangeRouting } from '../transport/tealstreet/routing/resolveExchangeRouting';

const OPEN_STATE = typeof WebSocket !== 'undefined' ? WebSocket.OPEN : 1;
const MOCK_RATE_LIMIT_WARNING =
  'Lighter rate limit temporarily active for this account/session. Please wait before retrying.';
const WS_PROXY_PRIVATE_ROBINHOOD = resolveExchangeRouting('lighter-robinhood', {
  tealstreetWsRoute: true,
}).wsPrivate.host!;

const createMockParent = () =>
  ({
    name: 'lighter',
    lighterDeploymentProfile: resolveLighterDeploymentProfile({ exchangeName: 'lighter', testnet: false }),
    options: {
      tealstreetWsRoute: false,
      testnet: false,
    },
    isDisposed: false,
    log: vi.fn(),
    emitter: {
      emit: vi.fn(),
    },
    syncSocketHealthToStore: vi.fn(),
    fetchAndSetPrivateSnapshot: vi.fn(),
    fetchAndSetBalance: vi.fn().mockResolvedValue(undefined),
    fetchOrders: vi.fn().mockResolvedValue([]),
    fetchPositions: vi.fn().mockResolvedValue([]),
    shouldSkipOrderReconciliationFetch: vi.fn(() => false),
    invalidateAccountSnapshotCache: vi.fn(),
    getPrivateWsAccountIndex: vi.fn().mockReturnValue('1'),
    getPrivateWsAuthToken: vi.fn().mockResolvedValue('token'),
    getAccountRateLimitGuidanceMessage: vi.fn(() => MOCK_RATE_LIMIT_WARNING),
    mapOrder: vi.fn().mockReturnValue([]),
    mapPosition: vi.fn().mockReturnValue(null),
    recentlyCancelledOrders: new Map<string, number>(),
    store: {
      marketsById: {},
      positions: [],
      addOrUpdatePositionsFull: vi.fn(),
      addOrUpdateOrders: vi.fn(),
      removeOrders: vi.fn(),
      updatePositions: vi.fn(),
      addOrUpdateOrdersFull: vi.fn(),
    },
  }) as any;

const createOpenWs = () =>
  ({
    readyState: OPEN_STATE,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
    send: vi.fn(),
  }) as any;

describe('LighterPrivateWebsocket close/backoff behavior', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    (LighterPrivateWebsocket as any).readonlyFallbackWarningAtByAccount?.clear?.();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('treats policy-style 1008 pre-open closes as handshake failures and enters readonly fallback', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).failedHandshakes = 2;
    (ws as any).readonlyFallbackFailures = 2;

    (ws as any).onClose({ code: 1008, reason: 'restricted jurisdiction policy block' });

    expect((ws as any).failedHandshakes).toBe(3);
    expect(ws.isReadonlyFallbackActive()).toBe(true);
  });

  it('does not treat auth-style 1008 closes as readonly fallback signal', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).failedHandshakes = 2;

    (ws as any).onClose({ code: 1008, reason: 'invalid auth token' });

    expect((ws as any).failedHandshakes).toBe(2);
    expect(ws.isReadonlyFallbackActive()).toBe(false);
  });

  it('applies rate-limit reconnect floor for 1013 closes', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    const setTimeoutSpy = vi.spyOn(global, 'setTimeout');

    (ws as any).onClose({ code: 1013, reason: 'Try again later' });

    const delays = setTimeoutSpy.mock.calls.map((call) => Number(call[1])).filter((delay) => Number.isFinite(delay));
    const reconnectDelay = delays[0];
    const minRateLimitDelay = (LighterPrivateWebsocket as any).RATE_LIMIT_RECONNECT_DELAY_MS as number;

    expect(reconnectDelay).toBeGreaterThanOrEqual(minRateLimitDelay);
    expect((ws as any).wsRateLimitedUntil).toBeGreaterThan(0);
  });

  it('does not enter readonly fallback on repeated 1013 rate-limit closes', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();

    (ws as any).onClose({ code: 1013, reason: 'Try again later' });
    (ws as any).onClose({ code: 1013, reason: 'Try again later' });
    (ws as any).onClose({ code: 1013, reason: 'Try again later' });

    expect(ws.isReadonlyFallbackActive()).toBe(false);
    expect((ws as any).failedHandshakes).toBe(3);
    expect((ws as any).readonlyFallbackFailures).toBe(0);
  });

  it('uses the Robinhood private proxy endpoint when ws route is enabled', async () => {
    const parent = createMockParent();
    parent.name = 'lighter-robinhood';
    parent.lighterDeploymentProfile = resolveLighterDeploymentProfile({
      exchangeName: 'lighter-robinhood',
      testnet: false,
    });
    parent.options.tealstreetWsRoute = true;
    const ws = new LighterPrivateWebsocket(parent);
    const createWebsocketSpy = vi.spyOn(ws as any, 'createWebsocket').mockReturnValue(createOpenWs());

    await ws.connectAndSubscribe();

    const firstUrl = String(createWebsocketSpy.mock.calls[0]?.[0] || '');
    expect(firstUrl).toBe(WS_PROXY_PRIVATE_ROBINHOOD);
    expect(firstUrl).not.toContain(parent.lighterDeploymentProfile.wsUrl);
  });

  it('uses backoff for early post-open close without private stream messages', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).openedSinceLastConnect = true;
    (ws as any).lastConnectTime = Date.now() - 1000;
    (ws as any).accountStreamMessageCount = 0;
    (ws as any).failedHandshakes = 2;
    const setTimeoutSpy = vi.spyOn(global, 'setTimeout');

    (ws as any).onClose({ code: 1006, reason: '' });

    const delays = setTimeoutSpy.mock.calls.map((call) => Number(call[1])).filter((delay) => Number.isFinite(delay));
    const reconnectDelay = delays[0];

    expect((ws as any).failedHandshakes).toBe(3);
    expect(reconnectDelay).toBeGreaterThan(WS_MIN_RECONNECTION_DELAY);
  });

  it('does not auto-schedule readonly promotion probes', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).reconnect = vi.fn();
    const setTimeoutSpy = vi.spyOn(global, 'setTimeout');

    // Trigger readonly fallback via handshake failures.
    (ws as any).failedHandshakes = 2;
    (ws as any).readonlyFallbackFailures = 2;
    (ws as any).onClose({ code: 1006, reason: '' });

    const scheduledDelays = setTimeoutSpy.mock.calls
      .map((call) => Number(call[1]))
      .filter((delay) => Number.isFinite(delay));
    expect(ws.isReadonlyFallbackActive()).toBe(true);
    expect(scheduledDelays).toHaveLength(1);
  });

  it('keeps readonly mode sticky after entering fallback', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).failedHandshakes = 2;
    (ws as any).readonlyFallbackFailures = 2;

    (ws as any).onClose({ code: 1006, reason: '' });
    expect(ws.isReadonlyFallbackActive()).toBe(true);

    // Subsequent close cycles should keep readonly sticky and not flip mode.
    (ws as any).onClose({ code: 1000, reason: 'normal closure' });
    expect(ws.isReadonlyFallbackActive()).toBe(true);
  });

  it('does not duplicate readonly fallback warning across close/open cycle', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).failedHandshakes = 2;
    (ws as any).readonlyFallbackFailures = 2;

    (ws as any).onClose({ code: 1006, reason: '' });
    ws.onOpen();

    const readonlyWarnings = parent.emitter.emit.mock.calls.filter(
      (call: any[]) =>
        call[0] === 'warning' && String(call[1]).includes('georestrictions detected; connectivity status is degraded'),
    );
    expect(readonlyWarnings).toHaveLength(1);
  });

  it('treats 30003 already-subscribed as benign and latches account_all_orders ack', () => {
    const parent = createMockParent();
    parent.options.tealstreetWsRoute = true;
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).hasPendingAccountAllOrdersSubscribe = true;
    (ws as any).hasReceivedAccountAllOrdersAck = false;

    ws.onMessage({
      data: JSON.stringify({
        type: 'error',
        error: {
          code: 30003,
          message: 'Already Subscribed to account_all_orders:718895',
        },
      }),
    } as MessageEvent);

    expect((ws as any).hasPendingAccountAllOrdersSubscribe).toBe(false);
    expect((ws as any).hasReceivedAccountAllOrdersAck).toBe(true);
    expect((ws as any).hasWarnedProxyStreamErrorToast).toBe(false);
    const warningEvents = parent.emitter.emit.mock.calls.filter((call: any[]) => call[0] === 'warning');
    expect(warningEvents).toHaveLength(0);
  });

  it('does not send duplicate account_all_orders subscribe after ack on the same connection', async () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    const socket = createOpenWs();
    ws.ws = socket;

    await (ws as any).authenticateAndSubscribe();
    expect(socket.send).toHaveBeenCalledWith(expect.stringContaining('account_all_orders/1'));

    socket.send.mockClear();
    (ws as any).hasReceivedAccountAllOrdersAck = true;

    await (ws as any).authenticateAndSubscribe();

    const sentChannels = socket.send.mock.calls.map(([payload]: [string]) => JSON.parse(payload).channel);
    expect(sentChannels).toContain('account_all/1');
    expect(sentChannels).not.toContain('account_all_orders/1');
  });

  it('skips WS-triggered order reconciliation when a recent fetch is active', async () => {
    const parent = createMockParent();
    parent.shouldSkipOrderReconciliationFetch.mockReturnValue(true);
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();

    ws.throttleFetchOrders(10, { skipIfRecent: true });
    await vi.advanceTimersByTimeAsync(10);

    expect(parent.fetchOrders).not.toHaveBeenCalled();

    ws.throttleFetchOrders(10);
    await vi.advanceTimersByTimeAsync(10);

    expect(parent.fetchOrders).toHaveBeenCalledTimes(1);
  });

  it('invalidates account snapshots and refreshes balance after account stream updates', async () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();

    ws.onMessage({
      data: JSON.stringify({
        channel: 'account_all/1',
        type: 'update/account_all',
        data: {
          positions: [],
        },
      }),
    } as MessageEvent);

    expect(parent.invalidateAccountSnapshotCache).toHaveBeenCalledTimes(1);
    expect(parent.fetchAndSetBalance).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(500);

    expect(parent.fetchAndSetBalance).toHaveBeenCalledWith({
      allowRecentAccountSnapshot: false,
      includeAccountLimits: false,
    });
  });

  it('still warns for non-benign private stream errors', () => {
    const parent = createMockParent();
    parent.options.tealstreetWsRoute = true;
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();

    ws.onMessage({
      data: JSON.stringify({
        type: 'error',
        error: {
          code: 30001,
          message: 'Some non-benign stream error',
        },
      }),
    } as MessageEvent);

    expect((ws as any).hasWarnedProxyStreamErrorToast).toBe(true);
    const warningEvents = parent.emitter.emit.mock.calls.filter((call: any[]) => call[0] === 'warning');
    expect(warningEvents.length).toBeGreaterThan(0);
  });

  it('keeps first self-healing rate-limit warning out of force-toast routing', () => {
    const parent = createMockParent();
    parent.options.tealstreetWsRoute = true;
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();

    ws.onMessage({
      data: JSON.stringify({
        type: 'error',
        error: {
          code: 30050,
          message: 'Too many websocket messages',
        },
      }),
    } as MessageEvent);

    expect((ws as any).hasWarnedProxyStreamErrorToast).toBe(false);
    const warningEvents = parent.emitter.emit.mock.calls.filter((call: any[]) => call[0] === 'warning');
    expect(warningEvents).toHaveLength(0);
  });

  it('force-toasts rate-limit warnings only after they persist past cooldown', () => {
    const parent = createMockParent();
    parent.options.tealstreetWsRoute = true;
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();

    const rateLimitMessage = {
      type: 'error',
      error: {
        code: 30050,
        message: 'Too many websocket messages',
      },
    };

    ws.onMessage({ data: JSON.stringify(rateLimitMessage) } as MessageEvent);
    vi.advanceTimersByTime((LighterPrivateWebsocket as any).RATE_LIMIT_COOLDOWN_MS + 1);
    ws.onMessage({ data: JSON.stringify(rateLimitMessage) } as MessageEvent);

    expect((ws as any).hasWarnedProxyStreamErrorToast).toBe(true);
    expect(parent.getAccountRateLimitGuidanceMessage).toHaveBeenCalled();
    expect(parent.emitter.emit).toHaveBeenCalledWith('warning', MOCK_RATE_LIMIT_WARNING, { forceToast: true });
  });

  it('uses parent-owned tier-aware rate-limit warning copy', () => {
    const parent = createMockParent();
    parent.options.tealstreetWsRoute = true;
    parent.getAccountRateLimitGuidanceMessage.mockReturnValue(
      'Lighter Standard account rate limit temporarily active. Standard accounts have very small API limits; please wait before retrying or switch the Lighter account tier to Plus or Premium if this persists.',
    );
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();

    const rateLimitMessage = {
      type: 'error',
      error: {
        code: 30050,
        message: 'Too many websocket messages',
      },
    };

    ws.onMessage({ data: JSON.stringify(rateLimitMessage) } as MessageEvent);
    vi.advanceTimersByTime((LighterPrivateWebsocket as any).RATE_LIMIT_COOLDOWN_MS + 1);
    ws.onMessage({ data: JSON.stringify(rateLimitMessage) } as MessageEvent);

    const warningEvents = parent.emitter.emit.mock.calls.filter((call: any[]) => call[0] === 'warning');
    expect(warningEvents).toHaveLength(1);
    expect(String(warningEvents[0][1])).toContain('Standard accounts have very small API limits');
    expect(String(warningEvents[0][1])).toContain('Plus or Premium');
  });

  it('uses degraded georestriction wording for rate-limit warnings in direct readonly fallback', () => {
    const parent = createMockParent();
    parent.options.tealstreetWsRoute = false;
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    (ws as any).useReadonlyFallback = true;

    ws.onMessage({
      data: JSON.stringify({
        type: 'error',
        error: {
          code: 30050,
          message: 'Too many websocket messages',
        },
      }),
    } as MessageEvent);

    expect((ws as any).hasWarnedProxyStreamErrorToast).toBe(true);
    const warningEvents = parent.emitter.emit.mock.calls.filter((call: any[]) => call[0] === 'warning');
    expect(warningEvents).toHaveLength(1);
    expect(String(warningEvents[0][1])).toContain('georestrictions detected; connectivity status is degraded');
    expect(String(warningEvents[0][1])).toContain('account tier changes will not fix this');
  });

  it('keeps private socket health green during normal quiet periods', () => {
    const parent = createMockParent();
    const ws = new LighterPrivateWebsocket(parent);
    ws.ws = createOpenWs();
    ws.lastReceived = Date.now() - 120_000;

    expect(ws.isHealthy).toBe(true);
  });
});
