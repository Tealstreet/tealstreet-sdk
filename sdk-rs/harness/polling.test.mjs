import assert from 'node:assert/strict';
import test from 'node:test';
import { createRecordingPolicy, deterministicReplay, recordingReady, replayTransportReady } from './polling.mjs';
import { ReplayClock, Tape } from './tape.mjs';

const playPolling = (tape, clock, start) => {
  let started = false;
  return tape.play(() => {
    if (!started) { started = true; start(); }
  }, undefined, undefined, clock);
};

test('only explicit polling transports may start without a socket and loaded state', () => {
  assert.equal(replayTransportReady({ polling: true }, 0, true), true);
  assert.equal(replayTransportReady({ polling: true }, 0, false), false);
  assert.equal(replayTransportReady({}, 0, true), false);
  assert.equal(replayTransportReady({ polling: false }, 0, true), false);
  assert.equal(replayTransportReady({}, 1, true), true);
  assert.equal(replayTransportReady({}, 1, false), false);
  assert.equal(deterministicReplay({ polling: true }, true), true);
  assert.equal(deterministicReplay({}, true), false);
  assert.equal(deterministicReplay({}, false), true);
});

test('polling recorder requires HTTP activity, loaded markets/tickers and requested books', () => {
  const state = { frames: 0, httpResponses: 2, loaded: true, books: 2, requestedBooks: 2 };
  assert.equal(recordingReady({ polling: true }, state), true);
  assert.equal(recordingReady({}, state), false);
  for (const patch of [{ httpResponses: 0 }, { loaded: false }, { books: 1 }])
    assert.equal(recordingReady({ polling: true }, { ...state, ...patch }), false);
  assert.equal(recordingReady({}, { ...state, frames: 1 }), true);
  assert.equal(recordingReady({}, { ...state, frames: 1, books: 1 }), false);
});

test('HTTP-only polling timeline advances timers and changes snapshots without sockets', async () => {
  const url = 'https://example.test/portfolio';
  const records = [
    { type: 'http', t: 0, method: 'GET', url, status: 200, body: 'initial', availableAfter: 0 },
    { type: 'http', t: 10, method: 'GET', url, status: 200, body: 'second', availableAfter: 10 },
    { type: 'http', t: 20, method: 'GET', url, status: 200, body: 'third', availableAfter: 20 },
  ];
  const tape = new Tape(records, { polling: true }, true);
  const clock = new ReplayClock(1700000000000);
  clock.install();
  const seen = [];
  let timer;
  try {
    assert.equal(tape.respond('GET', url).body, 'initial');
    const ticks = [];
    await tape.play((t) => ticks.push(t), async () => {}, async () => {}, clock, () => {
      timer = setInterval(() => seen.push(tape.respond('GET', url).body), 10);
    });
    assert.deepEqual(seen, ['second', 'third']);
    assert.deepEqual(ticks, [0, 10, 20]);
    assert.equal(tape.open.size, 0);
    assert.equal(tape.unserved, 0);
  } finally { clearInterval(timer); clock.restore(); }
  const socketTape = new Tape(records, {}, false);
  assert.deepEqual(socketTape.frames, []);
});

test('HTTP timeline does not consume expected mutations and still enforces exact writes', async () => {
  const url = 'https://example.test/orders';
  const tape = new Tape([
    { type: 'http', t: 0, method: 'POST', url, requestBody: '{"quantity":2}', status: 200, body: 'ok' },
  ], { polling: true, matchRequestBody: true }, false);
  await tape.play();
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(tape.respond('POST', url, '{"quantity":3}').status, 599);
  assert.equal(tape.respond('POST', url, '{"quantity":2}').body, 'ok');
  assert.equal(tape.unusedWrites(), 0);
  assert.equal(tape.respond('POST', url, '{"quantity":2}').status, 599);
});

test('a polling phase offset observes the final POST read at the next natural poll', async () => {
  const url = 'https://example.test/marketdata';
  const requestBody = '{"symbols":["ETHUSD"]}';
  const records = [0, 10000, 20000].map((t, index) => ({
    type: 'http', t, method: 'POST', url, requestBody, availableAfter: t, status: 200, body: `sample-${index}`,
  }));
  const validated = [];
  const tape = new Tape(records, {
    polling: true,
    isWrite: (_method, target) => target !== url,
    validateMatchedRequest(record, actual) {
      assert.equal(actual.body, requestBody);
      validated.push(record.body);
    },
  });
  const clock = new ReplayClock(1700000000000);
  clock.install();
  const seen = [];
  let timer;
  const poll = () => {
    seen.push({ t: Date.now() - clock.epoch, body: tape.respond('POST', url, requestBody).body });
    timer = setTimeout(poll, 2000);
  };
  try {
    assert.equal(tape.respond('POST', url, requestBody).body, 'sample-0');
    await playPolling(tape, clock, () => { timer = setTimeout(poll, 1995); });
    assert.deepEqual(seen.at(-1), { t: 21995, body: 'sample-2' });
    assert.equal(clock.time - clock.epoch, 21995);
    assert.equal(validated.at(-1), 'sample-2');
    assert.equal(tape.unserved, 0);
  } finally { clearTimeout(timer); clock.restore(); }
});

test('polling completion requires final touched snapshots, not every intermediate or untouched read', async () => {
  const url = 'https://example.test/quotes';
  const records = [0, 1, 2].map((t, index) => ({
    type: 'http', t, method: 'GET', url, effectiveAt: t, status: 200, body: `sample-${index}`,
  }));
  records.push({ type: 'http', t: 2, method: 'GET', url: 'https://example.test/unused', availableAt: 2, status: 200, body: 'unused' });
  const tape = new Tape(records, { polling: true });
  const clock = new ReplayClock(1700000000000);
  clock.install();
  let timer;
  const seen = [];
  try {
    seen.push(tape.respond('GET', url).body);
    await playPolling(tape, clock, () => {
      timer = setInterval(() => seen.push(tape.respond('GET', url).body), 10);
    });
    assert.deepEqual(seen, ['sample-0', 'sample-2']);
    assert.equal(tape.requests.size, 1);
    assert.equal(clock.time - clock.epoch, 10);
  } finally { clearInterval(timer); clock.restore(); }
});

test('TS request-body aliases retain the actual touched read queue for final completion', async () => {
  const url = 'https://example.test/marketdata';
  const records = [0, 20].map((t, index) => ({
    type: 'http', t, method: 'POST', url, requestBody: '{"native":true}', tsRequestBody: '{"ts":true}',
    availableAfter: t, status: 200, body: `sample-${index}`,
  }));
  const tape = new Tape(records, { polling: true, isWrite: () => false }, false, 30000, 'ts');
  const clock = new ReplayClock(1700000000000);
  clock.install();
  let timer;
  const seen = [];
  try {
    seen.push(tape.respond('POST', url, '{"ts":true}').body);
    await playPolling(tape, clock, () => {
      timer = setTimeout(() => seen.push(tape.respond('POST', url, '{"ts":true}').body), 25);
    });
    assert.deepEqual(seen, ['sample-0', 'sample-1']);
    assert.equal(clock.time - clock.epoch, 25);
  } finally { clearTimeout(timer); clock.restore(); }
});

test('missing final polling reads fail within a bounded number of timer or idle turns', { timeout: 2000 }, async () => {
  const url = 'https://example.test/quotes';
  for (const unrelatedTimer of [false, true]) {
    const tape = new Tape([
      { type: 'http', t: 0, method: 'GET', url, availableAfter: 0, status: 200, body: 'initial' },
      { type: 'http', t: 20, method: 'GET', url, availableAfter: 20, status: 200, body: 'final' },
    ], { polling: true });
    class BudgetClock extends ReplayClock {
      wait(promise, label, timeout, options) {
        // Exercise the production budget wiring without thousands of no-op
        // interval callbacks in this negative regression.
        assert.equal(options.maxSteps, 8192);
        return super.wait(promise, label, timeout, { ...options, maxSteps: 32 });
      }
    }
    const clock = new (unrelatedTimer ? BudgetClock : ReplayClock)(1700000000000);
    clock.install();
    let timer;
    let ticks = 0;
    try {
      tape.respond('GET', url);
      await assert.rejects(playPolling(tape, clock, () => {
        if (unrelatedTimer) timer = setInterval(() => ticks++, 1);
      }), unrelatedTimer ? /polling final HTTP anchors.*virtual step budget exceeded/ : /polling final HTTP anchors.*idle step budget exceeded/);
      assert.ok(ticks <= 52);
      assert.ok(clock.time - clock.epoch <= 52);
      if (!unrelatedTimer) assert.equal(clock.time - clock.epoch, 20);
    } finally { clearInterval(timer); clock.restore(); }
  }
});

test('non-polling replay retains its HTTP-free timeline and never waits for polling anchors', async () => {
  const url = 'https://example.test/quotes';
  const tape = new Tape([
    { type: 'http', t: 0, method: 'GET', url, availableAfter: 0, status: 200, body: 'initial' },
    { type: 'http', t: 20, method: 'GET', url, availableAfter: 20, status: 200, body: 'final' },
  ]);
  const clock = new ReplayClock(1700000000000);
  clock.install();
  let timer;
  let calls = 0;
  try {
    assert.equal(tape.respond('GET', url).body, 'initial');
    assert.equal(await tape.play(undefined, undefined, undefined, clock, () => {
      timer = setInterval(() => { calls++; tape.respond('GET', url); }, 10);
    }), 0);
    assert.equal(calls, 0);
    assert.equal(clock.time, clock.epoch);
    assert.deepEqual(tape.frames, []);
  } finally { clearInterval(timer); clock.restore(); }
});

test('polling completion preserves expected mutations and rejects unrecorded writes', async () => {
  const url = 'https://example.test/quotes';
  const orders = 'https://example.test/orders';
  for (const unexpectedWrite of [false, true]) {
    const tape = new Tape([
      { type: 'http', t: 0, method: 'GET', url, availableAfter: 0, status: 200, body: 'initial' },
      { type: 'http', t: 20, method: 'GET', url, availableAfter: 20, status: 200, body: 'final' },
      { type: 'http', t: 20, method: 'POST', url: orders, requestBody: '{"quantity":2}', status: 200, body: 'ack' },
    ], { polling: true });
    const clock = new ReplayClock(1700000000000);
    clock.install();
    let timer;
    try {
      tape.respond('GET', url);
      const playing = playPolling(tape, clock, () => {
        timer = setInterval(() => {
          if (unexpectedWrite && Date.now() - clock.epoch > 20)
            assert.equal(tape.respond('POST', orders, '{"quantity":3}').status, 599);
          tape.respond('GET', url);
        }, 15);
      });
      if (unexpectedWrite) await assert.rejects(playing, /unmatched HTTP request/);
      else await playing;
      assert.equal(tape.unusedWrites(), 1);
      assert.equal(tape.writes.length, unexpectedWrite ? 1 : 0);
    } finally { clearInterval(timer); clock.restore(); }
  }
});

test('natural future snapshots cannot be rewound to satisfy a superseded end anchor', async () => {
  const url = 'https://example.test/quotes';
  const tape = new Tape([
    { type: 'http', t: 0, method: 'GET', url, availableAt: 0, status: 200, body: 'initial' },
    { type: 'http', t: 20, method: 'GET', url, availableAt: 20, status: 200, body: 'end' },
    { type: 'http', t: 21, method: 'GET', url, availableAt: 22, status: 200, body: 'future' },
  ], { polling: true });
  const clock = new ReplayClock(1700000000000);
  clock.install();
  let timer;
  const seen = [];
  const poll = () => {
    seen.push(tape.respond('GET', url).body);
    timer = setTimeout(poll, 10);
  };
  try {
    tape.respond('GET', url);
    await assert.rejects(playPolling(tape, clock, () => {
      timer = setTimeout(poll, 19);
    }), /superseded before a natural poll/);
    assert.deepEqual(seen, ['initial', 'future', 'future']);
    assert.equal(clock.time - clock.epoch, 39);
  } finally { clearTimeout(timer); clock.restore(); }
});

test('polling completion preserves matched-request validation and fails unmatched reads', async () => {
  const url = 'https://example.test/quotes';
  for (const invalidMatch of [false, true]) {
    const tape = new Tape([
      { type: 'http', t: 0, method: 'GET', url, availableAfter: 0, status: 200, body: 'initial' },
      { type: 'http', t: 20, method: 'GET', url, availableAfter: 20, status: 200, body: 'final' },
    ], {
      polling: true,
      validateMatchedRequest(record) {
        if (invalidMatch && record.body === 'final') throw new Error('Invalid documented payload');
      },
    });
    const clock = new ReplayClock(1700000000000);
    clock.install();
    let timer;
    try {
      tape.respond('GET', url);
      await assert.rejects(playPolling(tape, clock, () => {
        timer = setTimeout(() => tape.respond('GET', invalidMatch ? url : `${url}/missing`), 25);
      }), invalidMatch ? /Invalid documented payload/ : /unmatched HTTP request/);
      assert.equal(tape.unserved, 1);
    } finally { clearTimeout(timer); clock.restore(); }
  }
});

test('successful final-anchor progress resets the idle budget for other pending asynchronous reads', async () => {
  const urls = ['https://example.test/quotes', 'https://example.test/portfolio'];
  const records = urls.flatMap((url) => [
    { type: 'http', t: 0, method: 'GET', url, availableAfter: 0, status: 200, body: 'initial' },
    { type: 'http', t: 20, method: 'GET', url, availableAfter: 20, status: 200, body: 'final' },
  ]).sort((a, b) => a.t - b.t);
  const tape = new Tape(records, { polling: true });
  const clock = new ReplayClock(1700000000000);
  clock.install();
  const afterTurns = (remaining, callback) => remaining ? setImmediate(() => afterTurns(remaining - 1, callback)) : callback();
  let timer;
  const seen = [];
  try {
    for (const url of urls) tape.respond('GET', url);
    await playPolling(tape, clock, () => {
      timer = setTimeout(() => afterTurns(40, () => {
        seen.push(tape.respond('GET', urls[0]).body);
        afterTurns(40, () => seen.push(tape.respond('GET', urls[1]).body));
      }), 25);
    });
    assert.deepEqual(seen, ['final', 'final']);
    assert.equal(clock.time - clock.epoch, 25);
  } finally { clearTimeout(timer); clock.restore(); }
});

test('credentialed recorder refuses missing sanitizers and live options in tape config', () => {
  const options = { key: 'actual-login', secret: 'actual-password' };
  assert.throws(() => createRecordingPolicy({}, options, true), /replay-safe auth sanitizers/);
  assert.throws(() => createRecordingPolicy({ recordSession: true }, options), /replay-safe auth sanitizers/);
  assert.throws(() => createRecordingPolicy({
    recordSession: true, sanitizeRecordingRecord: (r) => r, recordReplayOptions: () => options,
  }, options), /retained a live credential/);
});

const sessionVenue = {
  recordSession: true,
  recordReplayOptions: () => ({ key: 'replay-key', secret: 'replay-secret' }),
  sanitizeRecordingRecord(record) {
    if (record.url?.endsWith('/login')) return {
      ...record,
      requestBody: JSON.stringify({ username: 'replay-key', password: 'replay-secret' }),
      body: JSON.stringify({ sessionToken: 'replay-session', timeout: '00:30:00' }),
    };
    return record;
  },
};

test('credentialed recording substitutes login and removes response headers', () => {
  const policy = createRecordingPolicy(sessionVenue, { key: 'actual-login', secret: 'actual-password' });
  const safe = policy.sanitize({
    type: 'http', method: 'POST', url: 'https://example.test/login', status: 200,
    requestBody: '{"username":"actual-login","password":"actual-password"}',
    body: '{"sessionToken":"actual-session","timeout":"00:30:00"}',
    headers: 'set-cookie: private-cookie',
  });
  assert.deepEqual(JSON.parse(safe.body), { sessionToken: 'replay-session', timeout: '00:30:00' });
  assert.equal(safe.headers, '');
  assert.throws(() => policy.sanitize({ type: 'http', body: 'actual-session' }), /retained a live credential/);
  assert.throws(() => policy.sanitize({ type: 'http', url: 'https://example.test/users/actual-login' }), /retained a live credential/);
});

test('credential check handles quoted and URL-encoded secrets without exposing them in errors', () => {
  const options = { key: 'actual-login', secret: 'secret" with space' };
  const policy = createRecordingPolicy(sessionVenue, options);
  for (const body of [options.secret, encodeURIComponent(options.secret)])
    assert.throws(() => policy.sanitize({ type: 'http', body }), (error) =>
      /retained a live credential/.test(error.message) && !error.message.includes(options.secret));
});
