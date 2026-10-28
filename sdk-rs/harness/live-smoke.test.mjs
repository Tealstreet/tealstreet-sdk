import { test } from 'node:test';
import assert from 'node:assert/strict';
import { LIVE_SMOKE_MAX_NOTIONAL, smokeArgs, smokeOrder, marketRules, fencedTransport, selectSmokeMarket, readCredentials, main } from './live-smoke.mjs';
import { Readable } from 'node:stream';
import { MARKET_FLAG } from '../hosts/wasm/js/wire.mjs';
import { subscriptionKey } from './requests.mjs';
import { assertLiveSmokeComplete } from './live-smoke-proof.mjs';

const market = { symbol: 'BTCUSDT', id: 'BTC/USDT:USDT', quote: 'USDT', flags: MARKET_FLAG.ACTIVE,
  precisionPrice: 0.1, precisionAmount: 0.001, minAmount: 0.001, maxAmount: 1000, minPrice: NaN, contractSize: NaN };
const book = { bids: [100000, 1, 1], asks: [100001, 1, 1] };

test('CLI never infers a mainnet request and accepts exactly one environment', () => {
  assert.equal(smokeArgs(['--venue', 'binance']).mode, undefined);
  assert.equal(smokeArgs(['--venue', 'binance', '--mainnet']).mode, 'mainnet');
  for (const args of [[], ['--venue', 'binance', '--demo', '--testnet'], ['--venue', 'binance', '--timeout-ms', 'NaN'], ['--venue', 'binance', '--secret', 'bad']])
    assert.throws(() => smokeArgs(args));
});

test('default selection includes reviewed Bitget simulation quotes', () => {
  for (const quote of ['SUSDT', 'SUSDC']) {
    const simulated = { ...market, quote, symbol: `BTC${quote}` };
    assert.equal(selectSmokeMarket([{ ...simulated, flags: 0 }, simulated]), simulated);
    assert.equal(selectSmokeMarket([{ ...simulated, flags: MARKET_FLAG.ACTIVE | MARKET_FLAG.READ_ONLY }]), undefined);
  }
});

test('minimum resting order is aligned and covers notional at its lower amended price', () => {
  const order = smokeOrder(market, book, { minNotional: 50, lowerMultiplier: 0.8, notionalKnown: true });
  assert.deepEqual(order, { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.001, price: 90000, amendPrice: 89000, timeInForce: 'PostOnly' });
  assert.ok(order.amount * order.amendPrice >= 50);
  assert.ok((order.amount - market.precisionAmount) * order.amendPrice < 50);
});

test('fine ticks on high prices keep the floor-bound order and amendment one tick apart', () => {
  // 57000.97 / 0.01 is 5700096.9999999 in binary; a fixed epsilon rounded it to 57000.96.
  const order = smokeOrder({ ...market, precisionPrice: 0.01 }, { bids: [60000], asks: [60001] },
    { minNotional: 5, lowerMultiplier: 0.95, notionalKnown: true });
  assert.equal(order.price, 57000.97);
  assert.equal(order.amendPrice, 57000.96);
});

test('a real fraction of a step still rounds in the requested direction', () => {
  const lot = { ...market, precisionPrice: 1e-8, minAmount: 1e-8, precisionAmount: 1e-8, maxAmount: 1e6 };
  // Floor: 0.09999999995 sits half a tick below 0.1, so the order price is 0.09999999.
  const below = smokeOrder(lot, { bids: [0.09999999995 / 0.9], asks: [0.2] }, { minNotional: 0, notionalKnown: true });
  assert.equal(below.price, 0.09999999);
  // Ceil: 0.10000000005 units of minimum quantity must round up to 0.10000001.
  const above = smokeOrder({ ...lot, minAmount: 0.10000000005 }, { bids: [1], asks: [2] }, { minNotional: 0, notionalKnown: true });
  assert.equal(above.amount, 0.10000001);
});

test('inverse minimum notional uses quote contracts', () => {
  assert.equal(smokeOrder({ ...market, flags: MARKET_FLAG.ACTIVE | MARKET_FLAG.INVERSE,
    precisionAmount: 1, minAmount: 1, contractSize: 10 }, book, { minNotional: 21, notionalKnown: true }).amount, 3);
});

test('native lot rules override display quantities and bound the native maximum', () => {
  const rules = { notionalKnown: true, minNotional: 50, minimumAmount: 1, amountStep: 1,
    maximumAmount: 2, notionalPerAmount: 0.001, quoteQuantity: false };
  assert.equal(smokeOrder({ ...market, minAmount: 0.001, maxAmount: 0.002 }, book, rules).amount, 1);
  assert.throws(() => smokeOrder(market, book, { ...rules, minNotional: 200 }), /minimum-exceeds/);
  assert.equal(smokeOrder({ ...market, minAmount: 0 }, book, { ...rules, minNotional: 0 }).amount, 1);
});

test('invalid, stale-compatible shapes and no valid far price fail before orders', () => {
  for (const change of [{ flags: 0 }, { flags: 1 | MARKET_FLAG.READ_ONLY }, { precisionPrice: 0 }, { precisionAmount: NaN }, { minAmount: 0 }, { maxAmount: 0.0001 }])
    assert.throws(() => smokeOrder({ ...market, ...change }, book, { notionalKnown: true }));
  assert.throws(() => smokeOrder(market, { bids: [1], asks: [1] }, { notionalKnown: true }));
  assert.throws(() => smokeOrder(market, book, { lowerMultiplier: 0.99, notionalKnown: true }));
  assert.throws(() => smokeOrder({ ...market, maxAmount: 0.001 }, book, { minNotional: 1000, notionalKnown: true }));
  assert.throws(() => smokeOrder(market, book), /unknown-minimum-notional/);
});

test('instrument rules are selected by exact native or display market identity', () => {
  const records = [{ type: 'http', method: 'GET', url: 'https://fapi.binance.com/fapi/v1/exchangeInfo', status: 200, body: JSON.stringify({ symbols: [
    { symbol: 'ETHUSDT', filters: [{ filterType: 'MIN_NOTIONAL', notional: '99999' }] },
    { symbol: 'BTCUSDT', filters: [{ filterType: 'LOT_SIZE', minQty: '0.001', stepSize: '0.001', maxQty: '1000' },
      { filterType: 'MIN_NOTIONAL', notional: '100' }, { filterType: 'PERCENT_PRICE', multiplierDown: '0.8' }] },
  ] }) }];
  const rules = marketRules(records, market);
  assert.equal(rules.minNotional, 100);
  assert.equal(rules.lowerMultiplier, 0.8);
  assert.equal(rules.notionalKnown, true);
});

test('transport fences run before every HTTP, socket and outbound WS write', async () => {
  let calls = 0;
  const live = { request: () => calls++, connect: () => { calls++; return { send: () => calls++, close() {} }; } };
  const transport = fencedTransport(live, { allowUrl: (url) => url === 'https://test.example',
    validateRequest: () => { throw new Error('request-policy'); }, validateSocketMessage: () => { throw new Error('socket-policy'); } });
  assert.throws(() => transport.request({ url: 'https://mainnet.example' }), /refused/);
  assert.throws(() => transport.request({ url: 'https://test.example' }), /policy/);
  assert.equal(calls, 0);
  const socket = transport.connect('https://test.example', {});
  assert.throws(() => socket.send('bad'), /policy/);
  assert.equal(calls, 1);
});

test('private Binance liveness query is independent of real feed subscriptions', () => {
  assert.equal(subscriptionKey('wss://demo-fstream.binance.com/private/ws?listenKey=fake', { method: 'LIST_SUBSCRIPTIONS', id: 42 }), undefined);
  assert.match(subscriptionKey('wss://demo-fstream.binance.com/public/ws', { method: 'SUBSCRIBE', params: ['btcusdt@depth'], id: 42 }), /btcusdt@depth/);
});

test('failed/aborted captures cannot gate as completed live lifecycle proof', () => {
  const meta = { type: 'meta', liveSmoke: true };
  assert.throws(() => assertLiveSmokeComplete([meta]), /incomplete/);
  assert.throws(() => assertLiveSmokeComplete([meta, { type: 'live-smoke-result', ok: false }]), /failed/);
  assert.throws(() => assertLiveSmokeComplete([meta, { type: 'live-smoke-result', ok: true, cleanupComplete: true, authoritative: true, stage: 'complete', symbol: 'BTCUSDT', commands: [] },
    { type: 'live-smoke-verification', ok: true }]), /missing/);
  assert.doesNotThrow(() => assertLiveSmokeComplete([{ type: 'meta', private: true }]));
});


test('fixed live notional cap rejects rounded minimums, inverse face value and contract scales', () => {
  assert.equal(LIVE_SMOKE_MAX_NOTIONAL, 100);
  for (const [selected, rules] of [
    [market, { notionalKnown: true, minNotional: 100 }],
    [{ ...market, minAmount: 1, precisionAmount: 1 }, { notionalKnown: true, minNotional: 0 }],
    [{ ...market, flags: MARKET_FLAG.ACTIVE | MARKET_FLAG.INVERSE, minAmount: 1, precisionAmount: 1, contractSize: 101 },
      { notionalKnown: true, minNotional: 0 }],
    [market, { notionalKnown: true, minimumAmount: 1, amountStep: 1, maximumAmount: 1000, notionalPerAmount: 0.002 }],
    [market, { notionalKnown: true, minNotional: 0, notionalPerAmount: Number.MAX_VALUE }],
  ]) assert.throws(() => smokeOrder(selected, book, rules), /notional-cap-exceeded/);
  for (const minNotional of [NaN, Infinity, -1])
    assert.throws(() => smokeOrder(market, book, { notionalKnown: true, minNotional }), /unknown-minimum/);
  const order = smokeOrder(market, book, { notionalKnown: true, minNotional: 50 }, true);
  assert.equal(order.timeInForce, 'PostOnly');
  assert.ok(order.amount * order.price <= LIVE_SMOKE_MAX_NOTIONAL);
  assert.ok(order.amount * order.amendPrice <= LIVE_SMOKE_MAX_NOTIONAL);
  for (const option of ['--max-notional', '--notional-cap', '--amount', '--price'])
    assert.throws(() => smokeArgs(['--venue', 'binance', option, '999999']));
});


test('explicit market selection cannot treat a BTC or ETH quote as a dollar notional', () => {
  for (const quote of ['BTC', 'ETH', '', undefined]) {
    const selected = { ...market, quote };
    assert.equal(selectSmokeMarket([selected], market.symbol), undefined);
    assert.throws(() => smokeOrder(selected, book, { notionalKnown: true }), /unknown-notional-currency/);
  }
  assert.equal(smokeOrder(market, book, { notionalKnown: true }).timeInForce, 'PostOnly');
  assert.equal(smokeOrder(market, book, { notionalKnown: true }, false).timeInForce, undefined);
});

test('managed smoke fence validates final packet before bind and cancels refusal', async () => {
  let canceled = 0, bound = 0, sent = 0;
  const resource = { expiresAt: 100,
    cancel() { canceled++; },
    bind() { bound++; return { ready: () => true, dispatch: () => { sent++; } }; },
  };
  const live = { prepareHttp: () => resource };
  const transport = fencedTransport(live, { allowUrl: (url) => url === 'https://test.example/allowed',
    validateRequest: (packet) => assert.equal(packet.headers.proof, 'signed') });
  const prepared = await transport.prepareHttp({ method: 'POST', url: 'https://test.example/allowed' });
  assert.equal(prepared.expiresAt, 100);
  assert.throws(() => prepared.bind({ method: 'POST', url: 'https://test.example/refused', headers: { proof: 'signed' } }), /http-endpoint-refused/);
  assert.equal(canceled, 1);
  assert.equal(bound, 0);
  assert.equal(sent, 0);
  assert.equal(fencedTransport({}, { allowUrl: () => true }).prepareHttp, undefined);
});

test('managed smoke fence preserves receivers and validates signed data once before dispatch', async () => {
  const events = [];
  const bound = { ready() { assert.equal(this, bound); return true; }, cancel() { assert.equal(this, bound); events.push('bound-cancel'); },
    dispatch() { assert.equal(this, bound); events.push('send'); return 'response'; } };
  const resource = { cancel() { assert.equal(this, resource); events.push('prepare-cancel'); },
    bind(packet) { assert.equal(this, resource); assert.equal(packet.headers.proof, 'signed'); events.push('bind'); return bound; } };
  const live = { prepareHttp() { assert.equal(this, live); return resource; } };
  const transport = fencedTransport(live, { allowUrl: () => true,
    validateRequest: (packet) => { assert.equal(packet.headers.proof, 'signed'); events.push('validate'); } });
  const prepared = await transport.prepareHttp({ url: 'https://test.example', method: 'POST' });
  const ready = prepared.bind({ url: 'https://test.example?stamp=1', method: 'POST', headers: { proof: 'signed' } });
  assert.equal(ready.ready(), true);
  assert.deepEqual(events, ['validate', 'bind']);
  assert.equal(ready.dispatch(), 'response');
  assert.deepEqual(events, ['validate', 'bind', 'send']);
  ready.cancel(); prepared.cancel();
  assert.deepEqual(events.slice(-2), ['bound-cancel', 'prepare-cancel']);
});

test('CLI accepts stdin credentials, dry run and the credential listing without a venue', () => {
  assert.deepEqual(smokeArgs(['--list-credentials']), { listCredentials: true });
  const values = smokeArgs(['--venue', 'bitunix', '--mainnet', '--credentials-stdin', '--dry-run']);
  assert.equal(values.credentialsStdin, true);
  assert.equal(values.dryRun, true);
  assert.equal(smokeArgs(['--venue', 'binance']).dryRun, false);
});

test('stdin credentials are one bounded object of strings and are never echoed', async () => {
  const read = (text) => readCredentials(Readable.from([Buffer.from(text)]));
  assert.deepEqual(await read('{"KEY":"stdin-key","SECRET":"stdin-secret"}'), { KEY: 'stdin-key', SECRET: 'stdin-secret' });
  for (const text of ['not-json-secret-value', '["secret-value"]', '{"KEY":7}', 'null'])
    await assert.rejects(read(text), (error) => !error.message.includes('secret-value') && !error.message.includes('not-json'));
  await assert.rejects(readCredentials(Readable.from([Buffer.alloc(64 * 1024 + 1, 0x20)])), /too large/);
});

test('a credential failure on stdin prints only the generic refusal', async () => {
  const output = [];
  const log = console.log, error = console.error;
  console.log = (...items) => output.push(items.join(' '));
  console.error = (...items) => output.push(items.join(' '));
  try {
    const code = await main(['--venue', 'bitunix', '--mainnet', '--credentials-stdin'], {},
      Readable.from([Buffer.from('{"KEY":"stdin-key-must-not-print","SECRET":"x"}')]));
    assert.equal(code, 1);
  } finally { console.log = log; console.error = error; }
  assert.equal(output.some((line) => line.includes('stdin-key-must-not-print')), false);
  assert.equal(output.length, 1);
});
