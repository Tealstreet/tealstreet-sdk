import assert from 'node:assert/strict';
import test from 'node:test';
import { createRecordingPolicy, createRecordingTimeline, deterministicReplay, recordingReady, replayTransportReady } from './polling.mjs';
import { breakoutprop, BREAKOUT_REPLAY_TOKEN } from './breakoutprop-requests.mjs';
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
  assert.equal(deterministicReplay({ virtualPrivateClock: true }, true), true);
  assert.equal(deterministicReplay({ virtualPrivateClock: false }, true), false);
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

test('live polling recorder stamps post-ready reads and preserves startup, mutations and other venues', () => {
  const timeline = createRecordingTimeline({ polling: true, isWrite: (method, url) => method === 'POST' && url.endsWith('/orders') });
  const read = { type: 'http', t: 325, method: 'POST', url: 'https://example.test/marketdata?b=2&a=1', requestBody: '{"symbols":["ETHUSD"],"type":"Quote"}', status: 200, body: 'seed' };
  assert.equal(timeline(read), read);
  assert.equal(timeline({ ...read, t: 2325, url: 'https://example.test/marketdata?a=1&b=2', requestBody: '{"type":"Quote","symbols":["ETHUSD"]}' }, true).availableAfter, 2325);
  assert.equal(timeline({ ...read, requestBody: '{"symbols":["BTCUSD"],"type":"Quote"}' }, true).availableAfter, 325);
  assert.equal(timeline({ ...read, method: 'GET' }, true).availableAfter, 325);
  assert.equal(timeline({ ...read, url: 'https://example.test/other' }, true).availableAfter, 325);
  const mutation = { ...read, url: 'https://example.test/orders' };
  assert.equal(timeline(mutation, true), mutation);
  const meta = { type: 'meta', t: 0 };
  assert.equal(timeline(meta, true), meta);
  assert.equal(createRecordingTimeline({})(read, true), read);
  assert.equal(createRecordingTimeline({ polling: true })(mutation, true), mutation);
});

test('timed POST reads keep distinct request bodies and never expose a future first-seen query early', () => {
  const venue = { polling: true, isWrite: () => false };
  const timeline = createRecordingTimeline(venue);
  const base = { type: 'http', method: 'POST', url: 'https://example.test/marketdata', status: 200 };
  const ethBody = '{"symbols":["ETHUSD"],"type":"Quote"}';
  const btcBody = '{"symbols":["BTCUSD"],"type":"Quote"}';
  const tape = new Tape([
    timeline({ ...base, t: 325, requestBody: ethBody, body: 'ETH seed' }),
    timeline({ ...base, t: 2325, requestBody: btcBody, body: 'BTC future' }, true),
  ], venue);
  assert.equal(tape.respond('POST', base.url, ethBody).body, 'ETH seed');
  assert.equal(tape.respond('POST', base.url, btcBody).status, 599);
  tape.time = 2325;
  assert.equal(tape.respond('POST', base.url, btcBody).body, 'BTC future');
  assert.equal(tape.respond('POST', base.url, '{"type":"Quote","symbols":["ETHUSD"]}').body, 'ETH seed');
});

test('startup HTTP failures and retries stay ordered before virtual time starts', () => {
  const venue = { polling: true, isWrite: () => false };
  const timeline = createRecordingTimeline(venue);
  const base = { type: 'http', method: 'POST', url: 'https://example.test/marketdata', requestBody: '{"symbols":["ETHUSD"]}' };
  const records = [
    timeline({ ...base, t: 325, status: 500, body: 'temporary failure' }),
    timeline({ ...base, t: 2325, status: 200, body: 'startup recovered' }),
    timeline({ ...base, t: 4325, status: 200, body: 'future poll' }, true),
  ];
  const tape = new Tape(records, venue);
  assert.equal(tape.respond('POST', base.url, base.requestBody).status, 500);
  assert.equal(tape.respond('POST', base.url, base.requestBody).body, 'startup recovered');
  assert.equal(tape.respond('POST', base.url, base.requestBody).body, 'startup recovered');
  assert.equal(tape.time, 0);
  assert.equal(tape.unserved, 0);
});

test('sanitized login seeds survive a startup transport delay and retain definitive HTTP rejection', () => {
  for (const status of [200, 401]) {
    const options = { key: 'actual-login', secret: 'actual-password' };
    const policy = createRecordingPolicy(breakoutprop, options);
    const timeline = createRecordingTimeline(breakoutprop);
    // A transport timeout emits no HTTP record; its later retry can first return
    // at this offset while replay still needs the login at startup time zero.
    const login = timeline(policy.sanitize({
      type: 'http', t: 6500, method: 'POST', url: 'https://example.test/dxsca-web/login', status,
      requestBody: JSON.stringify({ domain: 'default', username: options.key, password: options.secret }),
      body: JSON.stringify(status === 200 ? { sessionToken: 'actual-session', timeout: '00:30:00' } : { message: 'Invalid credentials' }),
    }));
    assert.equal(login.t, 6500);
    assert.equal(login.availableAfter, undefined);
    assert.ok(!JSON.stringify(login).includes('actual-'));
    const tape = new Tape([login], breakoutprop);
    const response = tape.respond('POST', login.url, JSON.stringify({ domain: 'default', username: 'replay-key', password: 'replay-secret' }));
    assert.equal(response.status, status);
    assert.deepEqual(JSON.parse(response.body), status === 200
      ? { sessionToken: BREAKOUT_REPLAY_TOKEN, timeout: '00:30:00' }
      : { message: 'Invalid credentials' });
    assert.equal(tape.unserved, 0);
  }
});

const livePollingRecords = () => {
  const venue = { polling: true, isWrite: () => false };
  const timeline = createRecordingTimeline(venue);
  const url = 'https://example.test/marketdata';
  const requestBody = '{"symbols":["ETHUSD"]}';
  const records = [325, 2325, 4325].map((t, index) => timeline({
    type: 'http', t, method: 'POST', url, requestBody, status: 200, body: `sample-${index}`,
  }, index > 0));
  return { venue, records, url, requestBody };
};

test('live recorder-shaped polling tape preserves startup seeds and waits for a natural final poll', async () => {
  const { venue, records, url, requestBody } = livePollingRecords();
  const tape = new Tape(records, venue);
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
    assert.equal(tape.respond('POST', url, requestBody).body, 'sample-0');
    await tape.play(undefined, undefined, undefined, clock, () => { timer = setTimeout(poll, 2000); });
    assert.deepEqual(seen.map((row) => row.body), ['sample-0', 'sample-1', 'sample-2']);
    assert.ok(seen[0].t < 2325);
    assert.ok(seen[1].t >= 2325 && seen[1].t < 4325);
    assert.ok(seen[2].t >= 4325);
    assert.ok(clock.time - clock.epoch > records.at(-1).t);
    assert.equal(tape.unserved, 0);
  } finally { clearTimeout(timer); clock.restore(); }
});

test('live recorder-shaped polling tape rejects a missing poller within the idle budget', { timeout: 2000 }, async () => {
  const { venue, records, url, requestBody } = livePollingRecords();
  const tape = new Tape(records, venue);
  const clock = new ReplayClock(1700000000000);
  clock.install();
  try {
    assert.equal(tape.respond('POST', url, requestBody).body, 'sample-0');
    await assert.rejects(tape.play(undefined, undefined, undefined, clock), /polling final HTTP anchors.*idle step budget exceeded/);
    assert.deepEqual([...tape.selectedPollingResponses].map((row) => row.body), ['sample-0']);
    assert.equal(clock.time - clock.epoch, 4325);
  } finally { clock.restore(); }
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
    await tape.play((t) => {
      ticks.push(t);
      // Arm after the virtual clock starts, so wall-clock startup cannot poll.
      if (ticks.length === 1) timer = setInterval(() => seen.push(tape.respond('GET', url).body), 10);
    }, async () => {}, async () => {}, clock);
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
