import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash, createPrivateKey, sign } from 'node:crypto';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Tape } from './tape.mjs';
import { ORDERLY_REPLAY, VENUES, venueOf } from './venues.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
const { default: bs58 } = require('bs58');
const privateKey = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'),
  Buffer.from(bs58.decode(ORDERLY_REPLAY.secret.slice('ed25519:'.length)))]), format: 'der', type: 'pkcs8' });
const EPOCH = 1765805280000;
const generator = fileURLToPath(new URL('./private-tape-orderly.mjs', import.meta.url));
const originalHashes = {
  "orderly": [
    "304f45018c6754cdf51fcc61b5b45c68344f91337eb809e7f0d21e76f80b73cf",
    "3f1cba9a44276f72767bd070cfef94c4da2c5600f45d00aed3d2ec5528f762da"
  ],
  "tealstreet-dex": [
    "526c0b117ef5d4b72dee6ca45459b2e1fa9b2c1a65f80fe8927f0a896defa2fb",
    "54797276ee70e1093e7efc8694b4674d5ec00b45b8dff9d6d2ef7ebd369d1d7b"
  ],
  "kodiakfi": [
    "ea06c5c3af055adc8ba6ff099107e6bd9b7dadc79356ecde6a38cf7ae141cd7f",
    "5e9854daf8ee4fddd032a666e9714333c9255975de80b3cc7d10fd2e509738a3"
  ],
  "woofi": [
    "cb4afed27567e6d61e366b2a0ae3c9b9c913828b1ffdc7b2d243c172dc063b27",
    "092de9811f5ee9466c2b178fa6132ab3a8ba4c9013f7d62272ad5e854efc179b"
  ],
  "mode": [
    "b5b32fedb8b488daf757c2e4d03bb815fe8ddfe0931ffb6913428e7d7c7a3824",
    "aa448a77a7c389c763da9650ef3975afd2d216c03adfadf02da15eebb73ab4d0"
  ],
  "salsadex": [
    "ba075afaff5d773083faa9b35ac4775edd3d695e4adb1a186c703b696c0021a1",
    "825088146aafbc97564c0265a347e37b751076e26cf02fecc93957d67a64b310"
  ],
  "what": [
    "eb272f24e6f6fa4584522379e3b78425f8473c01346a6324b7a399fda60229da",
    "2ac7469d552d86750d46322fa869e4840aefff03d931ba1b5b9571031647d8a2"
  ]
};
const generated = new Map();
const fixture = (name, testnet) => {
  const key = `${name}:${testnet}`;
  if (!generated.has(key)) {
    const directory = mkdtempSync(join(tmpdir(), 'orderly-ack-'));
    try {
      const path = join(directory, 'tape.jsonl');
      const result = spawnSync(process.execPath, [generator, '--venue', name, '--out', path, ...(testnet ? ['--testnet'] : [])],
        { encoding: 'utf8', cwd: directory });
      assert.equal(result.status, 0, result.stderr);
      const bytes = readFileSync(path, 'utf8');
      generated.set(key, { bytes, records: bytes.trim().split('\n').map((line) => JSON.parse(line)) });
    } finally { rmSync(directory, { recursive: true, force: true }); }
  }
  return generated.get(key);
};
const headers = (method, url, body = '') => {
  const parsed = new URL(url), timestamp = String(Date.now());
  return { 'orderly-account-id': ORDERLY_REPLAY.applicationId, 'orderly-key': ORDERLY_REPLAY.key,
    'orderly-timestamp': timestamp,
    'orderly-signature': sign(null, Buffer.from(`${timestamp}${method}${parsed.pathname}${parsed.search}${body}`), privateKey).toString('base64url'),
    'Content-Type': ['GET', 'DELETE'].includes(method) ? 'application/x-www-form-urlencoded' : 'application/json' };
};
const send = (tape, method, url, body) => tape.respond(method, url, body, headers(method, url, body));
const write = (tape, record) => send(tape, record.method, record.url, record.tsRequestBody ?? record.requestBody);
const state = (response) => JSON.parse(response.body).data.rows.map((row) => ({
  id: row.order_id ?? row.algo_order_id, price: row.price ?? null,
  children: row.child_orders?.map((child) => child.algo_order_id) ?? [],
}));
const plain = (id, price) => ({ id, price, children: [] });
const algo = (id, ...children) => ({ id, price: null, children });
const snapshots = [
  [[plain(11, 50000), plain(21, 2000)], [algo(100, 102)]],
  [[plain(11, 50000), plain(30, 49900)], [algo(100, 102)]],
  [[plain(11, 50100), plain(30, 49900)], [algo(100, 102)]],
  [[plain(11, 50100), plain(30, 49900)], [algo(200, 201, 202)]],
  [[plain(11, 50100), plain(30, 49900)], [algo(300, 302)]],
  [[plain(11, 50100), plain(30, 49900), plain(40, 0)], [algo(300, 302), algo(400, 401, 402)]],
  [[plain(40, 0)], [algo(400, 401, 402)]],
];
const expected = (stage) => snapshots[Math.min(stage, 6)];

for (const name of Object.keys(originalHashes)) for (const testnet of [false, true]) {
  const label = `${name}/${testnet ? 'testnet' : 'mainnet'}`;
  test(`${label} reads wait for every command ACK despite elapsed timer pumping`, (t) => {
    t.mock.method(Date, 'now', () => EPOCH);
    const { records } = fixture(name, testnet);
    const tape = new Tape(records, venueOf(name), true, 30000, 'ts');
    tape.time = 100000;
    const readUrls = [...new Set(records.filter((record) => record.type === 'http' && record.afterCommand !== undefined &&
      /\/v1\/(?:algo\/)?orders\?status=INCOMPLETE/.test(record.url)).map((record) => record.url))];
    assert.equal(readUrls.length, 4);
    const check = (stage) => {
      for (const url of readUrls) assert.deepEqual(state(send(tape, 'GET', url)), expected(stage)[url.includes('/algo/') ? 1 : 0], url);
    };
    const mutations = records.filter((record) => record.type === 'http' && record.method !== 'GET');
    let cursor = 0;
    for (let command = 1; command <= 14; command++) {
      tape.commandIndex = command;
      tape.settledCommands = command - 1;
      const count = command <= 8 ? (command >= 3 && command <= 6 ? 2 : 1) : 0;
      const writes = mutations.slice(cursor, cursor += count);
      assert.equal(writes.length, count);
      check(writes.length ? command - 1 : command);
      for (const [index, record] of writes.entries()) {
        assert.equal(write(tape, record).status, 200);
        check(index + 1 === writes.length ? command : command - 1);
      }
      tape.settledCommands = command;
      check(command);
    }
    assert.equal(tape.missingWrites, 0);
    assert.equal(tape.invalidSignatures, 0);
    assert.equal(tape.pendingStageWrites.size, 0);
  });
  test(`${label} stripping only new mutation stages preserves original bytes and SHA`, () => {
    const { records } = fixture(name, testnet);
    assert.equal(records.length, 82);
    assert.equal(records.filter((record) => record.type === 'command').length, 14);
    assert.equal(records.filter((record) => record.type === 'http' && record.method !== 'GET').length, 12);
    const stripped = records.map((record) => {
      if (record.type !== 'http' || record.method === 'GET') return record;
      assert(Number.isSafeInteger(record.afterCommand) && record.afterCommand > 0);
      const copy = { ...record }; delete copy.afterCommand; return copy;
    });
    const bytes = stripped.map((record) => JSON.stringify(record)).join('\n') + '\n';
    assert.equal(createHash('sha256').update(bytes).digest('hex'), originalHashes[name][Number(testnet)]);
  });
}

const REST = 'https://api-evm.orderly.org';
const READ = `${REST}/v1/orders?status=INCOMPLETE`;
const readRecords = [{ t: 0, type: 'http', method: 'GET', url: READ, status: 200, body: '{"data":{"rows":[]}}', afterCommand: 0 },
  { t: 0, type: 'http', method: 'GET', url: READ, status: 200, body: '{"data":{"rows":[{"order_id":99}]}}', afterCommand: 1 }];
const acks = [1, 2].map((quantity) => ({ t: 0, type: 'http', method: 'POST', url: `${REST}/v1/order`,
  requestBody: JSON.stringify({ order_quantity: quantity }), status: 200, body: '{}', afterCommand: 1 }));
const ids = (tape) => JSON.parse(send(tape, 'GET', READ).body).data.rows.map((row) => row.order_id);

test('all-ACK policy belongs only to the seven Orderly catalog aliases', () => {
  assert.deepEqual(Object.keys(VENUES).filter((name) => VENUES[name].waitForAllStageWrites).sort(), Object.keys(originalHashes).sort());
});

test('rejected, premature and duplicate attempts do not decrement pending ACKs; HTTP errors do', (t) => {
  t.mock.method(Date, 'now', () => EPOCH);
  const tape = new Tape([...readRecords, { ...acks[0], status: 400, availableAt: 100 }, acks[1]], venueOf('orderly'), true);
  tape.time = 100000;
  assert.equal(write(tape, acks[0]).status, 599);
  assert.equal(tape.pendingStageWrites.get(1), 2);
  tape.commandIndex = 1;
  tape.time = 0;
  assert.equal(write(tape, acks[0]).status, 599);
  assert.equal(tape.pendingStageWrites.get(1), 2);
  const invalid = headers('POST', acks[0].url, acks[0].requestBody);
  assert.throws(() => tape.respond('POST', acks[0].url, acks[0].requestBody, { ...invalid, 'orderly-signature': 'invalid' }), /signature/);
  assert.equal(tape.pendingStageWrites.get(1), 2);
  assert.equal(write(tape, { ...acks[0], requestBody: '{"order_quantity":3}' }).status, 599);
  assert.equal(tape.pendingStageWrites.get(1), 2);
  tape.time = 100000;
  assert.equal(write(tape, acks[0]).status, 400);
  assert.equal(tape.pendingStageWrites.get(1), 1);
  assert.deepEqual(ids(tape), []);
  assert.equal(write(tape, acks[0]).status, 599);
  assert.equal(tape.pendingStageWrites.get(1), 1);
  assert.equal(write(tape, acks[1]).status, 200);
  assert.deepEqual(ids(tape), [99]);
  assert.equal(write(tape, acks[1]).status, 599);
  assert.equal(tape.pendingStageWrites.has(1), false);
});

test('settled-command fallback remains distinct from successful mutation evidence', (t) => {
  t.mock.method(Date, 'now', () => EPOCH);
  const tape = new Tape([...readRecords, ...acks], venueOf('orderly'), true);
  tape.commandIndex = 1;
  assert.deepEqual(ids(tape), []);
  tape.settledCommands = 1;
  assert.deepEqual(ids(tape), [99]);
  assert.equal(tape.pendingStageWrites.get(1), 2);
  assert.equal(tape.missingWrites, 2);
});

test('non-opt-in venues retain first-ACK read release', () => {
  const tape = new Tape([...readRecords, ...acks]);
  tape.commandIndex = 1;
  assert.deepEqual(JSON.parse(tape.respond('GET', READ).body).data.rows, []);
  assert.equal(tape.respond('POST', acks[0].url, acks[0].requestBody).status, 200);
  assert.equal(tape.readStage(), 1);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.pendingStageWrites, undefined);
});

test('pending ACK counts ignore records for the other implementation', () => {
  for (const impl of ['ts', 'rust']) {
    const records = [...readRecords, ...['ts', 'rust'].map((tag) => ({ ...acks[0], impl: tag }))];
    const tape = new Tape(records, { waitForAllStageWrites: true }, false, 30000, impl);
    tape.commandIndex = 1;
    assert.equal(tape.respond('POST', acks[0].url, acks[0].requestBody).status, 200);
    assert.equal(tape.readStage(), 1);
    assert.equal(tape.missingWrites, 0);
  }
});

test('response audit rejection does not release an all-ACK read fence', () => {
  const venue = { waitForAllStageWrites: true, createRequestAudit: () => ({ validate() {},
    response({ method }) { if (method !== 'GET') throw new Error('invalid matched ACK'); } }) };
  const tape = new Tape([...readRecords, acks[0]], venue);
  tape.commandIndex = 1;
  assert.throws(() => tape.respond('POST', acks[0].url, acks[0].requestBody), /invalid matched ACK/);
  assert.equal(tape.pendingStageWrites.get(1), 1);
  assert.equal(tape.readStage(), 0);
});

test('before-ACK socket events remain independent of final-snapshot release', async () => {
  const url = 'wss://replay.example/ws';
  const record = { type: 'ws', t: 0, url, beforeAck: true, afterCommand: 1, text: '{"event":"accepted"}' };
  const tape = new Tape([...readRecords, ...acks, record], { waitForAllStageWrites: true });
  const received = [];
  const socket = tape.connect(url, () => {}, (text) => received.push(JSON.parse(text)), () => {});
  await new Promise(setImmediate);
  tape.commandIndex = 1;
  assert.equal(tape.respond('POST', acks[0].url, acks[0].requestBody).status, 200);
  await Promise.resolve();
  assert.deepEqual(received, [{ event: 'accepted' }]);
  assert.equal(tape.pendingStageWrites.get(1), 1);
  assert.equal(tape.readStage(), 0);
  assert.equal(tape.respond('POST', acks[1].url, acks[1].requestBody).status, 200);
  await Promise.resolve();
  assert.equal(received.length, 1);
  assert.equal(tape.readStage(), 1);
  tape.close(socket);
});
