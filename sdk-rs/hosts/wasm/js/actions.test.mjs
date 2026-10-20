import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';

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
    const reply = (request, body) => {
      engine.httpStart(request, 200, '', now);
      engine.httpChunk(request, encoder.encode(body), now);
      return actions(engine.httpEnd(request, now));
    };
    let pending = actions(engine.start(now));
    pending = reply(pending.find((action) => action.op === 'http').request, products);
    pending = reply(pending.find((action) => action.op === 'http').request, tickers);
    const socket = pending.filter((action) => action.op === 'wsConnect').at(-1).socket;
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
