import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createRequire} from 'node:module';
import {ts, root, load} from './source.mjs';

const fixtures = JSON.parse(fs.readFileSync(new URL('./fixtures/reporting_values.json', import.meta.url)));
assert.equal(fixtures.length, 20, 'SOURCE independent raw reporting fixture floor');
function members(relative, className, names) {
  const filename = root + relative;
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === className);
  return names.map(name => {
    const member = klass.members.find(n => n.name?.getText(ast) === name);
    assert(member, `SOURCE tracked body ${name}`);
    let text = member.getText(ast);
    if (name === 'fetchPositions' && process.env.MEXC_REPORT_SOURCE_CONTROL === 'stringify') {
      const anchor = 'this.emitAndLogError(err?.response?.data?.message || err?.message);';
      assert(text.includes(anchor), 'SOURCE report mutation anchor');
      text = text.replace(anchor, 'this.emitAndLogError(String(err?.response?.data?.message || err?.message));');
    }
    return text;
  }).join('\n');
}
const base = members('packages/safe-cex/src/exchanges/base.ts', 'BaseExchange', ['noteHttpResponse', 'emitAndLogError']);
const exchange = members('packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts', 'MexcExchange', ['fetchPositions', 'fetchBalance', 'validateAccount', 'getAccount']);
const enhancerFile = root + 'packages/safe-cex/src/utils/axiosErrorEnhancer.ts';
const enhancer = ts.createSourceFile(enhancerFile, fs.readFileSync(enhancerFile, 'utf8'), ts.ScriptTarget.Latest, true);
const formatter = enhancer.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === 'formatErrorForSentry');
assert(formatter, 'SOURCE actual telemetry formatter');
const compiled = ts.transpileModule(`class Holder {${base}\n${exchange}}\n${formatter.getText(enhancer)}\nglobalThis.Holder=Holder;`,
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
const http = load('packages/safe-cex/src/utils/httpFailureContext.ts');
const rows = [];
let totalGets = 0, totalConsoles = 0, totalEvents = 0, totalReports = 0, totalPositionReads = 0;
const shapes = error => error instanceof Error || Object.prototype.toString.call(error) === '[object Error]'
  ? {name: error.name, message: error.message} : error;
if (!process.argv.includes('--listeners')) {
for (const worker of [false, true]) for (const fixture of fixtures) for (const method of ['positions', 'validate', 'account']) {
  const reports = [], events = [], consoles = []; let gets = 0, positionReads = 0;
  const selected = fixture[method === 'positions' ? 'position' : 'validation'];
  const expectedFormat = fixture[method === 'positions' ? 'positionFormat' : 'validationFormat'];
  const context = {exports: {}, recordHttpFailure: http.recordHttpFailure, readHttpFailureContext: http.readHttpFailureContext,
    reportException(error, data) { reports.push({error, data}); },
    Date: {now: () => 100}, self: worker ? {} : undefined, window: worker ? undefined : {},
    console: {error(...args) { consoles.push(['error', ...args]); }, trace() { consoles.push(['trace']); }},
    ENDPOINTS: {OPEN_POSITIONS: '/api/v1/private/position/open_positions', ASSETS: '/api/v1/private/account/assets'}};
  vm.runInNewContext(compiled, context);
  const owner = new context.Holder(); const fallback = [{id: 'fallback'}];
  const failure = Object.assign(new Error(fixture.transport), fixture.data === null ? {} : {response: {data: fixture.data}});
  Object.assign(owner, {name: 'mexc', options: {key: 'key', secret: 'secret', testnet: false, publicDataOnly: false},
    isDisposed: false, isPublicDataOnly: false,
    xhr: {async get() { gets++; throw failure; }},
    store: {get positions() { positionReads++; return fallback; }},
    emitter: {emit(name, ...args) { events.push(args); assert.equal(name, 'error'); }}});
  const actualReport = owner.emitAndLogError; let reportInput;
  owner.emitAndLogError = function(value, ...args) { reportInput = value; return actualReport.call(this, value, ...args); };
  owner.noteHttpResponse({status: 503, statusText: 'Unavailable'}, '/prior');
  let result, caught;
  try {
    const pending = owner[method === 'positions' ? 'fetchPositions' : method === 'validate' ? 'validateAccount' : 'getAccount']();
    assert.equal(gets, 1, 'SOURCE reporting eager GET before await');
    if (method === 'positions') { const value = await pending; assert.equal(value, fallback, 'SOURCE fallback live identity'); result = {ids: value.map(p => p.id)}; }
    else if (method === 'validate') result = (await pending).error;
    else await pending;
  } catch (error) { caught = {name: error.name, message: error.message}; }
  assert.equal(gets, 1, 'SOURCE independent reporting GET floor');
  assert.equal(consoles.length, 2, 'SOURCE independent console call floor');
  const coercion = fixture.label === 'coercion';
  assert.equal(events.length, coercion ? 0 : 1, `SOURCE raw emitter call floor ${fixture.label}`);
  assert.equal(positionReads, method === 'positions' && !coercion ? 1 : 0, 'SOURCE reporting fallback access');
  if (coercion || method === 'account') assert.deepEqual(caught, fixture.getError, `SOURCE direct error ${fixture.label}/${method}`);
  else assert.equal(caught, undefined);
  if (!coercion) {
    assert.deepEqual(events[0][0], expectedFormat, `SOURCE native raw formatted value ${fixture.label}/${method}`);
    assert.equal(typeof events[0][0], typeof expectedFormat, 'SOURCE formatted native typeof before wire');
    if (method === 'validate') assert.deepEqual(result, selected, `SOURCE validation raw value ${fixture.label}`);
    const original = worker ? events[0][1].sentryData.error : reports[0].error;
    assert.deepEqual(reportInput, selected, 'SOURCE selected raw report argument');
    if (typeof selected !== 'string') assert.equal(original, reportInput, 'SOURCE raw report input identity');
  }
  let report = null;
  if (!coercion) {
    const data = worker ? events[0][1].sentryData : reports[0].data;
    report = {...data}; delete report.error;
    if (Object.hasOwn(fixture, 'configExpected')) {
      if (fixture.configExpected === null) {
        assert.equal(Object.hasOwn(report.extra, 'config'), true, 'SOURCE own undefined config before JSON projection');
        assert.equal(report.extra.config, undefined, 'SOURCE falsy config value');
      }
      else {
        assert.deepEqual(Array.from(report.extra.config.headers), fixture.configExpected.headers, 'SOURCE independently expected header projection');
        assert.deepEqual(Object.keys(report.extra.config), ['baseURL','timeout','headers'], 'SOURCE exact native config fields');
        assert.equal(report.extra.config.baseURL, undefined); assert.equal(report.extra.config.timeout, undefined);
      }
    }
  }
  let after = null;
  if (coercion) {
    owner.emitAndLogError('after');
    const tags = worker ? events[0][1].sentryData.tags : reports[0].data.tags;
    assert.equal(tags.recentHttpStatus, 503, 'SOURCE coercion leaves correlation unconsumed');
    after = tags.recentHttpStatus;
  }
  totalGets += gets; totalConsoles += consoles.length; totalEvents += events.length;
  totalReports += reports.length; totalPositionReads += positionReads;
  rows.push({worker, label: fixture.label, method, result: result ?? null, error: caught ?? null,
    event: events.length ? events[0][0] : null, reported: !coercion ? shapes(worker ? events[0][1].sentryData.error : reports[0].error) : null,
    // Observe the real Error getters above separately from its enumerable wire shape.
    reportedJson: worker && !coercion ? JSON.stringify(events[0][1].sentryData.error) : null,
    positionReads, after, report});
}
assert.equal(rows.length, 120, 'SOURCE independent raw reporting method root floor');
assert.equal(totalGets, 120, 'SOURCE independent GET call floor');
assert.equal(totalConsoles, 252, 'SOURCE independent console call floor including six correlation replays');
assert.equal(totalEvents, 120, 'SOURCE independent emitter call floor including six correlation replays');
assert.equal(totalReports, 60, 'SOURCE independent main-thread reporting call floor');
assert.equal(totalPositionReads, 38, 'SOURCE independent Store fallback getter floor');
console.log(JSON.stringify(rows));
} else {
  const require = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../mexc-oracle.cjs'));
  const {TinyEmitter} = require('tiny-emitter');
  for (const worker of [false, true]) for (const method of ['positions', 'validate', 'account']) {
    const trace = [], reports = []; let gets = 0, positionReads = 0, firstCalls = 0, secondCalls = 0;
    const context = {exports: {}, recordHttpFailure: http.recordHttpFailure, readHttpFailureContext: http.readHttpFailureContext,
      reportException(error, data) { trace.push('report'); reports.push({error,data}); assert.equal(error, 7, 'SOURCE raw main-thread report before listener'); },
      Date: {now: () => 100}, self: worker ? {} : undefined, window: worker ? undefined : {},
      console: {error() { trace.push('error'); }, trace() { trace.push('trace'); }},
      ENDPOINTS: {OPEN_POSITIONS: '/api/v1/private/position/open_positions', ASSETS: '/api/v1/private/account/assets'}};
    vm.runInNewContext(compiled, context);
    const owner = new context.Holder(); const emitter = new TinyEmitter();
    Object.assign(owner, {name:'mexc', options:{key:'key',secret:'secret'}, isDisposed:false,isPublicDataOnly:false,emitter,
      xhr:{async get() { gets++; trace.push('get'); throw {message:'transport',response:{data:{message:7}}}; }},
      store:{get positions() { positionReads++; return []; }}});
    const original = Object.assign(new Error('listener failed'), {code:'LISTENER'});
    let reportedJson = null;
    emitter.on('error', (value, metadata) => {
      firstCalls++; trace.push('first');
      assert.equal(value, '7', 'SOURCE decoded listener raw formatted value');
      if (worker) {
        assert.equal(metadata.sentryData.error, 7, 'SOURCE worker raw report before listener');
        assert.equal(typeof metadata.sentryData.error, 'number');
        reportedJson = JSON.stringify(metadata.sentryData.error);
        assert.equal(reportedJson, '7', 'SOURCE native Number stringify before listener');
      }
      throw original;
    });
    emitter.on('error', () => { secondCalls++; });
    let caught;
    try { await owner[method === 'positions' ? 'fetchPositions' : method === 'validate' ? 'validateAccount' : 'getAccount'](); }
    catch (error) { caught = error; }
    assert.equal(caught, original, 'SOURCE same listener exception through method');
    assert.deepEqual(trace, worker ? ['get','error','trace','first'] : ['get','error','trace','report','first'], 'SOURCE exact listener child-call order');
    assert.equal(firstCalls,1); assert.equal(secondCalls,0); assert.equal(gets,1); assert.equal(positionReads,0);
    assert.equal(reports.length,worker ? 0 : 1);
    rows.push({worker,method,error:{name:caught.name,message:caught.message,code:caught.code},trace,gets,positionReads,reportedJson});
  }
  assert.equal(rows.length,6,'SOURCE independent throwing-listener root floor');
  console.log(JSON.stringify(rows));
}
