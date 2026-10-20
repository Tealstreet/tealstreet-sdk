import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deterministicReplay, frozenReplayStartup } from './polling.mjs';
import { ReplayClock } from './tape.mjs';
import { EngineHost } from '../hosts/wasm/js/host.mjs';

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
    import { KrakenSpotPublicWebsocket } from './packages/safe-cex/src/exchanges/kraken/krakenspot.ws-public.ts';
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
