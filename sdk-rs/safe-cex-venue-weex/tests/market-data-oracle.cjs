// Execute the actual declarations and imported CCXT precision implementation.
const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, header, capture, load} = require('./types-oracle.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['toNumber', 'normalizePercentage', 'normalizeMarket', 'normalizeTicker'];
const nodes = ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => names.includes(d.name.getText(ast))));
assert.equal(nodes.length, 4, 'market_data::source_inventory');
const context = {exports: {}, ...load(path.join(root, 'packages/ccxt-compat/src/functions/ccxt_exchange.ts'))};
vm.runInNewContext(ts.transpileModule(nodes.map(n => n.getText(ast)).join('\n'), {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText, context, {filename: file});
const base = {symbol: 'BTCUSDT', baseAsset: 'BTC', quoteAsset: 'USDT'};
const marketInputs = [
 ['distinct', {...base, pricePrecision: 3, quantityPrecision: 5, minOrderSize: '.007', maxOrderSize: 1234, minLeverage: 2, maxLeverage: 89, contractVal: 11, makerFeeRate: '.0002', takerFeeRate: '.0007', forwardContractFlag: false, extra: {retain: [1, 2]}}],
 ['defaults', base],
 ['aliases', {...base, pricePrecision: null, quotePrecision: 4, quantityPrecision: null, baseAssetPrecision: 6, minOrderSize: 'bad', contractVal: null}],
 ['zero', {...base, pricePrecision: 0, quantityPrecision: 0, minOrderSize: 0, minLeverage: 0, maxLeverage: 0, contractVal: 0, makerFeeRate: '-0', forwardContractFlag: 0}],
 ['invalid_precision', {...base, pricePrecision: 'bad', quantityPrecision: -4}],
 ['hex_precision', {...base, pricePrecision: '0x3', quantityPrecision: '2tail'}],
 ['coercion', {...base, pricePrecision: [2], quantityPrecision: [4], minOrderSize: ['.05'], maxOrderSize: [12], forwardContractFlag: null}],
 ['underflow', {...base, pricePrecision: 400, quantityPrecision: 400}],
 ['price_error', {...base, pricePrecision: {toString: null}}],
 ['eager_fallback', {...base, minOrderSize: 5, quantityPrecision: {toString: null}}],
 ['amount_error', {...base, minOrderSize: {toString: null}}],
 ['null', null],
];
const marketCases = marketInputs.map(([id, input]) => {
 const raw = JSON.stringify(input);
 try {
  const value = context.exports.normalizeMarket(input);
  assert.equal(value.info, input, 'market_data::market_source_info_alias');
  return {id, raw, strings: {id: value.id, symbol: value.symbol, base: value.base, quote: value.quote}, active: value.active,
   numbers: Object.fromEntries(Object.entries({amount: value.precision.amount, price: value.precision.price, minAmount: value.limits.amount.min, maxAmount: value.limits.amount.max, minLeverage: value.limits.leverage.min, maxLeverage: value.limits.leverage.max, contractSize: value.contractSize, makerFee: value.makerFee, takerFee: value.takerFee}).map(([k,v]) => [k, capture(v)]))};
 } catch (e) { return {id, raw, error: {name: e.name, message: e.message}}; }
});
const market = {id: 'venue-id', symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', active: true, precision: {amount: .001, price: .01}, limits: {amount: {min: .1, max: 100}, leverage: {min: 1, max: 99}}};
const tickerInputs = [
 ['distinct', {lastPrice: 101, openPrice: 97, markPrice: 102, indexPrice: 103, bidPrice: 98, askPrice: 105, priceChangePercent: '.0125', openInterest: 211, fundingRate: '-.0003', fundingTime: 123456, volume: 317, quoteVolume: 419, extra: {keep: []}}, {bidPrice: 99, askPrice: 104}],
 ['aliases', {c: 51, o: 50, m: 52, i: 53, b: 49, a: 54, P: 2, v: 17, q: 19}],
 ['nullish', {lastPrice: null, c: 13, openPrice: null, o: 11, markPrice: null, m: 14, indexPrice: null, i: 15, bidPrice: null, b: 12, askPrice: null, a: 16, volume: null, v: 21, quoteVolume: null, q: 23}, {bidPrice: null, askPrice: null}],
 ['absent', undefined, undefined], ['null', null, null], ['scalar', false, 3],
 ['zero', {lastPrice: 0, c: 99, markPrice: 0, indexPrice: 0, bidPrice: 0, askPrice: 0, volume: 0, v: 12}, {bidPrice: '', askPrice: 0}],
 ['fallback', {last: 125, openPrice: 100, markPrice: 'bad', indexPrice: 'bad', priceChangePercent: 'bad'}],
 ['percentage_ratio', {lastPrice: 33, priceChangePercent: -1}],
 ['percentage_points', {lastPrice: 33, priceChangePercent: -1.25}],
 ['coercion', {lastPrice: ['19.5'], markPrice: ['20.5'], volume: ['-0'], fundingRate: 'Infinity'}],
 ['open_error', {lastPrice: 5, openPrice: {toString: null}, markPrice: {toString: null}}],
 ['bid_error', {lastPrice: 5, bidPrice: {toString: null}, askPrice: {toString: null}}],
 ['book_priority', {lastPrice: 5, bidPrice: {toString: null}, askPrice: {toString: null}}, {bidPrice: 7, askPrice: 8}],
];
const tickerCases = tickerInputs.map(([id, ticker, book]) => {
 const inputs = {id, ...(ticker !== undefined ? {raw: JSON.stringify(ticker)} : {}), ...(book !== undefined ? {book: JSON.stringify(book)} : {})};
 try {
  const value = context.exports.normalizeTicker(ticker, market, book);
  assert.equal(value.info.ticker, ticker, 'market_data::ticker_source_info_alias');
  assert.equal(value.info.bookTicker, book, 'market_data::book_source_info_alias');
  const numbers = Object.fromEntries(['bid','ask','last','mark','index','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'].map(k => [k,capture(value[k])]));
  return {...inputs, strings: {id: value.id, symbol: value.symbol}, numbers};
 } catch (e) {return {...inputs, error: {name: e.name, message: e.message}};}
});
// Source returns these malformed string fields verbatim; the shared typed Market refuses them.
const shapeRefusals = [{baseAsset:'BTC',quoteAsset:'USDT'}, {...base,symbol:7}, {...base,baseAsset:null}].map(input => ({raw: JSON.stringify(input), symbolType: typeof context.exports.normalizeMarket(input).symbol, baseIsNull: context.exports.normalizeMarket(input).base === null}));
const utf16 = context.exports.normalizeTicker({lastPrice: 1, extra:'\ud800'}, market);
assert.equal(utf16.info.ticker.extra.charCodeAt(0), 0xd800);
const callable = context.exports.normalizeTicker({lastPrice: {toString: () => '7.25'}}, market);
assert.equal(callable.last, 7.25);
process.stdout.write(JSON.stringify({header, market, marketCases, tickerCases, shapeRefusals, callable: capture(callable.last)}));
