// Fresh-host proof only: real browser bindings, EngineHost and local WASM provider.
import assert from 'node:assert/strict';
import { EngineHost } from '../hosts/wasm/js/host.mjs';

const scalar = '0x' + '0'.repeat(63) + '1';
const agent = '0x7e5f4552091a69125d5dfcb7b8c2659029395bdf';
const account = '0x2222222222222222222222222222222222222222';
const meta = [{ universe: [{ name: 'BTC', szDecimals: 5, maxLeverage: 50 }] },
  [{ midPx: '100', markPx: '100', oraclePx: '100', prevDayPx: '90' }]];
const spotMeta = [{ tokens: [{ name: 'USDC', index: 0, szDecimals: 8 },
  { name: 'BTC', index: 1, szDecimals: 5 }], universe: [{ name: '@4', index: 4, tokens: [1, 0] }] },
[{ midPx: '100', markPx: '100', prevDayPx: '90' }]];

export async function checkHyperliquidLocalSigning(WasmEngine, spot) {
  let previousNonce = 0;
  for (const capsule of [false, true]) {
    let now = 1_800_000_000_000, timerId = 0, sends = 0, raw = 0, fixtureError;
    const timers = new Map(), sockets = new Set(), writes = [], prepared = [];
    const options = { key: agent, applicationId: account, publicMode: { requested: false } };
    const name = spot ? 'hyperliquidspot' : 'hyperliquid';
    const engine = new WasmEngine(name, JSON.stringify(capsule ? options : { ...options, secret: scalar }),
      capsule ? JSON.stringify({ primary: { secret: scalar, secretEncoding: 'utf8', stableId: 'explicit-fixture' } }) : undefined);
    const response = (packet) => {
      sends++;
      const body = JSON.parse(packet.body);
      if (packet.url.endsWith('/exchange')) {
        writes.push(body);
        assert.equal(body.action.type, 'order');
        assert.equal(body.action.orders.length, 1);
        assert.deepEqual(body.action.orders[0], { a: spot ? 10004 : 0, b: true, p: '100', s: '0.1', r: false,
          t: { limit: { tif: 'Gtc' } }, c: body.action.orders[0].c });
        assert.match(body.action.orders[0].c, /^0x[0-9a-f]{32}$/);
        assert.match(body.signature.r, /^0x[0-9a-f]{64}$/);
        assert.match(body.signature.s, /^0x[0-9a-f]{64}$/);
        assert.ok([27, 28].includes(body.signature.v));
        assert.ok(body.nonce > previousNonce);
        previousNonce = body.nonce;
        return { status: 'ok', response: { type: 'order', data: { statuses: [{ resting: { oid: 101 } }] } } };
      }
      assert.ok(packet.url.endsWith('/info'));
      switch (body.type) {
        case 'metaAndAssetCtxs': return meta;
        case 'spotMetaAndAssetCtxs': return spotMeta;
        case 'perpDexs': return [null];
        case 'userAbstraction': return 'disabled';
        case 'userFees': return { userCrossRate: '0.00045', userAddRate: '0.00015' };
        case 'maxBuilderFee': return '30';
        case 'frontendOpenOrders': return [];
        case 'clearinghouseState': return { assetPositions: [], time: now, marginSummary: { accountValue: '100', totalMarginUsed: '0' } };
        case 'spotClearinghouseState': return { balances: [{ coin: 'USDC', total: '100', hold: '0' }], tokenToAvailableAfterMaintenance: [[0, '100']] };
        default: assert.fail(`Unexpected Hyperliquid read ${body.type}`);
      }
    };
    const transport = {
      now: () => now,
      setTimeout(fn, delay) { const id = ++timerId; timers.set(id, { fn, at: now + delay }); return id; },
      clearTimeout(id) { timers.delete(id); },
      connect(_url, handlers) {
        const socket = { send() {}, close() { sockets.delete(socket); } };
        sockets.add(socket);
        queueMicrotask(() => { if (sockets.has(socket)) handlers.onOpen(); });
        return socket;
      },
      request() { raw++; assert.fail('Legacy HTTP escaped the managed funnel'); },
      prepareHttp(packet) {
        if (packet.url.endsWith('/exchange')) assert.equal(packet.intent, 'primary');
        prepared.push(packet.url);
        return { bind(bound) { return { ready: () => !bound.signal.aborted,
          dispatch: () => {
            try { return Promise.resolve({ status: 200, body: new TextEncoder().encode(JSON.stringify(response(bound))) }); }
            catch (error) { fixtureError = error; throw error; }
          } }; } };
      },
    };
    const host = new EngineHost(engine, transport, {}, { ownsEngine: true });
    async function tick() {
      for (let i = 0; i < 160; i++) {
        await Promise.resolve();
        for (const [id, timer] of timers) {
          if (timer.at <= now) { timers.delete(id); timer.fn(); }
        }
      }
    }
    try {
      assert.equal(engine.requiresExternalSigner(), false);
      assert.equal(engine.addressSetupReady(), false);
      host.start();
      await tick();
      if (fixtureError) throw fixtureError;
      assert.equal(host.dead, false);
      assert.equal(engine.addressSetupReady(), true);
      assert.equal(writes.length, 0);
      now += 1000;
      let terminal;
      const completion = host.command({ op: 'placeOrders', orders: [{ symbol: spot ? 'BTC/USDC' : 'BTC-PERP',
        type: 'limit', side: 'buy', price: 100, amount: 0.1 }] })
        .then(value => { terminal = { value }; }, error => { terminal = { error }; });
      await tick();
      if (fixtureError) throw fixtureError;
      assert.ok(terminal, 'real managed command must settle');
      await completion;
      if (terminal.error) throw terminal.error;
      assert.deepEqual(terminal.value, ['101']);
      assert.equal(writes.length, 1);
      assert.equal(raw, 0);
      assert.ok(sends > 1 && prepared.length === sends);
      assert.equal(host.requests.size, 0);
      assert.equal(host.managedHttp.resources.size, 0);
      assert.equal(host.commands.size, 0);
    } finally {
      host.dispose();
    }
    assert.equal(timers.size, 0);
    assert.equal(sockets.size, 0);
  }
}
