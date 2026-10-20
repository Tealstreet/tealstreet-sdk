import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ReplayClock, Tape } from './tape.mjs';

const epoch = 1700000000000;
const turn = () => new Promise(setImmediate);

test('logical startup ignores constructor CPU but carries genuine setup time into an existing quota', async () => {
  const run = async (cpuMs) => {
    const clock = new ReplayClock(epoch);
    clock.install();
    try {
      clock.beginStartup();
      const seen = [];
      let ready = false;
      setTimeout(() => seen.push(Date.now() - epoch), 1000);
      const end = performance.now() + cpuMs;
      while (performance.now() < end) { /* constructor/decoder CPU does not advance protocol time */ }
      setTimeout(() => { ready = true; }, 100);
      await clock.until(() => ready, 'setup', 1000);
      assert.equal(Date.now() - epoch, 100);
      clock.start(0);
      await clock.advance(999);
      assert.deepEqual(seen, []);
      await clock.advance(1000);
      return seen;
    } finally { clock.restore(); }
  };
  assert.deepEqual(await run(0), [1000]);
  assert.deepEqual(await run(40), [1000]);
});

test('logical startup waits for complete response consumption before advancing a watchdog', async () => {
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    const tape = new Tape([{ type: 'http', t: 156, method: 'GET', url: 'https://example.test/seed', body: '{}', status: 200 }]);
    tape.replayClock = clock;
    let activeRequests = 1, ready = false, watchdog = false;
    const consumedAt = [];
    clock.startupBlocked = () => activeRequests > tape.pendingResponses.size;
    setTimeout(() => { watchdog = true; }, 160);
    const request = (async () => {
      await tape.waitForResponse(tape.respond('GET', 'https://example.test/seed'));
      for (let i = 0; i < 40; i++) {
        await turn();
        consumedAt.push(Date.now() - epoch);
      }
      activeRequests--;
      ready = true;
    })();
    await clock.until(() => ready, 'stream and HTTP end', 1000);
    await request;
    assert.deepEqual(new Set(consumedAt), new Set([156]));
    assert.equal(watchdog, false);
    assert.equal(Date.now() - epoch, 156);
  } finally { clock.restore(); }
});

test('explicit asynchronous SDK preparation blocks clock jumps across idle event turns', async () => {
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    const release = clock.hold();
    let prepared = false, watchdog = false;
    setTimeout(() => { watchdog = true; }, 10);
    const preparation = (async () => {
      for (let i = 0; i < 40; i++) await turn();
      prepared = true;
      release();
    })();
    await clock.until(() => prepared, 'SDK preparation', 1000);
    await preparation;
    assert.equal(Date.now(), epoch);
    assert.equal(watchdog, false);
  } finally { clock.restore(); }
});

test('startup response availability remains request-time fenced while recorded completion drives logical time', async () => {
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    const tape = new Tape([
      { type: 'http', t: 156, method: 'GET', url: 'https://example.test/seed', body: 'seed', status: 200 },
      { type: 'http', t: 350, availableAfter: 300, method: 'GET', url: 'https://example.test/later', body: 'later', status: 200 },
    ]);
    tape.replayClock = clock;
    clock.startupBlocked = () => clock.work.size > tape.pendingResponses.size;
    const finish = clock.beginWork();
    let response;
    const request = tape.waitForResponse(tape.respond('GET', 'https://example.test/seed'))
      .then((value) => { response = value; finish(); });
    assert.equal(response, undefined);
    assert.equal(tape.respond('GET', 'https://example.test/later').status, 599);
    await clock.until(() => response !== undefined, 'recorded seed', 1000);
    await request;
    assert.equal(Date.now() - epoch, 156);
    assert.equal(response.body, 'seed');
    assert.equal(tape.respond('GET', 'https://example.test/later').status, 599);
    await clock.advance(300);
    assert.equal(tape.respond('GET', 'https://example.test/later').body, 'later');
  } finally { clock.restore(); }
});

test('logical startup delivers explicit lifecycle records in order and retains every original tick', async () => {
  const url = 'wss://example.test/public';
  const tape = new Tape([
    { type: 'connect', t: 40, url, connection: 0 },
    { type: 'open', t: 50, url, connection: 0 },
    { type: 'ws', t: 100, url, connection: 0, text: 'first' },
    { type: 'ws', t: 200, url, connection: 0, text: 'second' },
  ], { explicitOpens: true });
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    const seen = [], ticks = [];
    let ready = false;
    tape.readiness = () => ready;
    setTimeout(() => tape.connect(url,
      () => {
        seen.push(['open', Date.now() - epoch]);
        setTimeout(() => { ready = true; }, 20);
      },
      (text) => seen.push([text, Date.now() - epoch])), 40);
    const delivered = await tape.play((t) => ticks.push(t), undefined, undefined, clock);
    assert.equal(delivered, 3);
    assert.deepEqual(ticks, [50, 100, 200]);
    assert.deepEqual(seen, [['open', 50], ['first', 100], ['second', 200]]);
  } finally { clock.restore(); }
});

test('a blocked inline prefix cannot advance a timer beyond its next undelivered record', async () => {
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    let fired = false;
    setTimeout(() => { fired = true; }, 75);
    await assert.rejects(clock.until(() => fired, 'bounded prefix', 25, 50), /timeout/);
    assert.equal(fired, false);
    assert.equal(Date.now(), epoch);
  } finally { clock.restore(); }
});

test('actual production adapter releases transformed errors and permits retry backoff timers', async () => {
  const { readFileSync } = await import('node:fs');
  const { default: axios } = await import('axios');
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  const adapter = async (config) => {');
  const end = source.indexOf('  const { ExchangeWorker }', start);
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    const tape = new Tape([
      { type: 'http', t: 0, method: 'GET', url: 'https://example.test/normalize', body: '{}', status: 401 },
      { type: 'http', t: 0, method: 'GET', url: 'https://example.test/retry', body: '{}', status: 401 },
      { type: 'http', t: 20, availableAfter: 20, method: 'GET', url: 'https://example.test/retry', body: '{"ok":true}', status: 200 },
    ]);
    tape.replayClock = clock;
    clock.startupBlocked = () => clock.work.size > tape.pendingResponses.size;
    const adapter = new Function('tape', 'clock', `${source.slice(start, end)}; return adapter;`)(tape, clock);
    const normalized = axios.create({ adapter });
    normalized.interceptors.response.use(undefined, () => { throw new Error('normalized without config'); });
    let rejected = false;
    const invalid = normalized.get('https://example.test/normalize').catch((error) => {
      assert.equal(error.config, undefined);
      rejected = true;
    });
    await clock.until(() => rejected, 'normalized response', 1000);
    await invalid;
    assert.equal(clock.work.size, 0);
    const retried = axios.create({ adapter });
    retried.interceptors.response.use(undefined, async (error) => {
      await new Promise((resolve) => setTimeout(resolve, 20));
      return retried.request(error.config);
    });
    let result;
    const pending = retried.get('https://example.test/retry').then((response) => { result = response.data; });
    await clock.until(() => result !== undefined, 'retry backoff', 1000);
    await pending;
    assert.deepEqual(result, { ok: true });
    assert.equal(Date.now() - epoch, 20);
    assert.equal(clock.work.size, 0);
  } finally { clock.restore(); }
});

test('actual explicit open delivery cannot borrow an unbounded nested socket clock pump', async () => {
  const url = 'wss://example.test/public';
  const tape = new Tape([
    { type: 'connect', t: 0, url, connection: 0 },
    { type: 'open', t: 50, url, connection: 0 },
    { type: 'ws', t: 100, url, connection: 0, text: 'data' },
  ], { explicitOpens: true, timeoutMs: 30 });
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    let constructed = false;
    tape.readiness = () => constructed;
    setTimeout(() => {
      constructed = true;
      tape.connect(url, () => {}, () => {});
    }, 75);
    await assert.rejects(tape.play(undefined, undefined, undefined, clock), /timeout/);
    assert.equal(constructed, false);
    assert.equal(clock.time - epoch, 50);
  } finally { clock.restore(); }
});

test('a failed startup cancels its I/O drain rather than leaving a background scheduler', async () => {
  const { setTimeout: pause } = await import('node:timers/promises');
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    const release = clock.hold();
    let observations = 0;
    clock.observeStartup = () => observations++;
    await assert.rejects(clock.until(() => false, 'held forever', 15), /timeout/);
    const after = observations;
    await pause(15);
    assert.equal(observations, after);
    assert.equal(clock.startupOwner, false);
    release();
  } finally { clock.restore(); }
});

test('new subscription work reported by the observer settles before selecting a future watchdog', async () => {
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    clock.startupBlocked = () => clock.work.size > 0;
    let started = false, ready = false, watchdog = false;
    setTimeout(() => { watchdog = true; }, 10);
    clock.observeStartup = () => {
      if (started) return false;
      started = true;
      (async () => {
        for (let i = 0; i < 10; i++) await Promise.resolve();
        const finish = clock.beginWork();
        for (let i = 0; i < 10; i++) await turn();
        finish();
        ready = true;
      })();
      return true;
    };
    await clock.until(() => ready, 'subscription response', 1000);
    assert.equal(Date.now(), epoch);
    assert.equal(watchdog, false);
  } finally { clock.restore(); }
});

test('dense startup frames reuse settlement without repeating an idle barrier between records', async () => {
  const url = 'wss://example.test/dense';
  const frames = Array.from({ length: 1001 }, (_, t) => ({ type: 'ws', t, url, text: String(t) }));
  const tape = new Tape(frames);
  tape.synchronousTicks = true; // This fixture's tick is an audited synchronous consumer.
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    let ready = false, drains = 0;
    const original = clock.drainStartup.bind(clock);
    clock.drainStartup = async (...args) => { drains++; return original(...args); };
    tape.readiness = () => ready;
    setTimeout(() => { ready = true; }, 999);
    const seen = [], ticks = [];
    tape.connect(url, () => {}, (text) => seen.push(text));
    assert.equal(await tape.play((t) => ticks.push(t), undefined, undefined, clock), frames.length);
    assert.deepEqual(seen, frames.map((frame) => frame.text));
    assert.deepEqual(ticks, frames.map((frame) => frame.t));
    assert.ok(drains <= frames.length + 6, `repeated settlement barriers: ${drains}`);
  } finally { clock.restore(); }
});

test('startup settlement reuse is opt-in and is invalidated by work, holds, timers and failures', async () => {
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    let observations = 0;
    clock.observeStartup = () => { observations++; return false; };
    await clock.until(() => true, 'initial settlement', 1000);
    const initial = observations;
    await clock.advance(1, true);
    assert.equal(observations, initial);
    await clock.advance(2);
    assert.ok(observations > initial, 'generic advance must still settle');
    const finish = clock.beginWork();
    assert.equal(clock.startupSettled, false);
    finish();
    await clock.until(() => true, 'response consumed', 1000);
    const release = clock.hold();
    assert.equal(clock.startupSettled, false);
    release();
    await clock.until(() => true, 'preparation complete', 1000);
    setTimeout(() => { throw new Error('protocol callback failed'); }, 1);
    assert.equal(clock.startupSettled, false);
    await assert.rejects(clock.advance(3, true), /protocol callback failed/);
    assert.equal(clock.startupSettled, false);
    clock.restore();
    assert.equal(clock.startupSettled, false);
  } finally { clock.restore(); }
});

test('a generic tick that creates asynchronous owned work cannot lend its previous settlement to the next frame', async () => {
  const url = 'wss://example.test/tick';
  const tape = new Tape([{ type: 'ws', t: 0, url, text: 'first' }, { type: 'ws', t: 10, url, text: 'second' }]);
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    clock.startupBlocked = () => clock.work.size > 0;
    let ready = false, bodyDone = false, consumedBeforeWatchdog;
    tape.readiness = () => ready;
    setTimeout(() => { consumedBeforeWatchdog = bodyDone; }, 5);
    setTimeout(() => { ready = true; }, 20);
    tape.connect(url, () => {}, () => {});
    await tape.play((t) => {
      if (t !== 0) return;
      (async () => {
        for (let i = 0; i < 10; i++) await Promise.resolve();
        const finish = clock.beginWork();
        for (let i = 0; i < 10; i++) await turn();
        bodyDone = true;
        finish();
      })();
    }, undefined, undefined, clock);
    assert.equal(consumedBeforeWatchdog, true);
  } finally { clock.restore(); }
});

test('queued transport open is owned through invocation before subscription setup observes readiness', async () => {
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.beginStartup();
    const tape = new Tape([]);
    tape.replayClock = clock;
    clock.startupBlocked = () => clock.work.size > 0;
    let socket, setup = false, observedUnopened = false, ready = false;
    const finishResponse = clock.beginWork();
    const response = (async () => {
      await turn();
      socket = tape.connect('wss://example.test/queued', () => {}, () => {});
      finishResponse();
    })();
    clock.observeStartup = () => {
      if (!socket || setup) return false;
      setup = true;
      observedUnopened = !socket.opened;
      if (!socket.opened) setTimeout(() => { ready = true; }, 100);
      else ready = true;
      return true;
    };
    await clock.until(() => ready, 'queued connection', 1000);
    await response;
    assert.equal(observedUnopened, false);
    assert.equal(Date.now(), epoch);
    assert.equal(clock.holds.size, 0);
  } finally { clock.restore(); }
});
