import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { gunzipSync } from 'node:zlib';
import test from 'node:test';
import {
  callerIdsOf,
  canonicalUrl,
  isWriteRequest,
  requestKey,
  selectResponse,
  subscriptionMessage,
  subscriptionKey,
  subscriptionKeys,
} from './requests.mjs';

const info = 'https://api.hyperliquid.xyz/info';
const exchange = 'https://api.hyperliquid.xyz/exchange';

test('historical OKX socket ports alias default TLS without changing other identities', () => {
  for (const host of ['ws.okx.com', 'wspap.okx.com', 'wseea.okx.com', 'wseeapap.okx.com']) {
    for (const channel of ['public', 'private', 'business']) {
      assert.equal(canonicalUrl(`wss://${host}:8443/ws/v5/${channel}`),
        canonicalUrl(`wss://${host}/ws/v5/${channel}`));
    }
  }
  for (const [left, right] of [
    ['wss://ws.okx.com:8444/ws/v5/public', 'wss://ws.okx.com/ws/v5/public'],
    ['wss://ws.okx.com.evil:8443/ws/v5/public', 'wss://ws.okx.com.evil/ws/v5/public'],
    ['wss://ws.okx.com:8443/other', 'wss://ws.okx.com/other'],
    ['https://ws.okx.com:8443/ws/v5/public', 'https://ws.okx.com/ws/v5/public'],
    ['wss://ws.okx.com/ws/v5/public', 'wss://wseea.okx.com/ws/v5/public'],
    ['wss://ws.okx.com/ws/v5/public', 'wss://ws.okx.com/ws/v5/private'],
  ]) assert.notEqual(canonicalUrl(left), canonicalUrl(right));
});

test('wire body decoding preserves semantic form and multipart identity before normalization', () => {
  const options = { wireBody: (body) => typeof body === 'string' && body.includes('=')
    ? Object.fromEntries(new URLSearchParams(body)) : body };
  assert.equal(requestKey('POST', exchange, 'size=1&clientId=caller%2B1', new Set(), options),
    requestKey('POST', exchange, { size: '1', clientId: 'caller+1' }, new Set(), options));
  assert.notEqual(requestKey('POST', exchange, 'size=2&clientId=caller%2B1', new Set(), options),
    requestKey('POST', exchange, { size: '1', clientId: 'caller+1' }, new Set(), options));
});

test('nested JSON login payloads retain credentials while clocks and validated signatures vary', () => {
  const login = (timestamp, signature, apiKey = 'key') => JSON.stringify({ op: 'login', args: [JSON.stringify({
    type: 'login', httpMethod: 'GET', requestPath: '/ws/accounts', apiKey, passphrase: 'pass', timestamp, signature,
  })] });
  assert.equal(subscriptionKey(exchange, login(100, 'a')), subscriptionKey(exchange, login(200, 'b')));
  assert.notEqual(subscriptionKey(exchange, login(100, 'a')), subscriptionKey(exchange, login(100, 'a', 'other')));
  assert.notEqual(subscriptionKey(exchange, login(100, 'a')), subscriptionKey(exchange, { op: 'login', args: [{
    type: 'login', httpMethod: 'GET', requestPath: '/ws/accounts', apiKey: 'key', passphrase: 'pass', timestamp: 100, signature: 'a',
  }] }));
});
const cloid = (n) => `0x${String(n).padStart(32, '0')}`;
const callerIds = new Set([cloid(1), cloid(2), 'caller-a', 'caller-b']);

test('untimed HTTP replies retain consume-until-last queue semantics', () => {
  const queue = [{ body: 'first' }, { body: 'last' }];
  assert.equal(selectResponse(queue, 0).body, 'first');
  assert.equal(selectResponse(queue, 1000).body, 'last');
  assert.equal(selectResponse(queue, 2000).body, 'last');
  assert.equal(queue.length, 1);
  assert.equal(selectResponse([], 0), undefined);
  assert.equal(selectResponse(undefined, 0), undefined);
});

test('timed HTTP replies follow recorded state rather than polling frequency', () => {
  const startup = { availableAt: 0, body: '[]' };
  const placed = { availableAt: 500, body: '[{"oid":42}]' };
  const cancelled = { availableAt: 1500, body: '[]' };
  // Selection follows availability, even when records are not time-sorted.
  const queue = [startup, cancelled, placed];
  assert.equal(selectResponse(queue, 0), startup);
  for (let i = 0; i < 20; i++) assert.equal(selectResponse(queue, 499), startup);
  assert.equal(selectResponse(queue, 500), placed);
  for (let i = 0; i < 20; i++) assert.equal(selectResponse(queue, 1000), placed);
  assert.equal(selectResponse(queue, 1499), placed);
  assert.equal(selectResponse(queue, 1500), cancelled);
  assert.equal(selectResponse(queue, 9000), cancelled);
  assert.deepEqual(queue, [startup, cancelled, placed]);
});

test('timed HTTP replies never expose future state and treat unstamped startup state as time zero', () => {
  const future = { availableAt: 1000, body: 'later' };
  assert.equal(selectResponse([future], 0), undefined);
  const startup = { body: 'initial' };
  const first = { availableAt: 1000, body: 'first at threshold' };
  const queue = [startup, first, future];
  assert.equal(selectResponse(queue, 0), startup);
  assert.equal(selectResponse(queue, 999), startup);
  assert.equal(selectResponse(queue, 1000), future);
  assert.equal(queue.length, 3);
});

test('one write acknowledgement cannot satisfy a duplicate economic action', () => {
  const acknowledgement = { body: 'accepted' };
  const queue = [acknowledgement];
  assert.equal(selectResponse(queue, 0, { write: true }), acknowledgement);
  assert.equal(selectResponse(queue, 0, { write: true }), undefined);
  const twoAttempts = [{ body: 'first' }, { body: 'second' }];
  assert.equal(selectResponse(twoAttempts, 0, { write: true }).body, 'first');
  assert.equal(selectResponse(twoAttempts, 0, { write: true }).body, 'second');
  assert.equal(twoAttempts.length, 0);
});

test('POST info reads repeat while trading writes consume; other POSTs remain writes', () => {
  for (const url of [info, 'https://api.hyperliquid-testnet.xyz/info']) {
    assert.equal(isWriteRequest('POST', url), false);
    const queue = [{ body: 'snapshot' }];
    for (let i = 0; i < 10; i++)
      assert.equal(selectResponse(queue, 0, { write: isWriteRequest('POST', url) }).body, 'snapshot');
    assert.equal(queue.length, 1);
  }
  assert.equal(isWriteRequest('POST', exchange), true);
  assert.equal(isWriteRequest('POST', 'https://www.okx.com/api/v5/trade/order'), true);
  assert.equal(isWriteRequest('POST', 'https://other.example/info'), true);
  assert.equal(isWriteRequest('GET', exchange), false);
});

test('staged reads and writes cannot consume future command acknowledgements', () => {
  const initial = { body: 'initial' };
  const changed = { afterCommand: 1, body: 'changed' };
  const reads = [initial, changed];
  assert.equal(selectResponse(reads, 0), initial);
  assert.equal(selectResponse(reads, 0, { commandIndex: 1 }), changed);
  assert.equal(selectResponse(reads, 0, { commandIndex: 1 }), changed);
  const writes = [{ afterCommand: 2, availableAt: 100, body: 'accepted' }];
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 1 }), undefined);
  assert.equal(selectResponse(writes, 99, { write: true, commandIndex: 2 }), undefined);
  assert.equal(writes.length, 1);
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 2 }).body, 'accepted');
  assert.equal(writes.length, 0);
});

test('OKX generated broker IDs normalize while broker-prefixed caller IDs remain exact', () => {
  const url = 'https://www.okx.com/api/v5/trade/order';
  const generated = 'e847386590ce4dBC0000000000000000';
  const caller = 'e847386590ce4dBC0000000000000001';
  const options = {
    generatedBodyId: (key, value) => ['clOrdId', 'algoClOrdId'].includes(key) && value.startsWith('e847386590ce4dBC'),
  };
  const ids = new Set([caller]);
  const key = (id) => requestKey('POST', url, { instId: 'BTC-USDT-SWAP', clOrdId: id }, ids, options);
  assert.equal(key(generated), key('e847386590ce4dBC0000000000000002'));
  assert.notEqual(key(generated), key(caller));
  assert.notEqual(key(generated), key('other-exact-id'));
});

test('OKX login identity normalizes validated clocks/signatures and keeps passphrase', () => {
  const url = 'wss://ws.okx.com:8443/ws/v5/private';
  const login = (timestamp, sign, passphrase = 'passphrase') => ({
    op: 'login',
    args: [{ apiKey: 'key', passphrase, timestamp, sign }],
  });
  assert.equal(subscriptionKey(url, login('1', 'a')), subscriptionKey(url, login('2', 'b')));
  assert.notEqual(subscriptionKey(url, login('1', 'a')), subscriptionKey(url, login('1', 'a', 'other')));
  assert.deepEqual(
    subscriptionKeys(url, {
      op: 'subscribe',
      args: [
        { channel: 'orders', instType: 'SWAP' },
        { channel: 'positions', instType: 'ANY' },
      ],
    }),
    [
      ...subscriptionKeys(url, { op: 'subscribe', args: [{ channel: 'orders', instType: 'SWAP' }] }),
      ...subscriptionKeys(url, { op: 'subscribe', args: [{ channel: 'positions', instType: 'ANY' }] }),
    ],
  );
});

test('POST info identity includes every query field and ignores JSON key order', () => {
  const key = (body) => requestKey('post', info, body);
  assert.equal(key('{"user":"alice","type":"userFees"}'), key({ type: 'userFees', user: 'alice' }));
  for (const body of [
    { type: 'userFees', user: 'bob' },
    { type: 'userFunding', user: 'alice' },
    { type: 'userFees', user: 'alice', dex: 'xyz' },
  ]) {
    assert.notEqual(key(body), key({ type: 'userFees', user: 'alice' }));
  }
  assert.notEqual(key({ type: 'userFunding', startTime: 1 }), key({ type: 'userFunding', startTime: 2 }));
});

test('Hyperliquid generated cloids/signatures normalize while caller IDs and action fields remain exact', () => {
  const body = (id, price = '10', nonce = 1) => ({
    action: { type: 'order', orders: [{ a: 0, p: price, c: id }], grouping: 'na' },
    nonce,
    signature: { r: `r${nonce}`, s: 's', v: 27 },
  });
  const key = (body) => requestKey('POST', exchange, body, callerIds);
  assert.equal(key(body(cloid(3))), key(body(cloid(4), '10', 2)));
  assert.notEqual(key(body(cloid(1))), key(body(cloid(2))));
  assert.notEqual(key(body(cloid(1))), key(body(cloid(3))));
  assert.notEqual(key(body(cloid(3))), key(body(cloid(3), '11')));
  assert.notEqual(key({ action: { nonce: 1 } }), key({ action: { nonce: 2 } }));
});

test('Binance URL and batch caller IDs survive normalization', () => {
  const key = (id, timestamp = 1) =>
    requestKey(
      'POST',
      `https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&newClientOrderId=${id}&timestamp=${timestamp}&signature=s${timestamp}`,
      undefined,
      callerIds,
    );
  assert.equal(key('generated-a'), key('generated-b', 2));
  assert.equal(key('generated-a'), requestKey('POST', 'https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT'));
  assert.notEqual(key('caller-a'), key('caller-b'));
  const batch = (id) =>
    requestKey(
      'POST',
      `https://fapi.binance.com/fapi/v1/batchOrders?batchOrders=${encodeURIComponent(
        JSON.stringify([{ symbol: 'BTCUSDT', newClientOrderId: id, quantity: '1' }]),
      )}`,
      undefined,
      callerIds,
    );
  assert.equal(batch('generated-a'), batch('generated-b'));
  assert.notEqual(batch('caller-a'), batch('caller-b'));
});

test('large numeric IDs remain distinct in URL and JSON bodies', () => {
  assert.notEqual(requestKey('GET', `${info}?oid=9007199254740992`), requestKey('GET', `${info}?oid=9007199254740993`));
  assert.notEqual(
    requestKey('POST', info, '{"oid":9007199254740992}'),
    requestKey('POST', info, '{"oid":9007199254740993}'),
  );
});

test('form body identities normalize clocks and preserve caller IDs', () => {
  assert.equal(
    requestKey('POST', info, 'symbol=BTC&timestamp=1&signature=a'),
    requestKey('POST', info, new URLSearchParams('signature=b&timestamp=2&symbol=BTC')),
  );
  assert.notEqual(
    requestKey('POST', info, 'clientOrderId=caller-a', callerIds),
    requestKey('POST', info, 'clientOrderId=caller-b', callerIds),
  );
});

test('websocket identity ignores heartbeat and transport IDs but preserves subscriptions', () => {
  const socket = 'wss://api.hyperliquid.xyz/ws';
  assert.equal(subscriptionKey(socket, '{"method":"ping"}'), undefined);
  assert.equal(
    subscriptionKey(socket, '{"method":"SUBSCRIBE","id":1,"params":["a","b"]}'),
    subscriptionKey(socket, '{"params":["b","a"],"id":2,"method":"SUBSCRIBE"}'),
  );
  assert.deepEqual(subscriptionKeys(socket, '{"method":"SUBSCRIBE","id":1,"params":["a","b"]}'), [
    ...subscriptionKeys(socket, '{"method":"SUBSCRIBE","id":2,"params":["a"]}'),
    ...subscriptionKeys(socket, '{"method":"SUBSCRIBE","id":3,"params":["b"]}'),
  ]);
  assert.notEqual(
    subscriptionKey(socket, '{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC"}}'),
    subscriptionKey(socket, '{"method":"subscribe","subscription":{"type":"bbo","coin":"BTC"}}'),
  );
});

test('Bybit socket routing ignores cosmetic requestId while preserving other query fields', () => {
  const base = 'wss://stream.bybit.com/v5/public/linear';
  assert.equal(canonicalUrl(`${base}?requestId=recorded`), canonicalUrl(`${base}?requestId=live`));
  assert.equal(canonicalUrl(base), canonicalUrl(`${base}?requestId=live`));
  assert.notEqual(canonicalUrl(`${base}?max_active_time=1m`), canonicalUrl(`${base}?max_active_time=2m`));
});

test('Bybit auth expires/signatures normalize but the account and feed set stay exact', () => {
  const url = 'wss://stream.bybit.com/v5/private';
  const auth = (key, expires, signature) => ({ op: 'auth', args: [key, expires, signature] });
  assert.equal(
    subscriptionKey(url, auth('key-a', '100', 'a')),
    subscriptionKey(`${url}?requestId=generated`, auth('key-a', '200', 'b')),
  );
  assert.notEqual(subscriptionKey(url, auth('key-a', '100', 'a')), subscriptionKey(url, auth('key-b', '100', 'a')));
  assert.deepEqual(subscriptionKeys(url, { op: 'subscribe', args: ['order', 'position'], req_id: 'a' }), [
    ...subscriptionKeys(url, { op: 'subscribe', args: ['order'], req_id: 'b' }),
    ...subscriptionKeys(url, { op: 'subscribe', args: ['position'], req_id: 'c' }),
  ]);
  assert.notEqual(
    subscriptionKey(url, { op: 'subscribe', args: ['order'] }),
    subscriptionKey(url, { op: 'subscribe', args: ['wallet'] }),
  );
});

test('trim keeps body-specific replies, caller cloids and untimed config', () => {
  const dir = mkdtempSync(join(tmpdir(), 'rust-harness-'));
  try {
    const tape = join(dir, 'input.jsonl');
    const out = join(dir, 'output.jsonl.gz');
    const records = [
      { type: 'config', private: true },
      ...[1, 2, 3, 4].flatMap((n) => [
        { t: n, type: 'command', orders: [{ clientOrderId: cloid(n) }] },
        {
          t: n,
          type: 'http',
          method: 'POST',
          url: exchange,
          requestBody: JSON.stringify({ action: { type: 'order', orders: [{ c: cloid(n) }] }, nonce: n }),
          status: 200,
          body: '[]',
        },
      ]),
      ...[1, 2, 3, 4].map((n) => ({
        t: n,
        type: 'http',
        method: 'POST',
        url: info,
        requestBody: JSON.stringify({ type: `query${n}` }),
        status: 200,
        body: '[]',
      })),
    ];
    assert.equal(callerIdsOf(records).size, 4);
    writeFileSync(tape, records.map((record) => JSON.stringify(record)).join('\n'));
    const result = spawnSync(
      'corepack',
      ['yarn', '-s', 'rust:trim-tape', '--tape', tape, '--seconds', '1', '--out', out],
      { encoding: 'utf8' },
    );
    assert.equal(result.status, 0, result.stderr);
    const kept = gunzipSync(readFileSync(out))
      .toString()
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line));
    assert.equal(kept[0].type, 'config');
    assert.equal(kept.filter((record) => record.type === 'http').length, 8);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('Crypto.com envelope projection keeps exact caller IDs and unsafe nanosecond parameters', () => {
  const url = 'https://api.crypto.com/exchange/v1/private/get-trades';
  const options = { requestBody: (body) => body.params };
  const key = (end, caller = 'caller-a', nonce = 1) =>
    requestKey(
      'POST',
      url,
      `{"id":${nonce},"method":"private/get-trades","sig":"generated","params":{"end_time":${end},"client_oid":"${caller}"},"nonce":${nonce}}`,
      callerIds,
      options,
    );
  assert.equal(key('1610000000000000001'), key('1610000000000000001', 'caller-a', 2));
  assert.notEqual(key('1610000000000000001'), key('1610000000000000002'));
  assert.notEqual(key('1610000000000000001'), key('1610000000000000001', 'caller-b'));
});

test('Crypto.com subscriptions compare channels across batches and normalize audited auth', () => {
  const url = 'wss://stream.crypto.com/exchange/v1/user';
  const options = { cryptoProtocol: true };
  const keys = (message) => subscriptionKeys(url, message, new Set(), options);
  assert.deepEqual(
    keys({ id: 1, nonce: 100, method: 'subscribe', params: { channels: ['user.order', 'user.trade'] } }),
    [
      ...keys({ id: 2, nonce: 200, method: 'subscribe', params: { channels: ['user.order'] } }),
      ...keys({ id: 3, nonce: 300, method: 'subscribe', params: { channels: ['user.trade'] } }),
    ],
  );
  const auth = (api_key, nonce) => ({ id: nonce, nonce, method: 'public/auth', api_key, sig: String(nonce) });
  assert.deepEqual(keys(auth('key-a', 1)), keys(auth('key-a', 2)));
  assert.notDeepEqual(keys(auth('key-a', 1)), keys(auth('key-b', 1)));
  assert.deepEqual(keys({ id: 1, method: 'public/respond-heartbeat' }), []);
});

const url = 'https://example.test/orders';
const takeResponse = (queue, method, url, time = 0, commandIndex = Infinity) =>
  selectResponse(queue, time, { write: isWriteRequest(method, url), commandIndex });

test('BloFin cache busting omits only empty instrument queries', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('blofin');
  const root = 'https://openapi.blofin.com/api/v1/market/instruments';
  const key = (route) => requestKey('GET', route, undefined, new Set(), venue);
  assert.equal(key(`${root}?first`), key(`${root}?second`));
  assert.notEqual(key(`${root}?instType=SWAP`), key(root));
  assert.notEqual(key('https://openapi.blofin.com/other?first'), key('https://openapi.blofin.com/other'));
});

test('BloFin caller IDs stay exact while minted IDs and default trigger references vary', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('blofin');
  const caller = 'b3cdedc0a20880baCaller123';
  const key = (body) => requestKey('POST', url, body, new Set([caller]), venue);
  assert.equal(
    key({ clientOrderId: 'b3cdedc0a20880baMinted1', size: '1', triggerPriceType: 'last' }),
    key({ clientOrderId: 'b3cdedc0a20880baMinted2', size: '1' }),
  );
  assert.notEqual(
    key({ clientOrderId: caller, size: '1' }),
    key({ clientOrderId: 'b3cdedc0a20880baMinted2', size: '1' }),
  );
  assert.notEqual(key({ size: '1' }), key({ size: '2' }));
  assert.notEqual(key({ size: '1' }), key({ size: 1 }));
  assert.notEqual(key({ clientOrderId: 'external1' }), key({ clientOrderId: 'external2' }));
});

test('JSON batch query matching ignores object order but retains caller IDs and typed sizes', () => {
  const caller = 'broker-caller';
  const key = (orders) =>
    requestKey(
      'POST',
      `${url}?batchOrders=${encodeURIComponent(JSON.stringify(orders))}`,
      undefined,
      new Set([caller]),
    );
  assert.equal(
    key([{ newClientOrderId: 'minted1', quantity: '1', symbol: 'BTCUSDT' }]),
    key([{ symbol: 'BTCUSDT', quantity: '1', newClientOrderId: 'minted2' }]),
  );
  assert.notEqual(
    key([{ newClientOrderId: caller, quantity: '1' }]),
    key([{ newClientOrderId: 'changed', quantity: '1' }]),
  );
  assert.notEqual(key([{ quantity: '1' }]), key([{ quantity: 1 }]));
});

test('write matching preserves numeric/body types and exact caller IDs', () => {
  const ids = new Set(['caller-lowercase']);
  const key = (body) => requestKey('POST', url, body, ids);
  assert.notEqual(key({ quantity: 1 }), key({ quantity: '1' }));
  assert.notEqual(key({ takeProfit: { stopPrice: 1 } }), key({ takeProfit: '{"stopPrice":1}' }));
  assert.notEqual(key({ clientOrderId: 'caller-lowercase' }), key({ clientOrderId: 'generated' }));
  assert.equal(
    key({ clientOrderId: 'tsgenerated1', timestamp: 1, quantity: 1 }),
    key({ clientOrderId: 'tsgenerated2', timestamp: 2, quantity: 1 }),
  );
});
test('a single recorded write cannot serve duplicate submissions', () => {
  const response = { status: 200, body: '{}' };
  for (const method of ['POST', 'PUT', 'DELETE']) {
    const queue = [response];
    assert.equal(takeResponse(queue, method, url), response);
    assert.equal(takeResponse(queue, method, url), undefined);
  }
});
test('polling and listen-key reads can reuse responses; snapshots can change with tape time', () => {
  const first = { body: 'first' };
  const next = { body: 'next', availableAfter: 1000 };
  const queue = [first, next];
  assert.equal(takeResponse(queue, 'GET', url, 999), first);
  assert.equal(takeResponse(queue, 'GET', url, 1000), next);
  const auth = [first];
  const path = 'https://open-api.bingx.com/openApi/user/auth/userDataStream';
  assert.equal(takeResponse(auth, 'POST', path), first);
  assert.equal(takeResponse(auth, 'PUT', path), first);
  assert.equal(auth.length, 1);
});
test('Binance query matching preserves a caller ID while ignoring generated IDs', () => {
  const root = 'https://fapi.binance.com/fapi/v1/order';
  const ids = new Set(['caller']);
  assert.equal(
    requestKey('POST', `${root}?newClientOrderId=x&timestamp=1`),
    requestKey('POST', `${root}?newClientOrderId=y&timestamp=2`),
  );
  assert.notEqual(
    requestKey('POST', `${root}?newClientOrderId=caller`, undefined, ids),
    requestKey('POST', `${root}?newClientOrderId=changed`, undefined, ids),
  );
});

test('nested map order is ignored and unsafe numeric order IDs remain exact', () => {
  assert.equal(
    requestKey('POST', url, { stop: { price: 1, quantity: 2 } }),
    requestKey('POST', url, { stop: { quantity: 2, price: 1 } }),
  );
  assert.notEqual(
    requestKey('POST', url, '{"cancelOrderId":8389765519993909000}'),
    requestKey('POST', url, '{"cancelOrderId":8389765519993909001}'),
  );
  assert.notEqual(
    requestKey('POST', url, '{"cancelOrderId":8389765519993909000}'),
    requestKey('POST', url, '{"cancelOrderId":"8389765519993909000"}'),
  );
});

test('subscription matching ignores request IDs and batching, retaining native topics', () => {
  const socket = 'wss://ws-market-swap.qq-os.com/ws';
  assert.deepEqual(
    subscriptionKeys(socket, '{"id":"random","reqType":"sub","dataType":"market.contracts"}', new Set(), {
      bingxProtocol: true,
    }),
    ['WS wss://ws-market-swap.qq-os.com/ws? {"dataType":"market.contracts","reqType":"sub"}'],
  );
  assert.deepEqual(subscriptionKeys(socket, 'Ping', new Set(), { bingxProtocol: true }), []);
  assert.deepEqual(subscriptionKeys(socket, '{"pong":1}', new Set(), { bingxProtocol: true }), []);
  assert.deepEqual(
    subscriptionKeys(
      'wss://fstream.binance.com/public/ws',
      '{"method":"SUBSCRIBE","params":["btcusdt@depth@100ms","ethusdt@depth@100ms"],"id":1}',
    ),
    [
      'WS wss://fstream.binance.com/public/ws? {"method":"SUBSCRIBE","params":["btcusdt@depth@100ms"]}',
      'WS wss://fstream.binance.com/public/ws? {"method":"SUBSCRIBE","params":["ethusdt@depth@100ms"]}',
    ],
  );
});

test('Bybit connection IDs vary while body types and subscription topics stay exact', () => {
  const bytes = new TextEncoder().encode('{"category":"linear","qty":"1","requestId":"fresh"}');
  assert.equal(
    requestKey('POST', url, bytes),
    requestKey('POST', url, { qty: '1', category: 'linear', requestId: 'other' }),
  );
  assert.notEqual(requestKey('POST', url, bytes), requestKey('POST', url, { qty: 1, category: 'linear' }));
  assert.deepEqual(
    subscriptionKeys(
      'wss://stream.bybit.com/v5/private?requestId=fresh',
      '{"op":"subscribe","args":["order","execution"]}',
    ),
    [
      'WS wss://stream.bybit.com/v5/private? {"args":["order"],"op":"subscribe"}',
      'WS wss://stream.bybit.com/v5/private? {"args":["execution"],"op":"subscribe"}',
    ],
  );
});

test('command and time stages cannot serve future snapshots or reuse writes', () => {
  const initial = { method: 'GET', url, body: 'initial' };
  const staged = { method: 'GET', url, body: 'next', afterCommand: 2, availableAfter: 100 };
  const queue = [initial, staged];
  assert.equal(takeResponse(queue, 'GET', url, 200, 1), initial);
  assert.equal(takeResponse(queue, 'GET', url, 99, 2), initial);
  assert.equal(takeResponse(queue, 'GET', url, 100, 2), staged);
  const write = { method: 'POST', url, afterCommand: 2 };
  const writes = [write];
  assert.equal(takeResponse(writes, 'POST', url, 0, 1), undefined);
  assert.equal(takeResponse(writes, 'POST', url, 0, 2), write);
  assert.equal(takeResponse(writes, 'POST', url, 0, 2), undefined);
});

test('nested generated OKX IDs vary while caller IDs and body types stay exact', () => {
  const opts = { generatedBodyId: (key, value) => key === 'clOrdId' && value.startsWith('broker') };
  const body = (id, sz = '1') => [{ clOrdId: id, sz }];
  assert.equal(
    requestKey('POST', url, body('broker1'), new Set(), opts),
    requestKey('POST', url, body('broker2'), new Set(), opts),
  );
  assert.notEqual(
    requestKey('POST', url, body('broker1'), new Set(['broker1']), opts),
    requestKey('POST', url, body('broker2'), new Set(['broker1']), opts),
  );
  assert.notEqual(
    requestKey('POST', url, body('broker1'), new Set(), opts),
    requestKey('POST', url, body('broker2', 1), new Set(), opts),
  );
});

test('Crypto signing envelope varies while typed params and caller IDs remain exact', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('cryptocom');
  const route = 'https://api.crypto.com/exchange/v1/private/create-order';
  const key = (params, id = 1) =>
    requestKey(
      'POST',
      route,
      JSON.stringify({ id, method: 'private/create-order', nonce: id, sig: String(id), params }),
      new Set(['caller']),
      venue,
    );
  assert.equal(key({ quantity: '1', client_oid: 'caller' }, 1), key({ client_oid: 'caller', quantity: '1' }, 2));
  assert.notEqual(key({ quantity: '1', client_oid: 'caller' }), key({ quantity: 1, client_oid: 'caller' }));
  assert.notEqual(key({ client_oid: 'caller' }), key({ client_oid: 'other' }));
  assert.notEqual(requestKey('POST', route, '[1]'), requestKey('POST', route, '{"0":1}'));
  assert.deepEqual(
    subscriptionKeys(
      'wss://stream.crypto.com/exchange/v1/market',
      '{"method":"subscribe","params":{"channels":["ticker.BTCUSD-PERP"]}}',
    ),
    [
      'WS wss://stream.crypto.com/exchange/v1/market? {"method":"subscribe","params":{"channels":["ticker.BTCUSD-PERP"]}}',
    ],
  );
});

test('Bybit spot instrument equivalence leaves other categories and routes exact', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('bybitspot');
  const root = 'https://api.bybit.com/v5/market/instruments-info';
  const key = (url) => requestKey('GET', url, undefined, new Set(), venue);
  assert.equal(key(`${root}?category=spot&limit=1000&cursor=invalid`), key(`${root}?category=spot`));
  assert.notEqual(key(`${root}?category=linear&limit=1000`), key(`${root}?category=linear`));
  assert.notEqual(
    key('https://api.bybit.com/v5/order/realtime?category=spot&cursor=next'),
    key('https://api.bybit.com/v5/order/realtime?category=spot'),
  );
  assert.notEqual(
    requestKey('GET', `${root}?category=spot&limit=1000`, undefined, new Set(), venueOf('bybit')),
    requestKey('GET', `${root}?category=spot`, undefined, new Set(), venueOf('bybit')),
  );
});

test('Hyro generated identities vary while caller IDs, categories and native quantities stay exact', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('hyrotrader');
  const url = 'https://challenge.example.test/v5/order/create';
  const key = (body, ids = new Set()) => requestKey('POST', url, body, ids, venue);
  assert.equal(
    key({ category: 'linear', qty: '1', orderLinkId: 'generated-one' }),
    key({ category: 'linear', qty: '1', orderLinkId: 'generated-two' }),
  );
  assert.notEqual(
    key({ orderLinkId: 'caller' }, new Set(['caller'])),
    key({ orderLinkId: 'generated' }, new Set(['caller'])),
  );
  assert.notEqual(key({ category: 'linear', qty: '1' }), key({ category: 'inverse', qty: '1' }));
  assert.notEqual(key({ qty: '1' }), key({ qty: 1 }));
  assert.notEqual(key({ qty: '1' }), key({ qty: '2' }));
});

test('BloFin normalized login keeps credentials and topics while verified nonce and signature vary', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('blofin');
  const url = 'wss://openapi.blofin.com/ws/private';
  const login = (apiKey, nonce) =>
    subscriptionKeys(
      url,
      {
        op: 'login',
        args: [{ apiKey, passphrase: 'passphrase', timestamp: nonce, nonce: String(nonce), sign: String(nonce) }],
      },
      new Set(),
      venue,
    );
  assert.deepEqual(login('key-a', 1), login('key-a', 2));
  assert.notDeepEqual(login('key-a', 1), login('key-b', 1));
  assert.notDeepEqual(
    subscriptionKeys(url, { op: 'subscribe', args: [{ channel: 'orders', instId: 'ANY' }] }),
    subscriptionKeys(url, { op: 'subscribe', args: [{ channel: 'orders', instId: 'BTC-USDT' }] }),
  );
});

test('caller IDs are collected from every command shape without collecting tape responses', () => {
  assert.deepEqual(
    callerIdsOf([
      { type: 'command', orders: [{ clientOrderId: 'place' }], nested: { clientOrderId: 'amend' } },
      { type: 'http', body: { clientOrderId: 'unrelated' } },
    ]),
    new Set(['place', 'amend']),
  );
});

test('KuCoin topic batches and acknowledgement preferences retain exact token and channel identity', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('kucoin');
  const url = 'wss://bullet.test/socket?token=public&connectId=runtime';
  const keys = (topic, response, route = url, type = 'subscribe') =>
    subscriptionKeys(route, { type, id: 'generated', topic, privateChannel: false, response }, new Set(), venue);
  assert.deepEqual(keys('/contractMarket/snapshot:A,B', false), [
    ...keys('/contractMarket/snapshot:A', true, url.replace('runtime', 'recorded')),
    ...keys('/contractMarket/snapshot:B', true),
  ]);
  assert.notDeepEqual(
    keys('/contractMarket/snapshot:A', false),
    keys('/contractMarket/snapshot:A', false, url.replace('token=public', 'token=private')),
  );
  assert.notDeepEqual(keys('/contractMarket/snapshot:A', false), keys('/contractMarket/level2:A', false));
  assert.notDeepEqual(
    keys('/contractMarket/snapshot:A', false),
    keys('/contractMarket/snapshot:A', false, url, 'unsubscribe'),
  );
  assert.deepEqual(subscriptionKeys(url, '{"type":"ping","id":1}', new Set(), venue), []);
});

test('WOO native topics and auth identity remain exact while fresh clocks vary', () => {
  const url = 'wss://wss.woox.io/v2/ws/private/stream/app';
  const options = { wooProtocol: true };
  const keys = (value) => subscriptionKeys(url, JSON.stringify(value), new Set(), options);
  assert.notDeepEqual(
    keys({ event: 'subscribe', topic: 'executionreport' }),
    keys({ event: 'subscribe', topic: 'algoexecutionreportv2' }),
  );
  for (const event of ['ping', 'pong']) assert.deepEqual(keys({ event }), []);
  const auth = (apikey, timestamp, sign) => keys({ event: 'auth', params: { apikey, timestamp, sign } });
  assert.deepEqual(auth('key', 1, 'a'), auth('key', 2, 'b'));
  assert.notDeepEqual(auth('key', 1, 'a'), auth('other', 1, 'a'));
});

test('GRVT bare heartbeats are not subscriptions while all other raw identities remain visible', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('grvt');
  for (const url of ['wss://trades.grvt.io/ws', 'wss://market-data.grvt.io/ws/full']) {
    for (const text of ['ping', 'pong']) {
      assert.equal(subscriptionKey(url, text, new Set(), venue), undefined);
      assert.deepEqual(subscriptionKeys(url, text, new Set(), venue), []);
      for (const options of [{}, { rust: 'bybit' }]) {
        assert.match(subscriptionKey(url, text, new Set(), options), new RegExp(` ${text}$`));
        assert.equal(subscriptionKeys(url, text, new Set(), options).length, 1);
      }
    }
    for (const text of ['Ping', 'Pong', ' ping', 'pong ', 'ping\n', 'ping-other', 'status:ready', '"ping"', '"pong"']) {
      assert.equal(typeof subscriptionKey(url, text, new Set(), venue), 'string', text);
      assert.equal(subscriptionKeys(url, text, new Set(), venue).length, 1, text);
    }
  }
});

test('GRVT heartbeat identity correction preserves exact authentication and feed selectors', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('grvt');
  const url = 'wss://trades.grvt.io/ws';
  const keys = (message) => subscriptionKeys(url, JSON.stringify(message), new Set(), venue);
  const auth = { type: 'auth', session_cookie: 'rm=true; gravity=fixture-session-1', account_id: 'fixture-account', testnet: false };
  assert.equal(keys(auth).length, 1);
  for (const changed of [
    { session_cookie: 'rm=true; gravity=other-session' },
    { account_id: 'other-account' },
    { testnet: true },
  ]) assert.notDeepEqual(keys(auth), keys({ ...auth, ...changed }));
  const subscription = { method: 'subscribe', request_id: 1, stream: 'v1.order', feed: ['777'], is_full: false };
  assert.equal(keys(subscription).length, 1);
  for (const changed of [
    { feed: ['778'] }, { stream: 'v1.fill' }, { is_full: true }, { method: 'unsubscribe' },
  ]) assert.notDeepEqual(keys(subscription), keys({ ...subscription, ...changed }));
  const publicUrl = 'wss://market-data.grvt.io/ws/full';
  const ticker = (selector) => subscriptionKeys(publicUrl, JSON.stringify({
    jsonrpc: '2.0', method: 'subscribe', params: { stream: 'v1.ticker.d', selectors: [selector] },
  }), new Set(), venue);
  assert.notDeepEqual(ticker('BTC_USDT_Call_25Dec26_50000@500'), ticker('BTC_USDT_Call_25Dec26_60000@500'));
});

test('GRVT independent socket audit still rejects missed wrong extra and unauthenticated heartbeat replies', async () => {
  const { venueOf } = await import('./venues.mjs');
  const venue = venueOf('grvt');
  const url = 'wss://trades.grvt.io/ws';
  const authenticated = () => {
    const audit = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
    audit.connected(url, 1);
    audit.sent(url, JSON.stringify({ type: 'auth', session_cookie: 'rm=true; gravity=fixture-session-1',
      account_id: 'fixture-account', testnet: false }), 1);
    audit.received(url, '{"type":"auth_success"}', 1);
    for (const stream of ['v1.order', 'v1.position', 'v1.fill'])
      audit.sent(url, JSON.stringify({ method: 'subscribe', stream, feed: ['777'], is_full: false }), 1);
    return audit;
  };
  const valid = authenticated();
  valid.sent(url, 'ping', 1);
  valid.received(url, 'pong', 1);
  valid.received(url, 'ping', 1);
  valid.sent(url, 'pong', 1);
  assert.deepEqual(valid.errors(), []);
  const missing = authenticated();
  missing.received(url, 'ping', 1);
  assert.ok(missing.errors().includes('missing GRVT pong'));
  const wrong = authenticated();
  wrong.received(url, 'ping', 1);
  wrong.sent(url, '{"op":"pong"}', 1);
  assert.ok(wrong.errors().includes('missing GRVT pong'));
  assert.ok(wrong.errors().some((message) => message.includes('changed selector')));
  const extra = authenticated();
  extra.sent(url, 'pong', 1);
  assert.ok(extra.errors().includes('unexpected GRVT pong'));
  const unauthenticated = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
  unauthenticated.connected(url, 1);
  unauthenticated.sent(url, 'ping', 1);
  assert.ok(unauthenticated.errors().includes('GRVT private first frame was not authentication'));
});

test('Coinbase channel product arrays expand exact topics and batch identities', () => {
  const url = 'wss://advanced-trade-ws.coinbase.com';
  const batch = { type: 'subscribe', channel: 'level2', product_ids: ['BTC-USD', 'ETH-USD'] };
  assert.deepEqual(subscriptionMessage(batch), { op: 'subscribe', topics: ['level2.BTC-USD', 'level2.ETH-USD'] });
  assert.deepEqual(subscriptionMessage({ ...batch, type: 'unsubscribe', product_ids: ['ETH-USD'] }), {
    op: 'unsubscribe',
    topics: ['level2.ETH-USD'],
  });
  const batched = subscriptionKeys(url, JSON.stringify(batch)).sort();
  const individual = ['BTC-USD', 'ETH-USD']
    .flatMap((product) => subscriptionKeys(url, JSON.stringify({ ...batch, product_ids: [product] })))
    .sort();
  assert.deepEqual(batched, individual);
  assert.notDeepEqual(batched, subscriptionKeys(url, JSON.stringify({ ...batch, channel: 'candles' })));
  assert.notDeepEqual(
    batched,
    subscriptionKeys(url, JSON.stringify({ ...batch, product_ids: ['BTC-USD', 'SOL-USD'] })),
  );
  assert.deepEqual(
    subscriptionMessage({ type: 'subscribe', channel: 'heartbeats', product_ids: ['BTC-USD', 'ETH-USD'] }),
    { op: 'subscribe', topics: ['heartbeats'] },
  );
  assert.deepEqual(subscriptionMessage({ type: 'subscribe', channel: 'user' }), { op: 'subscribe', topics: ['user'] });
  assert.equal(
    subscriptionKeys(
      url,
      JSON.stringify({ type: 'subscribe', channel: 'heartbeats', product_ids: ['BTC-USD', 'ETH-USD'] }),
    ).length,
    1,
  );
});
