import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it } from 'vitest';

import { HttpTiming, httpTimingKind, tunnelHostLabel, upstreamMsFromHeaders, WINDOW_MS } from './httpTiming';

const harness = (ping?: number) => {
  const events: EngineEvent[] = [];
  let now = 1_000;
  const timing = new HttpTiming(
    'vultr-tyo',
    () => ping,
    (event) => events.push(event),
    () => now,
  );
  return { events, timing, advance: (ms: number) => (now += ms) };
};

describe('HttpTiming', () => {
  it('labels tunnel hosts and request kinds from closed sets', () => {
    expect(tunnelHostLabel('wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel')).toBe('vultr-tyo');
    expect(tunnelHostLabel('wss://fsjnpy143pbz.jp.tealstreet.io/tunnel')).toBe('aws-jp');
    expect(tunnelHostLabel('wss://fsjnpy143pbz.asia.tealstreet.io/tunnel')).toBe('aws-asia');
    expect(tunnelHostLabel('wss://example.test/tunnel')).toBe('other');
    expect(tunnelHostLabel(undefined)).toBe('none');
    expect(httpTimingKind('https://api.hyperliquid.xyz/exchange')).toBe('exchange');
    expect(httpTimingKind('https://api.hyperliquid.xyz/info')).toBe('info');
    expect(httpTimingKind('https://api.hyperliquid.xyz/other?x=1')).toBe('other');
  });

  it('reads the tunnel upstream time only from its own bounded header', () => {
    expect(upstreamMsFromHeaders('content-type: application/json\nx-tunnel-upstream-ms: 57')).toBe(57);
    expect(upstreamMsFromHeaders('X-Tunnel-Upstream-Ms: 8\n')).toBe(8);
    expect(upstreamMsFromHeaders('x-tunnel-upstream-ms: -1')).toBeUndefined();
    expect(upstreamMsFromHeaders('x-tunnel-upstream-ms: 1234567890')).toBeUndefined();
    expect(upstreamMsFromHeaders('x-other: 5')).toBeUndefined();
    expect(upstreamMsFromHeaders(undefined)).toBeUndefined();
  });

  it('emits one bounded event per write, with whole milliseconds', () => {
    const { events, timing } = harness(81.6);
    timing.record({
      route: 'tunnel',
      kind: 'exchange',
      outcome: 'ok',
      queuedMs: 3.4,
      wireMs: 212.6,
      upstreamMs: 57,
      status: 200,
    });
    expect(events).toEqual([
      {
        level: 'info',
        code: 'transport.http.timing',
        message: 'exchange ok 216ms via tunnel: queued 3ms, wire 213ms, upstream 57ms',
        fields: {
          route: 'tunnel',
          tunnelHost: 'vultr-tyo',
          requestKind: 'exchange',
          outcome: 'ok',
          elapsedMs: 216,
          queuedMs: 3,
          wireMs: 213,
          upstreamMs: 57,
          httpStatus: 200,
          tunnelPingMs: 82,
        },
      },
    ]);
  });

  it('a write off the tunnel names no tunnel host or ping', () => {
    const { events, timing } = harness(80);
    timing.record({ route: 'proxy', kind: 'exchange', outcome: 'timeout', queuedMs: 1, wireMs: 20_000 });
    expect(events[0].fields).toEqual({
      route: 'proxy',
      tunnelHost: 'none',
      requestKind: 'exchange',
      outcome: 'timeout',
      elapsedMs: 20_001,
      queuedMs: 1,
      wireMs: 20_000,
    });
  });

  it('folds reads into one window per route per minute', () => {
    const { events, timing, advance } = harness(75);
    for (const wireMs of [100, 90, 300, 110]) {
      timing.record({ route: 'tunnel', kind: 'info', outcome: 'ok', queuedMs: 1, wireMs });
      advance(1_000);
    }
    timing.record({ route: 'tunnel', kind: 'info', outcome: 'timeout', queuedMs: 1, wireMs: 20_000 });
    timing.record({ route: 'tunnel', kind: 'other', outcome: 'error', queuedMs: 1, wireMs: 5 });
    expect(events).toEqual([]);
    advance(WINDOW_MS);
    timing.record({ route: 'tunnel', kind: 'info', outcome: 'ok', queuedMs: 1, wireMs: 95 });
    expect(events).toEqual([
      {
        level: 'info',
        code: 'transport.http.window',
        message: 'reads via tunnel: 4 ok p50 110ms p95 300ms max 300ms, 1 timeouts, 1 errors',
        fields: {
          route: 'tunnel',
          tunnelHost: 'vultr-tyo',
          count: 4,
          timeouts: 1,
          errors: 1,
          p50Ms: 110,
          p95Ms: 300,
          maxMs: 300,
          tunnelPingMs: 75,
        },
      },
    ]);
    timing.flush();
    expect(events[1].fields).toMatchObject({ count: 1, p50Ms: 95 });
    timing.flush();
    expect(events).toHaveLength(2);
  });

  it('a throwing sink never fails the request', () => {
    const timing = new HttpTiming(
      'aws-jp',
      () => undefined,
      () => {
        throw new Error('sink');
      },
    );
    expect(() =>
      timing.record({ route: 'tunnel', kind: 'exchange', outcome: 'ok', queuedMs: 1, wireMs: 1 }),
    ).not.toThrow();
  });
});
