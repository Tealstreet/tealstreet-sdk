import { test } from 'node:test';
import assert from 'node:assert/strict';
import { gateTransport } from './gate-transport.mjs';

class FakeSocket {
  static made = [];
  constructor(url) { this.url = url; this.listeners = {}; this.sent = []; FakeSocket.made.push(this); }
  addEventListener(type, fn) { (this.listeners[type] ??= []).push(fn); }
  emit(type, event = {}) { for (const fn of this.listeners[type] ?? []) fn(event); }
  send(text) { this.sent.push(text); }
  close() { this.emit('close', { code: 1000 }); }
}

const base = () => {
  const calls = [];
  return { calls, transport: { webSocketHeaders: true, now: () => 0, connect(url, handlers) { calls.push([url, handlers]); return { send() {}, close() {} }; } } };
};

test('a socket without upgrade headers dials the global WebSocket', () => {
  FakeSocket.made = [];
  const { calls, transport } = base();
  const events = [];
  const socket = gateTransport(transport, FakeSocket).connect('wss://venue.test/public', {
    onOpen: () => events.push('open'), onMessage: (text) => events.push(text), onClose: (code) => events.push(code),
  });
  assert.equal(calls.length, 0, 'the ws library is not used');
  const [ws] = FakeSocket.made;
  assert.equal(ws.url, 'wss://venue.test/public');
  ws.emit('open');
  ws.emit('message', { data: 'frame' });
  socket.send('ping');
  assert.deepEqual(ws.sent, ['ping']);
  ws.emit('error');
  ws.emit('close', { code: 1006 });
  assert.deepEqual(events, ['open', 'frame', 1006], 'one close, reported once');
});

test('a socket with upgrade headers keeps the headered transport', () => {
  FakeSocket.made = [];
  const { calls, transport } = base();
  gateTransport(transport, FakeSocket).connect('wss://venue.test/private', { headers: { 'x-site-id': 'BRA_BTL' }, onOpen() {}, onMessage() {}, onClose() {} });
  assert.equal(FakeSocket.made.length, 0);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0][1].headers, { 'x-site-id': 'BRA_BTL' });
});

test('without a global WebSocket the native transport is returned unchanged', () => {
  const { transport } = base();
  assert.equal(gateTransport(transport, null), transport);
});
