import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BlofinPublicWebsocket } from './reference/blofin/blofin.ws-public';

const CONNECTION_FAILURE_NOTICE_DELAY_MS = 20_000;

const createMockParent = () =>
  ({
    name: 'blofin',
    options: { testnet: false, publicDataOnly: true, key: '' },
    isDisposed: false,
    log: vi.fn(),
    emitter: {
      emit: vi.fn(),
      on: vi.fn(),
      off: vi.fn(),
    },
    syncSocketHealthToStore: vi.fn(),
    store: {
      markets: [],
    },
  }) as any;

const createMockWs = () =>
  ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  }) as any;

describe('BlofinPublicWebsocket connection failure notice', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('force-toasts a clear warning when the public socket never delivers a message', () => {
    const parent = createMockParent();
    const websocket = new BlofinPublicWebsocket(parent);
    vi.spyOn(websocket as any, 'createWebsocket').mockReturnValue(createMockWs());

    websocket.connectAndSubscribe();
    vi.advanceTimersByTime(CONNECTION_FAILURE_NOTICE_DELAY_MS - 1);

    expect(parent.emitter.emit).not.toHaveBeenCalledWith('warning', expect.anything(), expect.anything());

    vi.advanceTimersByTime(1);

    expect(parent.emitter.emit).toHaveBeenCalledWith(
      'warning',
      expect.stringMatching(/BloFin live WebSocket connection failed to establish.*live charts and order books/i),
      { forceToast: true },
    );
    expect(parent.log).toHaveBeenCalledWith(expect.stringContaining('failed to establish'), 'warning');
  });

  it('does not repeat the warning while the same outage keeps reconnecting', () => {
    const parent = createMockParent();
    const websocket = new BlofinPublicWebsocket(parent);
    vi.spyOn(websocket as any, 'createWebsocket').mockReturnValue(createMockWs());

    websocket.connectAndSubscribe();
    vi.advanceTimersByTime(CONNECTION_FAILURE_NOTICE_DELAY_MS);
    websocket.connectAndSubscribe();
    vi.advanceTimersByTime(CONNECTION_FAILURE_NOTICE_DELAY_MS * 2);

    expect(parent.emitter.emit).toHaveBeenCalledTimes(1);
  });

  it('cancels the warning as soon as the socket delivers a message', () => {
    const parent = createMockParent();
    const websocket = new BlofinPublicWebsocket(parent);
    vi.spyOn(websocket as any, 'createWebsocket').mockReturnValue(createMockWs());

    websocket.connectAndSubscribe();
    vi.advanceTimersByTime(CONNECTION_FAILURE_NOTICE_DELAY_MS / 2);
    websocket.onMessage({ data: '{"event":"subscribe"}' } as MessageEvent);
    vi.advanceTimersByTime(CONNECTION_FAILURE_NOTICE_DELAY_MS / 2);

    expect(parent.emitter.emit).not.toHaveBeenCalledWith('warning', expect.anything(), expect.anything());
  });

  it('arms a new one-time warning when an established socket later closes', () => {
    const parent = createMockParent();
    const websocket = new BlofinPublicWebsocket(parent);
    const mockWs = createMockWs();
    vi.spyOn(websocket as any, 'createWebsocket').mockReturnValue(mockWs);

    websocket.connectAndSubscribe();
    websocket.onMessage({ data: '{"event":"subscribe"}' } as MessageEvent);
    const closeListener = mockWs.addEventListener.mock.calls.find(([event]: [string]) => event === 'close')?.[1];

    expect(closeListener).toBeTypeOf('function');
    closeListener?.();
    vi.advanceTimersByTime(CONNECTION_FAILURE_NOTICE_DELAY_MS);

    expect(parent.emitter.emit).toHaveBeenCalledWith(
      'warning',
      expect.stringContaining('live WebSocket connection failed to establish'),
      { forceToast: true },
    );
  });

  it('clears the pending notice when disposed', () => {
    const parent = createMockParent();
    const websocket = new BlofinPublicWebsocket(parent);
    vi.spyOn(websocket as any, 'createWebsocket').mockReturnValue(createMockWs());

    websocket.connectAndSubscribe();
    parent.isDisposed = true;
    websocket.dispose();
    vi.advanceTimersByTime(CONNECTION_FAILURE_NOTICE_DELAY_MS);

    expect(parent.emitter.emit).not.toHaveBeenCalledWith('warning', expect.anything(), expect.anything());
  });
});
