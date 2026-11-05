import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

/**
 * Per-request REST timing for Hyperliquid accounts, so log-ingest shows each
 * user's tunnel round trip. A write (`/exchange`) emits one
 * `transport.http.timing` split into queued (engine prepare to dispatch),
 * wire (dispatch to the tunnel's answer) and upstream (the tunnel's own
 * request to the venue, when it reports one); reads are folded into one
 * `transport.http.window` per route per minute. Every field is an enum or an
 * integer of milliseconds or counts: no URL, body, address or venue text.
 */
export type HttpTimingRoute = 'tunnel' | 'proxy' | 'direct';
export type HttpTimingKind = 'exchange' | 'info' | 'other';
export type HttpTimingOutcome = 'ok' | 'timeout' | 'error';

const TUNNEL_HOSTS: Record<string, string> = {
  'fsjnpy143pbz-tyo.tealstreet.io': 'vultr-tyo',
  'fsjnpy143pbz.jp.tealstreet.io': 'aws-jp',
  'fsjnpy143pbz.asia.tealstreet.io': 'aws-asia',
};

/** The tunnel deployment a `wss://` endpoint names, as a closed label. */
export const tunnelHostLabel = (endpoint: string | undefined): string => {
  if (!endpoint) return 'none';
  try {
    return TUNNEL_HOSTS[new URL(endpoint).hostname] ?? 'other';
  } catch {
    return 'other';
  }
};

export const httpTimingKind = (url: string): HttpTimingKind => {
  try {
    const path = new URL(url).pathname;
    return path === '/exchange' ? 'exchange' : path === '/info' ? 'info' : 'other';
  } catch {
    return 'other';
  }
};

export const WINDOW_MS = 60_000;
const MAX_WINDOW_SAMPLES = 4_096;

const whole = (ms: number) => Math.max(0, Math.min(2_147_483_647, Math.round(ms)));

const quantile = (sorted: number[], q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];

type Window = { startedAt: number; durations: number[]; timeouts: number; errors: number };

/** The tunnel's own request to the venue, from its `x-tunnel-upstream-ms` response header. */
export const TUNNEL_UPSTREAM_HEADER = 'x-tunnel-upstream-ms';

export const upstreamMsFromHeaders = (headers: string | undefined): number | undefined => {
  const match = headers?.match(/^x-tunnel-upstream-ms:\s*(\d{1,9})\s*$/im);
  return match ? Number(match[1]) : undefined;
};

export interface HttpTimingSample {
  route: HttpTimingRoute;
  kind: HttpTimingKind;
  outcome: HttpTimingOutcome;
  /**
   * From the engine's prepare (issued when it accepts the command) to dispatch:
   * proxy token, tunnel socket, signing, info budget and admission waits.
   */
  queuedMs: number;
  /** From dispatch to the tunnel's answer or the failure. */
  wireMs: number;
  /** The tunnel's own request to the venue, when the tunnel reports it. */
  upstreamMs?: number;
  status?: number;
}

export class HttpTiming {
  private readonly windows = new Map<HttpTimingRoute, Window>();

  constructor(
    private readonly tunnelHost: string,
    private readonly tunnelPing: () => number | undefined,
    private readonly emit: (event: EngineEvent) => void,
    private readonly now: () => number = Date.now,
  ) {}

  record(sample: HttpTimingSample) {
    try {
      if (sample.kind === 'exchange') this.emit(this.timing(sample));
      else this.accumulate(sample);
    } catch {
      /* Telemetry cannot fail the request. */
    }
  }

  /** Emits every open window; the next sample opens a new one. */
  flush() {
    for (const [route, window] of this.windows) {
      this.windows.delete(route);
      if (window.durations.length || window.timeouts || window.errors) this.emit(this.window(route, window));
    }
  }

  private timing(sample: HttpTimingSample): EngineEvent {
    const queuedMs = whole(sample.queuedMs);
    const wireMs = whole(sample.wireMs);
    const elapsedMs = whole(sample.queuedMs + sample.wireMs);
    const fields: EngineEvent['fields'] = {
      route: sample.route,
      tunnelHost: sample.route === 'tunnel' ? this.tunnelHost : 'none',
      requestKind: sample.kind,
      outcome: sample.outcome,
      elapsedMs,
      queuedMs,
      wireMs,
    };
    if (sample.upstreamMs !== undefined) fields.upstreamMs = whole(sample.upstreamMs);
    if (sample.status !== undefined && Number.isInteger(sample.status)) fields.httpStatus = sample.status;
    const ping = this.ping(sample.route);
    if (ping !== undefined) fields.tunnelPingMs = ping;
    const upstream = fields.upstreamMs === undefined ? '' : `, upstream ${fields.upstreamMs}ms`;
    return {
      level: 'info',
      code: 'transport.http.timing',
      message: `${sample.kind} ${sample.outcome} ${elapsedMs}ms via ${sample.route}: queued ${queuedMs}ms, wire ${wireMs}ms${upstream}`,
      fields,
    };
  }

  private accumulate(sample: HttpTimingSample) {
    const now = this.now();
    let window = this.windows.get(sample.route);
    if (window && now - window.startedAt >= WINDOW_MS) {
      this.windows.delete(sample.route);
      this.emit(this.window(sample.route, window));
      window = undefined;
    }
    if (!window) {
      window = { startedAt: now, durations: [], timeouts: 0, errors: 0 };
      this.windows.set(sample.route, window);
    }
    if (sample.outcome === 'timeout') window.timeouts++;
    else if (sample.outcome === 'error') window.errors++;
    else if (window.durations.length < MAX_WINDOW_SAMPLES) window.durations.push(sample.wireMs);
  }

  private window(route: HttpTimingRoute, window: Window): EngineEvent {
    const sorted = [...window.durations].sort((a, b) => a - b);
    const fields: EngineEvent['fields'] = {
      route,
      tunnelHost: route === 'tunnel' ? this.tunnelHost : 'none',
      count: sorted.length,
      timeouts: window.timeouts,
      errors: window.errors,
    };
    if (sorted.length) {
      fields.p50Ms = whole(quantile(sorted, 0.5));
      fields.p95Ms = whole(quantile(sorted, 0.95));
      fields.maxMs = whole(sorted[sorted.length - 1]);
    }
    const ping = this.ping(route);
    if (ping !== undefined) fields.tunnelPingMs = ping;
    const spread = sorted.length ? ` p50 ${fields.p50Ms}ms p95 ${fields.p95Ms}ms max ${fields.maxMs}ms` : '';
    return {
      level: 'info',
      code: 'transport.http.window',
      message: `reads via ${route}: ${sorted.length} ok${spread}, ${window.timeouts} timeouts, ${window.errors} errors`,
      fields,
    };
  }

  private ping(route: HttpTimingRoute): number | undefined {
    if (route !== 'tunnel') return undefined;
    const ping = this.tunnelPing();
    return typeof ping === 'number' && Number.isFinite(ping) ? whole(ping) : undefined;
  }
}
