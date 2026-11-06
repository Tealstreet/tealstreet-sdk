// Fresh-host proof only: real browser bindings, EngineHost and the local WASM
// Stark provider, for both key forms through native and capsule setup.
import assert from 'node:assert/strict';
import { EngineHost } from '../hosts/wasm/js/host.mjs';

const STARK = '0x' + '0'.repeat(63) + '1';
const ETHEREUM = '11'.repeat(32);
const ACCOUNT = { paradexAddress: '0x0123', subkeyAddress: '0x456' };
// Independent oracle addresses for ETHEREUM (sdk-rs/venues/paradex/tests/sign-vectors.json).
const DERIVED = '0x164f1d17c7a66fea8cb5b0716ec5f36e61bb9d56aa42b3e562bb64f9ede55c8';
const MARKETS = { results: [{ symbol: 'BTC-USD-PERP', base_currency: 'BTC', quote_currency: 'USD', asset_kind: 'PERP',
  order_size_increment: '0.00001', price_tick_size: '0.1', min_notional: '10', max_order_size: '100',
  delta1_cross_margin_params: { imf_base: '0.02' } }] };
const TICKERS = { results: [{ symbol: 'BTC-USD-PERP', bid: '99', ask: '101', last_traded_price: '100',
  mark_price: '100.5', underlying_price: '100.2', price_change_rate_24h: '0.02', funding_rate: '0.0001',
  volume_24h: '200', open_interest: '50' }] };
const CONFIG = { starknet_chain_id: 'PRIVATE_SN_PARACLEAR_MAINNET', l1_chain_id: '1',
  paraclear_account_hash: '0x123', paraclear_account_proxy_hash: '0x456' };
const SIGNATURE = /^\["[1-9]\d*","[1-9]\d*"\]$/;

export async function checkParadexLocalSigning(WasmEngine) {
  for (const key of [STARK, ETHEREUM]) {
    for (const capsule of [false, true]) {
      let now = 1_800_000_000_000, timerId = 0, fixtureError;
      const timers = new Map(), sockets = new Set(), auth = [], writes = [];
      const options = { ...ACCOUNT, publicMode: { requested: false } };
      const engine = new WasmEngine('paradex', JSON.stringify(capsule ? options : { ...options, key }),
        capsule ? JSON.stringify({ primary: { secret: key } }) : undefined);
      const account = key === STARK ? ACCOUNT.paradexAddress : DERIVED;
      const response = (packet) => {
        const path = new URL(packet.url).pathname.replace(/^\/v1/, '');
        const headers = Object.fromEntries(Object.entries(packet.headers ?? {}));
        if (path === '/auth/0x456') {
          assert.equal(packet.method, 'POST');
          assert.equal(packet.route, 'direct');
          assert.equal(headers['PARADEX-STARKNET-ACCOUNT'], account);
          assert.match(headers['PARADEX-STARKNET-SIGNATURE'], SIGNATURE);
          assert.equal(Number(headers['PARADEX-SIGNATURE-EXPIRATION']) - Number(headers['PARADEX-TIMESTAMP']), 300);
          auth.push(headers);
          return { jwt_token: 'jwt' };
        }
        if (path === '/orders/batch' && packet.method === 'POST') {
          const legs = JSON.parse(packet.body);
          assert.equal(legs.length, 1);
          assert.match(legs[0].signature, SIGNATURE);
          assert.equal(legs[0].signature_timestamp, now);
          writes.push(legs[0]);
          return { orders: [{ id: '101', client_id: legs[0].client_id }] };
        }
        switch (path) {
          case '/markets': return MARKETS;
          case '/markets/summary': return TICKERS;
          case '/system/config': return CONFIG;
          case '/account/margin': return { configs: [] };
          case '/positions': return { results: [] };
          case '/account': return { total_collateral: '1000', free_collateral: '1000' };
          case '/orders': return { results: [] };
          default: assert.fail(`Unexpected Paradex request ${packet.method} ${path}`);
        }
      };
      const serve = (packet) => {
        try {
          const body = new TextEncoder().encode(JSON.stringify(response(packet)));
          return Promise.resolve({ status: 200, headers: '', body });
        } catch (error) { fixtureError = error; throw error; }
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
        request: serve,
        prepareHttp(packet) {
          return { bind(bound) { return { ready: () => !bound.signal.aborted, dispatch: () => serve(bound) }; } };
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
        host.start();
        await tick();
        if (fixtureError) throw fixtureError;
        assert.equal(host.dead, false);
        assert.equal(auth.length, 1, 'the sealed provider signs /auth');
        now += 1000;
        let terminal;
        const completion = host.command({ op: 'placeOrders', orders: [{ symbol: 'BTC-USD-PERP',
          type: 'limit', side: 'buy', price: 100, amount: 0.1, clientOrderId: 'caller' }] })
          .then(value => { terminal = { value }; }, error => { terminal = { error }; });
        await tick();
        if (fixtureError) throw fixtureError;
        assert.ok(terminal, 'the signed write must settle');
        await completion;
        if (terminal.error) throw terminal.error;
        assert.deepEqual(terminal.value, ['101']);
        assert.equal(writes.length, 1);
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
}
