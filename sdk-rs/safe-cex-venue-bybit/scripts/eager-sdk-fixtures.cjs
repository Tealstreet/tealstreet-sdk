const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function subject(file, name, members) {
  const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/bybit', file), 'utf8');
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === name);
  assert(cls, name);
  const body = members.map(name => {
    const found = cls.members.filter(member => member.name?.getText(ast) === name);
    assert.equal(found.length, 1, name);
    return found[0].getText(ast);
  }).join('\n');
  const typesFile = path.join(root, 'packages/safe-cex/src/exchanges/bybit/bybit.types.ts');
  const typesAst = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
  const endpoints = typesAst.statements.flatMap(node => ts.isVariableStatement(node)
    ? Array.from(node.declarationList.declarations) : []).filter(node => node.name.getText(typesAst) === 'ENDPOINTS');
  assert.equal(endpoints.length, 1, 'source endpoint declaration');
  const context = vm.createContext({});
  vm.runInContext(ts.transpileModule(`const ${endpoints[0].getText(typesAst)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText, context);
  vm.runInContext(ts.transpileModule(`class Subject { ${body} } globalThis.api = new Subject();`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText, context);
  return context.api;
}
async function main() {
  let cases = 0;
  for (const [file, name] of [
    ['bybit.exchange.ts', 'BybitExchange'],
    ['bybitinverse.exchange.ts', 'BybitInverseExchange'],
    ['bybitspot.exchange.ts', 'BybitSpotExchange'],
  ]) {
    for (const disposed of [false, true]) {
      const api = subject(file, name, ['fetchBalance']);
      let release;
      let reads = 0;
      let formats = 0;
      const raw = { zero: -0, opaque: '\ud800' };
      const result = { total: -0 };
      api.isDisposed = false;
      api.store = { balance: { total: 0 } };
      api.fetchRawBalance = () => { reads++; return new Promise(resolve => { release = resolve; }); };
      api.formatBalance = value => { assert.equal(value, raw); formats++; return result; };
      void api.fetchBalance();
      assert.equal(reads, 1, `${name}: request before caller await`);
      assert.equal(formats, 0, `${name}: held response`);
      api.isDisposed = disposed;
      release(raw);
      await new Promise(setImmediate);
      assert.equal(formats, disposed ? 0 : 1, `${name}: discarded Promise continues with source disposal guard`);
      cases++;
    }
  }
  const api = subject('bybit.exchange.ts', 'BybitExchange', ['changePositionMode']);
  let calls = 0;
  let release;
  api.refuseOptionSymbol = () => {};
  api.changePositionModeHelper = (...args) => {
    assert.deepEqual(args, [true, 'linear', 'USDT']);
    calls++;
    return new Promise(resolve => { release = resolve; });
  };
  void api.changePositionMode(true);
  assert.equal(calls, 1, 'unit member calls original helper before await');
  release();
  await Promise.resolve();
  assert.equal(cases, 6);
  const account = subject('bybit.exchange.ts', 'BybitExchange', ['getAccount']);
  let accountCalls = 0;
  let accountRelease;
  account.xhr = { get: () => {
    accountCalls++;
    return new Promise(resolve => { accountRelease = resolve; });
  } };
  const accountWaiter = account.getAccount();
  assert.equal(accountCalls, 1, 'account member invokes client before await');
  accountRelease({ data: { retCode: 0, result: { userID: 73, inviterID: 91 } } });
  const accountResult = await accountWaiter;
  assert.equal(accountResult.userId, 73);
  assert.equal(accountResult.affiliateId, 91);
  return { balanceProfiles: cases, unitProfiles: 1, accountProfiles: 1 };
}
module.exports = main;
if (require.main === module) {
  main().then(result => process.stdout.write(JSON.stringify(result) + '\n'))
    .catch(error => { console.error(error); process.exitCode = 1; });
}
