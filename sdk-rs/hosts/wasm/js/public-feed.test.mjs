import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { Frame } from './wire.mjs';
import { releasedHttpActions } from './managed-http-fixture.mjs';

const built = new URL('../pkg/node/safe_cex_wasm.js', import.meta.url);
const enabled = { skip: !existsSync(built) };
const products = '{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}';
const tickers = '{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}';
const now = 1_700_000_000_000;
const actions = (json) => json ? JSON.parse(json) : [];
const construct = (options) => {
  const { WasmEngine } = createRequire(import.meta.url)('../pkg/node/safe_cex_wasm.js');
  return new WasmEngine('phemex', JSON.stringify(options));
};
const initialize = (engine, privateExpected = false) => {
  const pending = releasedHttpActions(engine, engine.start(now), now);
  const reply = (path, body) => {
    const index = pending.findIndex((action) => action.op === 'http' && new URL(action.url).pathname === path);
    assert.ok(index >= 0, `pending request for ${path}`);
    const [{ request }] = pending.splice(index, 1);
    pending.push(...releasedHttpActions(engine, engine.httpStart(request, 200, '', now), now));
    pending.push(...releasedHttpActions(engine, engine.httpChunk(request, new TextEncoder().encode(body), now), now));
    pending.push(...releasedHttpActions(engine, engine.httpEnd(request, now), now));
  };
  reply('/public/products', products);
  assert.equal(pending.filter((action) => action.op === 'wsConnect' && action.channel === 'private').length,
    Number(privateExpected), 'private authentication does not wait for the ticker seed');
  if (privateExpected) {
    assert.ok(pending.some((action) => action.op === 'http' && action.url.includes('/g-accounts/accountPositions')));
  }
  assert.ok(!pending.some((action) => action.op === 'wsConnect' && action.channel === 'public'));
  reply('/md/v3/ticker/24hr/all', tickers);
  return pending;
};

test('compiled socket policy keeps private authentication and suppresses public sockets', enabled, () => {
  for (const options of [{}, { privateOnly: true }, { publicFeed: { mode: 'disabled' } }, { publicFeed: { mode: 'shared' } }]) {
    const engine = construct({ key: 'test-key', secret: 'test-secret', ...options });
    try {
      const pending = initialize(engine, true);
      const connects = pending.filter((action) => action.op === 'wsConnect');
      const expectedPublic = !options.privateOnly && !options.publicFeed;
      assert.equal(connects.filter((action) => action.channel === 'public').length, Number(expectedPublic));
      const privateSocket = connects.find((action) => action.channel === 'private');
      assert.ok(privateSocket, 'private socket survives even at the same public URL');
      assert.ok(actions(engine.wsOpen(privateSocket.socket, now)).some((action) => action.op === 'wsSend' && JSON.parse(action.text).method === 'user.auth'));
      // A private close followed by every due reconnect must retain public suppression.
      const closed = actions(engine.wsClose(privateSocket.socket, 1006, now));
      const retries = closed.filter((action) => action.op === 'setTimer');
      for (const timer of retries) {
        const retried = actions(engine.timer(timer.timer, now + 100_000));
        if (!expectedPublic) assert.ok(!retried.some((action) => action.op === 'wsConnect' && action.channel === 'public'));
      }
    } finally { engine.free(); }
  }
});

test('compiled shared feed ingests public fields into native frames and snapshots only', enabled, () => {
  const engine = construct({ publicDataOnly: true, publicFeed: { mode: 'shared' } });
  try {
    assert.ok(!initialize(engine).some((action) => action.op === 'wsConnect'));
    engine.takeFrame(now);
    const command = {
      op: 'publicFeedUpdate',
      tickers: [{ symbol: 'UNKNOWN', bid: 1 }, { symbol: 'BTCUSDT', bid: 100, ask: 102, last: 101, mark: 101.5, index: 101.25, percentage: -2, openInterest: 7, fundingRate: -0.01, fundingTime: now, volume: 3, quoteVolume: 303 }],
      books: [{ symbol: 'BTCUSDT', bids: [[100, 2]], asks: [[102, 3]] }],
      candles: [{ symbol: 'BTCUSDT', interval: '1m', timestamp: now / 1000, open: 100, high: 102, low: 99, close: 101, volume: 3 }],
    };
    assert.equal(engine.command(JSON.stringify(command), now), '');
    const frame = new Frame(engine.takeFrame(now).buffer);
    assert.equal([...frame.books()][0].market, 0);
    assert.deepEqual([...([...frame.books()][0].bids)], [100, 2, 2]);
    assert.equal(frame.candles().count, 1);
    const snapshot = new Frame(engine.snapshotFrame(now).buffer);
    const table = snapshot.tickers();
    assert.equal(table.count, 1);
    assert.equal(snapshot.markets().symbol(table.market[0]), 'BTCUSDT');
    assert.equal(snapshot.markets().id(table.market[0]), 'BTC/USDT:USDT');
    assert.equal(table.bid[0], 100);
    assert.equal(table.ask[0], 102);
    assert.equal(table.percentage[0], -2);
    assert.equal(table.quoteVolume[0], 303);
    assert.equal([...snapshot.books()].length, 1);
    assert.throws(() => engine.command('{"op":"publicFeedUpdate","orders":[]}', now), /unknown field/);
    assert.throws(() => engine.command('{"op":"publicFeedUpdate","tickers":[{"symbol":"BTCUSDT","id":"foreign"}]}', now), /unknown field/);
    assert.throws(() => engine.command('{"op":"publicFeedUpdate","books":[{"symbol":"BTCUSDT","bids":[[-1,2]],"asks":[]}]}', now), /invalid public book/);
    assert.equal(new Frame(engine.snapshotFrame(now).buffer).tickers().bid[0], 100);
  } finally { engine.free(); }
  for (const mode of ['direct', 'disabled']) {
    const unshared = construct({ publicDataOnly: true, publicFeed: { mode } });
    try { assert.throws(() => unshared.command('{"op":"publicFeedUpdate"}', now), /requires shared/); }
    finally { unshared.free(); }
  }
  assert.throws(() => construct({ privateOnly: true, publicFeed: { mode: 'direct' } }), /privateOnly/);
});
