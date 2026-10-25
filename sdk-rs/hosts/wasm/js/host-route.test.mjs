import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

// Route rejection seam (docs/plans/RUST_BITGET_MANAGED_SOCKET_ROUTING.md): the
// transport reports exhaustion instead of dialling; it is the socket's only terminal.
function rig() {
  const calls = [];
  const sockets = [];
  const engine = {
    wsOpen: (...args) => (calls.push(['wsOpen', ...args]), ''),
    wsClose: (...args) => (calls.push(['wsClose', ...args]), ''),
    wsRouteExhausted: (...args) => (calls.push(['wsRouteExhausted', ...args]), ''),
  };
  const transport = {
    now: () => 9,
    connect: (url, callbacks) => {
      const socket = { url, callbacks, send() {}, close() {} };
      sockets.push(socket);
      return socket;
    },
    setTimeout: (fn) => setTimeout(fn, 0),
    clearTimeout: (handle) => clearTimeout(handle),
  };
  return { host: new EngineHost(engine, transport), calls, sockets };
}

test('rejectRoute reaches the transport context, and exhaustion is reported once without a close', () => {
  const r = rig();
  r.host.execute({ op: 'wsConnect', socket: 4, url: 'wss://ws.bitget.com/v2/ws/private',
    channel: 'private', context: { feed: 'private', recover: true, rejectRoute: true } });
  const { callbacks } = r.sockets[0];
  assert.deepEqual(callbacks.context, { feed: 'private', recover: true, rejectRoute: true });
  callbacks.onRouteExhausted();
  assert.deepEqual(r.calls, [['wsRouteExhausted', 4, 9]]);
  assert.equal(r.host.sockets.has(4), false);
  callbacks.onRouteExhausted();
  callbacks.onClose(1006);
  callbacks.onOpen();
  assert.deepEqual(r.calls, [['wsRouteExhausted', 4, 9]], 'no second terminal, close or open');
});

test('exhaustion from a replaced socket is inert', () => {
  const r = rig();
  r.host.execute({ op: 'wsConnect', socket: 4, url: 'wss://a.test/private', channel: 'private',
    context: { feed: 'private', recover: false } });
  const stale = r.sockets[0].callbacks;
  stale.onClose(1006);
  r.host.execute({ op: 'wsConnect', socket: 5, url: 'wss://a.test/private', channel: 'private',
    context: { feed: 'private', recover: false, rejectRoute: true } });
  stale.onRouteExhausted();
  assert.deepEqual(r.calls.map(([name]) => name), ['wsClose']);
});
