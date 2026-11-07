// Real HOW + reconnect loop with wall-clock jumps and no timer advancement.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { HOWWebSocket } from './HOWWebSocket';
import { onTunnelLaneFallback, resetTunnelLane, TOKYO_HOW_EGRESS, tunnelLanePort } from './tunnelEndpoints';

class ConnectingSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static instances: ConnectingSocket[] = [];
  static throwOnClose = false;
  readyState = 0;
  binaryType = 'arraybuffer';
  bufferedAmount = 0;
  closeCalls = 0;
  private listeners: Record<string, ((event: any) => void)[]> = {};
  constructor(readonly url: string) {
    ConnectingSocket.instances.push(this);
  }
  addEventListener(type: string, callback: (event: any) => void) {
    (this.listeners[type] ??= []).push(callback);
  }
  removeEventListener(type: string, callback: (event: any) => void) {
    this.listeners[type] = (this.listeners[type] ?? []).filter((value) => value !== callback);
  }
  fire(type: string, event: any = { type }) {
    for (const callback of [...(this.listeners[type] ?? [])]) callback(event);
  }
  send() {}
  close() {
    this.closeCalls++;
    if (ConnectingSocket.throwOnClose) throw new Error('handshake close refused');
    this.readyState = 3;
    this.fire('close', { code: 1000, reason: '' });
  }
  open() {
    this.readyState = 1;
    this.fire('open');
  }
}

const ports = () => ConnectingSocket.instances.map((socket) => new URL(socket.url).port || '443');

describe('HOW wall-clock connect expiry', () => {
  let how: HOWWebSocket;
  let page: EventTarget & { visibilityState: string };
  let browser: EventTarget;
  let fallback: ReturnType<typeof vi.fn<(failures: number) => void>>;
  let unsubscribe: () => void;
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-11-04T00:00:00Z'));
    vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'error').mockImplementation(() => {});
    page = Object.assign(new EventTarget(), { visibilityState: 'hidden' });
    browser = new EventTarget();
    vi.stubGlobal('document', page);
    vi.stubGlobal('window', browser);
    resetTunnelLane(true);
    ConnectingSocket.instances = [];
    ConnectingSocket.throwOnClose = false;
    fallback = vi.fn<(failures: number) => void>();
    unsubscribe = onTunnelLaneFallback(fallback);
    how = new HOWWebSocket(TOKYO_HOW_EGRESS, ConnectingSocket as unknown as typeof WebSocket);
    how.connectAndSubscribe();
  });
  afterEach(() => {
    ConnectingSocket.throwOnClose = false;
    how.dispose();
    unsubscribe();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    resetTunnelLane();
    vi.useRealTimers();
  });

  async function advanceToNextDial() {
    const count = ConnectingSocket.instances.length;
    for (let elapsed = 0; elapsed < 10_000 && ConnectingSocket.instances.length === count; elapsed += 1) {
      await vi.advanceTimersByTimeAsync(1);
    }
    expect(ConnectingSocket.instances).toHaveLength(count + 1);
  }

  it('a hidden tick abandons a 997-second-old attempt before its delayed timeout callback', async () => {
    await vi.advanceTimersByTimeAsync(1);
    const first = ConnectingSocket.instances[0];
    vi.setSystemTime(Date.now() + 997_000);
    await vi.advanceTimersByTimeAsync(1_001);
    expect(first.closeCalls).toBe(1);
    expect(ports()).toEqual(['8443']);
    expect(how.ws?.loopState.phase).toBe('retry-scheduled');
    expect(tunnelLanePort()).toBe('8443');
  });

  it.each(['visibilitychange', 'resume', 'pageshow'])(
    '%s immediately expires a 962-second-old connect',
    async (event) => {
      await vi.advanceTimersByTimeAsync(1);
      const first = ConnectingSocket.instances[0];
      vi.setSystemTime(Date.now() + 962_000);
      page.visibilityState = 'visible';
      (event === 'pageshow' ? browser : page).dispatchEvent(new Event(event));
      expect(first.closeCalls).toBe(1); // No timers ran, including RWS's one-shot.
      expect(ports()).toEqual(['8443']);
      expect(how.ws?.loopState.phase).toBe('retry-scheduled');
    },
  );

  it.each([false, true])(
    'counts each abandoned attempt once and falls back after two (close throws=%s)',
    async (throws) => {
      ConnectingSocket.throwOnClose = throws;
      await vi.advanceTimersByTimeAsync(1);
      page.visibilityState = 'visible';
      vi.setSystemTime(Date.now() + 6_001);
      page.dispatchEvent(new Event('visibilitychange'));
      expect(how.ws?.retryCount).toBe(1);
      expect(how.ws?.loopState.phase).toBe('retry-scheduled');
      await advanceToNextDial();
      expect(ports()).toEqual(['8443', '8443']);
      page.dispatchEvent(new Event('resume'));
      browser.dispatchEvent(new Event('pageshow'));
      expect(ConnectingSocket.instances[1].closeCalls).toBe(0);
      expect(tunnelLanePort()).toBe('8443');
      vi.setSystemTime(Date.now() + 6_001);
      browser.dispatchEvent(new Event('pageshow'));
      expect(how.ws?.retryCount).toBe(2);
      await advanceToNextDial();
      expect(ports()).toEqual(['8443', '8443', '443']);
      expect(fallback).toHaveBeenCalledExactlyOnceWith(2);
      expect(ConnectingSocket.instances.slice(0, 2).map((socket) => socket.closeCalls)).toEqual([1, 1]);
    },
  );

  it('retires a pre-dial stall without counting a handshake or changing the lane', async () => {
    expect(ConnectingSocket.instances).toHaveLength(0);
    const wrapper = how.ws;
    vi.setSystemTime(Date.now() + 997_000);
    page.dispatchEvent(new Event('resume'));
    expect(how.ws).toBe(wrapper);
    expect(how.ws?.loopState.phase).toBe('retry-scheduled');
    await advanceToNextDial();
    expect(ports()).toEqual(['8443']);
    vi.setSystemTime(Date.now() + 6_001);
    page.visibilityState = 'visible';
    page.dispatchEvent(new Event('visibilitychange'));
    await advanceToNextDial();
    expect(ports()).toEqual(['8443', '8443']);
    expect(tunnelLanePort()).toBe('8443');
    expect(fallback).not.toHaveBeenCalled();
  });

  it('lets the native timeout win through the audit margin', async () => {
    await vi.advanceTimersByTimeAsync(1);
    const wrapper = how.ws;
    const first = ConnectingSocket.instances[0];
    const now = Date.now();
    vi.setSystemTime(now + 5_999);
    page.dispatchEvent(new Event('resume'));
    expect(first.closeCalls).toBe(0);
    expect(how.ws).toBe(wrapper);
    // Restore normal timer delivery after checking the suspended-clock margin.
    vi.setSystemTime(now);
    await vi.advanceTimersByTimeAsync(4_000);
    expect(first.closeCalls).toBe(1);
    expect(how.ws).toBe(wrapper);
    expect(how.ws?.retryCount).toBe(1);
    expect(how.ws?.loopState.phase).toBe('retry-scheduled');
  });

  it('keeps the same wrapper and growing backoff after consecutive audited timeouts', async () => {
    await vi.advanceTimersByTimeAsync(1);
    const wrapper = how.ws;
    const waits: number[] = [];
    for (let attempt = 0; attempt < 3; attempt++) {
      vi.setSystemTime(Date.now() + 997_000);
      page.dispatchEvent(new Event('resume'));
      expect(how.ws).toBe(wrapper);
      expect(wrapper?.retryCount).toBe(attempt + 1);
      expect(wrapper?.loopState.phase).toBe('retry-scheduled');
      const started = Date.now();
      await advanceToNextDial();
      waits.push(Date.now() - started);
    }
    expect(waits[0]).toBeGreaterThanOrEqual(1_000);
    expect(waits[1]).toBeGreaterThan(waits[0]);
    expect(waits[2]).toBeGreaterThan(waits[1]);
  });

  it('keeps a fresh connect and a healthy open socket on visibility changes', async () => {
    await vi.advanceTimersByTimeAsync(1);
    page.visibilityState = 'visible';
    vi.setSystemTime(Date.now() + 3_998);
    page.dispatchEvent(new Event('visibilitychange'));
    expect(ConnectingSocket.instances[0].closeCalls).toBe(0);
    ConnectingSocket.instances[0].open();
    vi.setSystemTime(Date.now() + 997_000);
    page.dispatchEvent(new Event('resume'));
    browser.dispatchEvent(new Event('pageshow'));
    expect(how.isOpen).toBe(true);
    expect(ConnectingSocket.instances[0].closeCalls).toBe(0);
    expect(fallback).not.toHaveBeenCalled();
  });

  it('ignores stale open/close events from an abandoned socket and removes lifecycle hooks on dispose', async () => {
    await vi.advanceTimersByTimeAsync(1);
    const first = ConnectingSocket.instances[0];
    vi.setSystemTime(Date.now() + 6_001);
    page.dispatchEvent(new Event('resume'));
    await advanceToNextDial();
    first.open();
    first.fire('close', { code: 1006 });
    expect(how.isOpen).toBe(false);
    expect(tunnelLanePort()).toBe('8443');
    how.dispose();
    vi.setSystemTime(Date.now() + 997_000);
    page.visibilityState = 'visible';
    page.dispatchEvent(new Event('visibilitychange'));
    page.dispatchEvent(new Event('resume'));
    browser.dispatchEvent(new Event('pageshow'));
    await vi.advanceTimersByTimeAsync(10_000);
    expect(ConnectingSocket.instances).toHaveLength(2);
    expect(fallback).not.toHaveBeenCalled();
  });
});

describe('HOW in a browser worker (no window or document)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    resetTunnelLane();
    vi.useRealTimers();
  });

  it('dials and still expires a stuck connect by wall-clock age', async () => {
    vi.useFakeTimers();
    vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.stubGlobal('window', undefined);
    vi.stubGlobal('document', undefined);
    resetTunnelLane(true);
    ConnectingSocket.instances = [];
    const how = new HOWWebSocket(TOKYO_HOW_EGRESS, ConnectingSocket as unknown as typeof WebSocket);
    expect(() => how.connectAndSubscribe()).not.toThrow();
    await vi.advanceTimersByTimeAsync(1);
    const first = ConnectingSocket.instances[0];
    vi.setSystemTime(Date.now() + 997_000);
    await vi.advanceTimersByTimeAsync(1_001);
    expect(first.closeCalls).toBe(1);
    how.dispose();
  });

  it.each(['throwing getter', 'invalid URL'])('keeps the first dial retryable after a %s', async (failure) => {
    vi.useFakeTimers();
    vi.stubGlobal('window', undefined);
    vi.stubGlobal('document', undefined);
    ConnectingSocket.instances = [];
    const url = vi.fn().mockImplementationOnce(() => {
      if (failure === 'throwing getter') throw new Error('configured URL getter failed');
      return 'not a websocket URL';
    }).mockReturnValue(TOKYO_HOW_EGRESS);
    const how = new HOWWebSocket(url, ConnectingSocket as unknown as typeof WebSocket);
    try {
      expect(() => how.connectAndSubscribe()).toThrow();
      expect(how.isInitialized).toBe(false);
      expect(how.ws).toBeUndefined();
      // The adapter only tries the initial dial while this flag is false.
      if (!how.isInitialized) how.connectAndSubscribe();
      expect(how.isInitialized).toBe(true);
      expect(how.ws?.onopen).toBeTypeOf('function');
      expect(how.ws?.onmessage).toBeTypeOf('function');
      expect(how.ws?.onclose).toBeTypeOf('function');
      await vi.advanceTimersByTimeAsync(1);
      expect(ConnectingSocket.instances).toHaveLength(1);
      expect(url).toHaveBeenCalledTimes(2);
    } finally {
      how.dispose();
    }
  });

  it('never guards page globals with typeof, which Next folds to "object" in worker chunks', async () => {
    const { readFileSync } = await import('node:fs');
    const source = readFileSync(new URL('./HOWWebSocket.ts', import.meta.url), 'utf8');
    expect(source).not.toMatch(/typeof (window|document)\b/);
  });
});
