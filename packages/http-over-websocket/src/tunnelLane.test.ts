// The browser's 8443 tunnel lane: which urls it rewrites, when it falls back to
// 443, and that the HOW and relay sockets dial through it.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { HOWWebSocket } from './HOWWebSocket';
import { RelayWebSocket } from './RelayWebSocket';
import {
  buildRelayUrl,
  noteTunnelDialFailed,
  noteTunnelDialOpened,
  onTunnelLaneFallback,
  parseRelayUrl,
  resetTunnelLane,
  TOKYO_HOW_EGRESS,
  tunnelDialUrl,
  tunnelEgressUrl,
  tunnelLanePort,
  tunnelOutage,
} from './tunnelEndpoints';

const portOf = (url: string) => new URL(url).port || '443';

/** A WebSocket whose handshake fails on 8443 (or on every port, when `refuseAll`), else opens. */
class LaneSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  CONNECTING = 0;
  OPEN = 1;
  CLOSING = 2;
  CLOSED = 3;
  static dialed: string[] = [];
  static refuse = new Set(['8443']);

  readyState = 0;
  binaryType = 'blob';
  bufferedAmount = 0;
  private listeners: Record<string, Array<(event: any) => void>> = { open: [], message: [], close: [], error: [] };

  constructor(readonly url: string) {
    LaneSocket.dialed.push(url);
    setTimeout(() => {
      if (this.readyState !== 0) return;
      if (LaneSocket.refuse.has(portOf(url))) {
        this.readyState = 3;
        this.fire('error', { type: 'error' });
        this.fire('close', { code: 1006, reason: '', wasClean: false });
      } else {
        this.readyState = 1;
        this.fire('open', { type: 'open' });
      }
    }, 10);
  }

  addEventListener(type: string, listener: (event: any) => void) {
    this.listeners[type]?.push(listener);
  }
  removeEventListener(type: string, listener: (event: any) => void) {
    if (this.listeners[type]) this.listeners[type] = this.listeners[type].filter((l) => l !== listener);
  }
  private fire(type: string, event: any) {
    for (const listener of [...(this.listeners[type] ?? [])]) listener(event);
  }
  send() {}
  close() {
    if (this.readyState === 3) return;
    this.readyState = 3;
    this.fire('close', { code: 1000, reason: '', wasClean: true });
  }
}

beforeEach(() => {
  resetTunnelLane(true);
  LaneSocket.dialed = [];
  LaneSocket.refuse = new Set(['8443']);
});

afterEach(() => {
  vi.useRealTimers();
  resetTunnelLane();
});

describe('tunnelDialUrl', () => {
  it('moves only the tunnel socket to 8443, keeping its path and query', () => {
    expect(tunnelDialUrl(TOKYO_HOW_EGRESS)).toBe('wss://fsjnpy143pbz-tyo.tealstreet.io:8443/tunnel');
    const relay = buildRelayUrl('jp', 'wss://stream.bybit.com/v5/private');
    expect(tunnelDialUrl(parseRelayUrl(relay).tunnelUrl)).toBe('wss://fsjnpy143pbz-tyo.tealstreet.io:8443/tunnel');
    expect(tunnelDialUrl(`${TOKYO_HOW_EGRESS}?v=2&t=1`)).toBe(
      'wss://fsjnpy143pbz-tyo.tealstreet.io:8443/tunnel?v=2&t=1',
    );
  });

  it('leaves the per-venue paths, REST and other hosts on their port', () => {
    for (const url of [
      'wss://fsjnpy143pbz-tyo.tealstreet.io/grvt',
      'wss://fsjnpy143pbz-tyo.tealstreet.io/lighter/stream',
      'wss://fsjnpy143pbz-tyo.tealstreet.io/bybitbra/v5/private',
      'https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy',
      'wss://bybit-proxy-ja.tealstreet.io/v5/private',
      'ws://127.0.0.1:9000/tunnel',
    ])
      expect(tunnelDialUrl(url)).toBe(url);
  });

  it('never rewrites outside a browser realm (the hosted servers)', () => {
    resetTunnelLane(false);
    expect(tunnelDialUrl(TOKYO_HOW_EGRESS)).toBe(TOKYO_HOW_EGRESS);
    expect(tunnelLanePort()).toBe('443');
  });

  it('builds every production tunnel url as wss, on either port and after the fallback', () => {
    const relay = parseRelayUrl(buildRelayUrl('sg', 'wss://ws.okx.com/ws/v5/private')).tunnelUrl;
    const urls = () => [
      tunnelEgressUrl('jp'),
      tunnelEgressUrl('sg'),
      buildRelayUrl('jp', 'wss://ws.bitget.com/v2/ws/private'),
      tunnelDialUrl(tunnelEgressUrl('jp')),
      tunnelDialUrl(relay),
    ];
    const lane = urls();
    noteTunnelDialFailed(lane[3]);
    noteTunnelDialFailed(lane[3]);
    expect(tunnelLanePort()).toBe('443');
    const fallback = urls();
    expect(lane.map(portOf).slice(3)).toEqual(['8443', '8443']);
    expect(fallback.map(portOf).slice(3)).toEqual(['443', '443']);
    for (const url of [...lane, ...fallback]) expect(url.startsWith('wss://')).toBe(true);
  });
});

describe('lane fallback', () => {
  it('falls back to 443 after two consecutive 8443 failures, once, and stays there', () => {
    const fallbacks = vi.fn();
    const unsubscribe = onTunnelLaneFallback(fallbacks);
    const lane = tunnelDialUrl(TOKYO_HOW_EGRESS);
    noteTunnelDialFailed(lane);
    expect(tunnelLanePort()).toBe('8443');
    noteTunnelDialFailed(lane);
    expect(tunnelLanePort()).toBe('443');
    expect(fallbacks).toHaveBeenCalledWith(2);
    noteTunnelDialOpened(TOKYO_HOW_EGRESS);
    noteTunnelDialFailed(lane);
    expect(tunnelDialUrl(TOKYO_HOW_EGRESS)).toBe(TOKYO_HOW_EGRESS);
    expect(fallbacks).toHaveBeenCalledTimes(1);
    unsubscribe();
  });

  it('stays on 8443 when a handshake opens between failures', () => {
    const lane = tunnelDialUrl(TOKYO_HOW_EGRESS);
    noteTunnelDialFailed(lane);
    noteTunnelDialOpened(lane);
    noteTunnelDialFailed(lane);
    expect(tunnelLanePort()).toBe('8443');
  });

  it('does not count failures on 443 against the lane', () => {
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    expect(tunnelLanePort()).toBe('8443');
  });
});

describe('HOW socket on the lane', () => {
  it('dials 8443, and after two failed handshakes reconnects on 443 for good', async () => {
    vi.useFakeTimers();
    const how = new HOWWebSocket(() => tunnelEgressUrl('jp'), LaneSocket as unknown as typeof WebSocket);
    how.connectAndSubscribe();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(LaneSocket.dialed.slice(0, 3).map(portOf)).toEqual(['8443', '8443', '443']);
    expect(how.isOpen).toBe(true);
    expect(tunnelLanePort()).toBe('443');
    how.dispose();
  });

  it('stays on 8443 when the lane opens', async () => {
    vi.useFakeTimers();
    LaneSocket.refuse = new Set();
    const how = new HOWWebSocket(() => tunnelEgressUrl('jp'), LaneSocket as unknown as typeof WebSocket);
    how.connectAndSubscribe();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(how.isOpen).toBe(true);
    expect(LaneSocket.dialed.map(portOf)).toEqual(['8443']);
    expect(tunnelLanePort()).toBe('8443');
    how.dispose();
  });
});

describe('relay socket on the lane', () => {
  const relayUrl = buildRelayUrl('jp', 'wss://stream.bybit.com/v5/private');
  const dial = () => new RelayWebSocket(relayUrl, [], {}, LaneSocket as unknown as typeof WebSocket);

  it('dials 8443, and the third relay after two failed handshakes dials 443', async () => {
    vi.useFakeTimers();
    dial();
    dial();
    await vi.advanceTimersByTimeAsync(100);
    const third = dial();
    expect(LaneSocket.dialed.map(portOf)).toEqual(['8443', '8443', '443']);
    expect(third.tunnelUrl).toBe('wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel');
    third.close();
  });

  it('does not count a relay the caller closed before it opened', async () => {
    vi.useFakeTimers();
    dial().close();
    dial().close();
    await vi.advanceTimersByTimeAsync(100);
    expect(tunnelLanePort()).toBe('8443');
  });
});

describe('tunnel reachability', () => {
  it('is unreachable after two failed handshakes on 443 once the lane fell back, until one opens', () => {
    vi.useFakeTimers();
    const lane = tunnelDialUrl(TOKYO_HOW_EGRESS);
    noteTunnelDialFailed(lane);
    vi.advanceTimersByTime(5_000);
    noteTunnelDialFailed(lane);
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    expect(tunnelOutage()).toBeUndefined();
    vi.advanceTimersByTime(7_000);
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    expect(tunnelOutage()).toEqual({
      episode: expect.any(Number),
      portsTried: '8443+443',
      failures: 4,
      sinceFirstFailureMs: 12_000,
    });
    noteTunnelDialOpened(TOKYO_HOW_EGRESS);
    expect(tunnelOutage()).toBeUndefined();
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    expect(tunnelOutage()).toMatchObject({ portsTried: '443', failures: 2 });
  });

  it('never marks the tunnel unreachable while the lane is still on 8443, or outside a browser', () => {
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    expect(tunnelOutage()).toBeUndefined();
    resetTunnelLane(false);
    for (let attempt = 0; attempt < 4; attempt += 1) noteTunnelDialFailed(TOKYO_HOW_EGRESS);
    expect(tunnelOutage()).toBeUndefined();
  });
});
