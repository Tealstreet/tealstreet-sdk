import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deterministicReplay, frozenReplayStartup } from './polling.mjs';
import { ReplayClock } from './tape.mjs';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { setTimeout as realTimeout } from 'node:timers';

test('startup wall jitter cannot change public or polling virtual poll deadlines', async () => {
  const epoch = 1700000000000;
  for (const [venue, isPrivate] of [[{}, false], [{ polling: true }, false], [{ polling: true }, true], [{ virtualPrivateClock: true }, true]]) {
    assert.equal(deterministicReplay(venue, isPrivate), true);
    const receipts = [];
    for (const jitter of [0, 1, 31, 99]) {
      const originalNow = Date.now;
      let wall = epoch + 5000;
      const clock = new ReplayClock(epoch, frozenReplayStartup(venue, isPrivate));
      clock.original.now = () => wall;
      const calls = [];
      clock.install();
      try {
        const poll = () => { calls.push(Date.now()); setTimeout(poll, 1000); };
        setTimeout(poll, 1000);
        wall += jitter;
        assert.equal(Date.now(), epoch);
        clock.start();
        await clock.advance(999);
        assert.deepEqual(calls, []);
        await clock.advance(3000);
        receipts.push(calls);
      } finally {
        clock.restore();
        Date.now = originalNow;
      }
    }
    for (const receipt of receipts) assert.deepEqual(receipt, [epoch + 1000, epoch + 2000, epoch + 3000]);
  }
});

test('ordinary private replay retains its real-time startup and no virtual clock', () => {
  assert.equal(deterministicReplay({}, true), false);
  assert.equal(frozenReplayStartup({}, true), false);
  assert.equal(frozenReplayStartup({ virtualPrivateClock: false }, true), false);
});

test('frozen startup timers advance paced prerequisites by logical deadlines and rebase remaining timers without wall jitter', async () => {
  const epoch = 1700000000000;
  const traces = [];
  for (const jitter of [0, 99, 5000]) {
    const originalNow = Date.now;
    let wall = epoch;
    const clock = new ReplayClock(epoch, true);
    clock.original.now = () => wall;
    clock.install();
    try {
      const events = [];
      // Model a native 5-request window or TS token queue: the callback must
      // observe that its required startup delay has actually elapsed.
      setTimeout(() => {
        events.push(Date.now());
        setTimeout(() => events.push(Date.now()), 1000);
      }, 1000);
      for (let index = 0; index < 2; index++) {
        const next = [...clock.timers.values()].sort((a, b) => a.at - b.at)[0];
        clock.original.clearTimeout(next.handle);
        wall += 1000 + jitter;
        await clock.fire(next);
      }
      assert.deepEqual(events, [epoch + 1000, epoch + 2000]);
      setTimeout(() => events.push(Date.now()), 10_000);
      wall += jitter;
      clock.start();
      await clock.advance(9999);
      assert.equal(events.length, 2);
      await clock.advance(10_000);
      traces.push(events);
    } finally {
      clock.restore();
      Date.now = originalNow;
    }
  }
  for (const trace of traces) assert.deepEqual(trace, [epoch + 1000, epoch + 2000, epoch + 10_000]);
});

test('blocked native startup dispatch retains logical priority for nested timers and Promise children', async () => {
  const run = async (blocked, legacy, promiseChild) => {
    const epoch = 1700000000000;
    const clock = new ReplayClock(epoch, true);
    if (legacy) clock.fire = (timer) => clock.fireTimer(timer);
    clock.install();
    const events = [];
    let complete;
    const done = new Promise(resolve => { complete = resolve; });
    const record = (name) => {
      events.push([name, Date.now() - epoch]);
      if (events.length === 3) complete();
    };
    try {
      setTimeout(() => {
        record('A');
        const child = () => setTimeout(() => record('child'), 5);
        if (promiseChild) Promise.resolve().then(child);
        else child();
      }, 10);
      setTimeout(() => record('B'), 20);
      if (blocked) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 35);
      let deadline;
      try {
        await Promise.race([done, new Promise((_, reject) => { deadline = realTimeout(() => reject(new Error('startup callbacks did not complete')), 2000); })]);
      } finally { clearTimeout(deadline); }
      return events;
    } finally { clock.restore(); }
  };
  assert.deepEqual(await run(true, true, false), [['A', 10], ['B', 20], ['child', 20]]);
  for (const blocked of [false, true])
    for (const promiseChild of [false, true])
      assert.deepEqual(await run(blocked, false, promiseChild), [['A', 10], ['child', 15], ['B', 20]]);
});

test('startup interval drains invalidate native callbacks already queued before rearm and tape start', async () => {
  const epoch = 1700000000000;
  const clock = new ReplayClock(epoch, true);
  const originals = { ...clock.original };
  const wakes = [];
  clock.original.setTimeout = (callback) => { wakes.push(callback); return {}; };
  clock.original.clearTimeout = () => {};
  clock.install();
  const events = [];
  try {
    setInterval(() => events.push(['repeat', Date.now() - epoch]), 10);
    setTimeout(() => events.push(['B', Date.now() - epoch]), 25);
    await wakes[1](); // A later queued wake must drain repeats at 10 and 20.
    assert.deepEqual(events, [['repeat', 10], ['repeat', 20], ['B', 25]]);
    await wakes[0](); // Stale native repeat callback was already queued.
    assert.equal(events.length, 3);
    const pendingWake = wakes.at(-1);
    clock.start();
    await pendingWake(); // start() must invalidate already-queued wakes too.
    assert.equal(events.length, 3);
    await clock.advance(5);
    assert.deepEqual(events.at(-1), ['repeat', 5]);
  } finally {
    Object.assign(clock.original, originals);
    clock.restore();
  }
});

test('native watchdog heartbeat counts do not shift across the OKX EU tape horizon with startup audit cost', async () => {
  const epoch = 1700000000000;
  const replay = async (jitter, frozen) => {
    const originalNow = Date.now;
    let wall = epoch;
    const clock = new ReplayClock(epoch, frozen);
    clock.original.now = () => wall;
    clock.install();
    const sends = [];
    const actions = (now) => [
      { op: 'wsSend', socket: 1, text: 'ping' },
      { op: 'setTimer', timer: 7, at: now + 10_000 },
    ];
    // Exercise the actual host's absolute-deadline -> JS timer conversion and
    // native timer callback path, with the venue's 10-second watchdog cadence.
    const host = new EngineHost({ timer: (_id, now) => JSON.stringify(actions(now)) }, {
      now: () => Date.now(), setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer),
    });
    host.sockets.set(1, { send: (text) => sends.push(text), close() {} });
    try {
      host.run(JSON.stringify(actions(Date.now())));
      wall += jitter;
      clock.start();
      await clock.advance(19_953); // Actual last OKX EU XPerp public frame.
      return sends;
    } finally {
      host.dispose();
      clock.restore();
      Date.now = originalNow;
    }
  };
  // Mutation control: the former public clock behavior reproduces the CI
  // workload mismatch when audit startup exceeds the 47 ms tape-end margin.
  assert.deepEqual(await replay(0, false), ['ping', 'ping']);
  assert.deepEqual(await replay(99, false), ['ping', 'ping', 'ping']);
  for (const jitter of [0, 1, 47, 99])
    assert.deepEqual(await replay(jitter, frozenReplayStartup({}, false)), ['ping', 'ping']);
});

test('Kraken Spot replay registers books after open for either startup order and sends each subscription once', () => {
  const script = `
    import assert from 'node:assert/strict';
    import { KrakenSpotPublicWebsocket } from './packages/safe-cex/src/__tests__/reference/kraken/krakenspot.ws-public.ts';
    import { krakenspot } from './sdk-rs/harness/krakenspot.mjs';
    globalThis.WebSocket = { OPEN: 1 };
    const expectedBooks = ['BTC/USD', 'ETH/USD'].map(symbol => ({
      method: 'subscribe', params: { channel: 'book', symbol: [symbol], depth: 100 },
    }));
    const receipts = [];
    for (const order of ['registration-first', 'open-first']) {
      const sends = [];
      const markets = ['BTC/USD', 'ETH/USD'].map(symbol => ({ id: symbol, symbol }));
      const store = { loaded: { tickers: true }, markets, marketsBySymbol: Object.fromEntries(markets.map(m => [m.symbol, m])) };
      const exchange = { store, isDisposed: false };
      const socket = new KrakenSpotPublicWebsocket(exchange);
      exchange.publicWebsocket = socket;
      socket.ws = { readyState: 0, send: text => sends.push(JSON.parse(text)) };
      let registered = false;
      const registerWhenReady = () => {
        if (!registered && krakenspot.tsBooksReady(exchange, store)) {
          registered = true;
          for (const market of markets) socket.listenOrderBook(market.symbol, () => {});
        }
      };
      if (order === 'registration-first') {
        assert.equal(krakenspot.tsBooksReady(exchange, store), false);
        registerWhenReady();
        assert.equal(registered, false);
      }
      socket.ws.readyState = 1;
      socket.onOpen();
      registerWhenReady();
      assert.equal(registered, true);
      assert.deepEqual(sends.filter(message => message.params.channel === 'book'), expectedBooks);
      assert.equal(sends.filter(message => message.params.channel === 'ticker').length, 1);
      receipts.push(sends);
    }
    assert.deepEqual(receipts[0], receipts[1]);
    process.exit(0);
  `;
  const result = spawnSync(process.execPath, ['--import', 'tsx', '--input-type=module'], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)), input: script, encoding: 'utf8', timeout: 30_000,
  });
  assert.equal(result.status, 0, result.error?.stack ?? result.stderr ?? result.stdout);
});

test('Coinbase public book readiness fixes the seed/open boundary without shifting ticker polling or private readiness', () => {
  const script = `
    import assert from 'node:assert/strict';
    import { CoinbasePublicWebsocket } from './packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts';
    import { VENUES } from './sdk-rs/harness/venues.mjs';
    import { ReplayClock } from './sdk-rs/harness/tape.mjs';
    globalThis.WebSocket = { OPEN: 1 };
    const receipts = [];
    for (const name of ['coinbase', 'coinbasespot']) for (const order of ['registration-first', 'open-first']) {
      const sends = [];
      const store = { loaded: { markets: false, tickers: false }, markets: [{ symbol: 'BTC-USD' }, { symbol: 'ETH-USD' }] };
      const parent = { store, isDisposed: false, isPublicDataOnly: true };
      const socket = new CoinbasePublicWebsocket(parent);
      parent.publicWebsocket = socket;
      socket.ws = { readyState: 0, send: text => sends.push(JSON.parse(text)) };
      const ready = () => VENUES[name].tsBooksReady(parent, store);
      const clock = new ReplayClock(1700000000000, true);
      clock.install();
      try {
        let requests = 2; // Public markets and initial ticker seed.
        const poll = () => { requests++; setTimeout(poll, 1000); };
        setTimeout(poll, 1000);
        assert.equal(ready(), false);
        if (order === 'open-first') { socket.ws.readyState = 1; socket.onOpen(); }
        assert.equal(ready(), false);
        store.loaded.markets = true;
        assert.equal(ready(), false);
        store.loaded.tickers = true;
        if (order === 'registration-first') {
          assert.equal(ready(), false); // No 100 ms waitForConnected timer is created.
          socket.ws.readyState = 1;
          socket.onOpen();
        }
        assert.equal(ready(), true);
        for (const market of store.markets) socket.listenOrderBook(market.symbol, () => {});
        assert.deepEqual([...clock.timers.values()].map(timer => timer.delay), [1000]);
        clock.start();
        await clock.advance(7962); // Actual public fixture's terminal time.
        assert.equal(requests, 9);
        assert.deepEqual(sends, [
          { type: 'subscribe', product_ids: ['BTC-USD', 'ETH-USD'], channel: 'heartbeats' },
          { type: 'subscribe', product_ids: ['BTC-USD'], channel: 'level2' },
          { type: 'subscribe', product_ids: ['ETH-USD'], channel: 'level2' },
        ]);
        receipts.push({ requests, sends });
        parent.isPublicDataOnly = false;
        store.loaded = { markets: false, tickers: false };
        socket.ws.readyState = 0;
        assert.equal(ready(), true); // Existing private bootstrap is unchanged.
      } finally { clock.restore(); }
    }
    for (const receipt of receipts) assert.deepEqual(receipt, receipts[0]);
    process.exit(0);
  `;
  const result = spawnSync(process.execPath, ['--import', 'tsx', '--input-type=module'], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)), input: script, encoding: 'utf8', timeout: 30_000,
  });
  assert.equal(result.status, 0, result.error?.stack ?? result.stderr ?? result.stdout);
});
