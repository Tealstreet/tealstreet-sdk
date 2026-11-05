/**
 * Aggregate REST diagnostics for one process, per venue.
 *
 * Exists because three failure modes on the hosted server's private polling —
 * a 403 from something in front of the venue, a connection reset mid-response,
 * and a 20s client timeout — are indistinguishable from outside the container.
 * Everything that would tell them apart is already in hand at request time and
 * currently thrown away: the venue's own rate-limit headers, which layer
 * answered a non-2xx, and where the wall clock actually went.
 *
 * Aggregates only. No account, key, symbol, request body or request header ever
 * reaches this — request headers carry signatures, so they are never read.
 * `getVenueRestStats()` is safe for the public `/healthz`; the failure samples
 * from `drainVenueRestFailureSamples()` carry a scrubbed response-body snippet
 * and are for the container's authenticated Sentry path only.
 */

import { isUnsentSigningFailure } from './unknownSendOutcome';

/** Config keys, string-named to match the existing `safeCex*` config-tag convention. */
const T0 = 'safeCexRestDiagStartedAt';
const SIGN_MS = 'safeCexRestDiagSignMs';
const PROXY_HEADER = 'safeCexRestDiagProxyHeader';
const SIGNED_AT = 'safeCexRestDiagSignedAt';
const SIG_AGE_MS = 'safeCexRestDiagSignatureAgeMs';

const WINDOW_MS = 5 * 60_000;
const MAX_SAMPLES = 25;
const MAX_LATENCIES = 200;
/** Bounds `errorCounts`, whose keys come from error text rather than a closed set. */
const MAX_ERROR_KEYS = 20;
/**
 * Long enough to keep what distinguishes one cause from another. At 60 all
 * three remote-signer failures truncated to the same prefix and collapsed into
 * one meaningless bucket.
 */
const ERROR_KEY_CHARS = 120;
const SNIPPET_CHARS = 200;

/**
 * Venue quota headers. Bybit reports the remaining allowance of the endpoint's
 * bucket on EVERY private response, so how close we run to the limit is
 * observable continuously and never needs a failure to sample.
 */
const QUOTA_HEADERS = [
  'x-bapi-limit',
  'x-bapi-limit-status',
  'x-bapi-limit-reset-timestamp',
  'x-mbx-used-weight-1m',
  'ratelimit-remaining',
  'retry-after',
];

/** Names the layer that answered, which is the whole question on a bare 403. */
const ORIGIN_HEADERS = ['server', 'via', 'x-cache', 'x-amz-cf-id', 'cf-ray'];

export interface VenueRestFailureSample {
  at: number;
  exchange: string;
  path?: string;
  status?: number;
  code?: string;
  totalMs?: number;
  signMs?: number;
  signatureAgeMs?: number;
  bodyKind: string;
  bodySnippet?: string;
  /**
   * The shape of the proxy credential the request ACTUALLY went out with, read
   * off the final config at failure time. The shape only, never the value.
   *
   * `bearer` means we sent a well-formed credential, so a 401/403 from the edge
   * is about the token or the edge, not about us omitting it. `absent` and
   * `malformed` are both ours, and the edge cannot tell you which: its
   * viewer-request answers the same 401 for a missing header as for a value
   * that does not begin with `Bearer `. `bodySnippet` cannot tell you either —
   * it reaches Sentry as `[Filtered]`.
   *
   * `undefined` means the REST proxy was not in play, which is not the same as
   * unauthenticated: a `tealstreetTunnel` venue resolves no proxy URL and so is
   * never stamped, yet its request does carry our credential on the HOW socket.
   * Read `undefined` as "this field has nothing to say", not as a finding.
   *
   * DO NOT put `auth`, `token`, `credential`, `secret` or `key` back in this
   * field name. It shipped once as `sentProxyAuth` and arrived in production as
   * `[Filtered]`: Sentry's scrubber matches `auth` inside the name, and the
   * camelCase hump does not protect it — `sentproxyauth` contains `auth`.
   */
  restProxyHeaderSent?: 'bearer' | 'malformed' | 'absent';
  errorName?: string;
  errorMessage?: string;
  /** Failed while getting its signature, so it never reached the venue. */
  unsentSigning: boolean;
  endpoint?: string;
  headers: Record<string, string>;
}

interface ExchangeDiagnostics {
  requests: number;
  ok: number;
  failures: number;
  /** Hosted requests that failed while signing, so never reached the venue. Not failures OF the venue. */
  unsentSigning: number;
  timestamps: number[];
  totalMs: number[];
  signMs: number[];
  /** Lowest remaining quota seen in the window — the number that says "close to the limit". */
  minQuotaRemaining?: number;
  lastQuota?: Record<string, string>;
  statusCounts: Record<string, number>;
  codeCounts: Record<string, number>;
  errorCounts: Record<string, number>;
  lastEndpoint?: string;
  maxSignatureAgeMs?: number;
}

const byExchange = new Map<string, ExchangeDiagnostics>();
const failureSamples: VenueRestFailureSample[] = [];

const emptyDiagnostics = (): ExchangeDiagnostics => ({
  requests: 0,
  ok: 0,
  failures: 0,
  unsentSigning: 0,
  timestamps: [],
  totalMs: [],
  signMs: [],
  statusCounts: {},
  codeCounts: {},
  errorCounts: {},
});

const forExchange = (exchange: string): ExchangeDiagnostics => {
  const existing = byExchange.get(exchange);
  if (existing) return existing;
  const created = emptyDiagnostics();
  byExchange.set(exchange, created);
  return created;
};

const push = (values: number[], value: number) => {
  values.push(value);
  if (values.length > MAX_LATENCIES) values.splice(0, values.length - MAX_LATENCIES);
};

/**
 * Prune on WRITE, never only in the reader.
 *
 * `/healthz` is pull-only and nothing polls it, so a container nobody curls
 * would otherwise accumulate one timestamp per REST request for the life of the
 * process — and at a 500ms tick doing three signed calls, that is the one
 * structure here that grows without bound. Everything else is capped by
 * `MAX_LATENCIES`, `MAX_SAMPLES`, or the cardinality of its keys.
 */
const noteTimestamp = (diagnostics: ExchangeDiagnostics, now: number) => {
  diagnostics.timestamps.push(now);
  const cutoff = now - WINDOW_MS;
  if (diagnostics.timestamps[0] >= cutoff) return;
  diagnostics.timestamps = diagnostics.timestamps.filter((at) => at >= cutoff);
};

const bump = (counts: Record<string, number>, key: string) => {
  counts[key] = (counts[key] ?? 0) + 1;
};

const readConfigNumber = (config: unknown, key: string): number | undefined => {
  const value = (config as Record<string, unknown> | undefined)?.[key];
  return typeof value === 'number' ? value : undefined;
};

/**
 * Path only, no query string. Kraken rewrites `config.url` to `path?postData`
 * and several venues bake signed params into the URL, so a raw URL would carry
 * nonces and signatures into a diagnostic that is read by people.
 */
const pathOf = (url: string | undefined): string | undefined => {
  if (!url) return undefined;
  const path = url.split('?')[0];
  return path ? path.slice(0, 80) : undefined;
};

/**
 * Mask runs that look like a key, signature or token — and only those.
 *
 * A bare length threshold does not work. At 12 characters it ate `disconnected`
 * out of `WebSocket disconnected — signing aborted`, which is the word that
 * distinguishes one signer failure from another, plus `wallet-balance`,
 * `Transitional` and `Content-Type`. Real secrets here are api keys, HMAC
 * signatures and nanoids, all of which mix letters and digits; English words do
 * not. So require that mix, and keep a pure-length rule only for runs far
 * longer than any word.
 */
const looksLikeSecret = (run: string): boolean =>
  run.length >= 32 || (/\d/.test(run) && /[A-Za-z]/.test(run));

const scrub = (text: string): string =>
  text
    .replace(/[A-Za-z0-9_-]{12,}/g, (run) => (looksLikeSecret(run) ? '<redacted>' : run))
    .slice(0, SNIPPET_CHARS)
    .trim();

/**
 * Read an allowlist of headers off either a plain object or an `AxiosHeaders`.
 *
 * Node's axios hands back `AxiosHeaders`, so the accessor is tried first. Its
 * own properties are already normalised to lowercase and enumerable, but
 * relying on that alone is how the most important signal here would silently
 * read as absent after an axios upgrade rather than visibly break.
 */
const readHeaders = (source: unknown, names: string[]): Record<string, string> => {
  const out: Record<string, string> = {};
  if (!source || typeof source !== 'object') return out;
  const record = source as { get?: (name: string) => unknown } & Record<string, unknown>;
  const lower = new Map<string, unknown>();
  for (const [key, value] of Object.entries(record)) lower.set(key.toLowerCase(), value);
  for (const name of names) {
    const value = typeof record.get === 'function' ? (record.get(name) ?? lower.get(name)) : lower.get(name);
    if (value != null) out[name] = String(value).slice(0, 120);
  }
  return out;
};

/**
 * What KIND of thing answered, and the text when it was not the venue.
 *
 * A venue JSON error already reaches Sentry through the error message, so it
 * needs no snippet. An HTML body means something in front of the venue
 * answered instead, and its text is the only thing that says which.
 */
// An error page's reason line sits past its <head> and styles, beyond the snippet cut.
const visibleText = (html: string): string =>
  html
    // Bounded: the style/script strip rescans to the end for each unclosed tag.
    .slice(0, 64_000)
    .replace(/<(style|script)[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

const classifyBody = (data: unknown): { bodyKind: string; bodySnippet?: string } => {
  if (data == null || data === '') return { bodyKind: 'empty' };
  if (typeof data === 'object') {
    const record = data as Record<string, unknown>;
    if ('retCode' in record || 'retMsg' in record || 'code' in record) return { bodyKind: 'venue-json' };
    return { bodyKind: 'json' };
  }
  const text = String(data);
  const lower = text.toLowerCase();
  if (lower.includes('cloudfront')) return { bodyKind: 'cloudfront', bodySnippet: scrub(visibleText(text)) };
  if (lower.includes('<html') || lower.includes('<!doctype')) return { bodyKind: 'html', bodySnippet: scrub(visibleText(text)) };
  return { bodyKind: 'text', bodySnippet: scrub(text) };
};

/**
 * Collapse an error message to a bounded, indexable shape.
 *
 * Needed because the dominant failure class here is NOT an axios error: a bare
 * `throw` inside the adapter chain arrives with no response, no code, no config
 * and no url, so every field this module was built around is empty and the
 * message is the only thing that identifies it. Digits go to `#` so one cause
 * is one key rather than one key per id, timestamp or duration.
 */
const normalizeErrorKey = (message: string): string =>
  scrub(message).replace(/\d+/g, '#').replace(/\s+/g, ' ').trim().slice(0, ERROR_KEY_CHARS) || 'unknown';

const bumpBounded = (counts: Record<string, number>, key: string) => {
  if (counts[key] === undefined && Object.keys(counts).length >= MAX_ERROR_KEYS) {
    counts.other = (counts.other ?? 0) + 1;
    return;
  }
  counts[key] = (counts[key] ?? 0) + 1;
};

/**
 * The host we actually dialled, which is the question a CloudFront 403 cannot
 * answer on its own: both the venue and our own tunnel sit behind CloudFront
 * and return a byte-identical error page, and the distribution id in `via` is
 * scrubbed before it reaches anywhere readable.
 *
 * The proxied form embeds the upstream (`https://<proxy>/https://api.bybit.com`),
 * so keeping the whole base URL minus its query is what distinguishes direct
 * from tunnelled at a glance. No credential rides in a base URL.
 */
const endpointOf = (config: unknown): string | undefined => {
  const typed = config as { baseURL?: unknown; url?: unknown } | undefined;
  const base = typeof typed?.baseURL === 'string' ? typed.baseURL : '';
  const url = typeof typed?.url === 'string' ? typed.url : '';
  const absolute = /^https?:\/\//i.test(url) ? url : base;
  if (!absolute) return undefined;
  return absolute.split('?')[0].slice(0, 120);
};

/**
 * Record that the proxy-auth interceptor attached a credential (it refuses the
 * request rather than send none). Stamped on the CONFIG so that a missing header
 * at failure time reads as "lost on the way", distinct from "this request never
 * went through the proxy".
 */
export const markRestProxyHeader = <T>(config: T): T => {
  try {
    (config as Record<string, unknown>)[PROXY_HEADER] = 'present';
  } catch {
    /* a sealed config must not be able to fail the request */
  }
  return config;
};

/**
 * What the request ACTUALLY carried, read off the final config at failure time.
 *
 * The stamp alone was not enough, and the way it was not enough cost half a day.
 * It is written by the interceptor, which runs long before the wire: remote
 * signing then replaces the whole header set with the browser's, and the HOW
 * adapter rebuilds it again. So the stamp records what we INTENDED to send.
 * EXCHANGE-SERVER-27 reported `present` while the edge answered "Authorization
 * header is missing", and those two are only contradictory if you believe the
 * stamp describes the wire. It does not.
 *
 * The header is therefore read here, from `error.config` — the object axios
 * actually dispatched — and the stamp is kept only to tell "the proxy was never
 * in play" apart from "it was, and the header went missing on the way".
 *
 * `malformed` exists because the edge's viewer-request 401s on BOTH an absent
 * header and a value that does not begin with `Bearer `, while a bad token
 * inside a well-formed value is a 403. Those are three different bugs wearing
 * two status codes, and only this can separate them.
 */
const proxyHeaderOf = (config: unknown): 'bearer' | 'malformed' | 'absent' | undefined => {
  const stamped = (config as Record<string, unknown> | undefined)?.[PROXY_HEADER];
  if (stamped !== 'present') return undefined;

  const sent = readHeaders((config as { headers?: unknown } | undefined)?.headers, [
    'authorization',
    'tealstreet-authorization',
  ]);
  const value = sent['authorization'] || sent['tealstreet-authorization'];
  if (!value) return 'absent';
  return value.startsWith('Bearer ') ? 'bearer' : 'malformed';
};

export const markRestRequestStart = <T>(config: T, now = Date.now()): T => {
  try {
    (config as Record<string, unknown>)[T0] = now;
  } catch {
    /* a sealed config must not be able to fail the request */
  }
  return config;
};

/**
 * Record how long the browser round trip took, and how stale the signature it
 * produced already is. A signature that ages past the venue's `recvWindow`
 * while it waits in a client-side rate-limit queue is rejected by the venue and
 * looks nothing like a network problem, so the two must be separable.
 */
export const markRestSigned = <T>(config: T, signMs: number, now = Date.now()): T => {
  try {
    return stampSigned(config, signMs, now);
  } catch {
    /* a sealed config must not be able to fail the request */
    return config;
  }
};

const stampSigned = <T>(config: T, signMs: number, now: number): T => {
  const record = config as Record<string, unknown>;
  record[SIGN_MS] = signMs;
  record[SIGNED_AT] = now;
  // `AxiosHeaders` normalises to lowercase own-properties but is still a class
  // with its own accessor, and the signing adapter always hands one over. Go
  // through `get()` when it exists — a plain property read that quietly misses
  // would leave this number permanently absent rather than visibly wrong.
  const headers = record.headers as { get?: (name: string) => unknown } & Record<string, unknown>;
  const stamped =
    typeof headers?.get === 'function'
      ? headers.get('x-bapi-timestamp')
      : (headers?.['X-BAPI-TIMESTAMP'] ?? headers?.['x-bapi-timestamp']);
  const stampedMs = Number(stamped);
  if (Number.isFinite(stampedMs) && stampedMs > 0) record[SIG_AGE_MS] = now - stampedMs;
  return config;
};

export const noteRestSuccess = (exchange: string, response: unknown, now = Date.now()): void => {
  try {
    const typed = response as { config?: unknown; headers?: unknown } | undefined;
    const diagnostics = forExchange(exchange);
    const startedAt = readConfigNumber(typed?.config, T0);

    diagnostics.requests += 1;
    diagnostics.ok += 1;
    noteTimestamp(diagnostics, now);
    if (startedAt !== undefined) push(diagnostics.totalMs, now - startedAt);
    const signMs = readConfigNumber(typed?.config, SIGN_MS);
    if (signMs !== undefined) push(diagnostics.signMs, signMs);

    const endpoint = endpointOf(typed?.config);
    if (endpoint) diagnostics.lastEndpoint = endpoint;

    const quota = readHeaders(typed?.headers, QUOTA_HEADERS);
    if (Object.keys(quota).length > 0) {
      diagnostics.lastQuota = quota;
      const remaining = Number(quota['x-bapi-limit-status'] ?? quota['ratelimit-remaining']);
      if (Number.isFinite(remaining)) {
        diagnostics.minQuotaRemaining =
          diagnostics.minQuotaRemaining === undefined ? remaining : Math.min(diagnostics.minQuotaRemaining, remaining);
      }
    }
  } catch {
    /* diagnostics must never break a request */
  }
};

export const noteRestFailure = (exchange: string, error: unknown, now = Date.now()): void => {
  try {
    const typed = error as
      | { config?: unknown; code?: unknown; response?: { status?: number; headers?: unknown; data?: unknown } }
      | undefined;
    const diagnostics = forExchange(exchange);
    const startedAt = readConfigNumber(typed?.config, T0);
    const totalMs = startedAt === undefined ? undefined : now - startedAt;
    const signMs = readConfigNumber(typed?.config, SIGN_MS);
    const signatureAgeMs = readConfigNumber(typed?.config, SIG_AGE_MS);
    const status = typed?.response?.status;
    const code = typeof typed?.code === 'string' ? typed.code : undefined;

    const rawMessage = (typed as { message?: unknown } | undefined)?.message;
    const message = typeof rawMessage === 'string' ? rawMessage : undefined;
    // A signer that is asleep or gone is the device's state, not the venue's: counted
    // and sampled, but not as a venue failure, and its sample says so.
    const unsentSigning = !typed?.response && isUnsentSigningFailure(message);
    if (unsentSigning) {
      diagnostics.unsentSigning += 1;
    } else {
      // Rate and latency describe the venue, which an unsent request never reached.
      diagnostics.requests += 1;
      diagnostics.failures += 1;
      noteTimestamp(diagnostics, now);
      if (totalMs !== undefined) push(diagnostics.totalMs, totalMs);
      if (signMs !== undefined) push(diagnostics.signMs, signMs);
      if (signatureAgeMs !== undefined) {
        diagnostics.maxSignatureAgeMs = Math.max(diagnostics.maxSignatureAgeMs ?? 0, signatureAgeMs);
      }
      bump(diagnostics.statusCounts, status === undefined ? 'none' : String(status));
      if (code) bump(diagnostics.codeCounts, code);
    }

    // No response AND no code means nothing axios produced — a bare `throw`
    // from inside the adapter chain, where the message is the only identifier
    // there is. 408 bybit failures in the first 11 hours landed in exactly this
    // shape and the record named none of them.
    const errorName = (typed as { name?: unknown } | undefined)?.name;
    const isUnshapedFailure = !typed?.response && !code;
    if (isUnshapedFailure && message) bumpBounded(diagnostics.errorCounts, normalizeErrorKey(message));

    const { bodyKind, bodySnippet } = classifyBody(typed?.response?.data);
    failureSamples.push({
      at: now,
      exchange,
      path: pathOf((typed?.config as { url?: string } | undefined)?.url),
      status,
      code,
      totalMs,
      signMs,
      signatureAgeMs,
      bodyKind,
      bodySnippet,
      restProxyHeaderSent: proxyHeaderOf(typed?.config),
      endpoint: endpointOf(typed?.config),
      errorName: typeof errorName === 'string' ? errorName : undefined,
      errorMessage: message ? scrub(message) : undefined,
      unsentSigning,
      headers: {
        ...readHeaders(typed?.response?.headers, ORIGIN_HEADERS),
        ...readHeaders(typed?.response?.headers, QUOTA_HEADERS),
      },
    });
    if (failureSamples.length > MAX_SAMPLES) failureSamples.splice(0, failureSamples.length - MAX_SAMPLES);
  } catch {
    /* diagnostics must never break a request */
  }
};

const percentile = (values: number[], fraction: number): number | undefined => {
  if (values.length === 0) return undefined;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.floor(sorted.length * fraction));
  return Math.round(sorted[index]);
};

/**
 * Aggregates only — safe to serve publicly. Deliberately excludes the failure
 * samples, which carry response-body text.
 */
export const getVenueRestStats = (now = Date.now()): Record<string, unknown> => {
  const out: Record<string, unknown> = {};
  for (const [exchange, diagnostics] of byExchange) {
    const cutoff = now - WINDOW_MS;
    const recent = diagnostics.timestamps.filter((at) => at >= cutoff);
    out[exchange] = {
      requests: diagnostics.requests,
      ok: diagnostics.ok,
      failures: diagnostics.failures,
      unsentSigning: diagnostics.unsentSigning,
      perMinute: Math.round((recent.length / (WINDOW_MS / 60_000)) * 10) / 10,
      // The retained count, not the in-window count. Publishing it is what
      // makes the write-time prune observable — this is the one structure that
      // would otherwise grow for the life of the process on a container whose
      // /healthz nobody polls.
      trackedSamples: diagnostics.timestamps.length,
      totalP50Ms: percentile(diagnostics.totalMs, 0.5),
      totalP95Ms: percentile(diagnostics.totalMs, 0.95),
      signP50Ms: percentile(diagnostics.signMs, 0.5),
      signP95Ms: percentile(diagnostics.signMs, 0.95),
      minQuotaRemaining: diagnostics.minQuotaRemaining,
      lastQuota: diagnostics.lastQuota,
      maxSignatureAgeMs: diagnostics.maxSignatureAgeMs,
      statusCounts: diagnostics.statusCounts,
      codeCounts: diagnostics.codeCounts,
      // Keys are scrubbed, digit-masked and capped at 60 chars. This is the
      // only field here derived from error TEXT, so it is normalised hard
      // before it reaches a public payload.
      errorCounts: diagnostics.errorCounts,
      // Answers "direct or through our tunnel" without waiting for a failure.
      lastEndpoint: diagnostics.lastEndpoint,
    };
  }
  return out;
};

/** Drains, so one burst is reported once rather than on every poll. */
export const drainVenueRestFailureSamples = (): VenueRestFailureSample[] => failureSamples.splice(0);

export const resetVenueRestDiagnostics = (): void => {
  byExchange.clear();
  failureSamples.length = 0;
};
