// Live capture architecture is recorded per case; constructor defaults are finite.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import axios from 'axios';
import axiosRateLimit from 'axios-rate-limit';
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const parse = file => ts.createSourceFile(file, readFileSync(new URL(file, root), 'utf8'), ts.ScriptTarget.Latest, true);
const venue = parse('exchanges/bybit/bybit.exchange.ts');
const owner = venue.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitExchange');
const constructor = owner.members.find(ts.isConstructorDeclaration);
const base = parse('exchanges/base.ts');
const baseClass = base.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BaseExchange');
const secondaryMethod = baseClass.members.find(n => n.name?.getText(base) === 'createXhrWithSecondaryKey');
const helper = parse('createApiWithSecondaryKey.ts');
const declaration = helper.statements.filter(ts.isVariableStatement).flatMap(n => n.declarationList.declarations).find(n => n.name.getText(helper) === 'createApiWithSecondaryKey');
assert.ok(constructor && secondaryMethod && declaration?.initializer, 'actual constructor and shared secondary helper required');
const scalarNames = ['unifiedMarginStatus', 'marginType', 'needsReinit', 'lastReinitAt', 'reinitRetryMs', 'positionMode', 'lastWebsocketPositionsUpdate', 'apiKeyInfo', 'batchLimit', 'forceUnifiedBalance'];
const scalarMembers = scalarNames.map(name => {
  const member = owner.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(venue) === name);
  assert.ok(member, `actual scalar initializer ${name}`);
  return member.getText(venue);
}).join('\n');
const getters = ['accountType', 'supportsTradeDerivedCandles'].map(name => {
  const member = owner.members.find(n => ts.isGetAccessorDeclaration(n) && n.name.getText(venue) === name);
  assert.ok(member, `actual getter ${name}`);
  return member.getText(venue);
}).join('\n');
const hooks = ['requiresOrderLinkId', 'validateOrderCapabilities', 'inverseEnabled'].map(name => {
  const member = owner.members.find(n => ts.isMethodDeclaration(n) && n.name.getText(venue) === name);
  assert.ok(member, `actual hook ${name}`);
  return member.getText(venue);
}).join('\n');
const reinit = owner.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(venue) === 'reinitOnThrow');
assert.ok(reinit?.initializer, 'actual reinit operation');
const placementInputs = [
  {symbol:'BTCUSDT', type:'limit', side:'buy', amount:1, price:123},
  {symbol:'BTCUSDT', type:'market', side:'sell', amount:0},
  {symbol:'BTCUSDT', type:'limit', side:'sell', amount:-1, bybitBbo:{sideType:'Queue',level:1}},
  {symbol:'BTCUSDT', type:'market', side:'buy', amount:1, bybitSlippageToleranceTicks:2},
];
const retry = venue.statements.filter(ts.isVariableStatement).flatMap(n => n.declarationList.declarations).find(n => n.name.getText(venue) === 'REINIT_RETRY_MS');
const types = parse('types.ts');
const margin = types.statements.find(n => ts.isEnumDeclaration(n) && n.name.text === 'MarginType');
assert.ok(retry && margin);
const statusInputs = [null, '1', '1.0', '1e0', '1.00000000000000001', '2', '6', '0', '-0', '1e400', '"1"', 'true', 'null', '[]', '[1]', '{}'];
const cases = [];
let sourceRateOptions;
for (const inverse of [undefined, 'enabled', 'external']) for (const secondary of [undefined, '', 'read-key']) for (const failAt of [-1, 0, 1, 2]) {
  const options = {key:'write-key', secret:'write-secret', ...(secondary === undefined ? {} : {secondary_key:secondary, secondary_secret:'read-secret'}),
    ...(inverse === undefined ? {} : {enableInverseMarkets:true}), ...(inverse === 'external' ? {externalPublicData:{type:'websocket',url:'wss://gateway.invalid',exchangeId:'bybit'}} : {})};
  const trace = []; let count = 0;
  const context = vm.createContext({
    getTransport: () => ({publicCorsProxy: () => undefined}),
    createAPI: (opts, parent) => {
      trace.push(['api', opts.key, opts.secret, parent.name]);
      if (count++ === failAt) throw new Error('factory failure');
      return {};
    },
    rateLimit: (xhr, opts) => { assert.equal(opts.maxRPS, 5); sourceRateOptions = {...opts}; return xhr; },
    BybitPublicWebsocket: class { constructor(parent, category) { trace.push(['public', parent.name, category]); } },
    BybitPrivateWebsocket: class { constructor(parent) { trace.push(['private', parent.name]); } },
  });
  const source = `${margin.getText(types).replace(/^export /, '')}\n const ${retry.getText(venue)};\n const createApiWithSecondaryKey = ${declaration.initializer.getText(helper)};
    class Base { constructor(name, options, store) { this.name=name; this.options=options; this.store=store; } ${secondaryMethod.getText(base)} }
    class Subject extends Base { ${scalarMembers} ${getters} ${hooks} ${reinit.getText(venue)} ${constructor.getText(venue)} }
    globalThis.Subject = Subject;`;
  vm.runInContext(ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
  let error = null, state = null, statuses = [], placements = [], reinitializations = [];
  try {
    const subject = new context.Subject('bybit', options, {});
    state = Object.fromEntries(scalarNames.map(name => [name, subject[name] === undefined ? {undefined:true} : subject[name]]));
    for (const name of ['lastReinitAt', 'reinitRetryMs', 'batchLimit']) {
      assert.ok(Number.isFinite(subject[name]), "constructor default bit capture is finite");
      const bytes = Buffer.alloc(8); bytes.writeDoubleBE(subject[name]);
      state[name] = {bits:bytes.toString('hex')};
    }
    state.accountType = subject.accountType;
    state.supportsTradeDerivedCandles = subject.supportsTradeDerivedCandles;
    state.requiresOrderLinkId = subject.requiresOrderLinkId();
    placements = placementInputs.map(opts => {
      try { return {opts, result:typeof subject.validateOrderCapabilities(opts), error:null}; }
      catch (error) { return {opts, result:null, error:error.message}; }
    });
    for (const initial of [false,true]) for (const rejects of [false,true]) {
      subject.needsReinit=initial;
      const original={message:'original request result'};
      let received;
      try { received=await subject.reinitOnThrow(rejects?Promise.reject(original):Promise.resolve(original)); }
      catch (error) { received=error; }
      reinitializations.push({initial,rejects,same:received===original,needsReinit:subject.needsReinit});
    }
    subject.needsReinit=false;
    let reject;
    const pending=new Promise((_resolve,onReject)=>{reject=onReject;});
    // Observe the already-running source tail without awaiting its consumer.
    const abandoned=subject.reinitOnThrow(pending);
    const observed=abandoned.catch(()=>{});
    reject(new Error('late request failure'));
    await observed;
    reinitializations.push({abandoned:true,needsReinit:subject.needsReinit});
    statuses = statusInputs.map(raw => {
      subject.unifiedMarginStatus = raw === null ? undefined : JSON.parse(raw);
      return {raw, accountType:subject.accountType};
    });
  } catch (e) { error = e.message; }
  cases.push({captureArchitecture:process.arch,options, failAt, trace, error, state, statuses, placements, reinitializations});
}
assert.equal(cases.length, 36);
// Run the actual installed rate limiter with the maintained constructor's options.
// Timer registration leaves time unchanged; the sixth request needs a completed window.
const originalTimeout = globalThis.setTimeout;
const originalClearTimeout = globalThis.clearTimeout;
const originalNow = Date.now;
let windowProfiles = 0;
try {
  for (const fixedWallClock of [true, false]) {
    let now = 0;
    const timers = [];
    const delays = [];
    const dispatched = [];
    Date.now = () => fixedWallClock ? 1700000000000 : now;
    globalThis.setTimeout = (callback, delay, ...args) => {
      const timer = {callback: () => callback(...args), deadline: now + delay, active: true};
      timers.push(timer); if (delay === 1000) delays.push(delay); return timer;
    };
    globalThis.clearTimeout = timer => { if (timer) timer.active = false; };
    const flush = async () => { for (let i = 0; i < 64; i++) await Promise.resolve(); };
    const advance = async value => {
      now = value;
      let timer;
      while ((timer = timers.find(candidate => candidate.active && candidate.deadline <= now))) {
        timer.active = false;
        timer.callback();
        await flush();
      }
      await flush();
    };
    const client = axiosRateLimit(axios.create({adapter: async config => {
      dispatched.push([Number(config.url), now]);
      return {config, data: null, status: 200, statusText: 'OK', headers: {}};
    }}), sourceRateOptions);
    const requests = Array.from({length: 6}, (_, i) => client.get(String(i)));
    await flush();
    await advance(0);
    assert.deepEqual(dispatched, Array.from({length: 5}, (_, i) => [i, 0]));
    assert.equal(now, 0);
    assert.deepEqual(delays, [1000]);
    await advance(999);
    assert.equal(dispatched.length, 5);
    await advance(1000);
    await Promise.all(requests);
    assert.deepEqual(dispatched, [...Array.from({length: 5}, (_, i) => [i, 0]), [5, 1000]]);
    assert.deepEqual(delays, [1000, 1000]);
    windowProfiles++;
  }
} finally {
  globalThis.setTimeout = originalTimeout;
  globalThis.clearTimeout = originalClearTimeout;
  Date.now = originalNow;
}
assert.equal(windowProfiles, 2);
process.stdout.write(JSON.stringify(cases));
