'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute compiler installation required');
const ts = require(compiler);
const runtime = name => require(path.resolve(compiler, '..', name));
const axios = runtime('axios');
const retry = runtime('axios-retry');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
const control = process.env.GRVT_MARKETS_SOURCE_CONTROL;
assert(!control || ['double-parse', 'swapped-steps', 'skip-success-log', 'non-array-fallback', 'missing-log'].includes(control));
function source(file, dependencies) {
  const exports = {};
  const compiled = ts.transpileModule(fs.readFileSync(path.join(root, file), 'utf8'), {
    compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true},
    reportDiagnostics: true,
  });
  assert.equal(compiled.diagnostics.length, 0, `MARKETS source transpile ${file}`);
  vm.runInNewContext(compiled.outputText, {
    exports, require: name => {
      assert(Object.hasOwn(dependencies, name), `MARKETS unexpected import ${name}`);
      return dependencies[name];
    }, Error, Date, console, Promise, Map, WeakMap, global: {}, setTimeout, clearTimeout,
  });
  return exports;
}
function tag(value) {
  if (typeof value === 'number') {
    const bytes = Buffer.alloc(8);
    bytes.writeDoubleBE(value);
    return {bits: Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex')};
  }
  if (value === undefined) return {undefined: true};
  if (Array.isArray(value)) return Array.from(value, tag);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, tag(item)]));
  return value;
}
const text = fs.readFileSync(path.join(root, 'exchanges/grvt/grvt.exchange.ts'), 'utf8');
const ast = ts.createSourceFile('grvt.exchange.ts', text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'GrvtExchange');
const member = cls.members.find(n => n.name?.getText(ast) === 'fetchMarkets');
assert(member && ts.isArrowFunction(member.initializer), 'MARKETS actual arrow method');
let body = member.initializer.body.getText(ast);
if (control === 'swapped-steps') {
  assert(body.includes('amount: minSize,') && body.includes('price: tickSize,'));
  body = body.replace('amount: minSize,', 'amount: tickSize,').replace('price: tickSize,', 'price: minSize,');
}
if (control === 'skip-success-log') {
  const needle = 'this.log(`Fetched ${markets.length} GRVT markets`, LogSeverity.Debug);';
  assert(body.includes(needle));
  body = body.replace(needle, 'void markets.length;');
}
if (control === 'non-array-fallback') {
  assert(body.includes('response.data.r.map'));
  body = body.replace('response.data.r.map', '(Array.isArray(response.data.r) ? response.data.r : []).map');
}
if (control === 'missing-log') {
  assert(body.includes('if (!response.data?.r) {'));
  body = body.replace('if (!response.data?.r) {', 'if (!response.data?.r) { this.log("unexpected", LogSeverity.Debug);');
}
const tables = source('exchanges/grvt/grvt.types.ts', {});
const method = {exports: {}, ENDPOINTS: tables.ENDPOINTS, LogSeverity: {Debug: 'debug', Error: 'error'}};
const compiledMethod = ts.transpileModule('exports.call = async function() ' + body, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}, reportDiagnostics: true,
});
assert.equal(compiledMethod.diagnostics.length, 0, 'MARKETS method compiles');
vm.runInNewContext(compiledMethod.outputText, method);
const instrument = '{"i":"btc_perp","b":"BTC","q":"USDT","k":"PERPETUAL","ms":"0.013","ts":"0.2","mp":"27.5","bd":-0,"qd":1e400}';
const cases = [
  {name: 'missing', raw: '{}'},
  {name: 'null', raw: 'null'},
  {name: 'empty', raw: '{"r":[]}'},
  {name: 'quoted', raw: '"{\\"r\\":[' + instrument.replaceAll('"', '\\"') + ']}"'},
  {name: 'one', raw: '{"r":[' + instrument + ']}'},
  {name: 'two', raw: '{"r":[' + instrument + ',{"i":"eth_perp","b":"ETH","q":"USD","k":"FUTURE","ms":"0.07","ts":"1.5","ia":false,"mxs":"18.25"}]}'},
  {name: 'later-null', raw: '{"r":[{"i":"x","b":{},"q":"USD"},null]}'},
  {name: 'non-array', raw: '{"r":{}}'},
  {name: 'defaults', raw: '{"r":[{"i":"zero","b":"B","q":"Q","ms":0,"ts":false,"ia":0,"mp":0,"mxs":"19"}]}'},
  {name: 'success-log-throws', raw: '{"r":[' + instrument + ']}', logFailure: 1},
  {name: 'catch-log-throws', raw: '{"r":[' + instrument + ']}', logFailure: 2},
];
assert.equal(cases.length, 11, 'MARKETS source case total');
const captureControl = process.env.GRVT_MARKETS_CAPTURE_CONTROL;
assert(!captureControl || ['substitute-symbol', 'reverse'].includes(captureControl));
if (captureControl === 'substitute-symbol') cases[4].raw = cases[4].raw.replace('btc_perp', 'sol_perp');
if (captureControl === 'reverse') cases.reverse();
const original = axios.defaults.adapter;
(async () => {
  const rows = [];
  for (const input of cases) {
    const effects = [];
    let sends = 0;
    axios.defaults.adapter = async config => {
      sends++;
      effects.push(['request', config.url, config.data]);
      let data = input.raw;
      if (control === 'double-parse' && input.name === 'quoted') data = JSON.parse(data);
      return {data, status: 200, statusText: 'OK', headers: {}, config};
    };
    const diagnostics = {
      markRestRequestStart() { effects.push(['start']); },
      noteRestSuccess() { effects.push(['success']); },
      noteRestFailure() { effects.push(['failure']); },
    };
    const common = source('safeCexAxiosCreate.ts', {
      axios,
      './remoteSigningAdapter': {createRemoteSigningAdapter() { throw Error('unexpected signing'); }},
      './transport/registry': {getTransport() { return {rest({config}) { return {proxied: false, baseURL: config.baseURL}; }}; }},
      './utils/axiosErrorEnhancer': source('utils/axiosErrorEnhancer.ts', {axios}),
      './utils/isDevelopment': {isDevelopment: false},
      './utils/signingCapture': source('utils/signingCapture.ts', {}),
      './utils/venueRestDiagnostics': diagnostics,
      './utils/runtime': {isNodeOrBunRuntime: () => true},
    });
    const api = source('exchanges/grvt/grvt.api.ts', {
      'axios-retry': retry, '../../safeCexAxiosCreate': common,
      '../../types': {LogSeverity: {Debug: 'debug', Error: 'error'}},
      './grvtRetry': source('exchanges/grvt/grvtRetry.ts', {'./grvt.types': tables}),
      '../../utils/retryReadsOnly': source('utils/retryReadsOnly.ts', {'axios-retry': retry}),
      './grvt.types': tables,
    });
    let logs = 0;
    const exchange = {name: 'grvt', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], log(...args) {
      effects.push(['log', ...args]);
      logs++;
      if (input.logFailure && logs <= input.logFailure) throw Error('subscriber-' + logs);
    }};
    exchange.xhr = api.createAPI({}, exchange);
    let result, error;
    try { result = await method.exports.call.call(exchange); } catch (caught) { error = String(caught); }
    assert.equal(sends, 1, 'MARKETS request count');
    assert.deepEqual(effects.slice(0, 3), [['start'], ['request', '/lite/v1/all_instruments', '{}'], ['success']], 'MARKETS transport effects');
    rows.push({name: input.name, raw: input.raw, result: tag(result), error: error ?? null, effects});
  }
  const byName = Object.fromEntries(rows.map(row => [row.name, row]));
  for (const name of ['quoted', 'one', 'two', 'defaults']) assert.equal(byName[name].error, null, `MARKETS ${name} successful outcome`);
  for (const name of ['later-null', 'success-log-throws', 'catch-log-throws']) assert.deepEqual(byName[name].result, {undefined: true}, `MARKETS ${name} rejected outcome`);
  assert.equal(byName.one.result[0].isPerpetual, true, 'MARKETS perpetual runtime extra field');
  const transportEffects = [['start'], ['request', '/lite/v1/all_instruments', '{}'], ['success']];
  for (const name of ['missing', 'null']) {
    assert.deepEqual(byName[name].result, [], `MARKETS ${name} result`);
    assert.equal(byName[name].error, null, `MARKETS ${name} error`);
    assert.deepEqual(byName[name].effects, transportEffects, `MARKETS ${name} effects`);
  }
  assert.deepEqual(byName.empty.result, [], 'MARKETS empty result');
  assert.equal(byName.empty.error, null, 'MARKETS empty error');
  assert.deepEqual(byName.empty.effects, [...transportEffects, ['log', 'Fetched 0 GRVT markets', 'debug']], 'MARKETS empty effects');
  assert.equal(byName['non-array'].error, 'TypeError: response.data.r.map is not a function', 'MARKETS non-array error');
  assert.deepEqual(byName['non-array'].result, {undefined: true}, 'MARKETS non-array result');
  assert.deepEqual(byName['non-array'].effects, [...transportEffects, ['log', 'Error fetching markets: TypeError: response.data.r.map is not a function', 'error']], 'MARKETS non-array effects');
  assert.deepEqual(byName.quoted.result, [], 'MARKETS single Axios transform');
  assert.equal(byName.quoted.effects.length, 3, 'MARKETS quoted response no success log');
  assert.equal(byName.one.result.length, 1, 'MARKETS one native source market');
  assert.equal(byName.two.result.length, 2, 'MARKETS two native source markets');
  assert.deepEqual(byName.one.result[0].precision, {amount: tag(0.013), price: tag(0.2)}, 'MARKETS distinct native steps');
  assert.deepEqual(byName.one.result[0].info.bd, {bits: '8000000000000000'}, 'MARKETS signed zero info');
  assert.deepEqual(byName.one.result[0].info.qd, {bits: '7ff0000000000000'}, 'MARKETS overflow info');
  assert.equal(byName.two.result[1].symbol, 'ETH_PERP', 'MARKETS second native symbol');
  assert.equal(byName.two.result[1].active, false, 'MARKETS second native active');
  assert.equal(byName.two.result[1].isPerpetual, false, 'MARKETS runtime extra field');
  assert.equal(byName['later-null'].error, "TypeError: Cannot read properties of null (reading 'k')", 'MARKETS later error identity');
  assert.equal(byName['later-null'].effects.at(-1)[1], "Error fetching markets: TypeError: Cannot read properties of null (reading 'k')", 'MARKETS later error log');
  assert.equal(byName.defaults.result[0].active, true, 'MARKETS strict false activity');
  assert.deepEqual(byName.defaults.result[0].precision, {amount: tag(0.001), price: tag(0.1)}, 'MARKETS truthy defaults');
  assert.equal(byName['success-log-throws'].error, 'Error: subscriber-1', 'MARKETS success log failure');
  assert.deepEqual(byName['success-log-throws'].effects.slice(-2), [['log', 'Fetched 1 GRVT markets', 'debug'], ['log', 'Error fetching markets: Error: subscriber-1', 'error']], 'MARKETS catch after success log');
  assert.equal(byName['catch-log-throws'].error, 'Error: subscriber-2', 'MARKETS catch log replaces failure');
  assert.equal(rows.length, 11, 'MARKETS completed cases');
  process.stdout.write(JSON.stringify({architecture: process.arch, node: process.versions.node, axios: axios.VERSION, rows}));
})().finally(() => { axios.defaults.adapter = original; }).catch(error => {console.error(error); process.exitCode = 1;});
