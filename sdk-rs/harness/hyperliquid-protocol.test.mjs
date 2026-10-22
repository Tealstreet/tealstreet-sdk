import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { createHyperliquidRequestValidator, hyperliquidActionHash, recoverHyperliquidSigner } from './hyperliquid-protocol.mjs';
import { HYPERLIQUID_SPOT_REPLAY as options } from './hyperliquidspot.mjs';
import { signHyperliquidFrame } from './hyperliquid-test-utils.mjs';
import { Tape } from './tape.mjs';
import { venueOf } from './venues.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
const NOW = 1700000000000;
const VAULT = '0x1719884eb866cb12b2287399b15f7db5e7d775ea';
const CLOID = '0x00000000000000000000000000000001';
const row = (c = CLOID) => ({ a: 0, b: true, p: '90', s: '0.25', r: false, t: { limit: { tif: 'Gtc' } }, c });
const order = (...rows) => ({ type: 'order', orders: rows.length ? rows : [row()], grouping: 'na' });
let nextNonce = NOW;
const signed = (action = order(), extra = {}) => signHyperliquidFrame(action, { nonce: nextNonce++, ...extra });
const request = (frame, testnet = false) => ({ method: 'POST', url: `https://api.hyperliquid${testnet ? '-testnet' : ''}.xyz/exchange`, body: JSON.stringify(frame, (_key, value) => typeof value === 'bigint' ? JSON.rawJSON(String(value)) : value), now: NOW });
const validator = (extra = {}, context) => createHyperliquidRequestValidator({ ...options, ...extra }, context);
const info = (type, user) => ({ method: 'POST', url: 'https://api.hyperliquid.xyz/info', body: JSON.stringify({ type, ...(user === undefined ? {} : { user }) }) });

// Published independent vectors, not output from either comparison subject.
for (const [testnet, vaultAddress, r, s, v] of [
  [false, undefined, '53749d5b30552aeb2fca34b530185976545bb22d0b3ce6f62e31be961a59298', '755c40ba9bf05223521753995abb2f73ab3229be8ec921f350cb447e384d8ed8', 27],
  [true, undefined, '542af61ef1f429707e3c76c5293c80d01f74ef853e34b76efffcb57e574f9510', '17b8b32f086e8cdede991f1e2c529f5dd5297cbe8128500e00cbaf766204a613', 28],
  [false, VAULT, '3c548db75e479f8012acf3000ca3a6b05606bc2ec0c29c50c515066a326239', '4d402be7396ce74fbba3795769cda45aec00dc3125a984f2a9f23177b190da2c', 28],
  [true, VAULT, 'e281d2fb5c6e25ca01601f878e4d69c965bb598b88fac58e475dd1f5e56c362b', '7ddad27e9a238d045c035bc606349d075d5c5cd00a6cd1da23ab5c39d4ef0f60', 27],
]) test(`official L1 vector recovers configured signer (testnet=${testnet}, vault=${Boolean(vaultAddress)})`, () => {
  assert.equal(recoverHyperliquidSigner({ action: { type: 'dummy', num: 100000000000 }, nonce: 0,
    vaultAddress, signature: { r: `0x${r}`, s: `0x${s}`, v } }, testnet), options.key);
});

test('official order connection ID preserves msgpack field ordering', () => {
  const action = { type: 'order', orders: [{ a: 4, b: true, p: '1670.1', s: '0.0147', r: false, t: { limit: { tif: 'Ioc' } } }], grouping: 'na' };
  assert.equal(hyperliquidActionHash({ action, nonce: 1677777606040 }), '0x0fcbeda5ae3c4950a548021552a4fea2226858c4453571bf3f24ba017eac2908');
  assert.notEqual(hyperliquidActionHash({ action: { grouping: action.grouping, orders: action.orders, type: 'order' }, nonce: 1677777606040 }), hyperliquidActionHash({ action, nonce: 1677777606040 }));
});

test('valid core, HIP-3 and embedded Spot builder orders authenticate', async () => {
  for (const asset of [0, 110000, 10001]) {
    const action = { ...order({ ...row(), a: asset }), builder: { b: VAULT, f: 10 } };
    const frame = await signed(action);
    validator()(request(frame));
  }
});

test('L1 request allowlist does not admit the low-level dummy vector', async () => {
  const frame = await signed({ type: 'dummy', num: 100000000000 });
  assert.throws(() => validator()(request(frame)), /unsupported L1 action/);
});

test('signature validation rejects tampered action, nonce, signature and domain', async () => {
  const frame = await signed();
  for (const change of [
    { ...frame, nonce: frame.nonce + 1 },
    { ...frame, action: order({ ...row(), p: '91' }) },
    { ...frame, action: { grouping: frame.action.grouping, orders: frame.action.orders, type: 'order' } },
    { ...frame, signature: { ...frame.signature, r: '0x00' } },
  ]) assert.throws(() => validator()(request(change)));
  assert.throws(() => validator({ key: VAULT })(request(frame)), /invalid action signature/);
  const { ethers } = require('ethers');
  const encoded = await new ethers.Wallet(options.secret).signTypedData(
    { name: 'Exchange', version: '1', chainId: 1, verifyingContract: ethers.ZeroAddress },
    { Agent: [{ name: 'source', type: 'string' }, { name: 'connectionId', type: 'bytes32' }] },
    { source: 'a', connectionId: hyperliquidActionHash(frame) });
  const { r, s, v } = ethers.Signature.from(encoded);
  assert.throws(() => validator()(request({ ...frame, signature: { r, s, v } })), /invalid action signature/);
});

test('endpoint/network identity is bound independently from a valid signature', async () => {
  const frame = await signed();
  for (const url of ['http://api.hyperliquid.xyz/exchange', 'https://api.hyperliquid.xyz.evil/exchange',
    'https://api.hyperliquid.xyz/exchange?nonce=1', 'https://user@api.hyperliquid.xyz/exchange', 'https://api.hyperliquid.xyz/other'])
    assert.throws(() => validator()({ ...request(frame), url }), /endpoint\/network/);
  assert.throws(() => validator()({ ...request(frame), method: 'GET' }), /endpoint\/network/);
  assert.throws(() => validator({ testnet: true })(request(frame)), /endpoint\/network/);
  assert.throws(() => validator({ testnet: true })(request(frame, true)), /invalid action signature/);
  validator({ testnet: true })(request(await signed(order(), { testnet: true }), true));
});

test('private info binds account, vault and agent subjects; metadata stays unsigned', () => {
  const verify = validator({ applicationId: VAULT });
  for (const kind of ['metaAndAssetCtxs', 'perpDexs', 'spotMetaAndAssetCtxs']) verify(info(kind));
  for (const kind of ['userFees', 'clearinghouseState', 'frontendOpenOrders', 'spotClearinghouseState', 'userAbstraction', 'maxBuilderFee']) {
    verify(info(kind, VAULT));
    assert.throws(() => verify(info(kind, options.key)), /identity mismatch/);
    assert.throws(() => verify(info(kind)), /identity mismatch/);
  }
  for (const kind of ['userRole']) {
    verify(info(kind, options.key));
    assert.throws(() => verify(info(kind, VAULT)), /identity mismatch/);
  }
  const vaultAccount = validator({ vault: VAULT });
  vaultAccount(info('clearinghouseState', VAULT));
  vaultAccount(info('userFees', VAULT));
  assert.throws(() => vaultAccount(info('userFees', options.key)), /identity mismatch/);
});

test('Tape fee reads bind distinct signer, master and vault subjects independently', () => {
  const vault = `0x${'22'.repeat(20)}`;
  for (const [config, expected] of [[{ ...options, applicationId: VAULT }, VAULT],
    [{ ...options, applicationId: VAULT, vault }, vault]]) {
    const record = { t: 0, type: 'http', method: 'POST', url: info('userFees', expected).url,
      requestBody: info('userFees', expected).body, status: 200, body: '{}' };
    const tape = new Tape([record], venueOf('hyperliquid'), true, 30000, undefined, [], config);
    assert.throws(() => tape.respond(record.method, record.url, info('userFees', options.key).body), /identity mismatch/);
    if (config.vault) assert.throws(() => tape.respond(record.method, record.url, info('userFees', VAULT).body), /identity mismatch/);
    assert.equal(tape.respond(record.method, record.url, record.requestBody).status, 200);
  }
});

test('public anonymous fee probe preserves only the recorded 422 fallback', () => {
  const config = { publicDataOnly: true, key: '', secret: '', applicationId: undefined };
  const record = { t: 0, type: 'http', method: 'POST', url: info('userFees', '').url,
    requestBody: info('userFees', '').body, status: 422, body: 'Failed to deserialize the JSON body into the target type' };
  const tape = new Tape([record], venueOf('hyperliquid'), false, 30000, undefined, [], config);
  assert.equal(tape.respond(record.method, record.url, record.requestBody).status, 422);
  assert.equal(tape.invalidSignatures, 0);
  const success = new Tape([{ ...record, status: 200 }], venueOf('hyperliquid'), false, 30000, undefined, [], config);
  assert.throws(() => success.respond(record.method, record.url, record.requestBody), /recorded rejection/);
  assert.throws(() => validator()(info('userFees', '')), /identity mismatch/);
  assert.throws(() => validator(config)(info('clearinghouseState', '')), /identity mismatch/);
});

test('vault and signed expiry cannot be silently omitted or changed', async () => {
  const withoutExpiry = await signed();
  validator()(request({ ...withoutExpiry, expiresAfter: null }));
  const frame = await signed(order(), { vaultAddress: VAULT, expiresAfter: NOW + 10000 });
  validator({ vault: VAULT })(request(frame));
  assert.throws(() => validator()(request(frame)), /vault identity mismatch/);
  assert.throws(() => validator({ vault: VAULT })(request({ ...frame, expiresAfter: NOW + 10001 })), /invalid action signature/);
  assert.throws(() => validator({ vault: VAULT })(request({ ...frame, expiresAfter: undefined })), /invalid action signature/);
  assert.throws(() => validator({ vault: VAULT })(request({ ...frame, expiresAfter: NOW - 1 })), /expired/);
});

test('nonce validity follows documented open bounds and permits out-of-order values', async () => {
  const frame = await signed();
  for (const nonce of [NOW - 2 * 86400000, NOW + 86400000, -1, 1.5, '1700000000000'])
    assert.throws(() => validator()(request({ ...frame, nonce })), /nonce/);
  for (const nonce of [NOW + 86400000 - 1, NOW - 2 * 86400000 + 1, NOW]) validator()(request(await signed(order(), { nonce })));
});

test('signer nonce history rejects replay and eviction while allowing out-of-order unused values', async () => {
  const verify = validator();
  const action = { type: 'updateLeverage', asset: 0, isCross: true, leverage: 10 };
  for (let i = 1; i <= 100; i++) verify(request(await signed(action, { nonce: NOW + i * 2 })));
  const duplicate = await signed(action, { nonce: NOW + 100 });
  assert.throws(() => verify(request(duplicate)), /signer nonce/);
  verify(request(await signed(action, { nonce: NOW + 3 })));
  for (const nonce of [NOW + 1, NOW + 2]) {
    const evicted = await signed(action, { nonce });
    assert.throws(() => verify(request(evicted)), /signer nonce/);
  }
  const fresh = validator();
  const newest = await signed(action, { nonce: NOW + 1000 });
  fresh(request(newest));
  fresh(request(await signed(action, { nonce: NOW + 900 })));
  assert.throws(() => fresh(request(newest)), /signer nonce/);
});

test('cloid creation checks distinguish duplicate IDs, retries and references', async () => {
  let command = 1;
  const verify = validator({}, { getCommandIndex: () => command });
  const frame = await signed();
  verify(request(frame));
  verify(request(await signed(order(), { nonce: NOW + 1 })));
  for (const action of [{ type: 'cancelByCloid', cancels: [{ asset: 0, cloid: CLOID }] },
    { type: 'modify', oid: CLOID, order: row() }]) {
    const unbound = await signed(action);
    assert.throws(() => verify(request(unbound)), /lacks recorded intent/);
  }
  command++;
  assert.throws(() => verify(request(frame)), /reused creation cloid/);
  const duplicate = await signed(order(row(), row()));
  assert.throws(() => validator()(request(duplicate)), /duplicate creation cloid/);
});

test('malformed generated IDs and rejected signatures do not poison creation state', async () => {
  const verify = validator();
  const frame = await signed();
  assert.throws(() => verify(request({ ...frame, signature: { ...frame.signature, v: 1 } })), /signature/);
  verify(request(frame));
  for (const id of ['0x1', 'not-a-cloid', `0x${'a'.repeat(64)}`]) {
    const malformed = await signed(order(row(id)));
    assert.throws(() => validator()(request(malformed)), /cloid/);
  }
});

test('Tape rejects invalid original wire before normalization or ACK consumption', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  const frame = await signed();
  const records = [{ t: 0, type: 'command', op: 'placeOrders', orders: [{ clientOrderId: CLOID }] },
    { t: 0, type: 'http', method: 'POST', url: request(frame).url, requestBody: JSON.stringify(frame), body: '{}', status: 200, afterCommand: 1 }];
  const tape = new Tape(records, venueOf('hyperliquid'), true, 30000, undefined, [], options);
  tape.commandIndex = 1;
  const bad = { ...frame, signature: { ...frame.signature, r: '0x00' } };
  assert.throws(() => tape.respond('POST', request(frame).url, JSON.stringify(bad)));
  assert.equal(tape.invalidSignatures, 1);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.requests.size, 0);
  assert.equal(tape.receiptRequests.length, 0);
  assert.equal(tape.unacknowledgedStages.has(1), true);
  assert.equal(tape.respond('POST', request(frame).url, JSON.stringify(frame)).status, 200);
  assert.equal(tape.missingWrites, 0);
  assert.equal(tape.unacknowledgedStages.has(1), false);
});

const SECOND = '0x00000000000000000000000000000002';
const THIRD = '0x00000000000000000000000000000003';
const json = (value) => JSON.stringify(value, (_key, item) => typeof item === 'bigint' ? JSON.rawJSON(String(item)) : item);
const ack = (frame, afterCommand, extra = {}) => ({ t: 0, type: 'http', method: 'POST',
  url: request(frame).url, requestBody: json(frame), body: '{}', status: 200, afterCommand, ...extra });
const tapeOf = (records) => new Tape(records, venueOf('hyperliquid'), true, 30000, undefined, [], options);
const respond = (tape, frame) => tape.respond('POST', request(frame).url, json(frame));

test('Tape rejects signed missing creation IDs before the normalized ACK', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  const frame = await signed();
  const tape = tapeOf([ack(frame, 1)]);
  tape.commandIndex = 1;
  const missing = row(); delete missing.c;
  const bad = await signed(order(missing));
  assert.throws(() => respond(tape, bad), /missing creation cloid/);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.receiptRequests.length, 0);
  assert.equal(respond(tape, frame).status, 200);
});

test('Tape rejects signer nonce reuse across different actions without consuming the later ACK', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  const action = { type: 'updateLeverage', asset: 0, isCross: true, leverage: 10 };
  const first = await signed(action);
  const second = await signed({ ...action, leverage: 20 });
  const tape = tapeOf([ack(first, 1), ack(second, 2)]);
  tape.commandIndex = 1;
  assert.equal(respond(tape, first).status, 200);
  tape.commandIndex = 2;
  const reused = await signed(second.action, { nonce: first.nonce });
  assert.throws(() => respond(tape, reused), /signer nonce/);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.unacknowledgedStages.has(2), true);
  const wrongIntent = await signed({ ...action, leverage: 30 }, { nonce: second.nonce });
  assert.throws(() => respond(tape, wrongIntent), /recorded write intent/);
  assert.equal(respond(tape, second).status, 200);
  assert.equal(tape.missingWrites, 0);
});

test('Tape binds generated amend references to acknowledged creations', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  const creation = await signed();
  const modify = await signed({ type: 'modify', oid: 123, order: { ...row(), p: '91' } });
  const batch = await signed({ type: 'batchModify', modifies: [{ oid: 123, order: { ...row(), p: '92' } }] });
  const tape = tapeOf([ack(creation, 1), ack(modify, 2), ack(batch, 3)]);
  tape.commandIndex = 1;
  assert.equal(respond(tape, await signed(order(row(SECOND)))).status, 200);
  for (const [index, expected] of [[2, modify], [3, batch]]) {
    tape.commandIndex = index;
    const replace = (c) => expected.action.type === 'modify'
        ? { ...expected.action, order: { ...expected.action.order, c } }
        : { ...expected.action, modifies: [{ oid: 123, order: { ...expected.action.modifies[0].order, c } }] };
    const missing = tape.missingWrites;
    for (const c of [CLOID, THIRD, undefined]) {
      const bad = await signed(JSON.parse(JSON.stringify(replace(c))));
      assert.throws(() => respond(tape, bad), /cloid reference/);
      assert.equal(tape.missingWrites, missing);
      assert.equal(tape.unacknowledgedStages.has(index), true);
    }
    assert.equal(respond(tape, await signed(replace(SECOND))).status, 200);
    assert.equal(tape.missingWrites, missing - 1);
    assert.equal(tape.unacknowledgedStages.has(index), false);
  }
});

test('cancelByCloid uses documented asset/cloid fields with exact recorded references', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  const frame = await signed({ type: 'cancelByCloid', cancels: [{ asset: 0, cloid: CLOID }] });
  const tape = tapeOf([ack(frame, 1)]);
  tape.commandIndex = 1;
  const invented = await signed({ type: 'cancelByCloid', cancels: [{ a: 0, c: CLOID }] });
  assert.throws(() => respond(tape, invented), /cancel asset/);
  const wrong = await signed({ type: 'cancelByCloid', cancels: [{ asset: 0, cloid: SECOND }] });
  assert.throws(() => respond(tape, wrong), /recorded write intent/);
  assert.equal(tape.missingWrites, 1);
  assert.equal(respond(tape, frame).status, 200);
});

test('Tape rejects adding an unrecorded amend cloid and preserves caller ownership', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  const amended = row(); delete amended.c;
  const frame = await signed({ type: 'modify', oid: 123, order: amended });
  const tape = tapeOf([ack(frame, 1)]);
  tape.commandIndex = 1;
  const added = await signed({ ...frame.action, order: row(SECOND) });
  assert.throws(() => respond(tape, added), /presence mismatch/);
  assert.equal(tape.missingWrites, 1);
  assert.equal(respond(tape, frame).status, 200);

  const creation = await signed();
  const owned = tapeOf([{ t: 0, type: 'command', op: 'placeOrders', orders: [{ clientOrderId: CLOID }] }, ack(creation, 1)]);
  owned.commandIndex = 1;
  const changed = await signed(order(row(SECOND)));
  assert.throws(() => respond(owned, changed), /intent|ownership/);
  assert.equal(owned.missingWrites, 1);
  assert.equal(respond(owned, creation).status, 200);
});

test('premature signed writes cannot poison later generated ID bindings', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  for (const field of ['availableAt', 'effectiveAt', 'availableAfter']) {
    const creation = await signed();
    const modify = await signed({ type: 'modify', oid: 123, order: row() });
    const tape = tapeOf([ack(creation, 2, { [field]: 100 }), ack(modify, 3)]);
    tape.commandIndex = 1;
    const early = await signed(order(row(THIRD)));
    assert.throws(() => respond(tape, early), /recorded write intent/);
    tape.commandIndex = 2;
    assert.equal(respond(tape, early).status, 599);
    assert.equal(tape.missingWrites, 2);
    assert.equal(tape.unacknowledgedStages.has(2), true);
    tape.time = 100;
    assert.equal(respond(tape, await signed(order(row(SECOND)), { nonce: early.nonce })).status, 200);
    tape.commandIndex = 3;
    const good = await signed({ type: 'modify', oid: 123, order: row(SECOND) });
    assert.equal(respond(tape, good).status, 200);
    assert.equal(tape.missingWrites, 0);
  }
});

test('uint64 order IDs authenticate losslessly and stay exact in tape matching', async (t) => {
  t.mock.method(Date, 'now', () => NOW);
  const frame = await signed({ type: 'cancel', cancels: [{ a: 0, o: 18446744073709551615n }] });
  validator()(request(frame));
  const tape = tapeOf([ack(frame, 1)]);
  tape.commandIndex = 1;
  const wrong = await signed({ type: 'cancel', cancels: [{ a: 0, o: 18446744073709551614n }] });
  assert.throws(() => respond(tape, wrong), /recorded write intent/);
  const tampered = { ...frame, action: wrong.action };
  assert.throws(() => respond(tape, tampered), /signature/);
  assert.equal(tape.missingWrites, 1);
  assert.equal(respond(tape, frame).status, 200);
});

test('public metadata validation does not load signing crypto', () => {
  const module = new URL('./hyperliquid-protocol.mjs', import.meta.url).href;
  const code = `import assert from 'node:assert/strict'; import {createRequire} from 'node:module';
    const require=createRequire(${JSON.stringify(new URL('../../packages/safe-cex/package.json', import.meta.url).href)});
    const path=require.resolve('ethers'); assert.equal(require.cache[path],undefined);
    const {createHyperliquidRequestValidator}=await import(${JSON.stringify(module)});
    createHyperliquidRequestValidator({})({method:'POST',url:'https://api.hyperliquid.xyz/info',body:'{"type":"metaAndAssetCtxs"}'});
    assert.equal(require.cache[path],undefined);`;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
});
