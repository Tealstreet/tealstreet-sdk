import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { releasedHttpActions } from './managed-http-fixture.mjs';

const built = new URL('../pkg/node/safe_cex_wasm.js', import.meta.url);

test('WASM optional string and byte actions preserve UTF-8 validation and authenticated subscriptions', { skip: !existsSync(built) }, () => {
  const { WasmEngine } = createRequire(import.meta.url)('../pkg/node/safe_cex_wasm.js');
  const now = 1_700_000_000_000;
  const encoder = new TextEncoder();
  const actions = (json) => json ? JSON.parse(json) : [];
  // Documented Phemex PerpetualV2 products and 24-hour ticker shapes.
  const products = '{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}';
  const tickers = '{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}';
  const engines = [];
  const setup = () => {
    const engine = new WasmEngine('phemex', JSON.stringify({ key: 'test-key', secret: 'test-secret' }));
    engines.push(engine);
    const pending = releasedHttpActions(engine, engine.start(now), now);
    const reply = (path, body) => {
      const index = pending.findIndex((action) => action.op === 'http' && new URL(action.url).pathname === path);
      assert.ok(index >= 0, `pending request for ${path}`);
      const [{ request }] = pending.splice(index, 1);
      pending.push(...releasedHttpActions(engine, engine.httpStart(request, 200, '', now), now));
      pending.push(...releasedHttpActions(engine, engine.httpChunk(request, encoder.encode(body), now), now));
      pending.push(...releasedHttpActions(engine, engine.httpEnd(request, now), now));
    };
    reply('/public/products', products);
    const privateConnects = pending.filter((action) => action.op === 'wsConnect' && action.channel === 'private');
    assert.equal(privateConnects.length, 1, 'private authentication starts as soon as markets load');
    assert.ok(pending.some((action) => action.op === 'http' && action.url.includes('/g-accounts/accountPositions')));
    assert.ok(!pending.some((action) => action.op === 'wsConnect' && action.channel === 'public'));
    reply('/md/v3/ticker/24hr/all', tickers);
    assert.equal(pending.filter((action) => action.op === 'wsConnect' && action.channel === 'private').length, 1);
    const socket = privateConnects[0].socket;
    const auth = actions(engine.wsOpen(socket, now));
    assert.ok(auth.some((action) => action.op === 'wsSend' && JSON.parse(action.text).method === 'user.auth'));
    return { engine, socket };
  };
  try {
    const legacy = setup();
    const fast = setup();
    const bytes = setup();
    for (const note of ['', 'ASCII', 'é漢🦀', '\ud800', '\udc00', 'a'.repeat(1364) + '\ud800', '漢'.repeat(1366)]) {
      const quiet = `{"note":"${note}"}`;
      assert.equal(legacy.engine.wsMessage(legacy.socket, quiet, now), '');
      assert.equal(fast.engine.wsMessageActions(fast.socket, quiet, now), undefined);
      assert.equal(bytes.engine.wsMessageBytes(bytes.socket, encoder.encode(quiet), now), undefined);
    }
    // Invalid bytes must not authenticate or consume the auth transition.
    const invalidAck = new Uint8Array([
      ...encoder.encode('{"id":999,"error":null,"result":{"status":"success"},"note":"'),
      0xff,
      ...encoder.encode('"}'),
    ]);
    assert.equal(bytes.engine.wsMessageBytes(bytes.socket, invalidAck, now), undefined);
    const ack = '{"id":999,"error":null,"result":{"status":"success"},"note":"é漢🦀\ud800"}';
    const expected = legacy.engine.wsMessage(legacy.socket, ack, now);
    assert.ok(actions(expected).some((action) => action.op === 'wsSend' && JSON.parse(action.text).method === 'aop_p.subscribe'));
    assert.equal(fast.engine.wsMessageActions(fast.socket, ack, now), expected);
    assert.equal(bytes.engine.wsMessageBytes(bytes.socket, encoder.encode(ack), now), expected);
    assert.equal(legacy.engine.wsMessage(legacy.socket, '{}', now), '');
    assert.equal(fast.engine.wsMessageActions(fast.socket, '{}', now), undefined);
    assert.equal(bytes.engine.wsMessageBytes(bytes.socket, encoder.encode('{}'), now), undefined);
  } finally {
    for (const engine of engines) engine.free();
  }
});

test('WASM options parse errors do not echo secret-bearing malformed values', { skip: !existsSync(built) }, () => {
  const { WasmEngine } = createRequire(import.meta.url)('../pkg/node/safe_cex_wasm.js');
  assert.throws(() => new WasmEngine('binance', JSON.stringify({ key: 'sensitive-key', publicDataOnly: 'sensitive-secret' })),
    (error) => error.message === 'Invalid engine options');
});
