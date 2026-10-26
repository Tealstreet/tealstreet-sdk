import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { AbiCoder, getBytes, hashMessage, keccak256, SigningKey } from 'ethers';
import {
  asterRequestUrl,
  asterSocketAudit,
  ASTER_PRIVATE_KEY,
  ASTER_SIGNER,
  ASTER_USER,
  createAsterRequestValidator,
} from './asterdex-requests.mjs';
import { requestKey } from './requests.mjs';

const vectors = JSON.parse(
  readFileSync(new URL('../venues/asterdex/tests/fixtures/signing.json', import.meta.url)),
).cases;
const legacy = (params = {}, patch = {}) => {
  const now = 1748310859508;
  const fields = { symbol: 'BTCUSDT', quantity: '0.001', ...params, recvWindow: '20000', timestamp: String(now) };
  const payload = Object.entries(fields)
    .map(([k, v]) => `${k}=${v}`)
    .join('&');
  return {
    method: 'POST',
    url: `https://fapi.asterdex.com/fapi/v1/order?${new URLSearchParams({ ...fields, signature: createHmac('sha256', 'replay-secret').update(payload).digest('hex') })}`,
    headers: { 'X-MBX-APIKEY': 'replay-key' },
    now,
    ...patch,
  };
};
const options = { key: ASTER_SIGNER, secret: ASTER_PRIVATE_KEY, applicationId: ASTER_USER };

test('Aster V1 independent oracle binds key, business payload, clock, window and body', () => {
  const valid = legacy();
  assert.doesNotThrow(() => createAsterRequestValidator()(valid));
  assert.doesNotThrow(() => createAsterRequestValidator({ auth: { privateKey: ASTER_PRIVATE_KEY } })(valid));
  for (const bad of [
    { ...valid, headers: { 'X-MBX-APIKEY': 'wrong' } },
    { ...valid, url: valid.url.replace('quantity=0.001', 'quantity=0.01') },
    { ...valid, url: valid.url.replace('recvWindow=20000', 'recvWindow=5000') },
    { ...valid, now: valid.now + 60001 },
    { ...valid, body: '{}' },
    { ...valid, url: valid.url.replace('fapi.asterdex.com', 'wrong.example') },
    { ...valid, url: valid.url + '&quantity=0.001' },
  ])
    assert.throws(() => createAsterRequestValidator()(bad));
});

test('Aster V1 timing uses recvWindow and the strict one-second future bound', () => {
  for (const [offset, accepted] of [
    [-20001, false],
    [-20000, true],
    [999, true],
    [1000, false],
    [30000, false],
  ]) {
    const request = legacy();
    const parsed = new URL(request.url);
    parsed.searchParams.delete('signature');
    parsed.searchParams.set('timestamp', String(request.now + offset));
    const payload = [...parsed.searchParams].map(([key, value]) => `${key}=${value}`).join('&');
    parsed.searchParams.set('signature', createHmac('sha256', 'replay-secret').update(payload).digest('hex'));
    const check = () => createAsterRequestValidator()({ ...request, url: parsed.href });
    if (accepted) assert.doesNotThrow(check, `offset ${offset}`);
    else assert.throws(check, /timestamp\/window/, `offset ${offset}`);
  }
});

test('Aster V3 standard request verifies independent ethers vectors and nonce reuse', () => {
  const vector = vectors.find((row) => row.name === 'decimal-spelling-mainnet');
  const fields = {
    ...vector.params,
    recvWindow: '20000',
    timestamp: String(vector.nowMs),
    user: vector.user,
    signer: vector.signer,
    nonce: vector.nonce,
    signature: vector.signature,
  };
  const request = {
    method: 'GET',
    url: `https://fapi.asterdex.com/fapi/v3/account?${new URLSearchParams(fields)}`,
    headers: { 'X-MBX-APIKEY': ASTER_SIGNER },
    now: vector.nowMs,
  };
  const validator = createAsterRequestValidator(options);
  assert.doesNotThrow(() => validator(request));
  assert.throws(() => validator(request), /nonce/);
  for (const [from, to] of [
    ['quantity=0.00010000', 'quantity=0.00010001'],
    [`user=${encodeURIComponent(ASTER_USER)}`, `user=${encodeURIComponent(ASTER_SIGNER)}`],
    ['nonce=' + vector.nonce, 'nonce=0'],
  ]) {
    assert.throws(() => createAsterRequestValidator(options)({ ...request, url: request.url.replace(from, to) }));
  }
});

for (const testnet of [false, true])
  test(`Aster Code exact-query signature and ${testnet ? 'testnet' : 'mainnet'} domain`, () => {
    const vector = vectors.find((row) => row.name === `documented-order-${testnet ? 'testnet' : 'mainnet'}`);
    const origin = testnet ? 'https://fapi.asterdex-testnet.com' : 'https://fapi.asterdex.com';
    const request = {
      method: 'POST',
      url: `${origin}/fapi/v3/order?${vector.codeQuery}&signature=${vector.codeSignature}`,
      headers: { 'X-MBX-APIKEY': ASTER_SIGNER },
      now: vector.nowMs,
    };
    assert.doesNotThrow(() => createAsterRequestValidator(options)(request));
    assert.throws(() =>
      createAsterRequestValidator(options)({ ...request, url: request.url.replace('quantity=20', 'quantity=21') }),
    );
    assert.throws(() =>
      createAsterRequestValidator(options)({
        ...request,
        url: request.url.replace(origin, testnet ? 'https://fapi.asterdex.com' : 'https://fapi.asterdex-testnet.com'),
      }),
    );
    assert.throws(() =>
      createAsterRequestValidator(options)({
        ...request,
        url: request.url.replace('symbol=ASTERUSDT&type=LIMIT', 'type=LIMIT&symbol=ASTERUSDT'),
      }),
    );
  });

test('Aster request identity drops only validated nonce and common transport signing fields', () => {
  const options = { requestUrl: asterRequestUrl };
  const key = (query) =>
    requestKey('POST', `https://fapi.asterdex.com/fapi/v3/order?${query}`, undefined, new Set(), options);
  assert.equal(
    key('symbol=BTCUSDT&quantity=1&user=owner&signer=agent&nonce=1'),
    key('symbol=BTCUSDT&quantity=1&user=owner&signer=agent&nonce=2'),
  );
  for (const query of [
    'symbol=BTCUSDT&quantity=2&user=owner&signer=agent',
    'symbol=BTCUSDT&quantity=1&user=other&signer=agent',
    'symbol=BTCUSDT&quantity=1&user=owner&signer=other',
    'symbol=BTCUSDT&quantity=1&user=owner&signer=agent&feeRate=0.0001',
  ])
    assert.notEqual(key('symbol=BTCUSDT&quantity=1&user=owner&signer=agent'), key(query));
  assert.notEqual(
    asterRequestUrl('https://fapi.asterdex.com/fapi/v1/order?nonce=1'),
    asterRequestUrl('https://fapi.asterdex.com/fapi/v1/order?nonce=2'),
  );
});

test('Aster V3 rejects a different declared signer even when the supplied secret verifies its signature', () => {
  const vector = vectors.find((row) => row.name === 'documented-order-mainnet');
  const payload = { ...vector.params, recvWindow: '20000', timestamp: String(vector.nowMs) };
  const sorted = Object.fromEntries(
    Object.keys(payload)
      .sort()
      .map((key) => [key, String(payload[key])]),
  );
  const encoded = AbiCoder.defaultAbiCoder().encode(
    ['string', 'address', 'address', 'uint256'],
    [JSON.stringify(sorted), ASTER_USER, ASTER_USER, BigInt(vector.nonce)],
  );
  const signature = new SigningKey(ASTER_PRIVATE_KEY).sign(hashMessage(getBytes(keccak256(encoded)))).serialized;
  const fields = { ...payload, user: ASTER_USER, signer: ASTER_USER, nonce: vector.nonce, signature };
  const request = {
    method: 'GET',
    url: `https://fapi.asterdex.com/fapi/v3/account?${new URLSearchParams(fields)}`,
    headers: { 'X-MBX-APIKEY': ASTER_USER },
    now: vector.nowMs,
  };
  assert.throws(
    () => createAsterRequestValidator({ ...options, key: ASTER_USER })(request),
    /recoverable signature mismatch/,
  );
});

test('Aster protocol audit requires aggregate/partial subscriptions and reserved ping identities', () => {
  const audit = asterSocketAudit('rust', { isPrivate: true, books: ['BTCUSDT'] });
  const publicUrl = 'wss://fstream.asterdex.com/stream';
  const privateUrl = 'wss://fstream.asterdex.com/ws/documented-listen-key';
  audit.sent(publicUrl, JSON.stringify({ method: 'LIST_SUBSCRIPTIONS', id: -1 }));
  audit.sent(
    publicUrl,
    JSON.stringify({
      method: 'SUBSCRIBE',
      id: 1,
      params: ['!ticker@arr', '!markPrice@arr@1s', 'btcusdt@depth20@100ms'],
    }),
  );
  assert.equal(audit.ready(), false);
  audit.sent(privateUrl, JSON.stringify({ method: 'LIST_SUBSCRIPTIONS', id: 42 }));
  assert.equal(audit.ready(), true);
  audit.received(publicUrl, JSON.stringify({ stream: 'btcusdt@depth20@100ms', data: {} }));
  assert.deepEqual(audit.errors(), []);
  const bad = asterSocketAudit('rust', { isPrivate: false, books: ['BTCUSDT'] });
  bad.sent(publicUrl, JSON.stringify({ method: 'LIST_SUBSCRIPTIONS', id: 42 }));
  bad.received(publicUrl, JSON.stringify({ stream: 'btcusdt@depth', data: {} }));
  assert(bad.errors().length >= 2);
});
