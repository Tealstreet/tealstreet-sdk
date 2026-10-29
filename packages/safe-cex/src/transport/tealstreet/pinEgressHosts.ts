import type { TunnelEgress } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import type { WsHosts } from './routing/routingTable';
import type { ProxyHosts } from './transportResolution';

import { setTunnelEgressUrls } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { setRoutingWsHosts } from './routing/routingTable';
import { setProxyHosts } from './transportResolution';

export interface TealstreetEgressHosts extends ProxyHosts {
  tunnel: Record<TunnelEgress, string>;
  ws: WsHosts;
}

/**
 * Replaces every host the tealstreet transport dials — tunnel, relay, proxy pools,
 * per-venue sockets — for this runtime. The browser build never calls it. A host
 * app deployed on its own schedule calls it once at boot with constants it owns, so
 * a redeploy from a newer tree cannot move its egress (docs/plans/AWS_EXIT.md
 * step 4: the hosted exchange server).
 */
export const pinTealstreetEgressHosts = (hosts: TealstreetEgressHosts): void => {
  setTunnelEgressUrls(hosts.tunnel);
  setProxyHosts({ privateProxy: hosts.privateProxy, publicCorsProxy: hosts.publicCorsProxy });
  setRoutingWsHosts(hosts.ws);
};
