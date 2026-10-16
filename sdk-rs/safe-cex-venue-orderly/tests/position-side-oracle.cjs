const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/orderly/orderly.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = tree.statements.filter(s => ts.isClassDeclaration(s) && s.name?.text === 'OrderlyExchange');
assert.equal(owner.length, 1);
const members = owner[0].members.filter(m => ts.isPropertyDeclaration(m) && m.name?.getText(tree) === 'getPositionSideForOrder');
assert.equal(members.length, 1, 'actual getPositionSideForOrder initializer census');
const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
const types = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const enums = ['PositionMode', 'OrderSide'].map(name => {
  const found = types.statements.filter(s => ts.isEnumDeclaration(s) && s.name.text === name);
  assert.equal(found.length, 1);
  return found[0].getText(types);
});
// Retain the arrow's this binding in an actual fixture class, not an independent rewrite.
const fixtureSource = enums.join('\n') + '\nexport class Fixture { getPositionSideForOrder = ' + members[0].initializer.getText(tree) + '; }';
const code = ts.transpileModule(fixtureSource, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const mod = { exports: {} };
new vm.Script(`(function(module,exports){${code}\n})`, { filename: file }).runInThisContext()(mod, mod.exports);
(async () => {
  const rows = [];
  for (const mode of Object.values(mod.exports.PositionMode)) for (const side of Object.values(mod.exports.OrderSide)) for (const reduceOnly of [undefined, false, true]) {
    const opts = { symbol: 'BTC', side, type: 'limit', amount: 3.25 };
    if (reduceOnly !== undefined) opts.reduceOnly = reduceOnly;
    let calls = 0;
    const fixture = new mod.exports.Fixture();
    fixture.getPositionMode = async () => { calls++; return mode; };
    const result = await fixture.getPositionSideForOrder(opts);
    rows.push({ label: `${mode}/${side}/${reduceOnly}`, opts, mode, calls, kind: typeof result, result: result ?? null });
  }
  const rejected = [];
  for (const side of Object.values(mod.exports.OrderSide)) {
    const fixture = new mod.exports.Fixture();
    const error = { message: 'mode failed', marker: side };
    let calls = 0;
    fixture.getPositionMode = async () => { calls++; throw error; };
    try { await fixture.getPositionSideForOrder({ side, reduceOnly: true }); assert.fail('source must reject'); }
    catch (caught) { rejected.push({ side, calls, sameError: caught === error }); }
  }
  const fixture = new mod.exports.Fixture();
  let calls = 0, resolve, settled = false;
  fixture.getPositionMode = () => { calls++; return new Promise(r => { resolve = r; }); };
  const promise = fixture.getPositionSideForOrder({ side: 'buy', reduceOnly: true }).then(result => { settled = true; return result; });
  const before = { calls, settled };
  resolve('hedged');
  const result = await promise;
  const liveOpts = { side: 'buy', reduceOnly: true };
  const liveFixture = new mod.exports.Fixture();
  let liveResolve, liveCalls = 0;
  liveFixture.getPositionMode = () => { liveCalls++; return new Promise(r => { liveResolve = r; }); };
  const livePromise = liveFixture.getPositionSideForOrder(liveOpts);
  const immediateCalls = liveCalls;
  liveOpts.side = 'sell';
  liveResolve('hedged');
  const liveResult = await livePromise;
  assert.equal(liveResult, 'LONG', 'POSITION-SIDE source reads live options after await');
  console.log(JSON.stringify({ alias: { immediateCalls, result: liveResult }, runtime: { node: process.version, arch: process.arch }, rows, rejected, pending: { before, calls, settled, result } }));
})().catch(error => { console.error(error); process.exitCode = 1; });
