// The tradetunnel WebSocket endpoints, one per egress region. Pure data so both
// the HOW axios adapter (REST-over-WS) and the relay socket (private
// exchange WS through the private proxy fleet) dial the same hosts.
//
// `NEXT_PUBLIC_TRADETUNNEL_STAGING=1` swaps the path to the staging tunnel
// deployment on the same hostnames.

import { diagnosticUrl } from './diagnosticUrl';

export const TUNNEL_PATH =
  typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_TRADETUNNEL_STAGING === '1'
    ? '/tunnel-staging'
    : '/tunnel';

// The Vultr Tokyo tunnel (docs/plans/AWS_EXIT.md step 3e): every region egresses
// through it, so both names are the same host. The hosted exchange server dials it
// too (step 4); `setTunnelEgressUrls` exists for tests that tell the regions apart.
export const TOKYO_HOW_EGRESS = `wss://fsjnpy143pbz-tyo.tealstreet.io${TUNNEL_PATH}`;
export const SINGAPORE_HOW_EGRESS = TOKYO_HOW_EGRESS;

export type TunnelEgress = 'jp' | 'sg';

let tunnelEgressUrls: Readonly<Record<TunnelEgress, string>> = { jp: TOKYO_HOW_EGRESS, sg: SINGAPORE_HOW_EGRESS };

/** Substitutes the tunnel hosts (tests only). Read at dial time, so set it before the first request. */
export const setTunnelEgressUrls = (urls: Record<TunnelEgress, string>): void => {
  tunnelEgressUrls = { ...urls };
};

export const tunnelEgressUrl = (egress: TunnelEgress): string => tunnelEgressUrls[egress];

// ---- the browser's tunnel handshake lane ----
// Chrome allows one WebSocket handshake in progress per IP:port, and the tunnel host
// shares Cloudflare's anycast IPs with other zones' sockets; a hung handshake there
// held tunnel connects past their timeout. Browsers dial the tunnel on 8443 instead,
// falling back to 443 for the rest of the realm's life after repeated failures.
// The hosted servers (Node/Bun) and every other host and path keep their port.

const TUNNEL_HOST = 'fsjnpy143pbz-tyo.tealstreet.io';
export const TUNNEL_LANE_PORT = '8443';
const LANE_FAILURES_BEFORE_FALLBACK = 2;

export type TunnelLanePort = '8443' | '443';

const isBrowserRealm = (): boolean =>
  typeof document !== 'undefined' ||
  typeof (globalThis as { WorkerGlobalScope?: unknown }).WorkerGlobalScope !== 'undefined';

let laneEnabled = isBrowserRealm();
let lanePort: TunnelLanePort = '8443';
let laneFailures = 0;
const fallbackListeners = new Set<(failures: number) => void>();

/** The port this realm dials the tunnel on, for telemetry. */
export const tunnelLanePort = (): TunnelLanePort => (laneEnabled ? lanePort : '443');

/** Called once when this realm falls back from 8443 to 443. Returns the unsubscribe. */
export const onTunnelLaneFallback = (listener: (failures: number) => void): (() => void) => {
  fallbackListeners.add(listener);
  return () => fallbackListeners.delete(listener);
};

const isTunnelSocketUrl = (url: URL): boolean =>
  url.protocol === 'wss:' && url.hostname === TUNNEL_HOST && url.pathname === TUNNEL_PATH;

/** The url to dial for a tunnel socket url: the 8443 lane in a browser until it falls back. */
export const tunnelDialUrl = (url: string): string => {
  if (!laneEnabled || lanePort !== TUNNEL_LANE_PORT) return url;
  const parsed = new URL(url);
  if (!isTunnelSocketUrl(parsed) || parsed.port) return url;
  parsed.port = TUNNEL_LANE_PORT;
  return parsed.toString();
};

const dialedPort = (dialedUrl: string): TunnelLanePort | undefined => {
  try {
    const parsed = new URL(dialedUrl);
    if (!isTunnelSocketUrl(parsed)) return undefined;
    return parsed.port === TUNNEL_LANE_PORT ? '8443' : parsed.port ? undefined : '443';
  } catch {
    return undefined;
  }
};

// ---- tunnel reachability ----
// Once the lane is on 443, two more consecutive failed handshakes there mark the
// tunnel unreachable for this realm, and the engine sends REST through the HTTP
// proxy; any tunnel handshake that opens marks it reachable again.

const UNREACHABLE_AFTER_443_FAILURES = 2;

export interface TunnelOutage {
  /** Counts activations in this realm, so a reporter can tell one outage from the next. */
  episode: number;
  portsTried: '8443+443' | '443';
  /** Failed handshakes in the streak, on either port. */
  failures: number;
  /** From the streak's first failed handshake to the activation. */
  sinceFirstFailureMs: number;
}

let streakFailures = 0;
let streakTriedLane = false;
let streakStartedAt = 0;
let failuresOn443 = 0;
let episode = 0;
let outage: TunnelOutage | undefined;
let lastOpenedAt: number | undefined;

/** When a tunnel handshake last opened in this realm, for telling a Hosted-only outage from a wider one. */
export const tunnelLastOpenedAt = (): number | undefined => lastOpenedAt;

/** The current outage when this realm's tunnel is unreachable on every port, else undefined. */
export const tunnelOutage = (): TunnelOutage | undefined => outage;

// Visibly failing comes one handshake before unreachable: the lane is on 443 and a
// handshake failed there too. Unsigned public reads take the HTTP proxy from here.
const failingListeners = new Set<() => void>();

/** This realm's tunnel has failed on 443 since it last opened (always true during an outage). */
export const tunnelFailing = (): boolean => failuresOn443 > 0;

/** Called each time the tunnel starts failing. Returns the unsubscribe. */
export const onTunnelFailing = (listener: () => void): (() => void) => {
  failingListeners.add(listener);
  return () => failingListeners.delete(listener);
};

/** A socket dialed from `tunnelDialUrl` opened its handshake. */
export const noteTunnelDialOpened = (dialedUrl: string): void => {
  if (!dialedPort(dialedUrl)) return;
  lastOpenedAt = Date.now();
  laneFailures = 0;
  failuresOn443 = 0;
  streakFailures = 0;
  streakTriedLane = false;
  outage = undefined;
};

/** A socket dialed from `tunnelDialUrl` closed or timed out before its handshake opened. */
export const noteTunnelDialFailed = (dialedUrl: string): void => {
  const port = dialedPort(dialedUrl);
  if (!laneEnabled || !port) return;
  if (streakFailures === 0) streakStartedAt = Date.now();
  streakFailures += 1;
  if (port === '8443') {
    if (lanePort !== TUNNEL_LANE_PORT) return;
    streakTriedLane = true;
    laneFailures += 1;
    if (laneFailures < LANE_FAILURES_BEFORE_FALLBACK) return;
    lanePort = '443';
    for (const listener of fallbackListeners) listener(laneFailures);
    return;
  }
  if (lanePort !== '443') return;
  failuresOn443 += 1;
  if (failuresOn443 === 1) for (const listener of [...failingListeners]) listener();
  if (failuresOn443 < UNREACHABLE_AFTER_443_FAILURES || outage) return;
  episode += 1;
  outage = {
    episode,
    portsTried: streakTriedLane ? '8443+443' : '443',
    failures: streakFailures,
    sinceFirstFailureMs: Date.now() - streakStartedAt,
  };
};

/** Resets the lane (tests only); `enabled` stands in for the browser-realm check. */
export const resetTunnelLane = (enabled: boolean = isBrowserRealm()): void => {
  laneEnabled = enabled;
  lanePort = '8443';
  laneFailures = 0;
  failuresOn443 = 0;
  streakFailures = 0;
  streakTriedLane = false;
  outage = undefined;
  lastOpenedAt = undefined;
};

// ---- rpc.relay urls (pure; no socket construction so safe-cex's routing table
// can import this without dragging `ws`/configureHOW into its test mocks) ----

export const RELAY_QUERY_PARAM = 'ts-relay';

/** `wss://<tunnel>/tunnel?ts-relay=<upstream>` for the given egress region. */
export const buildRelayUrl = (egress: TunnelEgress, upstream: string): string => {
  const url = new URL(tunnelEgressUrl(egress));
  url.searchParams.set(RELAY_QUERY_PARAM, upstream);
  return url.toString();
};

export const isRelayUrl = (url: string): boolean => {
  try {
    return new URL(url).searchParams.has(RELAY_QUERY_PARAM);
  } catch {
    return false;
  }
};

/** Splits a relay url into the tunnel endpoint to dial and the upstream to name. */
export const parseRelayUrl = (url: string): { tunnelUrl: string; upstream: string } => {
  const parsed = new URL(url);
  const upstream = parsed.searchParams.get(RELAY_QUERY_PARAM);
  if (!upstream) throw new Error(`not a relay url: ${diagnosticUrl(url)}`);
  parsed.searchParams.delete(RELAY_QUERY_PARAM);
  return { tunnelUrl: parsed.toString(), upstream };
};
