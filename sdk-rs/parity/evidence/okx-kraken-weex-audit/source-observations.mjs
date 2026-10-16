// Audit-only source observations. No native equivalence or SDK assembly claim.
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {ts} from '../../../safe-cex-store/tests/source_loader.mjs';

const root = fileURLToPath(new URL('../../../../', import.meta.url));
const sourceRoot = path.join(root, 'packages/safe-cex/src');
const dependencies = createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH), 'oracle.cjs'));
const require = createRequire(import.meta.url);
const {load: loadSource} = require('../../../safe-cex-venue-weex/tests/types-oracle.cjs');
const control = process.argv.includes('--control');
const selectedCase = process.argv[2];
const observations = [];
const high = JSON.parse('"\\ud800"');
const units = value => Array.from({length: value.length}, (_, i) => value.charCodeAt(i));
function replaceOnce(text, before, after) {
  assert.equal(text.split(before).length - 1, 1, 'CONTROL-SELECTOR unique actual-source expression');
  return text.replace(before, after);
}
function evaluate(relative, names, bindings = {}, owner, transform = text => text) {
  const file = path.join(sourceRoot, relative);
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const container = owner ? ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === owner).members : ast.statements;
  let text = names.map(name => {
    const matches = [...container].filter(n => owner ? n.name?.getText(ast) === name : ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ast) === name));
    assert.equal(matches.length, 1, `actual source declaration ${relative}::${name}`);
    return matches[0].getText(ast);
  }).join('\n');
  text = transform(text);
  text = owner ? `class Selected {${text}}\nexports.Selected=Selected;` : `${text}\nObject.assign(exports,{${names.join(',')}});`;
  const compiled = ts.transpileModule(text, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}, reportDiagnostics: true});
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  const exports = {};
  vm.runInNewContext(compiled.outputText, {exports, ...bindings}, {filename: relative});
  return owner ? new exports.Selected() : exports;
}
const enums = loadSource(path.join(sourceRoot, 'types.ts'));
const cases = {};
const handlers = [
  ['okx-ticker-unit', 'handleTickerEvents', 'last', 7],
  ['okx-mark-unit', 'handleMarkPriceEvents', 'mark', 11],
  ['okx-index-unit', 'handleIndexTickerEvents', 'index', 13],
  ['okx-funding-unit', 'handleFundingRateEvents', 'fundingRate', 17],
];
for (const [id, method, field, expected] of handlers) cases[id] = () => {
  const receiver = evaluate('exchanges/okx/okx.ws-public.ts', [method], loadSource(path.join(sourceRoot, 'utils/roundUSD.ts')), 'OKXPublicWebsocket', text => control ? text.replaceAll('update.instId', "'replacement'") : text);
  // Execute the actual Store key-lookup/write body with a fixed receiver.
  const store = evaluate('store/store.base.ts', ['stripTickerInfoFromChanges', 'updateTicker'], {}, 'DefaultStore');
  const ticker = {id: high, symbol: 'owned-audit-symbol'};
  let notifications = 0;
  store.tickersMap = {[high]: ticker}; store.tickersBySymbolMap = {};
  store.notify = () => notifications++;
  receiver.store = store;
  receiver.parent = {
    markPriceLoaded: id => assert.deepEqual(units(id), [0xd800], 'SOURCE-OKX-MARK-UNIT parent argument'),
    getMarketsForIndexTickerInstId: id => {
      assert.deepEqual(units(id), [0xd800], 'SOURCE-OKX-INDEX-UNIT selector argument');
      return [{id}];
    },
    indexPriceLoaded: id => assert.deepEqual(units(id), [0xd800], 'SOURCE-OKX-INDEX-UNIT loaded argument'),
  };
  const update = {instId: high, open24h: '3', last: '7', bidPx: '5', askPx: '9', volCcy24h: '19', vol24h: '23', markPx: '11', idxPx: '13', fundingRate: '17', fundingTime: '29'};
  receiver[method]({data: [update]});
  assert.equal(ticker[field], expected, `SOURCE-${id.toUpperCase()} actual Store key lookup`);
  assert.equal(notifications, 1, `SOURCE-${id.toUpperCase()} notification`);
  assert.deepEqual(units(ticker.id), [0xd800], `SOURCE-${id.toUpperCase()} retained unit`);
  return {id, units: units(ticker.id), field, value: ticker[field], notifications};
};
cases['okx-pretty-unit'] = () => {
  const receiver = evaluate('exchanges/okx/okx.exchange.ts', ['formatMarketPrettySymbol'], {}, 'OKXExchange', text => control ? replaceOnce(text, '${base}/${quote} XPERP', '${quote}/${base} XPERP') : text);
  const value = receiver.formatMarketPrettySymbol({instType:'FUTURES', ruleType:'xperp'}, high, 'Q');
  assert.deepEqual(units(value), [0xd800,47,81,32,88,80,69,82,80], 'SOURCE-OKX-PRETTY-UNIT returned String');
  return {id:'okx-pretty-unit', units: units(value)};
};
cases['okx-finite-callable'] = () => {
  const receiver = evaluate('exchanges/okx/okx.ws-private.ts', ['parseFinite','sumFinite'], {}, 'OKXPrivateWebsocket', text => control ? replaceOnce(text, 'Number.isFinite(parsed) ? parsed : 0', 'Number.isFinite(parsed) ? 0 : 0') : text);
  const value = {toString: () => '7.5'};
  assert.equal(receiver.parseFinite(value), 7.5, 'SOURCE-OKX-FINITE-CALLABLE parse');
  assert.equal(receiver.sumFinite([value]), 7.5, 'SOURCE-OKX-FINITE-CALLABLE sum');
  return {id:'okx-finite-callable', parsed:7.5, summed:7.5, jsonView:JSON.stringify(value)};
};
cases['okx-map-series-prefix'] = async () => {
  let {mapSeries} = dependencies('p-iteration');
  if (control) mapSeries = vm.runInNewContext('('+replaceOnce(mapSeries.toString(), 'await array[i]', 'array[i]')+')');
  let calls = 0;
  const pending = mapSeries([{}], () => {calls++; return 'id';});
  assert.equal(calls, 0, 'SOURCE-OKX-MAPSERIES-PREFIX imported await before callback');
  await Promise.resolve();
  assert.equal(calls, 1, 'SOURCE-OKX-MAPSERIES-PREFIX callback after payload await');
  await pending;
  return {id:'okx-map-series-prefix', beforeMicrotask:0, afterMicrotask:1, findingWithdrawn:'No eager POST before initial payload await'};
};
const fill = {fill_id:high, symbol:'PF_X', fillTime:'2026-08-14T12:34:56Z', side:'buy', price:'3', size:'5'};
cases['kraken-fill-unit'] = () => {
  const f = evaluate('exchanges/kraken/kraken.fills.ts', ['toSeconds','num','optionalNum','mapKrakenFill'], {OrderSide:enums.OrderSide}, undefined, text => control ? replaceOnce(text, "String(row?.fill_id ?? '')", "'replacement'") : text);
  const value = f.mapKrakenFill(fill, () => undefined);
  assert.deepEqual(units(value.id), [0xd800], 'SOURCE-KRAKEN-FILL-UNIT returned id');
  return {id:'kraken-fill-unit', units:units(value.id)};
};
cases['kraken-account-log-unit'] = () => {
  const f = evaluate('exchanges/kraken/kraken.income.ts', ['isPositionSizeLeg','num','toMs','mapKrakenAccountLogRow','mapKrakenAccountLog'], {}, undefined, text => control ? replaceOnce(text, "String(row.booking_uid ?? row.id ?? '')", "'replacement'") : text);
  const value = f.mapKrakenAccountLog([{booking_uid:high, date:'2026-08-14T12:34:56Z', asset:'usd', contract:'PF_X', realized_pnl:'3'}], () => undefined);
  assert.equal(value.length, 1, 'SOURCE-KRAKEN-LOG-UNIT fixed one-record expectation');
  assert.deepEqual(units(value[0].tranId), [0xd800,58,112,110,108], 'SOURCE-KRAKEN-LOG-UNIT returned tranId');
  return {id:'kraken-account-log-unit', units:units(value[0].tranId)};
};
cases['kraken-error-unit'] = () => {
  const f = evaluate('exchanges/kraken/kraken.api.ts', ['createKrakenApiError'], {}, undefined, text => control ? replaceOnce(text, 'new Error(message,', "new Error('replacement',") : text);
  const value = f.createKrakenApiError(high);
  assert.deepEqual(units(value.message), [0xd800], 'SOURCE-KRAKEN-ERROR-UNIT returned Error.message');
  return {id:'kraken-error-unit', units:units(value.message), consumer:'returned Error.message inspected by caller; no human-only assumption'};
};
cases['kraken-leverage-key'] = async () => {
  const receiver = evaluate('exchanges/kraken/kraken.exchange.ts', ['fetchLeverageSettings'], {ENDPOINTS:{LEVERAGE_PREFERENCES:'audit-only'}, console}, 'KrakenExchange', text => control ? replaceOnce(text, 'this.leverageSettings.set(pref.symbol, pref.maxLeverage)', "this.leverageSettings.set('replacement', pref.maxLeverage)") : text);
  receiver.leverageSettings = new Map();
  receiver.xhr = {get: async () => ({data:{result:'success',leveragePreferences:[{symbol:high,maxLeverage:7}]}})};
  await receiver.fetchLeverageSettings();
  assert.equal(receiver.leverageSettings.get(high), 7, 'SOURCE-KRAKEN-LEVERAGE-KEY stored lookup');
  return {id:'kraken-leverage-key', units:units([...receiver.leverageSettings.keys()][0]), value:receiver.leverageSettings.get(high)};
};
cases['kraken-candle-prefix'] = async () => {
  const trace = [];
  const receiver = evaluate('exchanges/kraken/kraken.exchange.ts', ['fetchOHLCV'], {Date:{now:()=>{trace.push('clock');return 123456789;}}}, 'KrakenExchange', text => control ? replaceOnce(text, 'const to = opts.to || Date.now();', 'const to = opts.to || 123456789;') : text);
  receiver.options = {};
  let release;
  receiver.xhr = {get: () => {trace.push('get');return new Promise(resolve => {release=resolve;});}};
  receiver.emitAndLogError = () => {};
  const pending = receiver.fetchOHLCV({symbol:'PF_X',interval:'1h',limit:1});
  assert.deepEqual(trace, ['clock','get'], 'SOURCE-KRAKEN-CANDLE-PREFIX before returned Promise is awaited');
  release({data:{candles:[]}}); await pending;
  return {id:'kraken-candle-prefix', trace, nativeComparison:'inspected async fn; not executed'};
};
cases['kraken-spot-position-unit'] = async () => {
  const receiver = evaluate('exchanges/kraken/krakenspot.exchange.ts', ['fetchPositions'], enums, 'KrakenSpotExchange', text => control ? replaceOnce(text, 'id: `${market.symbol}:long`,\n        symbol: market.symbol,', "id: `${market.symbol}:long`,\n        symbol: 'replacement',") : text);
  receiver.store = {markets:[{active:true,symbol:high}]};
  const value = await receiver.fetchPositions();
  assert.equal(value.length, 2, 'SOURCE-KRAKEN-SPOT-UNIT fixed two-position expectation');
  assert.deepEqual(units(value[0].symbol), [0xd800], 'SOURCE-KRAKEN-SPOT-UNIT returned symbol');
  assert.deepEqual(units(value[0].id), [0xd800,58,108,111,110,103], 'SOURCE-KRAKEN-SPOT-UNIT long id');
  assert.deepEqual(units(value[1].id), [0xd800,58,115,104,111,114,116], 'SOURCE-KRAKEN-SPOT-UNIT short id');
  return {id:'kraken-spot-position-unit', units:units(value[0].symbol), long:units(value[0].id), short:units(value[1].id)};
};
cases['kraken-account-page-unit'] = async () => {
  const income = evaluate('exchanges/kraken/kraken.income.ts', ['isPositionSizeLeg','num','toMs','mapKrakenAccountLogRow','mapKrakenAccountLog']);
  const receiver = evaluate('exchanges/kraken/kraken.exchange.ts', ['_fetchAccountLogPage'], {...income,ENDPOINTS:{ACCOUNT_LOG:'audit-only'}}, 'KrakenExchange', text => control ? replaceOnce(text, 'mapKrakenAccountLog(logs,', "mapKrakenAccountLog(logs.map(row => ({...row,booking_uid:'replacement'})),") : text);
  receiver.store = {marketsBySymbol:{}};
  receiver.historyXhr = {get:async()=>({data:{logs:[{booking_uid:high,date:'2026-08-14T12:34:56Z',asset:'usd',contract:'PF_X',realized_pnl:'3'}]}})};
  const value = await receiver._fetchAccountLogPage({startTime:1,limit:2});
  assert.equal(value.length, 1, 'SOURCE-KRAKEN-PAGE-UNIT fixed one-record expectation');
  assert.deepEqual(units(value[0].tranId), [0xd800,58,112,110,108], 'SOURCE-KRAKEN-PAGE-UNIT returned tranId');
  return {id:'kraken-account-page-unit', units:units(value[0].tranId)};
};
const weexFile = 'exchanges/weex/weex.utils.ts';
const weexBase = {symbol:'B/Q',baseAsset:'B',quoteAsset:'Q',extra:{probe:1}};
const precision = loadSource(path.join(root, 'packages/ccxt-compat/src/functions/ccxt_exchange.ts'));
cases['weex-market-alias'] = () => {
  const f = evaluate(weexFile, ['toNumber','normalizeMarket'], precision, undefined, text => control ? replaceOnce(text, 'info: raw,', 'info: {...raw, extra:{...raw.extra}},') : text);
  const input = {...weexBase,extra:{probe:1}};
  const value = f.normalizeMarket(input);
  input.extra.probe=7;
  assert.equal(value.info.extra.probe, 7, 'SOURCE-WEEX-MARKET-ALIAS input-to-output');
  value.info.extra.probe=11;
  assert.equal(input.extra.probe, 11, 'SOURCE-WEEX-MARKET-ALIAS output-to-input');
  return {id:'weex-market-alias', inputToOutput:7, outputToInput:11};
};
cases['weex-ticker-alias-unit'] = () => {
  const f = evaluate(weexFile, ['toNumber','normalizePercentage','normalizeTicker'], {}, undefined, text => control ? replaceOnce(text, 'ticker: rawTicker,', 'ticker: {...rawTicker},') : text);
  const input = {lastPrice:3,extra:high}, book = {bidPrice:2};
  const value = f.normalizeTicker(input, {id:'id',symbol:'B/Q'}, book);
  assert.deepEqual(units(value.info.ticker.extra), [0xd800], 'SOURCE-WEEX-TICKER-UNIT returned info');
  input.lastPrice=7;
  assert.equal(value.info.ticker.lastPrice, 7, 'SOURCE-WEEX-TICKER-ALIAS input-to-output');
  value.info.ticker.lastPrice=11;
  assert.equal(input.lastPrice, 11, 'SOURCE-WEEX-TICKER-ALIAS output-to-input');
  book.bidPrice=13;
  assert.equal(value.info.bookTicker.bidPrice, 13, 'SOURCE-WEEX-TICKER-ALIAS book-input-to-output');
  value.info.bookTicker.bidPrice=17;
  assert.equal(book.bidPrice, 17, 'SOURCE-WEEX-TICKER-ALIAS book-output-to-input');
  return {id:'weex-ticker-alias-unit', units:units(value.info.ticker.extra), directions:[7,11,13,17]};
};
cases['weex-ticker-unit'] = () => {
  const f = evaluate(weexFile, ['toNumber','normalizePercentage','normalizeTicker'], {}, undefined, text => control ? replaceOnce(text, 'ticker: rawTicker,', "ticker: {...rawTicker, extra:'replacement'},") : text);
  const value = f.normalizeTicker({lastPrice:3,extra:high}, {id:'id',symbol:'B/Q'});
  assert.deepEqual(units(value.info.ticker.extra), [0xd800], 'SOURCE-WEEX-TICKER-UNIT returned info');
  return {id:'weex-ticker-unit', units:units(value.info.ticker.extra)};
};
cases['weex-fake-unit'] = () => {
  const f = evaluate(weexFile, ['fakePositionsForMarket'], {...enums,...loadSource(path.join(sourceRoot,'utils/createPositionId.ts'))}, undefined, text => control ? replaceOnce(text, 'symbol: market.symbol,', "symbol: 'replacement',") : text);
  const value = f.fakePositionsForMarket({symbol:high}, enums.PositionMode.Hedged, enums.MarginType.Cross, 3);
  assert.equal(value.length, 2, 'SOURCE-WEEX-FAKE-UNIT fixed two-position expectation');
  assert.deepEqual(units(value[0].symbol), [0xd800], 'SOURCE-WEEX-FAKE-UNIT returned symbol');
  return {id:'weex-fake-unit', units:units(value[0].symbol)};
};
cases['weex-integer-unit'] = () => {
  const f = evaluate(weexFile, ['toIntegerString'], {}, undefined, text => control ? replaceOnce(text, "String(value ?? '')", "'replacement'") : text);
  const value = f.toIntegerString(high);
  assert.deepEqual(units(value), [0xd800], 'SOURCE-WEEX-INTEGER-UNIT returned String');
  return {id:'weex-integer-unit', units:units(value)};
};
cases['weex-response-unit'] = () => {
  const parser = evaluate('../../json-parse/src/jsonParse.ts', ['jsonParse'], {}, undefined, text => control ? replaceOnce(text, 'return JSON.parse(str);', "return {assets:[{name:'replacement'}],symbols:[]};") : text);
  const value = parser.jsonParse('{"assets":[{"name":"\\ud800"}],"symbols":[]}');
  assert.deepEqual(units(value.assets[0].name), [0xd800], 'SOURCE-WEEX-RESPONSE-UNIT materialized assets[0].name');
  return {id:'weex-response-unit', units:units(value.assets[0].name), scope:'actual parser; declared shape inspected separately, not runtime TS validation'};
};
assert.equal(Object.keys(cases).length, 20, 'AUDIT-SOURCE fixed twenty-profile inventory');
if (selectedCase) {
  assert.ok(Object.hasOwn(cases, selectedCase), 'named source profile');
  observations.push(await cases[selectedCase]());
} else {
  assert.equal(control, false, 'control requires exact profile');
  for (const fn of Object.values(cases)) observations.push(await fn());
}
// All materialized-unit/alias observations above precede wire capture.
process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,controlled:control,observations})+'\n');
