// Real Wasm host + scripted Binance transport. No network primitives are used.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadEngine } from './engine.mjs';
import { prepareFetch } from '../hosts/wasm/js/managed-http.mjs';
import { runLiveSmoke, fencedTransport } from './live-smoke.mjs';
import { liveSmokeProfile } from './live-smoke-venues.mjs';
import { venueOf } from './venues.mjs';
import { assertLiveSmokeComplete } from './live-smoke-proof.mjs';
import { expectedCheckpointPlan } from './tape.mjs';
import { checkpointCoverageErrors, snapshotCoverageErrors } from './snapshot-compare.mjs';

function fakeBinance({ startupOrders = [], losePlacement = false, failAmend = false, delayPlacementMs = 0, unknownLeverage = false,
  lyingCancel = false, staleLeverage = false, minimumNotional = 50 } = {}) {
  const calls = [], sockets = [], orders = new Map(startupOrders.map((o) => [String(o.orderId), o]));
  let next = 1000;
  let remoteLeverage = 20;
  const notify = (row, status) => {
    setImmediate(() => {
      const event = { e: 'ORDER_TRADE_UPDATE', E: Date.now(), T: Date.now(), o: {
        s: row.symbol, c: row.clientOrderId, S: 'BUY', o: 'LIMIT', f: row.timeInForce, q: row.origQty,
        p: row.price, ap: '0', sp: '0', x: status === 'NEW' ? 'NEW' : 'CANCELED', X: status,
        i: row.orderId, l: '0', z: '0', L: '0', N: 'USDT', n: '0', T: Date.now(), t: 0,
        b: '0', a: '0', m: false, R: false, wt: 'CONTRACT_PRICE', ot: 'LIMIT', ps: 'BOTH', cp: false, rp: '0',
      } };
      for (const socket of sockets.filter((s) => s.url.includes('listenKey'))) socket.handlers.onMessage(JSON.stringify(event));
    });
  };
  const symbol = { symbol: 'BTCUSDT', status: 'TRADING', baseAsset: 'BTC', quoteAsset: 'USDT', marginAsset: 'USDT', contractType: 'PERPETUAL',
    filters: [{ filterType: 'PRICE_FILTER', minPrice: '0.1', maxPrice: '10000000', tickSize: '0.1' },
      { filterType: 'LOT_SIZE', minQty: '0.001', maxQty: '1000', stepSize: '0.001' },
      { filterType: 'MARKET_LOT_SIZE', minQty: '0.001', maxQty: '1000', stepSize: '0.001' },
      { filterType: 'MIN_NOTIONAL', notional: String(minimumNotional) }] };
  const transport = {
    prepareHttp(request) {
      return prepareFetch(request, (url, init) => {
        const response = this.request({ ...request, ...init, url });
        return Promise.resolve(response).then((value) => new Response(value.body, { status: value.status }));
      });
    },
    now: () => Date.now(), webSocketHeaders: true,
    setTimeout, clearTimeout,
    connect(url, handlers) {
      const socket = { url, handlers }; sockets.push(socket);
      setImmediate(() => handlers.onOpen());
      return { close() {}, send(text) {
        const message = JSON.parse(text);
        for (const topic of message.params ?? []) if (topic.endsWith('@depth')) setImmediate(() => handlers.onMessage(JSON.stringify({
          e: 'depthUpdate', E: Date.now(), T: Date.now(), s: 'BTCUSDT', U: 100, u: 101, pu: 99,
          b: [['99999.9', '1']], a: [['100000', '1']],
        })));
      } };
    },
    async request(request) {
      calls.push(request);
      const url = new URL(request.url), p = url.searchParams, path = url.pathname;
      let body;
      if (path.endsWith('/exchangeInfo')) body = { timezone: 'UTC', symbols: [symbol] };
      else if (path.endsWith('/leverageBracket')) body = [{ symbol: 'BTCUSDT', brackets: [{ initialLeverage: 125, notionalCap: 1000000, notionalFloor: 0, maintMarginRatio: 0.004, cum: 0 }] }];
      else if (path.endsWith('/ticker/24hr')) body = [{ symbol: 'BTCUSDT', lastPrice: '100000', priceChangePercent: '0', volume: '100', quoteVolume: '10000000' }];
      else if (path.endsWith('/ticker/bookTicker')) body = [{ symbol: 'BTCUSDT', bidPrice: '99999.9', askPrice: '100000' }];
      else if (path.endsWith('/premiumIndex')) body = [{ symbol: 'BTCUSDT', markPrice: '100000', indexPrice: '100000', lastFundingRate: '0', nextFundingTime: Date.now() + 1000000 }];
      else if (path.endsWith('/time')) body = { serverTime: Date.now() };
      else if (path.endsWith('/listenKey')) body = { listenKey: 'fake-session-token-that-must-be-redacted' };
      else if (path.endsWith('/positionSide/dual')) body = { dualSidePosition: false };
      else if (path.endsWith('/positionRisk')) body = [];
      else if (path.endsWith('/symbolConfig')) body = unknownLeverage ? [] : [{ symbol: 'BTCUSDT', marginType: 'CROSSED', leverage: remoteLeverage, isAutoAddMargin: false, maxNotionalValue: '1000000' }];
      else if (path.endsWith('/account')) body = { availableBalance: '5000', totalWalletBalance: '5000', totalUnrealizedProfit: '0',
        assets: [{ asset: 'USDT', walletBalance: '5000', availableBalance: '5000', initialMargin: '0', unrealizedProfit: '0' }], positions: [] };
      else if (path.endsWith('/openOrders')) body = [...orders.values()];
      else if (path.endsWith('/openAlgoOrders')) body = [];
      else if (path.endsWith('/depth')) body = { lastUpdateId: 100, E: Date.now(), T: Date.now(), bids: [['99999.9', '1']], asks: [['100000', '1']] };
      else if (path.endsWith('/order') && request.method === 'POST') {
        if (failAmend && next > 1000) return { status: 400, body: new TextEncoder().encode('{"code":-1013,"msg":"fake amendment failure"}') };
        const row = { symbol: p.get('symbol'), orderId: ++next, status: 'NEW', clientOrderId: p.get('newClientOrderId'),
          price: p.get('price'), origQty: p.get('quantity'), executedQty: '0', avgPrice: '0',
          type: 'LIMIT', origType: 'LIMIT', side: 'BUY', positionSide: 'BOTH', reduceOnly: false, closePosition: false,
          timeInForce: p.get('timeInForce'), time: Date.now(), updateTime: Date.now() };
        orders.set(String(row.orderId), row); notify(row, 'NEW'); body = row;
        if (delayPlacementMs) await new Promise((resolve) => setTimeout(resolve, delayPlacementMs));
        if (losePlacement) throw new Error('fake lost acknowledgement');
      } else if (path.endsWith('/order') && request.method === 'DELETE') {
        const row = orders.get(p.get('orderId'));
        if (!row) return { status: 400, body: new TextEncoder().encode('{"code":-2011,"msg":"Unknown order"}') };
        if (!lyingCancel) orders.delete(String(row.orderId));
        if (staleLeverage) remoteLeverage = 7;
        if (!lyingCancel) notify(row, 'CANCELED'); body = { ...row, status: 'CANCELED' };
      } else if (path.endsWith('/allOpenOrders') || path.endsWith('/algoOpenOrders')) {
        if (!lyingCancel) for (const row of orders.values()) notify(row, 'CANCELED');
        if (!lyingCancel) orders.clear(); body = { code: 200, msg: 'success' };
      } else if (path.endsWith('/leverage')) {
        if (!staleLeverage) remoteLeverage = Number(p.get('leverage'));
        body = { symbol: 'BTCUSDT', leverage: Number(p.get('leverage')), maxNotionalValue: '1000000' };
      }
      else throw new Error(`Unscripted fake request: ${request.method} ${path}`);
      return { status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify(body)) };
    },
  };
  return { transport, calls, orders };
}

const { WasmEngine } = await loadEngine();
const profile = liveSmokeProfile('binance', 'testnet', {
  RUST_LIVE_SMOKE_BINANCE_TESTNET_KEY: 'fake-live-key-do-not-persist', RUST_LIVE_SMOKE_BINANCE_TESTNET_SECRET: 'fake-live-secret-do-not-persist',
});
const run = async (fake, records, signal, Engine = WasmEngine) => runLiveSmoke({ venue: venueOf('binance'), profile,
  transport: fencedTransport(fake.transport, profile), write: (row) => records.push(row), symbol: 'BTCUSDT', timeoutMs: 2000, WasmEngine: Engine, signal });

test('real Wasm scripted lifecycle replays every command and protocol through TS and Rust', async () => {
  const fake = fakeBinance(), records = [];
  const result = await run(fake, records);
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.equal(result.cleanupComplete, true);
  assert.equal(fake.orders.size, 0);
  const placements = fake.calls.filter((r) => r.method === 'POST' && new URL(r.url).pathname.endsWith('/order'));
  assert.ok(placements.length >= 1);
  for (const request of placements) {
    const params = new URL(request.url).searchParams;
    assert.equal(params.get('timeInForce'), 'GTX');
    assert.ok(Number(params.get('quantity')) * Number(params.get('price')) <= 100);
  }
  assert.deepEqual(records.filter((r) => r.type === 'command').map((r) => r.op), ['placeOrders', 'amendOrder', 'cancelOrders', 'cancelSymbolOrders', 'setLeverage', 'setLeverage']);
  assert.deepEqual(fake.calls.filter((r) => new URL(r.url).pathname.endsWith('/leverage')).map((r) => new URL(r.url).searchParams.get('leverage')), ['19', '20']);
  assert.equal(records.find((r) => r.type === 'command' && r.op === 'cancelSymbolOrders').symbol, 'BTCUSDT');
  const serialized = records.map((r) => JSON.stringify(r)).join('\n') + '\n';
  for (const secret of [...Object.values(profile.secrets), 'fake-session-token-that-must-be-redacted']) assert.equal(serialized.includes(secret), false);
  const directory = mkdtempSync(join(tmpdir(), 'rust-live-smoke-'));
  try {
    const tape = join(directory, 'private.jsonl'); writeFileSync(tape, serialized, { mode: 0o600 });
    // A fresh capture has random client IDs and wall timestamps. Its job here
    // is lifecycle and transport evidence, not a parity claim. CI separately
    // runs the full strict financial gate on the checked-in, byte-stable
    // fixtures/binance-scripted-lifecycle-private.jsonl case, whose genuine
    // divergences can only be declared by its exact SHA in the single manifest.
    const proof = assertLiveSmokeComplete(records);
    const venue = venueOf('binance');
    for (const impl of ['ts', 'rust']) {
      const out = join(directory, `${impl}.json`);
      const replay = spawnSync('corepack', ['yarn', '-s', 'rust:replay', '--impl', impl,
        '--venue', 'binance', '--tape', tape, '--books', proof.symbol, '--out', out],
      { encoding: 'utf8', timeout: 120000 });
      assert.equal(replay.status, 0, `${impl}: ${replay.stdout}\n${replay.stderr}`);
      const output = JSON.parse(readFileSync(out, 'utf8'));
      assert.equal(output.commands.length, proof.commands.length, `${impl}: every lifecycle command completed`);
      for (const [index, command] of output.commands.entries()) {
        assert.equal(Object.hasOwn(command, 'ok'), true, `${impl}: ${proof.commands[index]} failed: ${JSON.stringify(command)}`);
        assert.equal(Object.hasOwn(command, 'error'), false, `${impl}: ${proof.commands[index]} rejected`);
      }
      for (const field of ['unserved', 'missingWrites', 'unusedWrites', 'invalidSignatures'])
        assert.equal(output[field], 0, `${impl}: ${field}`);
      assert.deepEqual(output.protocolErrors, [], `${impl}: protocol audit`);
      assert.deepEqual(snapshotCoverageErrors(output, `${impl} final`, { isPrivate: true }), []);
      assert.deepEqual(checkpointCoverageErrors(output.checkpoints, expectedCheckpointPlan(records, venue, impl), impl, { isPrivate: true }), []);
      assert(output.markets.length > 0 && output.tickers.length > 0, `${impl}: public seed present`);
      assert(Object.hasOwn(output.books, proof.symbol), `${impl}: production book egress present`);
    }
    if (process.env.RUST_LIVE_SMOKE_FAKE_TAPE_OUT) writeFileSync(process.env.RUST_LIVE_SMOKE_FAKE_TAPE_OUT, readFileSync(tape), { mode: 0o600 });
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('lost placement acknowledgement never resends placement and cancels possible order', async () => {
  const fake = fakeBinance({ losePlacement: true }), records = [];
  const result = await run(fake, records);
  assert.equal(result.ok, false);
  assert.equal(result.stage, 'place');
  assert.equal(fake.calls.filter((r) => r.method === 'POST' && new URL(r.url).pathname.endsWith('/order')).length, 1);
  assert.equal(fake.orders.size, 0);
  assert.equal(result.cleanupComplete, true);
});

test('amendment failure cleans the account and preserves failing stage', async () => {
  const fake = fakeBinance({ failAmend: true }), records = [];
  const result = await run(fake, records);
  assert.equal(result.ok, false);
  assert.equal(result.stage, 'amend');
  assert.equal(fake.orders.size, 0);
  assert.equal(result.cleanupComplete, true);
});

test('nonempty private snapshot refuses before placing or canceling anything', async () => {
  const fake = fakeBinance({ startupOrders: [{ symbol: 'BTCUSDT', orderId: 77, status: 'NEW', clientOrderId: 'preexisting',
    side: 'BUY', type: 'LIMIT', origType: 'LIMIT', price: '50000', origQty: '0.001', executedQty: '0', reduceOnly: false, positionSide: 'BOTH', timeInForce: 'GTC' }] });
  const result = await run(fake, []);
  assert.equal(result.ok, false);
  assert.equal(result.stage, 'account-snapshot');
  assert.equal(fake.calls.filter((r) => /\/(order|allOpenOrders|algoOpenOrders)$/.test(new URL(r.url).pathname)).length, 0);
});

test('unknown original leverage refuses before trading', async () => {
  const fake = fakeBinance({ unknownLeverage: true });
  const result = await run(fake, []);
  assert.equal(result.ok, false);
  assert.equal(result.stage, 'account-snapshot');
  assert.equal(result.commands.length, 0);
});

test('interruption drains a late placement acknowledgement before cleanup', async () => {
  const fake = fakeBinance({ delayPlacementMs: 150 });
  const controller = new AbortController();
  const interval = setInterval(() => {
    if (fake.orders.size) controller.abort();
  }, 5);
  try {
    const result = await run(fake, [], controller.signal);
    assert.equal(result.ok, false);
    assert.equal(result.stage, 'place');
    assert.equal(result.cleanupComplete, true);
    assert.equal(fake.orders.size, 0);
    assert.equal(fake.calls.filter((r) => r.method === 'POST' && new URL(r.url).pathname.endsWith('/order')).length, 1);
  } finally { clearInterval(interval); }
});

test('fresh startup rejects falsely optimistic cancellation acknowledgements', async () => {
  const fake = fakeBinance({ lyingCancel: true }), records = [];
  const result = await run(fake, records);
  assert.equal(result.ok, false);
  assert.equal(result.stage, 'verify-readback');
  assert.equal(result.cleanupComplete, false);
  assert.equal(records.find((r) => r.type === 'live-smoke-verification').ok, false);
});

test('fresh settings read rejects a stale locally successful leverage restore', async () => {
  const fake = fakeBinance({ staleLeverage: true }), records = [];
  const result = await run(fake, records);
  assert.equal(result.ok, false);
  assert.equal(result.stage, 'verify-readback');
  assert.equal(result.cleanupComplete, false);
  assert.equal(result.authoritative, false);
});

test('a fill first exposed by the final flush overrides lifecycle success', async () => {
  class FinalFlushEngine extends WasmEngine {
    httpEnd(id, now) {
      const actions = super.httpEnd(id, now);
      for (const action of JSON.parse(actions || '[]')) if (action.op === 'wsConnect' && action.channel === 'private') this.privateSocket = action.socket;
      return actions;
    }
    command(text, now) {
      const command = JSON.parse(text);
      if (command.op === 'setLeverage' && command.leverage === 20) this.injectFill = true;
      return super.command(text, now);
    }
    takeFrame(now, marketOnly) {
      if (this.injectFill) {
        this.injectFill = false;
        super.wsMessage(this.privateSocket, JSON.stringify({ e: 'ORDER_TRADE_UPDATE', E: now, T: now, o: {
          s: 'BTCUSDT', c: 'fake-filled-order', S: 'BUY', o: 'LIMIT', ot: 'LIMIT', f: 'GTC', q: '0.002', p: '89000', ap: '89000',
          sp: '0', x: 'TRADE', X: 'FILLED', i: 1002, l: '0.002', z: '0.002', L: '89000', n: '0', N: 'USDT', T: now,
          t: 999, b: '0', a: '0', m: true, R: false, wt: 'CONTRACT_PRICE', ps: 'BOTH', cp: false, rp: '0',
        } }), now);
      }
      return super.takeFrame(now, marketOnly);
    }
  }
  const result = await run(fakeBinance(), [], undefined, FinalFlushEngine);
  assert.equal(result.ok, false);
  assert.equal(result.cleanupComplete, false);
  assert.equal(result.stage, 'unexpected-execution');
});


test('an oversized venue minimum fails before every account mutation', async () => {
  const fake = fakeBinance({ minimumNotional: 100 }), records = [];
  const result = await run(fake, records);
  assert.equal(result.ok, false);
  assert.equal(result.reason, 'notional-cap-exceeded');
  assert.deepEqual(result.commands, []);
  assert.equal(fake.orders.size, 0);
  assert.equal(fake.calls.filter((r) => r.method !== 'GET' &&
    !new URL(r.url).pathname.endsWith('/listenKey')).length, 0);
});
