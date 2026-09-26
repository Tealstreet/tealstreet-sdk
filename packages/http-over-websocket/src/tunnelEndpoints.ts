// The tradetunnel WebSocket endpoints, one per egress region. Pure data so both
// the HOW axios adapter (REST-over-WS) and the relay socket (private
// exchange WS through the private proxy fleet) dial the same hosts.
//
// `NEXT_PUBLIC_TRADETUNNEL_STAGING=1` swaps the path to the staging tunnel
// deployment on the same hostnames.

export const TUNNEL_PATH =
  typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_TRADETUNNEL_STAGING === '1'
    ? '/tunnel-staging'
    : '/tunnel';

export const SINGAPORE_HOW_EGRESS = `wss://fsjnpy143pbz.asia.tealstreet.io${TUNNEL_PATH}`;
export const TOKYO_HOW_EGRESS = `wss://fsjnpy143pbz.jp.tealstreet.io${TUNNEL_PATH}`;

export type TunnelEgress = 'jp' | 'sg';

export const tunnelEgressUrl = (egress: TunnelEgress): string =>
  egress === 'jp' ? TOKYO_HOW_EGRESS : SINGAPORE_HOW_EGRESS;

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
  if (!upstream) throw new Error(`not a relay url: ${url}`);
  parsed.searchParams.delete(RELAY_QUERY_PARAM);
  return { tunnelUrl: parsed.toString(), upstream };
};
