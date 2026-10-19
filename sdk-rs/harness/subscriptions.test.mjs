import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { gunzipSync } from 'node:zlib';
import { SocketSubscriptions } from './subscriptions.mjs';

// Frozen pre-index audit. Candidate selection must preserve every comparison,
// inference, failure and canonical Map replacement/unsubscribe behavior.
const original = (socket, message) => {
  if (!message.arg?.channel || !Array.isArray(message.data)) return;
  const topics = [...socket.topics.values()].filter((topic) => topic.channel === message.arg.channel);
  const rows = message.data.length ? message.data : [null];
  for (const row of rows) {
    const object = row && !Array.isArray(row) && typeof row === 'object' ? row : {};
    const covered = topics.some((topic) => Object.entries(topic).every(([key, value]) => {
      if (key === 'channel') return true;
      if (key === 'instType' && value === 'ANY') return true;
      const values = [message.arg[key], object[key]].filter((v) => v !== undefined && v !== '');
      if (!values.length && key === 'instType') {
        const id = object.instId ?? message.arg.instId;
        if (id?.endsWith('-SWAP')) values.push('SWAP');
      }
      return values.length > 0 && values.every((v) => v === value);
    }));
    if (!covered) throw new Error(`Unsubscribed ${message.arg.channel} frame on ${socket.url}: ${JSON.stringify(message.arg)}${object.instId ? ` instId=${object.instId}` : ''}`);
  }
};
const key = (topic) => JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
const fixture = () => ({ url: 'wss://fixture/public', topics: new Map(), index: new SocketSubscriptions() });
const add = (socket, topic) => { socket.topics.set(key(topic), topic); socket.index.set(key(topic), topic); };
const remove = (socket, topic) => { socket.topics.delete(key(topic)); socket.index.delete(key(topic)); };
const result = (fn) => {
  try { fn(); return undefined; } catch (error) { return [error.name, error.message]; }
};
const same = (socket, message) => {
  assert.deepEqual(result(() => socket.index.assertSubscribed(socket, message)), result(() => original(socket, message)), JSON.stringify(message));
};

test('indexed coverage preserves row/arg conflicts, wildcard scopes, odd IDs and unsubscribe', () => {
  const socket = fixture();
  const topics = [
    { channel: 'orders', instType: 'SWAP' },
    { channel: 'orders', instId: 'BTC-USDT-SWAP' },
    { channel: 'orders', instType: 'ANY' },
    { channel: 'orders', instId: 'BTC-USDT-SWAP', extra: 'x' },
    { channel: 'orders', instId: 'BTC-USDT-SWAP', extra: 'y' },
    { channel: 'orders', extra: 'x' },
    { channel: 'orders', instId: null },
    { channel: 'orders', instId: 0 },
    { channel: 'orders', instId: false },
    { channel: 'orders', instId: '' },
    { channel: 'orders', instId: undefined },
    { channel: 'orders' },
  ];
  const messages = [
    { arg: { channel: 'orders' }, data: [] },
    { arg: { channel: 'orders', instId: 'BTC-USDT-SWAP' }, data: [{ instId: 'BTC-USDT-SWAP' }, { instId: 'ETH-USDT-SWAP' }] },
    { arg: { channel: 'orders', instType: 'SWAP' }, data: [{ instType: 'SPOT', instId: 'BTC-USDT-SWAP' }] },
    { arg: { channel: 'orders', extra: 'x' }, data: [{ extra: 'y' }] },
    { arg: { channel: 'orders' }, data: [null, [], {}, { instId: '' }, { instId: null }] },
  ];
  for (const instId of [undefined, '', null, 0, false, [], {}, 'BTC-USDT-SWAP', 'ETH-USDT-SWAP']) {
    for (const rowId of [undefined, '', null, 0, false, 'BTC-USDT-SWAP']) {
      messages.push({ arg: { channel: 'orders', instId }, data: [{ instId: rowId }] });
    }
  }
  for (const topic of topics) {
    add(socket, topic);
    for (const message of messages) same(socket, message);
  }
  // A duplicate replaces in-place, including canonically equivalent key order.
  for (const topic of [topics[1], { instId: 'BTC-USDT-SWAP', channel: 'orders' }, { instId: 'BTC-USDT-SWAP', channel: 'orders', ignored: undefined }]) {
    add(socket, topic);
    for (const message of messages) same(socket, message);
  }
  for (const topic of [...topics].reverse()) {
    remove(socket, topic);
    for (const message of messages) same(socket, message);
  }
  assert.equal(socket.index.topics.size, 0);
  assert.equal(socket.index.channels.size, 0);
});

test('deterministic mutation sequences match the frozen audit', () => {
  const socket = fixture();
  let state = 0x3d67009;
  const random = (n) => { state = (Math.imul(state, 1664525) + 1013904223) >>> 0; return state % n; };
  const ids = [undefined, '', null, 0, false, 'BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'BTC-USDT'];
  const types = [undefined, '', 'ANY', 'SWAP', 'SPOT'];
  const object = (topic = false) => {
    const value = {};
    if (topic || random(3)) value.channel = ['orders', 'books', 'tickers'][random(3)];
    const fields = [['instId', ids[random(ids.length)]], ['instType', types[random(types.length)]], ['extra', [undefined, '', null, 'x', 'y'][random(5)]]];
    if (random(2)) fields.reverse();
    for (const [name, field] of fields) if (random(3)) value[name] = field;
    return value;
  };
  const subscriptions = Array.from({ length: 36 }, () => object(true));
  for (let i = 0; i < 600; i++) {
    const topic = subscriptions[random(subscriptions.length)];
    if (random(3)) add(socket, topic);
    else remove(socket, topic);
    same(socket, { arg: object(), data: Array.from({ length: random(3) }, () => random(4) ? object() : null) });
  }
});

test('the complete recorded OKX fixture retains exact subscription coverage', () => {
  const records = gunzipSync(readFileSync(new URL('./fixtures/okx-public.jsonl.gz', import.meta.url))).toString().trim().split('\n').map(JSON.parse);
  const markets = JSON.parse(records.find((row) => row.type === 'http' && row.url.includes('/public/instruments')).body).data
    .filter((row) => row.ctType === 'linear' && ['USDT', 'USDC'].includes(row.settleCcy));
  const socket = fixture();
  for (const channel of ['tickers', 'mark-price', 'index-tickers', 'open-interest', 'funding-rate']) {
    for (const market of markets) add(socket, { channel, instId: channel === 'index-tickers' ? market.instId.replace(/-SWAP$/, '') : market.instId });
  }
  for (const instId of ['BTC-USDT-SWAP', 'ETH-USDT-SWAP']) add(socket, { channel: 'books', instId });
  for (const record of records) {
    if (record.type !== 'ws' || !record.text.trimStart().startsWith('{')) continue;
    const message = JSON.parse(record.text);
    assert.equal(result(() => socket.index.assertSubscribed(socket, message)), undefined);
    same(socket, message);
  }
  remove(socket, { channel: 'books', instId: 'BTC-USDT-SWAP' });
  const message = records.filter((row) => row.type === 'ws' && row.text.includes('"channel":"books"'))
    .map((row) => JSON.parse(row.text)).find((row) => row.arg.instId === 'BTC-USDT-SWAP' && Array.isArray(row.data));
  same(socket, message);
  assert.match(result(() => socket.index.assertSubscribed(socket, message))[1], /Unsubscribed books/);
});
