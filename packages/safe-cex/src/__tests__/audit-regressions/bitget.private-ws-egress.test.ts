import { afterEach, describe, expect, it, vi } from 'vitest';

import { buildRelayUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';
import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { DefaultStore } from '../../store/store.base';
import { BROWSER_WS_HOSTS, setRoutingWsHosts } from '../../transport/tealstreet/routing/routingTable';

import '../../transport/registerTealstreetTransport';

// Bitget rejects a private login whose source IP the key is not bound to
// (40018). A proxied account dials ws-proxy first, then the relay, which egresses
// from the same fleet as its REST.
const WS_PROXY_V2 = 'wss://fsjnpy143pbz.asia.tealstreet.io/bitget/v2/private';
const WS_PROXY_V3 = 'wss://fsjnpy143pbz.asia.tealstreet.io/bitget/v3/private';
const RELAY_V2 = buildRelayUrl('jp', 'wss://ws.bitget.com/v2/ws/private');
const RELAY_V3 = buildRelayUrl('jp', 'wss://ws.bitget.com/v3/ws/private');
// Since AWS_EXIT.md step 3e the host IS the relay in every runtime (the hosted
// exchange server's ws-proxy pin went in step 4). Pin a ws-proxy here so the
// two-route walk stays covered.
setRoutingWsHosts({ ...BROWSER_WS_HOSTS, bitgetPrivate: WS_PROXY_V2, bitgetPrivateUta: WS_PROXY_V3 });
const DIRECT_V2 = 'wss://ws.bitget.com/v2/ws/private';

const live: BitgetExchange[] = [];
afterEach(() => live.splice(0).forEach((exchange) => exchange.dispose()));

const fakeSocket = (tunnel: boolean) => {
  const socket = tunnel ? Object.create(ReconnectingWebSocket.prototype) : {};
  return Object.assign(socket, { addEventListener: vi.fn(), removeEventListener: vi.fn(), close: vi.fn() });
};

const setup = ({ proxy = true, uta = false } = {}) => {
  const exchange = new BitgetExchange(
    'bitget',
    { key: 'k', secret: 's', passphrase: 'p', tealstreetProxy: proxy, tuning: { consoleLogging: false } },
    new DefaultStore(),
  );
  live.push(exchange);
  (exchange as any).isUta = uta;
  const ws = (exchange as any).privateWebsocket;
  const urls: string[] = [];
  vi.spyOn(ws, 'createWebsocket').mockImplementation((url: any) => {
    urls.push(url);
    return fakeSocket(url !== DIRECT_V2);
  });
  const reconnect = vi.spyOn(ws, 'reconnect').mockImplementation(() => {});
  vi.spyOn(ws, 'trackLastReceived').mockImplementation(() => {});
  const emit = vi.spyOn(exchange.emitter, 'emit');
  const frame = (payload: Record<string, unknown>) => ws.onMessage({ data: JSON.stringify(payload) });
  return { exchange, ws, urls, reconnect, emit, frame };
};

const refused = { event: 'error', code: 40018, msg: 'Current IP does not match the bound IP' };
const errors = (emit: any) => emit.mock.calls.filter(([name]: [string]) => name === 'error');

describe('Bitget private WS egress', () => {
  it('a proxied Classic account dials ws-proxy v2; a UTA account dials ws-proxy v3', () => {
    const classic = setup();
    classic.ws.connectAndSubscribe();
    expect(classic.urls).toEqual([WS_PROXY_V2]);

    const uta = setup({ uta: true });
    uta.ws.connectAndSubscribe();
    expect(uta.urls).toEqual([WS_PROXY_V3]);
  });

  it('40018 moves to the relay, and once every route is refused says why, once, and stops', () => {
    const { ws, urls, reconnect, emit, frame } = setup();
    ws.connectAndSubscribe();

    frame(refused);
    expect(urls).toEqual([WS_PROXY_V2, RELAY_V2]);
    expect(errors(emit)).toHaveLength(0);

    frame(refused);
    frame(refused);
    expect(urls).toEqual([WS_PROXY_V2, RELAY_V2]);
    expect(errors(emit)).toHaveLength(1);
    expect(String(errors(emit)[0][1])).toContain("allows none of Tealstreet's WebSocket egress IPs");
    expect(reconnect).not.toHaveBeenCalled();
  });

  it('a restricted-location refusal (40024) also moves the route', () => {
    const { ws, urls, frame } = setup();
    ws.connectAndSubscribe();
    frame({ event: 'error', code: 40024, msg: 'restricted location' });
    expect(urls).toEqual([WS_PROXY_V2, RELAY_V2]);
  });

  it('a UTA account walks the v3 routes', () => {
    const { ws, urls, frame } = setup({ uta: true });
    ws.connectAndSubscribe();
    frame(refused);
    expect(urls).toEqual([WS_PROXY_V3, RELAY_V3]);
  });

  it('a tunnel route that opens and closes before login twice is abandoned for the next one', () => {
    const { ws, urls } = setup();
    vi.spyOn(ws, 'auth' as any).mockImplementation(async () => {});
    vi.spyOn(ws, 'ping').mockImplementation(() => {});
    ws.connectAndSubscribe();

    ws.onOpen();
    ws.onTunnelClose();
    expect(urls).toEqual([WS_PROXY_V2]);
    ws.onOpen();
    ws.onTunnelClose();
    expect(urls).toEqual([WS_PROXY_V2, RELAY_V2]);
    expect(ws.ws).toBeDefined();
  });

  it('a short outage (connects failing before open) does not move the route', () => {
    const { ws, urls } = setup();
    ws.connectAndSubscribe();
    for (let i = 0; i < 5; i++) ws.onTunnelClose();
    expect(urls).toEqual([WS_PROXY_V2]);
    ws.onTunnelClose();
    expect(urls).toEqual([WS_PROXY_V2, RELAY_V2]);
  });

  it('a route switch starts the new route with a clean close count', () => {
    const { ws, urls, frame } = setup();
    vi.spyOn(ws, 'auth' as any).mockImplementation(async () => {});
    vi.spyOn(ws, 'ping').mockImplementation(() => {});
    ws.connectAndSubscribe();
    ws.onOpen();
    ws.onTunnelClose(); // one strike on ws-proxy
    frame(refused); // 40018 moves to the relay
    ws.onOpen();
    ws.onTunnelClose(); // one strike on the relay must not exhaust it
    expect(urls).toEqual([WS_PROXY_V2, RELAY_V2]);
  });

  it('removes its listeners from the socket it abandons', () => {
    const { ws, frame } = setup();
    ws.connectAndSubscribe();
    const old = ws.ws;
    frame(refused);
    expect(old.close).toHaveBeenCalled();
    expect(old.removeEventListener).toHaveBeenCalledWith('message', ws.onMessage);
    expect(old.removeEventListener).toHaveBeenCalledWith('close', ws.onTunnelClose);
  });

  it('a close after a successful login does not count against the route', () => {
    const { ws, urls, frame } = setup();
    vi.spyOn(ws, 'subscribe' as any).mockImplementation(() => {});
    ws.connectAndSubscribe();

    frame({ event: 'login', code: 0 });
    ws.onTunnelClose();
    ws.onTunnelClose();
    expect(urls).toEqual([WS_PROXY_V2]);
  });

  it('a direct account reconnects onto the relay after 40018', () => {
    const { ws, urls, reconnect, frame } = setup({ proxy: false });
    ws.connectAndSubscribe();
    expect(urls).toEqual([DIRECT_V2]);

    frame(refused);
    expect(reconnect).toHaveBeenCalledTimes(1);
    ws.connectAndSubscribe(); // what the native close path runs
    expect(urls).toEqual([DIRECT_V2, RELAY_V2]);
  });

  it('repeated non-IP login rejections (bad key, skew) never move the route', () => {
    const { ws, urls, frame } = setup();
    vi.spyOn(ws, 'auth' as any).mockImplementation(async () => {});
    vi.spyOn(ws, 'ping').mockImplementation(() => {});
    ws.connectAndSubscribe();
    for (let i = 0; i < 4; i++) {
      ws.onOpen();
      frame({ event: 'error', code: 30015, msg: 'Invalid sign' });
      ws.onTunnelClose();
    }
    expect(urls).toEqual([WS_PROXY_V2]);
  });

  it('a non-IP login rejection still reports and reconnects the same route', () => {
    const { ws, urls, reconnect, emit, frame } = setup();
    ws.connectAndSubscribe();
    frame({ event: 'error', code: 30015, msg: 'Invalid sign' });

    expect(errors(emit)).toHaveLength(1);
    expect(reconnect).toHaveBeenCalledTimes(1);
    expect(urls).toEqual([WS_PROXY_V2]);
  });
});
