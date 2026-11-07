import assert from 'node:assert/strict';
import test from 'node:test';
import { normalizeTsBybitSubscriptions } from './bybit-subscriptions.mjs';

const url = 'wss://stream.bybit.com/v5/private';
const key = (topic, generation = 1, origin = url, op = 'subscribe') => `${generation} ${origin} ${op} ${topic}`;

test('missing or malformed subscription collections remain visible to gate validation', () => {
  for (const subscriptions of [undefined, null, 'order', { topic: 'order' }]) {
    assert.equal(normalizeTsBybitSubscriptions('bybitinverse', subscriptions, url), subscriptions);
  }
});

test('TS all-in-one topics compare with the expected native Bybit category only', () => {
  assert.deepEqual(
    normalizeTsBybitSubscriptions(
      'bybitinverse',
      [key('execution'), key('order'), key('position'), key('wallet')],
      url,
    ),
    [key('execution.inverse'), key('order.inverse'), key('position.inverse'), key('wallet')],
  );
  assert.deepEqual(normalizeTsBybitSubscriptions('bybitspot', [key('execution'), key('order'), key('wallet')], url), [
    key('execution.spot'),
    key('order.spot'),
    key('wallet'),
  ]);
  const original = [key('order')];
  normalizeTsBybitSubscriptions('bybitinverse', original, url);
  assert.deepEqual(original, [key('order')], 'the TS recording is not mutated');
  assert.deepEqual(normalizeTsBybitSubscriptions('hyrotrader', original, url), original);
});

test('qualified, malformed, unexpected-origin and unsupported topics remain exact', () => {
  const untouched = [
    key('order.spot'),
    key('order.inverse'),
    key('order.linear'),
    key('orders'),
    key('order.extra'),
    key('order', 1, 'wss://other.example/v5/private'),
    key('order', 1, url + '/extra'),
    key('order', 1, url, 'other'),
    key('order', 0),
    key('order', '01'),
    key('order') + ' ',
    'malformed',
  ];
  assert.deepEqual(normalizeTsBybitSubscriptions('bybitinverse', untouched, url), [...untouched].sort());
  assert.deepEqual(normalizeTsBybitSubscriptions('bybitspot', [key('position')], url), [key('position')]);
});

test('wrong Rust category, socket origin, operation or generation still fails comparison', () => {
  const ts = normalizeTsBybitSubscriptions('bybitinverse', [key('order')], url);
  for (const wrong of [
    key('order.spot'),
    key('order.linear'),
    key('order.inverse', 2),
    key('order.inverse', 1, 'wss://other.example/v5/private'),
    key('order.inverse', 1, url, 'unsubscribe'),
  ]) {
    assert.notDeepEqual(ts, [wrong]);
  }
  assert.deepEqual(normalizeTsBybitSubscriptions('bybitinverse', [key('order', 2, url, 'unsubscribe')], url), [
    key('order.inverse', 2, url, 'unsubscribe'),
  ]);
});

test('canonical wire subscription scope changes only TS unqualified private topics', () => {
  const wire = (topic, generation = 1, origin = url + '?', op = 'subscribe') =>
    `${generation} WS ${origin} ${JSON.stringify({ args: [topic], op })}`;
  const result = normalizeTsBybitSubscriptions('bybitinverse', [wire('order'), wire('wallet')], url);
  assert.deepEqual(result, [wire('order.inverse'), wire('wallet')]);
  assert.deepEqual(normalizeTsBybitSubscriptions('bybitspot', [wire('execution')], url), [wire('execution.spot')]);
  const untouched = [
    wire('order.spot'),
    wire('order.linear'),
    wire('orders'),
    wire('order', 0),
    wire('order', '01'),
    wire('order', 1, url + '?extra=1'),
    wire('order', 1, 'wss://other.example/v5/private?'),
    wire('order', 1, url + '?', 'other'),
    wire('order') + ' ',
    `1 WS ${url}? {"args":["order"],"extra":1,"op":"subscribe"}`,
  ];
  assert.deepEqual(normalizeTsBybitSubscriptions('bybitinverse', untouched, url), [...untouched].sort());
  for (const wrong of [
    wire('order.spot'),
    wire('order.inverse', 2),
    wire('order.inverse', 1, url + '?extra=1'),
    wire('order.inverse', 1, url + '?', 'unsubscribe'),
  ])
    assert.notDeepEqual(normalizeTsBybitSubscriptions('bybitinverse', [wire('order')], url), [wrong]);
});
