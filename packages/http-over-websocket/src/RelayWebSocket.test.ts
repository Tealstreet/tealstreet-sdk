import { AddressInfo, createServer, type Socket } from 'node:net';

import { WebSocketServer, WebSocket as WsClient } from 'ws';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { configureHOW } from './config';
import { buildRelayUrl, isRelayUrl, parseRelayUrl, RELAY_QUERY_PARAM, RelayWebSocket } from './RelayWebSocket';

// A mock tradetunnel: greets with tunnel chatter, answers rpc.auth / rpc.relay,
// then echoes every frame prefixed with "echo:". Behaviour is steered by the
// token and upstream the client sends so one server covers every case.
const startMockTunnel = async () => {
  const server = new WebSocketServer({ port: 0 });
  await new Promise<void>((resolve) => server.once('listening', resolve));
  const seen: string[] = [];
  server.on('connection', (socket: WsClient) => {
    socket.send(JSON.stringify({ type: 'serverTime', serverTime: Date.now() }));
    let relayed = false;
    socket.on('message', (raw) => {
      const text = raw.toString();
      seen.push(text);
      if (relayed) {
        socket.send(`echo:${text}`);
        return;
      }
      const json = JSON.parse(text);
      if (json.type === 'rpc.auth') {
        socket.send(
          json.token === 'good-token'
            ? JSON.stringify({ type: 'rpc.auth', status: 'success' })
            : JSON.stringify({ type: 'rpc.auth', error: 'ExpiredSignature' }),
        );
      } else if (json.type === 'rpc.relay') {
        if (json.upstream.includes('/v5/private')) {
          relayed = true;
          socket.send(JSON.stringify({ type: 'rpc.relay', status: 'open', target: 'private' }));
        } else {
          socket.send(JSON.stringify({ type: 'rpc.relay', error: 'upstream not on the relay allowlist' }));
        }
      }
    });
  });
  const { port } = server.address() as AddressInfo;
  return { server, seen, url: `ws://127.0.0.1:${port}/tunnel` };
};

const relayUrlFor = (tunnelUrl: string, upstream: string) => {
  const url = new URL(tunnelUrl);
  url.searchParams.set(RELAY_QUERY_PARAM, upstream);
  return url.toString();
};

const collect = (ws: RelayWebSocket) => {
  const events: string[] = [];
  const messages: string[] = [];
  ws.addEventListener('open', () => events.push('open'));
  ws.addEventListener('error', () => events.push('error'));
  ws.addEventListener('close', () => events.push('close'));
  ws.addEventListener('message', (e) => messages.push(String((e as MessageEvent).data)));
  return { events, messages };
};

const waitFor = (pred: () => boolean, ms = 3000) =>
  new Promise<void>((resolve, reject) => {
    const started = Date.now();
    const tick = () => {
      if (pred()) return resolve();
      if (Date.now() - started > ms) return reject(new Error('timeout'));
      setTimeout(tick, 10);
    };
    tick();
  });

describe('relay url helpers', () => {
  it('builds, detects and parses the ts-relay query on the regional tunnel url', () => {
    const url = buildRelayUrl('sg', 'wss://stream.bybit.com/v5/private');
    expect(url).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fstream.bybit.com%2Fv5%2Fprivate',
    );
    expect(buildRelayUrl('jp', 'wss://ws.okx.com:8443/ws/v5/private')).toContain(
      'fsjnpy143pbz.jp.tealstreet.io/tunnel?',
    );
    expect(isRelayUrl(url)).toBe(true);
    expect(isRelayUrl('wss://stream.bybit.com/v5/private?requestId=x')).toBe(false);
    expect(isRelayUrl('not a url')).toBe(false);
    expect(parseRelayUrl(url)).toEqual({
      tunnelUrl: 'wss://fsjnpy143pbz.asia.tealstreet.io/tunnel',
      upstream: 'wss://stream.bybit.com/v5/private',
    });
  });
});

describe('RelayWebSocket', () => {
  let tunnel: Awaited<ReturnType<typeof startMockTunnel>>;
  let token = 'good-token';

  beforeEach(async () => {
    token = 'good-token';
    configureHOW({ getProxyToken: async () => token });
    tunnel = await startMockTunnel();
  });
  afterEach(async () => {
    await new Promise<void>((resolve) => tunnel.server.close(() => resolve()));
  });

  it('opens only after the relay handshake, flushes queued sends, and hides tunnel chatter', async () => {
    const ws = new RelayWebSocket(relayUrlFor(tunnel.url, 'wss://stream.bybit.com/v5/private'));
    const { events, messages } = collect(ws);
    expect(ws.readyState).toBe(RelayWebSocket.CONNECTING);
    ws.send('{"op":"ping"}'); // sent before open: must be queued, not dropped

    await waitFor(() => events.includes('open'));
    expect(ws.readyState).toBe(RelayWebSocket.OPEN);
    await waitFor(() => messages.length >= 1);
    expect(messages).toEqual(['echo:{"op":"ping"}']);

    ws.send('{"op":"auth"}');
    await waitFor(() => messages.length >= 2);
    expect(messages[1]).toBe('echo:{"op":"auth"}');
    // The serverTime greeting never reached the caller, and the preface went
    // out in order before the queued frame.
    expect(messages.some((m) => m.includes('serverTime'))).toBe(false);
    expect(tunnel.seen.slice(0, 3).map((m) => JSON.parse(m).type ?? JSON.parse(m).op)).toEqual([
      'rpc.auth',
      'rpc.relay',
      'ping',
    ]);

    ws.close();
    await waitFor(() => events.includes('close'));
    expect(ws.readyState).toBe(RelayWebSocket.CLOSED);
    expect(events.filter((e) => e === 'close')).toHaveLength(1);
  });

  it('authenticates with the token lookup bound at construction', async () => {
    let context = 'account-a';
    configureHOW({
      getProxyToken: async () => context,
      bindProxyToken: () => {
        const bound = context;
        return async () => bound;
      },
    });
    try {
      const ws = new RelayWebSocket(relayUrlFor(tunnel.url, 'wss://stream.bybit.com/v5/private'));
      context = 'account-b';
      await waitFor(() => tunnel.seen.length > 0);
      expect(JSON.parse(tunnel.seen[0])).toMatchObject({ type: 'rpc.auth', token: 'account-a' });
      ws.close();
    } finally {
      configureHOW({ bindProxyToken: undefined });
    }
  });

  it('reports error + close and never open when the tunnel refuses the upstream', async () => {
    const ws = new RelayWebSocket(relayUrlFor(tunnel.url, 'wss://stream.bybit.com/v5/public/linear'));
    const { events, messages } = collect(ws);
    await waitFor(() => events.includes('close'));
    expect(events).toContain('error');
    expect(events).not.toContain('open');
    expect(messages).toEqual([]);
    expect(events.filter((e) => e === 'close')).toHaveLength(1);
    expect(() => ws.send('x')).toThrow();
  });

  it('fails closed when the proxy token is rejected', async () => {
    token = 'stale-token';
    const ws = new RelayWebSocket(relayUrlFor(tunnel.url, 'wss://stream.bybit.com/v5/private'));
    const { events } = collect(ws);
    await waitFor(() => events.includes('close'));
    expect(events).toContain('error');
    expect(events).not.toContain('open');
  });

  it('fails closed when no proxy token can be obtained', async () => {
    configureHOW({
      getProxyToken: async () => {
        throw new Error('not logged in');
      },
    });
    const ws = new RelayWebSocket(relayUrlFor(tunnel.url, 'wss://stream.bybit.com/v5/private'));
    const { events } = collect(ws);
    await waitFor(() => events.includes('close'));
    expect(events).not.toContain('open');
    // Nothing was ever sent to the tunnel.
    expect(tunnel.seen).toEqual([]);
  });
});

describe('relay handshake timeout', () => {
  it('closes a socket whose upgrade never completes without an uncaught ws abort error', async () => {
    // A TCP server that accepts and never answers the HTTP upgrade.
    const held: Socket[] = [];
    const server = createServer((socket) => held.push(socket));
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const { port } = server.address() as AddressInfo;
    const uncaught: unknown[] = [];
    const onUncaught = (error: unknown) => uncaught.push(error);
    process.on('uncaughtException', onUncaught);
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      const ws = new RelayWebSocket(relayUrlFor(`ws://127.0.0.1:${port}/tunnel`, 'wss://stream.bybit.com/v5/private'));
      const { events } = collect(ws);
      await new Promise((resolve) => setImmediate(resolve));
      vi.advanceTimersByTime(15_000);
      expect(ws.readyState).toBe(RelayWebSocket.CLOSED);
      // ws emits its abort error on the next tick; give it several.
      for (let i = 0; i < 5; i++) await new Promise((resolve) => setImmediate(resolve));
      expect(uncaught).toEqual([]);
      expect(events).toEqual(['error', 'close']);
    } finally {
      vi.useRealTimers();
      process.off('uncaughtException', onUncaught);
      for (const socket of held) socket.destroy();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });
});
