// Independent oracle evidence from https://docs.txflow-testnet.xyz/api/signing.md
// and the two documented websocket control planes. No adapter signer is imported.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';
import { performance } from 'node:perf_hooks';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createTxFlowRequestValidator, initializeTxFlowCrypto, validateTxFlowSocketMessage } from './txflow-protocol.mjs';
import { ReplayClock, Tape } from './tape.mjs';
import { VENUES } from './venues.mjs';

const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
const ethers = require('ethers');
const { encode } = require('@msgpack/msgpack');
const EPOCH = 1_700_000_000_000;
const ORIGIN = 'https://rpc.txflow-testnet.xyz';
const SECRET = `0x${'ab'.repeat(32)}`; // Official, explicitly disposable vector key.
const OFFICIAL_SIGNER = '0xe239cdc5fbe977a8a141b72194d3cf8c41bc5bc6';
const ACCOUNT = '0x0000000000000000000000000000000000000001';
const VAULT = `0x${'12'.repeat(20)}`;
const OPTIONS = { applicationId: ACCOUNT, secret: SECRET, testnet: true };
const DOMAIN = { name: 'TxFlow-Testnet', version: '1', chainId: 2511, verifyingContract: ethers.ZeroAddress };
const AGENT = { Agent: [{ name: 'txflowNetwork', type: 'string' }, { name: 'chainId', type: 'uint32' }, { name: 'apiVersion', type: 'uint32' }, { name: 'connectionId', type: 'bytes32' }] };
const VECTOR_SIGNATURE = { r: '0x4064848dd881985c52dacdc00f7046f1010f7f370e34620d36fcf3b2d1ecb3e0', s: '0x25c45ad67b9506ac653fc3658f47aa8b6002bfa86a31ae619b334d71aa8b4f6f', v: 27 };
const native = (cloid) => ({ grouping: 'na', orders: [{ a: 1, b: true, p: '50000.0', s: '0.01', r: false, t: { limit: { tif: 'Gtc' } }, ...(cloid === undefined ? {} : { c: cloid }) }] });
const request = (body) => ({ method: 'POST', url: `${ORIGIN}/exchange`, headers: { 'Content-Type': 'application/json', 'x-api-version': '2026-08-16' }, body });
const freeze = (run) => {
  const original = Date.now;
  Date.now = () => EPOCH;
  try { return run(); } finally { Date.now = original; }
};
function independentlySigned(body = native('test-order'), { nonce = EPOCH, vault, secret = SECRET, domain = DOMAIN, preimage = body } = {}) {
  const suffix = Buffer.alloc(vault === undefined ? 9 : 29);
  suffix.writeBigUInt64BE(BigInt(nonce));
  if (vault !== undefined) { suffix[8] = 1; suffix.set(ethers.getBytes(vault), 9); }
  const connectionId = ethers.keccak256(Buffer.concat([encode(preimage), suffix]));
  const digest = ethers.TypedDataEncoder.hash(domain, AGENT, { txflowNetwork: 'TxFlow-Testnet', chainId: 2511, apiVersion: 1, connectionId });
  const signature = new ethers.SigningKey(secret).sign(digest);
  return { body: { action: { type: 'order', ...body }, nonce, signature: { r: signature.r, s: signature.s, v: signature.v }, confirm: 'executed', ...(vault === undefined ? {} : { vaultAddress: vault }) }, connectionId, digest };
}
test.before(initializeTxFlowCrypto);

test('published raw order vector independently fixes MessagePack bytes, connection ID, digest and signature', () => freeze(() => {
  const body = native();
  assert.equal(Buffer.from(encode(body)).toString('hex'), '82a867726f7570696e67a26e61a66f72646572739186a16101a162c3a170a735303030302e30a173a4302e3031a172c2a17481a56c696d697481a3746966a3477463');
  const vector = independentlySigned(body);
  assert.equal(vector.connectionId, '0x6546d1c3acf430a28a0b2552c66e985f6543646660874734732ae9e9daf4d806');
  assert.equal(vector.digest, '0xeae1ae63709636fb9567accd33bc5dd2717bfac4d97e9a853f91c7fb391e1cc6');
  assert.deepEqual(vector.body.signature, VECTOR_SIGNATURE);
  assert.equal(ethers.recoverAddress(vector.digest, VECTOR_SIGNATURE).toLowerCase(), OFFICIAL_SIGNER);
  assert.doesNotThrow(() => createTxFlowRequestValidator(OPTIONS)(request(vector.body)));
}));

test('profile constructs a separate verifier for each TS/Rust implementation and honors exact cloids', () => freeze(() => {
  assert.equal(VENUES.txflow.createRequestValidator, createTxFlowRequestValidator);
  assert.equal(VENUES.txflow.virtualPrivateClock, true);
  assert.equal(VENUES.txflow.compareFillPrices, true);
  assert.equal(VENUES.txflow.compareWriteOrder, true);
  assert.equal(VENUES.txflow.compareClientIds, true, 'stored caller IDs and limit time-in-force must reach parity comparison');
  assert.equal(VENUES.txflow.independentLeverage, true, 'both configured leverage rails must reach parity comparison');
  assert.notEqual(VENUES.txflow.replayCredentials.applicationId.toLowerCase(), new ethers.Wallet(VENUES.txflow.replayCredentials.secret).address.toLowerCase(), 'the replay must distinguish the main account from its approved agent');
  const signed = independentlySigned(native('caller-Ä'));
  for (const impl of ['ts', 'rust']) {
    const tape = new Tape([{ type: 'command', op: 'placeOrders', orders: [{ clientOrderId: 'caller-Ä' }] }, { type: 'http', method: 'POST', url: `${ORIGIN}/exchange`, requestBody: JSON.stringify(signed.body), status: 200, body: '{}' }], VENUES.txflow, true, 100, impl, OPTIONS);
    assert.equal(tape.respond('POST', `${ORIGIN}/exchange`, signed.body, request().headers).status, 200);
    assert.equal(tape.missingWrites, 0);
    tape.assertWritesConsumed();
  }
}));

test('preimage field order, inclusion of action type and decimal spelling are independently significant', () => freeze(() => {
  const body = native('ordered');
  const row = body.orders[0];
  const wrongPreimages = [
    { orders: body.orders, grouping: body.grouping },
    { grouping: body.grouping, orders: [{ b: row.b, a: row.a, p: row.p, s: row.s, r: row.r, t: row.t, c: row.c }] },
    { type: 'order', ...body },
    { ...body, orders: [{ ...row, p: '50000' }] },
  ];
  for (const preimage of wrongPreimages) {
    const bad = independentlySigned(body, { preimage });
    assert.throws(() => createTxFlowRequestValidator(OPTIONS)(request(bad.body)), /signature/);
  }
}));

test('mutating fee, native asset type, signed nonce, extra keys or null optional fields cannot pass', () => freeze(() => {
  const original = independentlySigned().body;
  const mutation = (change) => { const body = structuredClone(original); change(body); return body; };
  for (const body of [
    mutation((e) => { e.action.builder = { b: ACCOUNT, f: 2 }; }),
    mutation((e) => { e.action.orders[0].a = '1'; }),
    mutation((e) => { e.nonce += 1; }),
    mutation((e) => { e.action.unrecognized = true; }),
    mutation((e) => { e.action.orders[0].expiresAt = null; }),
    mutation((e) => { e.action.orders[0].c = ''; }),
  ]) assert.throws(() => createTxFlowRequestValidator(OPTIONS)(request(body)));
}));

test('wrong signing domain, signer, repeated nonce and consensus-only confirmation are rejected', () => freeze(() => {
  for (const config of [
    { domain: { ...DOMAIN, name: 'Exchange', chainId: 1337 } },
    { secret: `0x${'cd'.repeat(32)}` },
    { nonce: EPOCH + 60_001 },
  ]) assert.throws(() => createTxFlowRequestValidator(OPTIONS)(request(independentlySigned(undefined, config).body)));
  const validate = createTxFlowRequestValidator(OPTIONS);
  const signed = independentlySigned().body;
  validate(request(signed));
  assert.throws(() => validate(request(signed)), /nonce/);
  const acceptedOnly = { ...signed, confirm: 'accepted' };
  assert.throws(() => createTxFlowRequestValidator(OPTIONS)(request(acceptedOnly)), /confirmation/);
}));

test('vault presence and bytes are bound to the configured account and the signature', () => freeze(() => {
  const options = { ...OPTIONS, vault: VAULT };
  const signed = independentlySigned(undefined, { vault: VAULT }).body;
  assert.doesNotThrow(() => createTxFlowRequestValidator(options)(request(signed)));
  assert.throws(() => createTxFlowRequestValidator(options)(request(independentlySigned().body)), /vault/i);
  assert.throws(() => createTxFlowRequestValidator(OPTIONS)(request(signed)), /vault/i);
  const changed = { ...signed, vaultAddress: `0x${'34'.repeat(20)}` };
  assert.throws(() => createTxFlowRequestValidator(options)(request(changed)), /vault|signature/i);
}));

test('private reads require the main account or configured vault while public metadata remains unsigned', () => freeze(() => {
  const read = (body) => ({ ...request(body), url: `${ORIGIN}/info` });
  const validate = createTxFlowRequestValidator(OPTIONS);
  assert.doesNotThrow(() => validate(read({ type: 'perpMeta' })));
  assert.doesNotThrow(() => validate(read({ type: 'clearinghouseState', user: ACCOUNT })));
  assert.throws(() => validate(read({ type: 'clearinghouseState', user: `0x${'34'.repeat(20)}` })), /account|subject|user/i);
  assert.throws(() => validate(read({ type: 'frontendOpenOrders' })), /account|subject|user/i);
  assert.throws(() => validate(read({ type: 'clearinghouseState', user: OFFICIAL_SIGNER })), /account|subject|user/i);
  const vaulted = createTxFlowRequestValidator({ ...OPTIONS, vault: VAULT });
  assert.doesNotThrow(() => vaulted(read({ type: 'activeAssetData', user: VAULT, coin: '1' })));
  assert.throws(() => vaulted(read({ type: 'userFills', user: ACCOUNT })), /account|subject|user/i);
}));

test('both request families reject missing version, non-testnet origins and unrelated paths', () => freeze(() => {
  for (const path of ['/info', '/exchange']) {
    const req = { ...request(independentlySigned().body), url: `${ORIGIN}${path}`, headers: {} };
    assert.throws(() => createTxFlowRequestValidator(OPTIONS)(req), /version/);
  }
  const req = request(independentlySigned().body);
  assert.throws(() => createTxFlowRequestValidator(OPTIONS)({ ...req, url: 'https://rpc.txflow.com/exchange' }), /testnet/);
  assert.throws(() => createTxFlowRequestValidator(OPTIONS)({ ...req, url: `${ORIGIN}/account` }));
}));

test('both socket control planes require a subscription channel and exact placement of node type', () => {
  const node = { url: 'wss://rpc.txflow-testnet.xyz/ws' };
  const api = { url: 'wss://rpc.txflow-testnet.xyz/api-ws' };
  for (const method of ['subscribe', 'unsubscribe']) {
    assert.doesNotThrow(() => validateTxFlowSocketMessage(node, JSON.stringify({ method, type: 'orderUpdates', subscription: { type: 'orderUpdates', user: ACCOUNT } })));
    assert.doesNotThrow(() => validateTxFlowSocketMessage(api, JSON.stringify({ method, subscription: { type: 'candle', coin: '1', interval: '1m' } })));
    assert.throws(() => validateTxFlowSocketMessage(node, JSON.stringify({ method, subscription: { type: 'orderUpdates' } })));
    assert.throws(() => validateTxFlowSocketMessage(api, JSON.stringify({ method, type: 'candle', subscription: { type: 'candle' } })));
    for (const socket of [node, api]) {
      assert.throws(() => validateTxFlowSocketMessage(socket, JSON.stringify({ method })));
      assert.throws(() => validateTxFlowSocketMessage(socket, JSON.stringify({ method, subscription: { type: '' } })));
    }
  }
  for (const socket of [node, api]) assert.doesNotThrow(() => validateTxFlowSocketMessage(socket, '{"method":"ping"}'));
  assert.throws(() => validateTxFlowSocketMessage({ url: 'wss://rpc.txflow.com/ws' }, '{"method":"ping"}'));
});

test('private replay advances logical startup deadlines then rebases pacing and 10-second pings without wall waits', { timeout: 5000 }, async () => {
  const clock = new ReplayClock(EPOCH, true);
  const tape = new Tape([{ type: 'command', t: 0, op: 'pacedWrite' }, { type: 'command', t: 10_000, op: 'afterPings' }], { virtualPrivateClock: true }, true, 2000);
  clock.install();
  const wall = performance.now();
  const timestamps = [];
  const pings = [];
  try {
    assert.equal(Date.now(), EPOCH);
    await new Promise((resolve) => setTimeout(resolve, 1));
    assert.equal(Date.now(), EPOCH + 1);
    const interval = setInterval(() => pings.push(Date.now()), 10_000);
    const frames = await tape.play(undefined, async () => {}, async (frame) => {
      timestamps.push(Date.now());
      if (frame.op === 'pacedWrite') await new Promise((resolve) => setTimeout(resolve, 110));
      timestamps.push(Date.now());
      return [];
    }, clock);
    clearInterval(interval);
    assert.equal(frames, 2);
    assert.deepEqual(timestamps, [EPOCH, EPOCH + 110, EPOCH + 10_000, EPOCH + 10_000]);
    assert.deepEqual(pings, [EPOCH + 10_000]);
    assert(performance.now() - wall < 5000, 'a 10-second private tape must use virtual scheduling');
  } finally { clock.restore(); }
});

test('generated cloids stay distinct before normalization, including correctly signed reuse', () => freeze(() => {
  const body = native('ts_generated-duplicate');
  const batch = { ...body, orders: [body.orders[0], { ...body.orders[0], p: '50001.0' }] };
  assert.throws(() => createTxFlowRequestValidator(OPTIONS)(request(independentlySigned(batch).body)), /cloid|client|reuse|duplicate/i);
  assert.throws(() => createTxFlowRequestValidator(OPTIONS)(request(independentlySigned(native('')).body)), /cloid/i);
  const validate = createTxFlowRequestValidator(OPTIONS);
  validate(request(independentlySigned(body).body));
  assert.throws(() => validate(request(independentlySigned(body, { nonce: EPOCH + 1 }).body)), /cloid|client|reuse|duplicate/i);
}));

test('connector replay rejects a freshly signed missing generated cloid before normalization can hide it', () => freeze(() => {
  const expected = independentlySigned(native('ts_fixture-generated')).body;
  const missing = independentlySigned(native()).body;
  // The native protocol permits omission; the connector contract always mints
  // an identity so it can resolve execution acknowledgements authoritatively.
  assert.doesNotThrow(() => createTxFlowRequestValidator(OPTIONS)(request(missing)));
  for (const impl of ['ts', 'rust']) {
    const tape = new Tape([
      { type: 'command', op: 'placeOrders', orders: [{ symbol: 'BTC-USDC', type: 'limit' }] },
      { type: 'http', method: 'POST', url: `${ORIGIN}/exchange`, requestBody: JSON.stringify(expected), status: 200, body: '{}' },
    ], VENUES.txflow, true, 100, impl, [], OPTIONS);
    assert.throws(() => tape.respond('POST', `${ORIGIN}/exchange`, missing, request().headers), /cloid|client|creation/i);
    assert.equal(tape.invalidSignatures, 1);
    assert.equal(tape.respond('POST', `${ORIGIN}/exchange`, expected, request().headers).status, 200);
    assert.throws(() => tape.assertWritesConsumed(), /invalid replay signature/i);
  }
}));

test('generated private tape addresses initial sockets and the explicit reconnect without watchdog help', { timeout: 10_000 }, async () => {
  const directory = mkdtempSync(join(tmpdir(), 'txflow-lifecycle-'));
  const output = join(directory, 'private.jsonl');
  try {
    const generated = spawnSync('corepack', ['yarn', '-s', 'rust:private-tape-txflow', '--out', output], {
      cwd: fileURLToPath(new URL('../../', import.meta.url)), encoding: 'utf8', timeout: 5000,
    });
    assert.equal(generated.status, 0, generated.stderr || generated.error?.message);
    const records = readFileSync(output, 'utf8').trim().split('\n').map((line) => JSON.parse(line));
    const reads = records.filter(record => record.type === 'http' && new URL(record.url).pathname === '/info')
      .map(record => JSON.parse(record.requestBody));
    const initialTickerCount = reads.filter(body => body.type === 'activeAssetCtx').length;
    assert.equal(initialTickerCount, 2);
    const accountFills = reads.filter(body => body.type === 'userFills' && body.limit === 500);
    assert.ok(accountFills.length > 0);
    for (const body of accountFills) assert.equal(body.startTimeMs, EPOCH + initialTickerCount * 25);
    const queryFills = reads.filter(body => body.type === 'userFills' && body.limit === 2000);
    assert.ok(queryFills.length > 0);
    for (const body of queryFills) assert.equal(body.startTimeMs, EPOCH);
    const closeIndex = records.findIndex((record) => record.type === 'close');
    assert(closeIndex >= 0, 'the fixture must exercise a disconnect');
    const closed = records[closeIndex];
    assert.equal(closed.socket, 0);
    for (const record of records.slice(0, closeIndex).filter((record) => record.type === 'ws')) assert.equal(record.socket, 0);
    const reconnected = records.slice(closeIndex + 1).filter((record) => record.type === 'ws' && record.url === closed.url);
    assert(reconnected.length > 0, 'the fixture must deliver native state after reconnect');
    for (const record of reconnected) assert.equal(record.socket, 1);
    for (const record of records.slice(closeIndex + 1).filter((record) => record.type === 'ws' && record.url !== closed.url)) assert.equal(record.socket, 0);

    const first = records.find((record) => record.type === 'ws' && record.url === closed.url);
    const tape = new Tape([{ ...first, t: 0 }, { ...closed, t: 1 }, { ...reconnected[0], t: 2 }], VENUES.txflow, true, 100);
    const clock = new ReplayClock(EPOCH, true);
    const received = [];
    let connections = 0;
    const connect = () => {
      connections++;
      tape.connect(closed.url, () => {}, (text) => received.push(text), () => connect());
    };
    clock.install();
    try {
      connect();
      assert.equal(await tape.play(undefined, undefined, undefined, clock), 3);
      assert.equal(connections, 2, 'only the fixture close may cause a reconnect');
      assert.deepEqual(received, [first.text, reconnected[0].text]);
      assert.equal(Date.now(), EPOCH + 2, 'no watchdog deadline may be needed to find a fixture socket');
    } finally { clock.restore(); }
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('request validation errors remain fatal even if transport catches one and later serves the valid write', () => freeze(() => {
  const signed = independentlySigned(native('ts_sticky-signature')).body;
  const tape = new Tape([{ type: 'http', method: 'POST', url: `${ORIGIN}/exchange`, requestBody: JSON.stringify(signed), status: 200, body: '{}' }], VENUES.txflow, true, 100, 'rust', OPTIONS);
  const altered = structuredClone(signed);
  altered.action.orders[0].p = '123';
  assert.throws(() => tape.respond('POST', `${ORIGIN}/exchange`, altered, request().headers), /signature/i);
  assert.equal(tape.respond('POST', `${ORIGIN}/exchange`, signed, request().headers).status, 200);
  assert.equal(tape.invalidSignatures, 1);
  assert.throws(() => tape.assertWritesConsumed(), /invalid replay signature/i);
}));

test('read and write Content-Type must identify JSON regardless of header casing', () => freeze(() => {
  for (const path of ['/info', '/exchange']) {
    const signed = independentlySigned().body;
    const req = { ...request(path === '/info' ? { type: 'perpMeta' } : signed), url: `${ORIGIN}${path}` };
    assert.doesNotThrow(() => createTxFlowRequestValidator(OPTIONS)({ ...req, headers: { 'X-API-VERSION': '2026-08-16', 'CONTENT-TYPE': 'application/json; charset=utf-8' } }));
    assert.throws(() => createTxFlowRequestValidator(OPTIONS)({ ...req, headers: { 'x-api-version': '2026-08-16' } }), /content|json/i);
    assert.throws(() => createTxFlowRequestValidator(OPTIONS)({ ...req, headers: { ...req.headers, 'Content-Type': 'text/plain' } }), /content|json/i);
  }
}));
