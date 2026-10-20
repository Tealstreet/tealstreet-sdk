import assert from 'node:assert/strict';
import test from 'node:test';
import { Tape } from './tape.mjs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const forbidden = () => { throw new Error('audit observer ran in performance mode'); };

test('performance Tape excludes verifier factories and observer callbacks while keeping responses, topics and auth replies', async () => {
  const url = 'https://example.test/seed';
  const socketUrl = 'wss://example.test/private';
  const venue = {
    createRequestValidator: forbidden, createRequestAudit: forbidden, createSocketAudit: forbidden,
    validateRequest: forbidden, checkRequest: forbidden, verifyRequest: forbidden,
    verifySubscribe: forbidden, validateSocketMessage: forbidden, validateSocketFrame: forbidden,
    validateMatchedRequest: forbidden, validateLogin: true, validateSubscriptions: true,
    socketReplies: (_url, message) => message?.op === 'login' ? ['{"event":"login","code":"0"}'] : [],
    pingReply: (text) => text === 'ping' ? 'pong' : undefined,
    onSocketSend: (socket) => { socket.role = 'private'; },
  };
  const tape = new Tape([{ type: 'http', method: 'GET', url, status: 200, body: '{}' }], venue, true, 30000, 'ts', [], {}, { performance: true });
  assert.equal(tape.auditEnabled, false);
  assert.equal(tape.respond('GET', url).status, 200);
  const received = [];
  const socket = tape.connect(socketUrl, () => {}, (text) => received.push(text), () => {});
  await new Promise(setImmediate);
  tape.send(socket, '{"op":"login","args":[{}]}');
  tape.send(socket, '{"op":"subscribe","args":[{"channel":"book","instId":"BTCUSDT"}]}');
  tape.send(socket, 'ping');
  await new Promise(setImmediate);
  assert.equal(socket.loginVerified, true);
  assert.equal(socket.role, 'private');
  assert.equal(socket.topics.size, 1);
  assert.deepEqual(received, ['{"event":"login","code":"0"}', 'pong']);
  assert.equal(tape.requests.size, 0);
  assert.deepEqual(tape.writes, []);
  assert.equal(tape.subscriptions.size, 0);
  assert.equal(tape.sentRequests.size, 0);
  tape.assertWritesConsumed();
  tape.close(socket);
});

test('audited Tape continues to construct and invoke the same verifiers', () => {
  assert.throws(() => new Tape([], { createRequestValidator: forbidden }), /audit observer/);
  const url = 'https://example.test/seed';
  const tape = new Tape([{ type: 'http', method: 'GET', url, status: 200, body: '{}' }], { validateRequest: forbidden });
  assert.throws(() => tape.respond('GET', url), /audit observer/);
  assert.equal(tape.invalidSignatures, 1);
  assert.throws(() => tape.assertWritesConsumed(), /invalid replay signature/);
});

test('performance preserves generated Bitget client-ID binding for subsequent HTTP matching', () => {
  const place = 'https://example.test/api/v2/mix/order/place-order';
  const cancel = 'https://example.test/api/v2/mix/order/cancel-order';
  const tape = new Tape([
    { type: 'http', method: 'POST', url: place, status: 200, body: '{"code":"00000"}', requestBody: '{"clientOid":"recorded-generated"}' },
    { type: 'http', method: 'POST', url: cancel, status: 200, body: '{"code":"00000"}', requestBody: '{"clientOid":"recorded-generated"}' },
  ], { rust: 'bitget' }, true, 30000, 'rust', [], {}, { performance: true });
  assert.equal(tape.respond('POST', place, '{"clientOid":"runtime-generated"}').status, 200);
  assert.equal(tape.generatedAliases.get('runtime-generated'), 'recorded-generated');
  assert.equal(tape.respond('POST', cancel, '{"clientOid":"runtime-generated"}').status, 200);
  assert.equal(tape.unserved, 0);
  tape.assertWritesConsumed();
});

test('performance retains recorded socket-request correlation without audit history', async () => {
  const url = 'wss://example.test/query';
  const venue = {
    socketRequest: (message) => message.method === 'query' ? { id: message.id, key: 'BTCUSDT' } : undefined,
    socketResponse: (message) => message.result ? { id: message.id, key: 'BTCUSDT' } : undefined,
  };
  const tape = new Tape([
    { type: 'ws-send', url, socket: 0, text: '{"method":"query","id":100}' },
  ], venue, false, 30000, 'rust', [], {}, { performance: true });
  const received = [];
  const socket = tape.connect(url, () => {}, (text) => received.push(JSON.parse(text)), () => {});
  await new Promise(setImmediate);
  tape.send(socket, '{"method":"query","id":999}');
  tape.deliver(socket, '{"result":true,"id":100}');
  assert.deepEqual(received, [{ result: true, id: 999 }]);
  assert.equal(socket.requests.size, 0);
  assert.equal(socket.responseIds.size, 0);
  assert.equal(tape.sentRequests.size, 0);
  tape.close(socket);
});

test('performance still rejects unmatched requests and unconsumed write workloads', () => {
  const tape = new Tape([
    { type: 'http', method: 'POST', url: 'https://example.test/order', status: 200, body: '{}', requestBody: '{}' },
  ], {}, false, 30000, 'rust', [], {}, { performance: true });
  assert.equal(tape.respond('GET', 'https://example.test/missing').status, 599);
  assert.equal(tape.unserved, 1);
  assert.throws(() => tape.assertWritesConsumed(), /taped write/);
});

test('audited and performance transport receipts match actual requests, mutations and socket generations', async () => {
  const url = 'https://example.test/seed';
  const order = 'https://example.test/order';
  const socketUrl = 'wss://example.test/public';
  const receipts = [];
  for (const performance of [false, true]) {
    const tape = new Tape([
      { type: 'http', method: 'GET', url, status: 200, body: '{}' },
      { type: 'http', method: 'POST', url: order, status: 200, body: '{}', requestBody: '{"amount":1}' },
    ], {}, false, 30000, 'rust', [], {}, { performance });
    tape.respond('GET', url);
    tape.respond('GET', url);
    tape.respond('POST', order, '{"amount":1}');
    const socket = tape.connect(socketUrl, () => {}, () => {}, () => {});
    await new Promise(setImmediate);
    tape.send(socket, JSON.stringify({ op: 'subscribe', id: performance ? 999 : 111, args: ['book.BTCUSDT'] }));
    tape.close(socket);
    receipts.push(tape.transportReceipt());
  }
  assert.deepEqual(receipts[0], receipts[1]);
  assert.equal(receipts[0].requests.find(([key]) => key.startsWith('GET '))[1], 2);
  assert.equal(receipts[0].writes.length, 1);
  assert.equal(receipts[0].subscriptions.length, 1);
  assert.deepEqual(receipts[0].connections, [{ url: `${socketUrl}?`, ordinal: 0, closed: true }]);
});

test('transport receipt detects repeated reads and changed subscription targets', async () => {
  const url = 'https://example.test/seed';
  const tape = new Tape([{ type: 'http', method: 'GET', url, status: 200, body: '{}' }], {}, false, 30000, 'rust', [], {}, { performance: true });
  tape.respond('GET', url);
  const first = tape.transportReceipt();
  tape.respond('GET', url);
  assert.notDeepEqual(first, tape.transportReceipt());
  const socket = tape.connect('wss://example.test/public', () => {}, () => {}, () => {});
  await new Promise(setImmediate);
  tape.send(socket, '{"op":"subscribe","args":["book.BTCUSDT"]}');
  const btc = tape.transportReceipt();
  tape.send(socket, '{"op":"subscribe","args":["book.ETHUSDT"]}');
  assert.notDeepEqual(btc, tape.transportReceipt());
  tape.close(socket);
});


test('replay CLI refuses to disable private-tape correctness auditing', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'private-performance-'));
  context.after(() => rmSync(directory, { recursive: true, force: true }));
  const tape = join(directory, 'private.jsonl');
  writeFileSync(tape, JSON.stringify({ type: 'config', private: true }) + '\n');
  const result = spawnSync(process.execPath, [
    fileURLToPath(new URL('./replay.mjs', import.meta.url)),
    '--impl', 'rust', '--venue', 'binance',
    '--tape', tape,
    '--out', join(directory, 'output.json'), '--performance',
  ], { encoding: 'utf8', timeout: 10000 });
  assert.ifError(result.error);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Performance mode requires a public tape; private tapes gate correctness only/);
});
