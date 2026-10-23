import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import { Frame } from '../hosts/wasm/js/wire.mjs';
import { BaseExchange } from '../../packages/safe-cex/src/exchanges/base.ts';
import { RustEngineExchange } from '../../packages/safe-cex/src/exchanges/rust/rust.exchange.ts';
import { DefaultStore } from '../../packages/safe-cex/src/store/store.base.ts';
import { prepareCliOrder } from '../../packages/cli-core/src/order/prepareCliOrder.ts';
import { classifyMarket } from '../../packages/safe-cex/src/instrument-class/resolve.ts';

test('native Bybit discovery survives transfer and the real CLI TradFi preflight', () => {
  const require = createRequire(import.meta.url);
  const { WasmEngine } = require('../hosts/wasm/pkg/node/safe_cex_wasm.js');
  const engine = new WasmEngine('bybit', JSON.stringify({ publicDataOnly: true }));
  const labels = [
    ['SHOPUSDT', 'stock', 'equity', 'SHOP', 'US'],
    ['ARKKUSDT', 'ETF', 'etf', 'ARKK', 'US'],
    ['XAUUSDT', 'commodity', 'commodity', 'XAU', ''],
    ['EURUSDUSDT', 'forex', 'fx', 'EURUSD', ''],
    ['BTCUSDT', '', 'crypto', '', ''],
    ['AEONUSDT', 'innovation', 'crypto', '', ''],
    ['OLDUSDT', undefined, undefined, undefined, undefined],
    ['NEWUSDT', 'new_native_label', undefined, 'NEW', 'HK'],
  ];
  const instruments = labels.map(([symbol, symbolType, , underlyingTicker, marketRegion]) => ({
    symbol, symbolType, underlyingTicker, marketRegion,
    status: 'Trading', contractType: 'LinearPerpetual', baseCoin: symbol.slice(0, -4), quoteCoin: 'USDT', settleCoin: 'USDT', deliveryTime: '0',
    leverageFilter: { minLeverage: '1', maxLeverage: '100' },
    priceFilter: { minPrice: '0.01', tickSize: '0.01' },
    lotSizeFilter: { minOrderQty: '0.001', maxOrderQty: '1000', qtyStep: '0.001' },
  }));
  try {
    const queue = [];
    const collect = (json) => { if (json) queue.push(...JSON.parse(json)); };
    collect(engine.start(1000));
    let resources = 0;
    for (let step = 0; queue.length && step < 100; step++) {
      let action = queue.shift();
      // Managed reads reach the fake venue only after the engine's own claim.
      if (action.op === 'managedHttp') {
        if (action.phase === 'prepare') {
          collect(engine.httpPrepared(action.preparation, String(++resources), undefined, 1000));
          continue;
        }
        if (action.phase !== 'bind') continue;
        const released = JSON.parse(engine.httpClaim(action.permit, 1000)).status === 'released';
        collect(engine.prepareDispatch(1000));
        if (!released) continue;
        action = { ...action.packet, op: 'http' };
      }
      if (action.op !== 'http') continue;
      const url = new URL(action.url);
      let list;
      if (url.pathname === '/v5/market/instruments-info') list = url.searchParams.get('status') === 'PreLaunch' ? [] : instruments;
      else if (url.pathname === '/v5/market/tickers') list = instruments.map((row) => ({ symbol: row.symbol, lastPrice: '100', markPrice: '100', bid1Price: '99', ask1Price: '101' }));
      else assert.fail(`Unexpected native discovery request ${action.url}`);
      const body = JSON.stringify({ retCode: 0, result: { category: 'linear', list, nextPageCursor: '' } });
      collect(engine.httpStart(action.request, 200, '', 1000));
      collect(engine.httpChunk(action.request, new TextEncoder().encode(body), 1000));
      collect(engine.httpEnd(action.request, 1000));
    }
    const bytes = engine.snapshotFrame(1000).slice();
    const source = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    const transferred = structuredClone(source, { transfer: [source] });
    assert.equal(source.byteLength, 0);
    const frame = new Frame(transferred);
    assert.ok(frame.instrumentClassifications(), 'Native instrument classifications must be present');
    assert.equal(frame.instrumentClassifications().count, labels.length);
    const options = { key: '', secret: '', publicDataOnly: true };
    const worker = new RustEngineExchange('bybit', options, new DefaultStore());
    const main = new BaseExchange('bybit', options, new DefaultStore());
    worker.onFrame = (buffer) => main.applyWorkerFrame(structuredClone(buffer, { transfer: [buffer] }));
    try {
      worker.receiveFrame(transferred);
      assert.equal(transferred.byteLength, 0);
      for (const store of [worker.store, main.store]) {
      assert.equal(store.markets.length, labels.length);
      for (const [symbol, label, assetClass, ticker, region] of labels) {
        const market = store.marketsBySymbol[symbol];
        assert.equal(market.info.symbolType, label);
        assert.equal(market.info.underlyingTicker, ticker);
        assert.equal(market.info.marketRegion, region);
        const classification = classifyMarket('bybit', market);
        assert.equal(classification.kind, assetClass ? 'classified' : 'unknown');
        if (assetClass) assert.equal(classification.assetClass, assetClass);
        const order = () => prepareCliOrder({ name: 'bybit', store }, { symbol, side: 'buy', type: 'market', amount: 1 });
        if (assetClass && assetClass !== 'crypto') assert.throws(order, /may require an agreement/);
        else assert.equal(order().symbol, symbol);
      }
      }
    } finally {
      worker.dispose();
      main.dispose();
    }
  } finally {
    engine.free();
  }
});
