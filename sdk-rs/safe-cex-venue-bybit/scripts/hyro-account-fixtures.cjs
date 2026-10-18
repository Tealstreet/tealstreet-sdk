const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function source(file) {
  const text = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges', file), 'utf8');
  return ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
}
function subject() {
  const ast = source('bybit/hyrotrader.exchange.ts');
  const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'HyrotraderExchange');
  const members = ['validateAccount', 'fetchMarginAccountInfos', 'fetchIncomeHistoryRange', 'optionsEnabled', 'inverseEnabled'].map(name => {
    const found = cls.members.filter(node => node.name?.getText(ast) === name);
    assert.equal(found.length, 1, name);
    return found[0].getText(ast);
  });
  const base = source('base.ts');
  const helpers = ['baseCapabilityFallback', 'markBaseCapabilityFallback', 'isBaseCapabilityFallback'].map(name => {
    const found = base.statements.flatMap(node => ts.isVariableStatement(node) ? Array.from(node.declarationList.declarations) : []).filter(node => node.name.getText(base) === name);
    assert.equal(found.length, 1, name);
    return `const ${found[0].getText(base)};`;
  });
  const context = vm.createContext({});
  vm.runInContext(ts.transpileModule(`${helpers.join('\n')} class Subject { ${members.join('\n')} } globalThis.api = new Subject(); globalThis.errorPrototype = Error.prototype; globalThis.branded = isBaseCapabilityFallback;`, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
  return context;
}
async function main() {
  let cases = 0;
  for (const disposed of [false, true]) {
    const {api, branded} = subject();
    api.options = {enableOptionMarkets:true, enableInverseMarkets:true};
    assert.equal(api.optionsEnabled(), false); assert.equal(api.inverseEnabled(), false); cases += 2;
    assert(branded(api.fetchIncomeHistoryRange, 'fetchIncomeHistoryRange'));
    const poison = new Proxy({}, {get(){throw Error('range must ignore arguments');}});
    const a = api.fetchIncomeHistoryRange(poison, NaN, '\ud800');
    const b = api.fetchIncomeHistoryRange(poison);
    assert.notEqual(a, b); assert.notEqual(await a, await b); assert.equal((await a).length, 0); cases++;
    const balance = {total:-0, opaque:'\ud800'}; let release; let calls = 0;
    api.fetchBalance = () => {calls++; return new Promise(resolve => release = resolve);};
    api.isDisposed = false; const writes = []; api.store = {update: value => writes.push(value)};
    const pending = api.fetchMarginAccountInfos();
    assert.equal(calls, 1); assert.equal(writes.length, 0);
    api.isDisposed = disposed; release(balance); assert.equal(await pending, undefined);
    assert.equal(writes.length, disposed ? 0 : 1); if (!disposed) assert.equal(writes[0].balance, balance); cases++;
    api.fetchBalance = async () => balance;
    const result = await api.validateAccount(); assert.equal(result.error, ''); assert.equal(result.balance, balance); cases++;
  }
  for (const kind of ['inherited', 'primitive', 'object', 'raw-message', 'detached']) {
    const context = subject(), {api, errorPrototype} = context;
    let reason, expected;
    if (kind === 'inherited') {expected='\ud800a'; reason=Object.assign(Object.create(errorPrototype),{message:expected});}
    if (kind === 'primitive') {reason=expected='\udfff';}
    if (kind === 'object') {reason={toString:()=> '\ud800-object'}; expected='\ud800-object';}
    if (kind === 'raw-message') {expected={zero:-0};reason=Object.assign(Object.create(errorPrototype),{message:expected});}
    if (kind === 'detached') {reason=vm.runInContext('new Error("original")',context);Object.setPrototypeOf(reason,null);reason.toString=()=> '\ud800-detached';expected='\ud800-detached';}
    for (const synchronous of [false, true]) {
      const logs=[];api.emitAndLogError=value=>logs.push(value);
      api.fetchBalance=()=>{if(synchronous)throw reason;return Promise.reject(reason);};
      const result=await api.validateAccount();assert.equal(result.error,expected);assert.equal(logs.length,1);assert.equal(logs[0],expected);assert(!Object.hasOwn(result,'balance'));cases++;
    }
  }
  const {api} = subject(), reason={text:'\ud800'}, logging={text:'\udfff'};
  api.fetchBalance=()=>Promise.reject(reason);api.emitAndLogError=()=>{throw logging;};
  await assert.rejects(api.validateAccount(), error=>error===logging);cases++;
  assert.equal(cases, 21);
  return {profiles:cases};
}
module.exports=main;
if(require.main===module)main().then(value=>process.stdout.write(JSON.stringify(value)+'\n')).catch(error=>{console.error(error);process.exitCode=1;});
