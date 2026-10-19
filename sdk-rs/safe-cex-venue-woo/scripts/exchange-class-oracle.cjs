const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo/woo.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='WOOXExchange');assert(klass);
const code=ts.transpileModule(klass.getText(ast).replace(/^export /,'this.C='),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
if (process.argv.includes('--class-state')) {
  const {WOOXExchange} = require(file);
  const {DefaultStore} = require(path.resolve(__dirname, '../../../packages/safe-cex/src/store/store.base.ts'));
  const owner = new WOOXExchange('woo', {publicDataOnly:true, applicationId:'fixture'}, new DefaultStore());
  const other = new WOOXExchange('woo', {publicDataOnly:true, applicationId:'fixture'}, new DefaultStore());
  const fields = klass.members.filter(n => ts.isPropertyDeclaration(n) && !n.modifiers?.some(m => m.kind === ts.SyntaxKind.StaticKeyword)).map(n => n.name.getText(ast));
  assert.equal(fields.length, 86);
  const keys = Object.keys(owner).filter(k => fields.includes(k));
  assert.equal(keys.length, 86);
  const arrows = klass.members.filter(n => ts.isPropertyDeclaration(n) && n.initializer && ts.isArrowFunction(n.initializer)).map(n => n.name.getText(ast));
  assert.equal(arrows.length, 68);
  for (const key of arrows) assert.notEqual(owner[key], other[key]);
  const maps = keys.filter(k => owner[k] instanceof Map || owner[k] instanceof Set);
  assert.equal(maps.length, 6);
  for (const key of maps) { assert.equal(owner[key].size, 0); assert.notEqual(owner[key], other[key]); }
  process.stdout.write(JSON.stringify({fields:keys, arrows, maps, constants:Object.fromEntries(['MAX_FILL_POSITION_REFRESH_ATTEMPTS','FILL_POSITION_REFRESH_BASE_DELAY_MS','FUNDING_REFRESH_INTERVAL_MS'].map(k=>[k,WOOXExchange[k]]))}));
  owner.dispose(); other.dispose();
  return;
}
if (process.argv.includes('--bound-response') || process.argv.includes('--fix-audit-boundary') || process.argv.includes('--schedule-cancel')) {
  const utilityFile = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/woo/woo.utils.ts');
  const utility = {exports: {}, require: require('./oracle-runtime.cjs')};
  vm.runInNewContext(ts.transpileModule(fs.readFileSync(utilityFile, 'utf8'), {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true},
  }).outputText, utility);
  const constructed = [], trace = [];
  let subscriptions = 0;
  const clients = [{id:31}, {id:37}, {id:41}], proxy = {id:43};
  let ScheduleBase = class {};
  if (process.argv.includes('--schedule-cancel')) {
    const baseFile = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ts');
    const baseAst = ts.createSourceFile(baseFile, fs.readFileSync(baseFile, 'utf8'), ts.ScriptTarget.Latest, true);
    const base = baseAst.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BaseExchange');
    const member = base.members.find(n => n.name?.getText(baseAst) === 'scheduleCancel');
    assert(member && ts.isPropertyDeclaration(member));
    const scope = {};
    vm.runInNewContext(ts.transpileModule(`class ScheduleBase { ${member.getText(baseAst)} } this.ScheduleBase = ScheduleBase;`, {
      compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
    }).outputText, scope);
    ScheduleBase = scope.ScheduleBase;
  }
  class BaseExchange extends ScheduleBase {
    constructor(name, options) { super(); assert.equal(name, 'woo'); this.options = options; constructed.push(this); }
    on(name, handler) { assert.equal(name, 'fill'); assert.equal(typeof handler, 'function'); subscriptions++; }
  }
  const bound = {
    BaseExchange, GRAFANA_ENDPOINTS_EVERY:'every', GRAFANA_ENDPOINTS_PERIODIC:'periodic',
    ...utility.exports,
    createApiWithSecondaryKey() { trace.push('clients'); return clients; },
    createAPI() { trace.push('proxy'); return proxy; },
    getTransport: () => ({requireProxy: options => options}),
    WooPublicWebsocket: class {constructor(owner) {this.owner = owner;}},
    WooPrivateWebscoket: class {constructor(owner) {this.owner = owner;}},
  };
  vm.runInNewContext(code, bound);
  const owner = new bound.C('woo', {}, {}), other = new bound.C('woo', {}, {});
  assert.equal(constructed.length, 2); assert.equal(subscriptions, 2);
  assert.deepEqual(trace, ['clients', 'proxy', 'clients', 'proxy']);
  for (const instance of constructed) {
    assert.equal(instance.xhr, clients[0]); assert.equal(instance._primaryXhr, clients[1]);
    assert.equal(instance._secondaryXhr, clients[2]); assert.equal(instance._proxyEnforcedXhr, proxy);
    assert.equal(instance.publicWebsocket.owner, instance); assert.equal(instance.privateWebsocket.owner, instance);
  }
  if (process.argv.includes('--schedule-cancel')) {
    (async () => {
      for (const value of [undefined, null, 0, NaN]) {
        await assert.rejects(owner.scheduleCancel(value), error => error.name === 'Error' && error.message === 'Not implemented');
      }
      process.stdout.write(JSON.stringify({name: 'Error', message: 'Not implemented', cases: 4}));
    })().catch(error => { console.error(error); process.exitCode = 1; });
    return;
  }
  if (process.argv.includes('--fix-audit-boundary')) {
    const directory = path.dirname(file), visited = new Set();
    function visit(module) {
      if (visited.has(module)) return;
      assert(!path.basename(module).startsWith('woo.v3.'), `V3 production dependency: ${module}`);
      visited.add(module);
      const tree = ts.createSourceFile(module, fs.readFileSync(module, 'utf8'), ts.ScriptTarget.Latest, true);
      for (const statement of tree.statements) {
        if (!(ts.isImportDeclaration(statement) || ts.isExportDeclaration(statement)) || !statement.moduleSpecifier) continue;
        if (statement.importClause?.isTypeOnly || statement.isTypeOnly) continue;
        const name = statement.moduleSpecifier.text;
        if (name.startsWith('.')) {
          const target = path.resolve(path.dirname(module), name + '.ts');
          if (target.startsWith(directory + path.sep)) visit(target);
        }
      }
    }
    for (const module of ['woo.exchange.ts', 'woo.api.ts', 'woo.ws-public.ts', 'woo.ws-private.ts']) visit(path.join(directory, module));
    const contract = ts.createSourceFile('contracts.ts', fs.readFileSync(path.join(directory, 'woo.v3.contracts.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
    const gate = contract.statements.filter(ts.isVariableStatement).flatMap(s => [...s.declarationList.declarations]).find(d => d.name.getText(contract) === 'WOO_V3_MIGRATION_GATES');
    assert(gate && ts.isCallExpression(gate.initializer));
    const gates = gate.initializer.arguments[0].properties;
    assert.equal(gates.length, 8);
    for (const item of gates) assert.equal(item.initializer.kind, ts.SyntaxKind.FalseKeyword);
    for (const instance of constructed) {
      assert.equal(Object.keys(instance).filter(k => typeof instance[k] === 'function').length, 68);
      assert.equal('WooV3OrderBookRecoveryCoordinator' in instance, false);
      assert.equal('pushUpdate' in instance, false);
      assert.equal('normalizeUpdate' in instance, false);
    }
    process.stdout.write(JSON.stringify({constructed: 2, v3Imports: 0, gates: gates.length, legacyModules: visited.size}));
    process.exit(0);
  }
  const arrow = owner.normalizeRequestedSymbol;
  assert.notEqual(arrow, other.normalizeRequestedSymbol);
  for (const [raw, expected] of [['x:btcusdt.P','BTCUSDT'], ['x:','X'], ['PERP/BTC/USDT','BTCUSDT'],
    ['x:btc_PERP-PERP.P','BTC'], ['ß','SS'], [undefined,undefined], ['',undefined], ['PERP',undefined]]) {
    assert.equal(arrow.call(other, raw), expected, `WOO-CLASS-RESPONSE source default ${raw}`);
  }
  const result = {result:97}, seen = [];
  owner.normalizeTransactionSymbol = function(raw) {assert.equal(this, owner); seen.push(raw); return result;};
  assert.equal(arrow.call(other, 'btc.p'), result);
  assert.equal(arrow.call(other, 'PERP'), result);
  assert.deepEqual(seen, ['BTC', '']);
  const error = Error('original method error');
  owner.normalizeTransactionSymbol = () => {throw error;};
  assert.throws(() => arrow.call(other, 'btc'), caught => caught === error);
  const getterError = Error('distinct getter error'); let getterCalls = 0, methodCalls = 0;
  owner.normalizeTransactionSymbol = () => {methodCalls++;};
  Object.defineProperty(owner, 'normalizeTransactionSymbol', {get() {getterCalls++; throw getterError;}});
  assert.throws(() => arrow.call(other, 'btc'), caught => caught === getterError);
  assert.equal(getterCalls, 1); assert.equal(methodCalls, 0);
  process.stdout.write(JSON.stringify({constructed:2, subscriptions:2, defaultCases:8}));
  process.exit(0);
}
const objects=[],sentinel=Error('factory sentinel'),args=['woo',{},{}];let draws=0;
class BaseExchange{constructor(...input){assert.deepEqual(input,args);objects.push(this);}}
const context={BaseExchange,GRAFANA_ENDPOINTS_EVERY:'every',GRAFANA_ENDPOINTS_PERIODIC:'periodic',createAPI:{},createApiWithSecondaryKey(opts,receiver){assert.equal(receiver,objects.at(-1));assert.equal(opts,args[1]);assert.equal(typeof receiver.isAlgoOrder,'function');throw sentinel;},Math:Object.assign(Object.create(Math),{random:()=>draws})};
vm.runInNewContext(code,context);for(let i=0;i<2;i++)assert.throws(()=>new context.C(...args),e=>e===sentinel);
const [a,b]=objects,keys=Object.keys(a);assert.equal(keys.length,86);const functions=keys.filter(k=>typeof a[k]==='function').map(k=>[a[k].name,a[k].length,Object.getPrototypeOf(a[k]).constructor.name==='AsyncFunction']);assert.equal(functions.length,68);
for(const key of keys){const d=Object.getOwnPropertyDescriptor(a,key);assert(d.writable&&d.enumerable&&d.configurable);if(typeof a[key]==='function'||typeof a[key]==='object'&&a[key]!==null)assert.notEqual(a[key],b[key]);}
for(const[key,value]of[['MAX_FILL_POSITION_REFRESH_ATTEMPTS',4],['FILL_POSITION_REFRESH_BASE_DELAY_MS',150],['FUNDING_REFRESH_INTERVAL_MS',60000]])assert.equal(context.C[key],value);
assert.equal(context.C.prototype.getPositionMode.name,'getPositionMode');assert.equal(context.C.prototype.getPositionMode.length,0);assert.equal(Object.getPrototypeOf(context.C.prototype.getPositionMode).constructor.name,'AsyncFunction');
const containers=['_fillPositionRefreshTimers','_fillPositionRefreshInFlight','_fillExpectedPositionSignatureBySymbol','_positionMetadataCache','_positionMetadataInFlight','_positionMetadataRateLimitedUntil'];
let pairs=0;for(let i=0;i<6;i++)for(let j=i+1;j<6;j++){assert.notEqual(a[containers[i]],a[containers[j]]);pairs++;}assert.equal(pairs,15);
const minted=[0,.5,.9999999999999999].map(x=>{draws=x;return a.clientOrderIdRule.mint();});
const validated=['1','0','01','-1','+1','1.0','1e3',' 1','1\n','1\r','1\u2028','\ufeff1','9007199254740991','9007199254740992'].map(x=>a.clientOrderIdRule.isValid(x));
process.stdout.write(JSON.stringify({keys,functions,minted,validated}));
