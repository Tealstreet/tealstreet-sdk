import { test } from 'node:test';
import assert from 'node:assert/strict';
import { smokeOrder, LIVE_SMOKE_MAX_NOTIONAL } from './live-smoke.mjs';
import { MARKET_FLAG } from '../hosts/wasm/js/wire.mjs';
import { venueMarketRules, LIVE_SMOKE_RULE_SOURCES, LIVE_SMOKE_UNKNOWN_RULES } from './live-smoke-rules.mjs';

const market = { id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT',
  minAmount: 0.001, precisionAmount: 0.001, maxAmount: 1000 };
const http = (url, body, extra = {}) => ({ type: 'http', method: 'GET', status: 200, url,
  body: JSON.stringify(body), ...extra });
const filters = [
  { filterType: 'LOT_SIZE', minQty: '0.001', maxQty: '100', stepSize: '0.001' },
  { filterType: 'MIN_NOTIONAL', notional: '5' },
  { filterType: 'PERCENT_PRICE', multiplierDown: '0.85' },
];

// Small documented discovery examples, independent of the Rust mappers. Values
// exercise units; the source URL for each schema is exported beside the parser.
const fixtures = [
  ['binance', 'binance', market,
    http('https://demo-fapi.binance.com/fapi/v1/exchangeInfo', { symbols: [{ symbol: 'BTCUSDT', filters }] }), 5, 0.001, 0.001, 1],
  ['asterdex', 'asterdex', market,
    http('https://fapi.asterdex-testnet.com/fapi/v1/exchangeInfo', { symbols: [{ symbol: 'BTCUSDT', filters }] }), 5, 0.001, 0.001, 1],
  ['bybit', 'bybit', market,
    http('https://api-testnet.bybit.com/v5/market/instruments-info?category=linear', { retCode: 0, result: { category: 'linear', list: [{ symbol: 'BTCUSDT', lotSizeFilter: { minNotionalValue: '5', minOrderQty: '0.001', qtyStep: '0.001', maxOrderQty: '100' } }] } }), 5, 0.001, 0.001, 1],
  ['bitget', 'bitget', market,
    http('https://api.bitget.com/api/v2/mix/market/contracts?productType=susdt-futures', { code: '00000', data: [{ symbol: 'BTCUSDT', minTradeNum: '0.001', sizeMultiplier: '0.001', minTradeUSDT: '5', maxOrderQty: '100' }] }), 5, 0.001, 0.001, 1],
  ['bitrue', 'bitrue', market,
    http('https://fapi.bitrue.com/fapi/v1/contracts', [{ symbol: 'BTCUSDT', multiplier: 0.001, minOrderVolume: 1, minOrderMoney: 5, maxLimitVolume: 1000 }]), 5, 0.001, 0.001, 1],
  ['bingx', 'bingx', market,
    http('https://open-api.bingx.com/openApi/swap/v2/quote/contracts', { code: 0, data: [{ symbol: 'BTCUSDT', tradeMinQuantity: 0.001, tradeMinUSDT: 2, quantityPrecision: 3 }] }), 2, 0.001, 0.001, 1],
  ['orderly', 'orderly', { ...market, symbol: 'BTCUSDC', quote: 'USDC' },
    http('https://testnet-api-evm.orderly.org/v1/public/info', { success: true, data: { rows: [{ symbol: 'PERP_BTC_USDC', base_min: 0.00001, base_tick: 0.00001, base_max: 20, min_notional: 1 }] } }), 1, 0.00001, 0.00001, 1],
  ['woo', 'woo', market,
    http('https://api.staging.woox.io/v1/public/info', { success: true, rows: [{ symbol: 'PERP_BTC_USDT', base_min: 0.0001, base_tick: 0.0001, base_max: 20, min_notional: 0.02 }] }), 0.02, 0.0001, 0.0001, 1],
  ['lighter', 'lighter', { ...market, id: '1', symbol: 'BTC' },
    http('https://testnet-api.lighter.xyz/api/v1/orderBookDetails', { code: 200, order_book_details: [{ market_id: 1, symbol: 'BTC', min_base_amount: '0.001', min_quote_amount: '1', supported_size_decimals: 3 }] }), 1, 0.001, 0.001, 1],
  ['paradex', 'paradex', { ...market, symbol: 'BTC-USD-PERP' },
    http('https://api.testnet.paradex.trade/v1/markets', { results: [{ symbol: 'BTC-USD-PERP', min_notional: '10', order_size_increment: '0.001' }] }), 10, 0.001, 0.001, 1],
  ['hyperliquid', 'hyperliquid', { ...market, id: 'BTC', symbol: 'BTC-PERP' },
    http('https://api.hyperliquid-testnet.xyz/info', [{ universe: [{ name: 'BTC', szDecimals: 5 }] }, []], { method: 'POST', requestBody: '{"type":"metaAndAssetCtxs"}' }), 10, 0.00001, 0.00001, 1],
  ['hyperliquidspot', 'hyperliquidspot', { ...market, id: '@142', symbol: 'UBTC/USDC', base: 'UBTC', quote: 'USDC' },
    http('https://api.hyperliquid.xyz/info', [{ tokens: [{ index: 0, name: 'USDC', szDecimals: 8 }, { index: 197, name: 'UBTC', szDecimals: 5 }],
      universe: [{ name: '@142', index: 142, tokens: [197, 0] }] }, []], { method: 'POST', requestBody: '{"type":"spotMetaAndAssetCtxs"}' }), 10, 0.00001, 0.00001, 1],
  ['okx', 'okx', { ...market, id: 'BTC-USDT-SWAP' },
    http('https://openapi.okx.com/api/v5/public/instruments?instType=SWAP', { code: '0', data: [{ instId: 'BTC-USDT-SWAP', ctType: 'linear', ctVal: '0.01', minSz: '0.1', lotSz: '0.1', maxLmtSz: '1000' }] }), 0, 0.1, 0.1, 0.01],
  ['blofin', 'blofin', { ...market, id: 'BTC-USDT' },
    http('https://demo-trading-openapi.blofin.com/api/v1/market/instruments', { code: '0', data: [{ instId: 'BTC-USDT', contractType: 'linear', contractValue: '0.001', minSize: '0.1', lotSize: '0.1', maxLimitSize: '10000' }] }), 0, 0.1, 0.1, 0.001],
  ['apexomni', 'apexomni', { ...market, id: 'BTC-USDT' },
    http('https://testnet.omni.apex.exchange/api/v3/symbols', { code: 0, data: { contractConfig: { perpetualContract: [{ symbol: 'BTC-USDT', crossSymbolName: 'BTCUSDT', minOrderSize: '0.001', stepSize: '0.001', maxOrderSize: '50' }] } } }), 0, 0.001, 0.001, 1],
  ['backpack', 'backpack', { ...market, symbol: 'BTC_USDC_PERP' },
    http('https://api.backpack.exchange/api/v1/markets', [{ symbol: 'BTC_USDC_PERP', filters: { quantity: { minQuantity: '0.001', maxQuantity: '1000', stepSize: '0.001' } } }]), 0, 0.001, 0.001, 1],
  ['kraken', 'kraken', { ...market, symbol: 'PF_XBTUSD' },
    http('https://demo-futures.kraken.com/derivatives/api/v3/instruments', { result: 'success', instruments: [{ symbol: 'PF_XBTUSD', contractValueTradePrecision: 4, contractSize: 1 }] }), 0, 0.0001, 0.0001, 1],
  ['kucoin', 'kucoin', { ...market, id: 'XBTUSDTM', symbol: 'BTCUSDT' },
    http('https://api-futures.kucoin.com/api/v1/contracts/active', { code: '200000', data: [{ symbol: 'XBTUSDTM', lotSize: 1, multiplier: 0.001, isInverse: false, maxOrderQty: 1000000 }] }), 0, 1, 1, 0.001],
  ['mexc', 'mexc', { ...market, id: 'BTC_USDT' },
    http('https://api.mexc.com/api/v1/contract/detail', { success: true, code: 0, data: [{ symbol: 'BTC_USDT', minVol: 1, volUnit: 1, contractSize: 0.0001, maxVol: 5000000 }] }), 0, 1, 1, 0.0001],
  ['weex', 'weex', market,
    http('https://api-contract.weex.com/capi/v3/market/exchangeInfo', { symbols: [{ symbol: 'BTCUSDT', minOrderSize: 0.0001, maxOrderSize: 10000 }] }), 0, 0.0001, 0.0001, 1],
  ['polymarket', 'polymarket', { ...market, id: '12345678901234567890123456789', symbol: 'event-Yes' },
    http('https://clob.polymarket.com/book?token_id=12345678901234567890123456789', { asset_id: '12345678901234567890123456789', min_order_size: '5' }), 0, 5, 0.01, 1],
  ['coinbase', 'coinbasespot', { ...market, id: 'BTC-USDC', symbol: 'BTC-USDC' },
    http('https://api.coinbase.com/api/v3/brokerage/products', { products: [{ product_id: 'BTC-USDC', product_type: 'SPOT', base_min_size: '0.000001', base_increment: '0.000001', base_max_size: '100', quote_min_size: '1' }] }), 1, 0.000001, 0.000001, 1],
];

for (const [family, venue, selected, record, notional, minimum, step, scale] of fixtures) {
  test(`${family} documented discovery establishes monetary and placement-unit minima`, () => {
    assert.match(LIVE_SMOKE_RULE_SOURCES[family], /^https:\/\//);
    const result = venueMarketRules({ rust: venue }, [record], selected);
    assert.equal(result.notionalKnown, true);
    assert.equal(result.minNotional, notional);
    assert.equal(result.minimumAmount, minimum);
    assert.equal(result.amountStep, step);
    assert.equal(result.notionalPerAmount, scale);
    assert.equal(result.quoteQuantity, false);
    if (family === 'binance' || family === 'asterdex') assert.equal(result.lowerMultiplier, 0.85);
    const body = JSON.parse(record.body);
    assert.deepEqual(venueMarketRules(venue, [{ ...record, status: 500 }], selected), { notionalKnown: false, minNotional: 0 });
    assert.deepEqual(venueMarketRules(venue, [{ ...record, body: '{' }], selected), { notionalKnown: false, minNotional: 0 });
    assert.deepEqual(venueMarketRules(venue, [{ ...record, url: record.url.replace(new URL(record.url).hostname, 'evil.example') }], selected), { notionalKnown: false, minNotional: 0 });
    assert.deepEqual(JSON.parse(record.body), body, 'no mutation of discovery');
  });
}

test('coverage census includes every enabled rule family, unsupported names fail closed', () => {
  assert.equal(fixtures.length, 22);
  assert.deepEqual([...new Set(fixtures.map(([family]) => family))].sort(), Object.keys(LIVE_SMOKE_RULE_SOURCES).sort());
  for (const venue of [...Object.keys(LIVE_SMOKE_UNKNOWN_RULES), 'future-unknown-venue'])
    assert.deepEqual(venueMarketRules(venue, fixtures.map((fixture) => fixture[3]), market), { notionalKnown: false, minNotional: 0 });
});

test('absent monetary fields do not infer zero from a plausible minimum quantity', () => {
  for (const [family, venue, selected, record] of fixtures.filter(([family]) =>
    ['binance', 'asterdex', 'bybit', 'bitget', 'bitrue', 'bingx', 'orderly', 'woo', 'lighter', 'paradex', 'coinbase'].includes(family))) {
    const remove = (value) => {
      if (!value || typeof value !== 'object') return;
      for (const key of Object.keys(value)) {
        if (['minNotionalValue', 'minOrderAmt', 'minTradeUSDT', 'minOrderMoney', 'tradeMinUSDT', 'min_notional', 'min_quote_amount', 'quote_min_size', 'notional'].includes(key)) delete value[key];
        else remove(value[key]);
      }
    };
    const body = JSON.parse(record.body); remove(body);
    assert.equal(venueMarketRules(venue, [{ ...record, body: JSON.stringify(body) }], selected).notionalKnown, false, family);
  }
});

test('zero monetary minimum is known only when explicitly present in a documented field', () => {
  const record = http('https://api.staging.woox.io/v1/public/info', { success: true, rows: [{ symbol: 'PERP_BTC_USDT', base_min: 0.001, base_tick: 0.001, min_notional: 0 }] });
  assert.equal(venueMarketRules('woo', [record], market).notionalKnown, true);
  for (const value of ['', null, -1, 'NaN', 'Infinity', false, '0x10']) {
    const body = JSON.parse(record.body); body.rows[0].min_notional = value;
    assert.equal(venueMarketRules('woo', [{ ...record, body: JSON.stringify(body) }], market).notionalKnown, false);
  }
});

test('OKX and BloFin maxima remain native lots despite base-sized engine minima', () => {
  for (const family of ['okx', 'blofin']) {
    const [, venue, selected, record] = fixtures.find(([name]) => name === family);
    const result = venueMarketRules(venue, [record], { ...selected, minAmount: 0.0001 });
    assert.equal(result.minimumAmount, 0.1);
    assert.equal(result.maximumAmount, family === 'okx' ? 1000 : 10000);
    assert.equal(result.amountStep, 0.1);
  }
});

test('inverse Bybit and Bitget quantity is USD face value, not price times quantity', () => {
  const bybit = fixtures.find(([name]) => name === 'bybit')[3];
  const body = JSON.parse(bybit.body); body.result.category = 'inverse';
  const result = venueMarketRules('bybitinverse', [{ ...bybit,
    url: bybit.url.replace('category=linear', 'category=inverse'), body: JSON.stringify(body) }], market);
  assert.equal(result.notionalKnown, true);
  assert.equal(result.quoteQuantity, true);
  const bitget = fixtures.find(([name]) => name === 'bitget')[3];
  assert.equal(venueMarketRules('bitgetinverse', [bitget], market).notionalKnown, false);
  assert.equal(venueMarketRules('bitgetinverse', [{ ...bitget, url: bitget.url.replace('susdt-futures', 'scoin-futures') }], market).quoteQuantity, true);
});

test('native maximum overrides use the tighter venue limit and reject malformed or below-minimum caps', () => {
  const [, venue, selected, record] = fixtures.find(([name]) => name === 'blofin');
  const body = JSON.parse(record.body); body.data[0].maxMarketSize = '50';
  assert.equal(venueMarketRules(venue, [{ ...record, body: JSON.stringify(body) }], selected).maximumAmount, 50);
  for (const value of ['0', '-1', 'NaN', '', '0.01']) {
    body.data[0].maxMarketSize = value;
    assert.equal(venueMarketRules(venue, [{ ...record, body: JSON.stringify(body) }], selected).notionalKnown, false);
  }
});

test('later matching malformed rules invalidate old rules; unrelated pages do not', () => {
  const record = fixtures[0][3];
  const body = JSON.parse(record.body); delete body.symbols[0].filters;
  assert.equal(venueMarketRules('binance', [record, { ...record, body: JSON.stringify(body) }], market).notionalKnown, false);
  body.symbols[0].symbol = 'ETHUSDT';
  assert.equal(venueMarketRules('binance', [record, { ...record, body: JSON.stringify(body) }], market).notionalKnown, true);
  const duplicate = JSON.parse(record.body); duplicate.symbols.push(duplicate.symbols[0]);
  assert.equal(venueMarketRules('binance', [{ ...record, body: JSON.stringify(duplicate) }], market).notionalKnown, false);
});

test('Polymarket Gamma token matching preserves uint256 identity and matches full token strings', () => {
  const selected = { ...market, id: '12345678901234567890123456789', symbol: 'event-Yes' };
  const record = http('https://gamma-api.polymarket.com/markets/keyset', { markets: [{ clobTokenIds: '["12345678901234567890123456789","2"]', orderMinSize: 5 }] });
  assert.equal(venueMarketRules('polymarket', [record], selected).minimumAmount, 5);
  assert.equal(venueMarketRules('polymarket', [record], { ...selected, id: '12345678901234567890123456788' }).notionalKnown, false);
});

test('Hyperliquid static documented floor requires matching perpetual universe and request type', () => {
  const [, venue, selected, record] = fixtures.find(([name]) => name === 'hyperliquid');
  assert.equal(venueMarketRules(venue, [{ ...record, requestBody: '{"type":"spotMetaAndAssetCtxs"}' }], selected).notionalKnown, false);
  assert.equal(venueMarketRules(venue, [record], { ...selected, id: 'OTHER' }).notionalKnown, false);
});

test('broker aliases retain exact Orderly settlement suffix and share reviewed sizing', () => {
  const selected = { ...market, id: 'BTC/USDC:USDC_broker', symbol: 'BTCUSDC_broker', quote: 'USDC' };
  const record = http('https://testnet-api-evm.orderly.org/v1/public/info', { success: true, data: { rows: [{ symbol: 'PERP_BTC_USDC_broker', base_min: 0.001, base_tick: 0.001, min_notional: 1 }] } });
  for (const name of ['orderly', 'tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'])
    assert.equal(venueMarketRules(name, [record], selected).notionalKnown, true);
  assert.equal(venueMarketRules('orderly', [record], { ...selected, symbol: 'BTCUSDC' }).notionalKnown, false);
});

test('Aster V3 discovery uses its own versioned path with the same documented filters', () => {
  const [, venue, selected, record] = fixtures.find(([name]) => name === 'asterdex');
  assert.equal(venueMarketRules(venue, [{ ...record, url: record.url.replace('/v1/', '/v3/') }], selected).minNotional, 5);
  const [, , , binance] = fixtures[0];
  assert.equal(venueMarketRules('binance', [{ ...binance, url: binance.url.replace('/v1/', '/v3/') }], market).notionalKnown, false);
});

test('malformed scalar and null rows fail closed without throwing', () => {
  for (const [, venue, selected, record] of fixtures) {
    for (const payload of [null, 0, false, 'unexpected'])
      assert.equal(venueMarketRules(venue, [{ ...record, body: JSON.stringify(payload) }], selected).notionalKnown, false);
  }
  const source = http('https://demo-fapi.binance.com/fapi/v1/exchangeInfo', {
    symbols: [null, 0, ['BTCUSDT'], { symbol: 'BTCUSDT', filters: [null, 0] }],
  });
  assert.equal(venueMarketRules('binance', [source], market).notionalKnown, false);
});


test('Hyperliquid Spot sizing binds the exact pair, base lot and USDC quote', () => {
  const [, venue, selected, record] = fixtures.find(([family]) => family === 'hyperliquidspot');
  const check = (edit) => {
    const payload = JSON.parse(record.body); edit(payload[0]);
    assert.deepEqual(venueMarketRules(venue, [{ ...record, body: JSON.stringify(payload) }], selected),
      { notionalKnown: false, minNotional: 0 });
  };
  for (const edit of [
    (m) => { m.universe[0].name = '@143'; },
    (m) => { m.universe[0].index = 143; },
    (m) => { m.universe.push({ ...m.universe[0] }); },
    (m) => { m.universe[0].tokens = [0, 197]; },
    (m) => { m.tokens[0].name = 'HYPE'; },
    (m) => { m.tokens[1].name = 'ETH'; },
    (m) => { delete m.tokens[1].szDecimals; },
    (m) => { m.tokens.push({ ...m.tokens[1] }); },
  ]) check(edit);
  for (const patch of [{ method: 'GET' }, { requestBody: '{"type":"metaAndAssetCtxs"}' }])
    assert.deepEqual(venueMarketRules(venue, [{ ...record, ...patch }], selected), { notionalKnown: false, minNotional: 0 });
  assert.deepEqual(venueMarketRules(venue, [record], { ...selected, quote: 'HYPE' }), { notionalKnown: false, minNotional: 0 });
});


test('Hyperliquid Spot later ambiguous metadata invalidates earlier sizing', () => {
  const [, venue, selected, record] = fixtures.find(([family]) => family === 'hyperliquidspot');
  const payload = JSON.parse(record.body);
  payload[0].universe.push({ ...payload[0].universe[0] });
  assert.deepEqual(venueMarketRules(venue, [record, { ...record, body: JSON.stringify(payload) }], selected),
    { notionalKnown: false, minNotional: 0 });
});


test('Hyperliquid Spot minimum order remains post-only, step-aligned and below the smoke cap', () => {
  const [, venue, selected, record] = fixtures.find(([family]) => family === 'hyperliquidspot');
  const rules = venueMarketRules(venue, [record], selected);
  const order = smokeOrder({ ...selected, flags: MARKET_FLAG.ACTIVE | MARKET_FLAG.SPOT,
    precisionPrice: 0.1, minPrice: 0.1 }, { bids: [100000, 1], asks: [100001, 1] }, rules);
  assert.equal(order.timeInForce, 'PostOnly');
  assert.equal(order.side, 'buy');
  assert.ok(order.price < 100000 * 0.98);
  assert.ok(order.amount * order.amendPrice >= 10);
  assert.ok(order.amount * order.price <= LIVE_SMOKE_MAX_NOTIONAL);
  assert.ok(Math.abs(order.amount / rules.amountStep - Math.round(order.amount / rules.amountStep)) < 1e-8);
});
