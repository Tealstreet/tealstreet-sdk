import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it } from 'vitest';

import {
  HttpTiming,
  httpTimingKind,
  httpTimingStage,
  tunnelHostLabel,
  upstreamMsFromHeaders,
  WINDOW_MS,
} from './httpTiming';

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
  it('extracts only closed order-stage labels and never copies body values', () => {
    const origin = 'https://api.hyperliquid.xyz';
    for (const [action, expected] of [
      ['order', 'place'],
      ['cancel', 'cancel'],
      ['cancelByCloid', 'cancel'],
      ['batchModify', 'modify'],
    ] as const) {
      expect(
        httpTimingStage(`${origin}/exchange`, JSON.stringify({ action: { type: action, secret: 'never-log' } })),
      ).toBe(expected);
    }
    for (const [type, expected] of [
      ['orderStatus', 'orderStatus'],
      ['maxBuilderFee', 'builderApproval'],
      ['userRole', 'identity'],
    ] as const) {
      expect(httpTimingStage(`${origin}/info`, JSON.stringify({ type, user: 'never-log' }))).toBe(expected);
    }
    for (const body of [undefined, '', 'null', '[1]', '{', '{"type":"candleSnapshot"}', '{"type":"never-log"}']) {
      expect(httpTimingStage(`${origin}/info`, body)).toBeUndefined();
    }
    expect(httpTimingStage(`${origin}/other`, '{"type":"orderStatus"}')).toBeUndefined();
  });
  it('times an order prerequisite individually while ordinary info reads stay aggregated', () => {
    const { events, timing } = harness();
    timing.record({ route: 'direct', kind: 'info', stage: 'orderStatus', outcome: 'ok', queuedMs: 4_000, wireMs: 20 });
    expect(events).toEqual([
      expect.objectContaining({
        code: 'transport.http.timing',
        fields: expect.objectContaining({ stage: 'orderStatus', queuedMs: 4_000, wireMs: 20 }),
      }),
    ]);
    timing.record({ route: 'direct', kind: 'info', outcome: 'ok', queuedMs: 1, wireMs: 10 });
    expect(events).toHaveLength(1);
    timing.flush();
    expect(events[1].code).toBe('transport.http.window');
  });
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
