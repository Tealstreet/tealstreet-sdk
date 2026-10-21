import assert from 'node:assert/strict';
import test from 'node:test';
import { createPrivateKey, createPublicKey, verify } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { COINBASE_ES256_REPLAY } from './coinbase-protocol.mjs';
import { continuousReplayStartup, deterministicReplay, frozenReplayStartup } from './polling.mjs';
import { ReplayClock, Tape } from './tape.mjs';
import { VENUES } from './venues.mjs';

const EPOCH = 1765805280000;
const CENSUS = 'https://api.coinbase.com/api/v3/brokerage/orders/historical/batch?order_status=OPEN&page=';

// Model both census paths: a 2 s write-reconcile debounce and a 5 s tick, each
// reading three pages 334 ms apart. Busy waits model CI wall-time jitter.
const replayCensus = async (venue, jitter) => {
  const stage = (afterCommand) => [0, 1, 2].map((page) => ({
    type: 'http', method: 'GET', url: CENSUS + page, status: 200, afterCommand, body: JSON.stringify({ stage: afterCommand }),
  }));
  const records = [...stage(0), ...stage(1), ...stage(2),
    { type: 'command', t: 1000, op: 'cancelOrders' }, { type: 'command', t: 3100, op: 'cancelOrders' }, { type: 'wait', t: 4000 }, { type: 'wait', t: 6000 }];
  const clock = deterministicReplay(venue, true) ? new ReplayClock(EPOCH, frozenReplayStartup(venue, true)) : undefined;
  const tape = new Tape(records, {}, true, 2000, 'rust');
  const stall = () => { const until = performance.now() + jitter(); while (performance.now() < until); };
  let orders = 'startup';
  let reconcile;
  let ticker;
  const census = async () => {
    const pages = [];
    for (const page of [0, 1, 2]) {
      if (page) await new Promise((resolve) => setTimeout(resolve, 334));
      stall();
      pages.push(JSON.parse(tape.respond('GET', CENSUS + page).body).stage);
    }
    orders = `${Date.now() - EPOCH}:${pages.join('/')}`;
  };
  clock?.install();
  try {
    const tick = () => { void census(); ticker = setTimeout(tick, 5000); };
    ticker = setTimeout(tick, 5000);
    const checkpoints = [];
    await tape.play(undefined, async () => stall(), async () => {
      stall();
      clearTimeout(reconcile);
      reconcile = setTimeout(census, 2000);
      return [];
    }, clock, () => {}, ({ ordinal }) => { checkpoints.push(`${ordinal} ${orders}`); });
    return checkpoints;
  } finally {
    clearTimeout(ticker);
    clearTimeout(reconcile);
    clock?.restore();
  }
};

test('Coinbase private replays run their census timers on the frozen-startup virtual clock', () => {
  for (const name of ['coinbase', 'coinbasespot']) {
    assert.equal(deterministicReplay(VENUES[name], true), true, name);
    assert.equal(frozenReplayStartup(VENUES[name], true), true, name);
    assert.equal(continuousReplayStartup(VENUES[name], true), true, name);
  }
  // Tapes whose fixtures are anchored to their recorded epoch keep the rewind.
  for (const name of ['txflow', 'weex', 'apexomni']) assert.equal(continuousReplayStartup(VENUES[name], true), false, name);
  assert.equal(continuousReplayStartup(VENUES.coinbase, false), false);
});

test('a Coinbase census in flight at a checkpoint is pending there and lands at one fixed later checkpoint', { timeout: 10_000 }, async () => {
  const jitters = [() => 0, () => 15, () => Math.random() * 40];
  const wall = performance.now();
  const traces = [];
  for (const jitter of jitters) traces.push(await replayCensus(VENUES.coinbase, jitter));
  assert(performance.now() - wall < 5000, 'a 6-second private tape must not wait on wall time');
  // The first reconcile starts at 3000, before the second command at 3100: it is
  // pending at checkpoint 2 and lands at 3668 with pages from both stages.
  for (const trace of traces)
    assert.deepEqual(trace, ['0 startup', '1 startup', '2 startup', '3 3668:1/2/2', '4 5768:2/2/2']);
});

test('Coinbase TS ES256 replay signing settles in the calling turn and remains verifiable', async () => {
  const { subtle } = globalThis.crypto;
  try {
    await VENUES.coinbase.loadTs({ isPrivate: true });
    const secret = createPrivateKey(COINBASE_ES256_REPLAY.secret);
    const key = await subtle.importKey('pkcs8', secret.export({ format: 'der', type: 'pkcs8' }),
      { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign']);
    const message = Buffer.from('header.payload');
    const signing = subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, key, message);
    const signature = await Promise.race([signing, Promise.resolve('thread pool')]);
    assert.notEqual(signature, 'thread pool', 'a WebCrypto thread-pool signature lands at a wall-time-dependent turn');
    assert.equal(verify('sha256', message, { key: createPublicKey(secret), dsaEncoding: 'ieee-p1363' },
      Buffer.from(signature)), true);
  } finally {
    delete subtle.importKey;
    delete subtle.sign;
  }
});

// The real TS reference signer, loaded the way replay loads it (tsx), settles
// its second JWT before any macrotask and the JWT verifies independently.
test('the TS reference ES256 signer settles in microtasks once replay installs the synchronous signer', () => {
  const script = `
    const { installSynchronousEs256, COINBASE_ES256_REPLAY, verifyCoinbaseJwt } = await import(${JSON.stringify(new URL('./coinbase-protocol.mjs', import.meta.url).href)});
    installSynchronousEs256();
    const { sign } = await import(${JSON.stringify(new URL('../../packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts', import.meta.url).href)});
    const { key, secret } = COINBASE_ES256_REPLAY;
    await sign(key, secret, 'GET api.coinbase.com/api/v3/brokerage/accounts');
    const token = await Promise.race([sign(key, secret, 'GET api.coinbase.com/api/v3/brokerage/accounts'),
      new Promise((resolve) => setImmediate(() => resolve('macrotask')))]);
    console.log(JSON.stringify({ token, valid: verifyCoinbaseJwt(token, { method: 'GET', path: '/api/v3/brokerage/accounts', algorithm: 'ES256' }) }));`;
  const run = spawnSync(process.execPath, ['--import', 'tsx', '--input-type=module', '-e', script],
    { encoding: 'utf8', cwd: fileURLToPath(new URL('../..', import.meta.url)) });
  assert.equal(run.status, 0, run.stderr);
  const { token, valid } = JSON.parse(run.stdout.trim().split('\n').at(-1));
  assert.notEqual(token, 'macrotask');
  assert.equal(valid, true);
});
