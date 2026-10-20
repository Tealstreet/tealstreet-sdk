import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { WorkersWebSocket, workersTransport } from '../../../sdk-rs/hosts/wasm/js/workers.mjs';
import { createAxiosHttpOverWebsocketAdapter } from './axiosHttpOverWebsocketAdapter';
import { configureHOW } from './config';
import { RelayWebSocket } from './RelayWebSocket';

class WorkerSocket extends EventTarget {
  binaryType: BinaryType = 'arraybuffer';
  sent: string[] = [];
  accepted = false;
  closed = false;
  relayed = false;

  accept() {
    this.accepted = true;
    queueMicrotask(() => this.message(JSON.stringify({ type: 'serverTime', serverTime: Date.now() })));
  }

  message(data: unknown) {
    if (!this.closed) this.dispatchEvent(new MessageEvent('message', { data }));
  }

  send(text: string) {
    this.sent.push(text);
    if (this.relayed) {
      this.message(text);
      return;
    }
    const frame = JSON.parse(text);
    if (frame.type === 'rpc.auth') {
      queueMicrotask(() => this.message('{"type":"rpc.auth","status":"success"}'));
    } else if (frame.type === 'rpc.http') {
      queueMicrotask(() => this.message(JSON.stringify({
        type: 'rpc.http', nonce: frame.nonce,
        data: { status: 200, headers: { 'x-upstream': 'venue' }, data_raw: '{"id":18446744073709551615}' },
      })));
    } else if (frame.type === 'rpc.relay') {
      this.relayed = true;
      queueMicrotask(() => this.message('{"type":"rpc.relay","status":"open"}'));
    }
  }

  close(code = 1000) {
    if (this.closed) return;
    this.closed = true;
    const event = new Event('close');
    Object.assign(event, { code, reason: '', wasClean: true });
    this.dispatchEvent(event);
  }
}

const config = (): InternalAxiosRequestConfig => ({
  method: 'POST', url: 'https://api.bybit.com/v5/order/create?key=one&key=two',
  baseURL: 'https://api.bybit.com', headers: new AxiosHeaders({ 'X-Venue-Auth': 'signature' }),
  data: ' {"id":18446744073709551615, "price":1.2300}\n', timeout: 2000,
});

const upgradeHeaders = {
  Origin: 'https://preview.tealstreet.io',
  'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36',
};

const disposals: Array<() => void> = [];
let upgrades: Array<{ url: string; init: RequestInit; socket: WorkerSocket }>;

beforeEach(() => {
  upgrades = [];
  configureHOW({ getProxyToken: async () => 'worker-account-token', isTokenExpired: () => false, simulateHttpOverWebsocket: false });
  vi.stubGlobal('fetch', vi.fn(async (url: string, init: RequestInit) => {
    const socket = new WorkerSocket();
    upgrades.push({ url, init, socket });
    return { webSocket: socket };
  }));
});

afterEach(() => {
  for (const dispose of disposals.splice(0)) dispose();
  vi.unstubAllGlobals();
});

describe('real HOW and relay protocols over Workers fetch upgrades', () => {
  it('HOW forwards required WAF headers and preserves signed bytes through the actual worker socket', async () => {
    const transport = workersTransport();
    expect(transport.serverHeaders).toBe(true);
    const adapter = createAxiosHttpOverWebsocketAdapter({ region: 'JA', customParser: (body) => body, WebSocket: transport.WebSocket });
    disposals.push(adapter.dispose);
    const input = config();
    const response = await adapter(input);
    expect(response.data).toBe('{"id":18446744073709551615}');
    expect(upgrades).toHaveLength(1);
    const { url, init, socket } = upgrades[0];
    const target = new URL(url);
    expect(target.origin + target.pathname).toBe('https://fsjnpy143pbz.jp.tealstreet.io/tunnel');
    expect(target.searchParams.get('v')).toBe('2');
    expect(Number(target.searchParams.get('t'))).toBeGreaterThan(0);
    expect(init.headers).toEqual({ ...upgradeHeaders, Upgrade: 'websocket' });
    const frames = socket.sent.map((text) => JSON.parse(text));
    expect(frames.map((frame) => frame.type)).toEqual(['rpc.auth', 'rpc.http']);
    expect(frames[0]).toEqual({ type: 'rpc.auth', token: 'worker-account-token', source: 'worker' });
    expect(frames[1].arg).toEqual({ url: input.url, method: 'POST', headers: { 'X-Venue-Auth': 'signature' }, params: {}, data: input.data });
    adapter.dispose();
    expect(socket.closed).toBe(true);
  });

  it('each injected HOW adapter owns its account socket and disposal leaves peers alive', async () => {
    const first = createAxiosHttpOverWebsocketAdapter({ region: 'SG', WebSocket: WorkersWebSocket });
    const second = createAxiosHttpOverWebsocketAdapter({ region: 'SG', WebSocket: WorkersWebSocket });
    disposals.push(first.dispose, second.dispose);
    await Promise.all([first(config()), second(config())]);
    expect(upgrades).toHaveLength(2);
    first.dispose();
    expect(upgrades[0].socket.closed).toBe(true);
    expect(upgrades[1].socket.closed).toBe(false);
    await second(config());
    expect(upgrades).toHaveLength(2);
  });

  it('disposal while acquiring a token cancels the request and cannot send the stale write', async () => {
    let resolveToken!: (token: string) => void;
    const token = new Promise<string>((resolve) => { resolveToken = resolve; });
    const getToken = vi.fn(() => token);
    configureHOW({ getProxyToken: getToken });
    const adapter = createAxiosHttpOverWebsocketAdapter({ region: 'SG', WebSocket: WorkersWebSocket });
    disposals.push(adapter.dispose);
    const pending = adapter(config());
    const rejected = expect(pending).rejects.toThrow('adapter disposed');
    await vi.waitFor(() => expect(getToken).toHaveBeenCalledOnce());
    adapter.dispose();
    await rejected;
    resolveToken('arrived-after-disposal');
    await token;
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(upgrades[0].socket.sent).toEqual([]);
    expect(upgrades[0].socket.closed).toBe(true);
  });

  it('relay uses the same header-capable worker constructor and hides its handshake', async () => {
    const url = new URL('wss://fsjnpy143pbz.asia.tealstreet.io/tunnel');
    url.searchParams.set('ts-relay', 'wss://stream.bybit.com/v5/private');
    const relay = new RelayWebSocket(url.toString(), [], { headers: upgradeHeaders }, WorkersWebSocket);
    disposals.push(() => relay.close());
    const events: string[] = [];
    const messages: unknown[] = [];
    relay.addEventListener('open', () => events.push('open'));
    relay.addEventListener('message', (event) => messages.push((event as MessageEvent).data));
    relay.send('venue-auth');
    await vi.waitFor(() => expect(events).toEqual(['open']));
    expect(upgrades[0].url).toBe('https://fsjnpy143pbz.asia.tealstreet.io/tunnel');
    expect(upgrades[0].init.headers).toEqual({ ...upgradeHeaders, Upgrade: 'websocket' });
    expect(upgrades[0].socket.sent.map((text) => text.startsWith('{') ? JSON.parse(text).type : text)).toEqual(['rpc.auth', 'rpc.relay', 'venue-auth']);
    expect(messages).toEqual(['venue-auth']);
    const raw = new Uint8Array([0x1f, 0x8b, 0x08]);
    upgrades[0].socket.message(raw.buffer);
    expect(messages[1]).toBe(raw.buffer);
  });
});
