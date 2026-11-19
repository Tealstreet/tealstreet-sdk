import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { AbiCoder, getBytes, hashMessage, keccak256, SigningKey, TypedDataEncoder } from 'ethers';
import {
  asterRequestUrl,
  asterSocketAudit,
  ASTER_PRIVATE_KEY,
  ASTER_SIGNER,
  ASTER_USER,
  createAsterRequestValidator,
} from './asterdex-requests.mjs';
import { requestKey } from './requests.mjs';
import { buildAsterTape, ASTER_EPOCH, ASTER_READ_PRIVATE_KEY } from './private-tape-asterdex.mjs';
import { createTape } from './tape.mjs';

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

for (const secondary of [false, true]) {
  for (const endpoint of ['account', 'positionSide/dual']) {
    test(`Aster V3 tape matches current ${endpoint} signing${secondary ? ' with a read agent' : ''}`, (t) => {
      t.mock.method(Date, 'now', () => ASTER_EPOCH);
      const records = buildAsterTape({ v3: true, secondary });
      const row = records.find((record) => record.type === 'http' && record.method === 'GET'
        && new URL(record.url).pathname === `/fapi/v3/${endpoint}`);
      const parsed = new URL(row.url);
      parsed.searchParams.set('asterChain', 'Mainnet');
      parsed.searchParams.set('nonce', String(ASTER_EPOCH * 1000));
      const query = parsed.searchParams.toString();
      const digest = TypedDataEncoder.hash(
        { name: 'AsterSignTransaction', version: '1', chainId: 1666, verifyingContract: '0x0000000000000000000000000000000000000000' },
        { Message: [{ name: 'msg', type: 'string' }] }, { msg: query },
      );
      const signature = new SigningKey(secondary ? ASTER_READ_PRIVATE_KEY : ASTER_PRIVATE_KEY).sign(digest).serialized;
      const tape = createTape(records, { requestUrl: asterRequestUrl, createRequestValidator: createAsterRequestValidator },
        true, { credentials: records.find((record) => record.type === 'config').options });
      const response = tape.respond('GET', `${parsed.origin}${parsed.pathname}?${query}&signature=${signature}`,
        undefined, { 'X-MBX-APIKEY': parsed.searchParams.get('signer') });
      assert.equal(response.status, 200);
      assert.equal(response.body, row.body);
      assert.equal(tape.invalidSignatures, 0);
    });
  }
}

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

test('Aster V3 account request verifies independent EIP-712 vectors and nonce reuse', () => {
  const vector = vectors.find((row) => row.name === 'documented-order-mainnet');
  const request = {
    method: 'GET',
    url: `https://fapi.asterdex.com/fapi/v3/account?${vector.codeQuery}&signature=${vector.codeSignature}`,
    headers: { 'X-MBX-APIKEY': ASTER_SIGNER },
    now: vector.nowMs,
  };
  const validator = createAsterRequestValidator(options);
  assert.doesNotThrow(() => validator(request));
  assert.throws(() => validator(request), /nonce/);
  for (const [from, to] of [
    ['quantity=20', 'quantity=21'],
    [`user=${encodeURIComponent(ASTER_USER)}`, `user=${encodeURIComponent(ASTER_SIGNER)}`],
    ['nonce=' + vector.nonce, 'nonce=0'],
  ]) {
    assert.throws(() => createAsterRequestValidator(options)({ ...request, url: request.url.replace(from, to) }));
  }
});

for (const testnet of [false, true]) {
  for (const [method, endpoint] of [
    ['GET', 'account'],
    ['GET', 'positionSide/dual'],
    ['GET', 'positionRisk'],
    ['GET', 'openOrders'],
    ['GET', 'order'],
    ['DELETE', 'order'],
    ['DELETE', 'allOpenOrders'],
    ['POST', 'listenKey'],
    ['PUT', 'listenKey'],
    ['POST', 'leverage'],
    ['POST', 'marginType'],
    ['POST', 'positionSide/dual'],
    ['GET', 'stpMode'],
    ['POST', 'stpMode'],
    ['GET', 'allOrders'],
    ['GET', 'userTrades'],
    ['GET', 'income'],
    ['GET', 'leverageBracket'],
  ]) {
    test(`Aster V3 EIP-712 ${method} ${endpoint} ${testnet ? 'testnet' : 'mainnet'}`, () => {
      const vector = vectors.find((row) => row.name === `empty-${testnet ? 'testnet' : 'mainnet'}`);
      const origin = testnet ? 'https://fapi.asterdex-testnet.com' : 'https://fapi.asterdex.com';
      const request = {
        method,
        url: `${origin}/fapi/v3/${endpoint}?${vector.codeQuery}&signature=${vector.codeSignature}`,
        headers: { 'X-MBX-APIKEY': ASTER_SIGNER },
        now: vector.nowMs,
      };
      const validate = createAsterRequestValidator(options);
      assert.doesNotThrow(() => validate(request));
      assert.throws(() => validate(request), /nonce was reused/);
      for (const [from, to, error] of [
        ['asterChain=' + (testnet ? 'Testnet' : 'Mainnet'), 'asterChain=wrong', /fields\/domain/],
        ['nonce=' + vector.nonce, 'nonce=0', /nonce/],
        ['&signature=', '&timestamp=' + vector.nowMs + '&signature=', /fields\/domain/],
        ['&signature=', '&recvWindow=20000&signature=', /fields\/domain/],
      ]) {
        assert.throws(() => createAsterRequestValidator(options)({
          ...request, url: request.url.replace(from, to),
        }), error);
      }
    });
  }
}

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
  const fields = { ...vector.params, asterChain: 'Mainnet', user: ASTER_USER, signer: ASTER_USER, nonce: vector.nonce };
  const query = Object.entries(fields).map(([key, value]) => `${key}=${value}`).join('&');
  const digest = TypedDataEncoder.hash(
    { name: 'AsterSignTransaction', version: '1', chainId: 1666, verifyingContract: '0x0000000000000000000000000000000000000000' },
    { Message: [{ name: 'msg', type: 'string' }] },
    { msg: query },
  );
  const signature = new SigningKey(ASTER_PRIVATE_KEY).sign(digest).serialized;
  const request = {
    method: 'GET',
    url: `https://fapi.asterdex.com/fapi/v3/account?${query}&signature=${signature}`,
    headers: { 'X-MBX-APIKEY': ASTER_USER },
    now: vector.nowMs,
  };
  assert.throws(
    () => createAsterRequestValidator({ ...options, key: ASTER_USER })(request),
    /recoverable signature mismatch/,
  );
});

test('Aster V3 rejects obsolete EIP-191 requests without consuming the signer nonce', () => {
  const vector = vectors.find((row) => row.name === 'documented-order-mainnet');
  const payload = { ...vector.params, recvWindow: '20000', timestamp: String(vector.nowMs) };
  const sorted = Object.fromEntries(Object.entries(payload).sort(([a], [b]) => a.localeCompare(b)));
  const encoded = AbiCoder.defaultAbiCoder().encode(
    ['string', 'address', 'address', 'uint256'],
    [JSON.stringify(sorted), ASTER_USER, ASTER_SIGNER, BigInt(vector.nonce)],
  );
  const signature = new SigningKey(ASTER_PRIVATE_KEY).sign(hashMessage(getBytes(keccak256(encoded)))).serialized;
  const fields = { ...payload, user: ASTER_USER, signer: ASTER_SIGNER, nonce: vector.nonce, signature };
  const request = {
    method: 'GET', url: `https://fapi.asterdex.com/fapi/v3/account?${new URLSearchParams(fields)}`,
    headers: { 'X-MBX-APIKEY': ASTER_SIGNER }, now: vector.nowMs,
  };
  const validate = createAsterRequestValidator(options);
  assert.throws(() => validate(request), /fields\/domain/);
  assert.doesNotThrow(() => validate({ ...request,
    url: `https://fapi.asterdex.com/fapi/v3/account?${vector.codeQuery}&signature=${vector.codeSignature}`,
  }));
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
