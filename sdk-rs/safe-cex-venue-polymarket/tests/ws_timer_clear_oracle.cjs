const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const venues = ['public', 'private'];
const cases = [];
for (const venue of venues) {
  const filename = path.resolve(__dirname, `../../../packages/safe-cex/src/exchanges/polymarket/polymarket.ws-${venue}.ts`);
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const name = venue === 'public' ? 'PolymarketPublicWebSocket' : 'PolymarketPrivateWebSocket';
  const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === name);
  assert(klass, name);
  const field = klass.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(ast) === 'pingInterval');
  assert.equal(field.type.getText(ast), 'NodeJS.Timeout', 'opaque truthy Node timer field');
  assert(field.questionToken, 'optional timer field');
  const method = klass.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(ast) === 'clearPingInterval');
  assert(method?.initializer && ts.isArrowFunction(method.initializer), 'source cleanup method');
  const text = ts.transpileModule(`module.exports=function() ${method.initializer.body.getText(ast)}`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  for (const scenario of ['absent', 'present', 'cancel-throws']) {
    const handle = { id: 17 };
    const failure = new Error('host cancellation failed');
    const instance = { pingInterval: scenario === 'absent' ? undefined : handle };
    const calls = [];
    const module = { exports: {} };
    vm.runInNewContext(text, {
      module,
      clearTimeout(timer) {
        assert.equal(timer, handle, 'cancel original native timer handle');
        assert.equal(instance.pingInterval, handle, 'cancel before clearing source field');
        calls.push(timer.id);
        if (scenario === 'cancel-throws') throw failure;
      },
    });
    const steps = [];
    for (let iteration = 0; iteration < 2; iteration++) {
      let failed = false;
      try { module.exports.call(instance); }
      catch (error) { assert.equal(error, failure, 'source host error identity'); failed = true; }
      const expectedCount = scenario === 'absent' ? 0 : scenario === 'cancel-throws' ? iteration + 1 : 1;
      assert.deepEqual(calls, Array(expectedCount).fill(17), 'independent source cancellation floor');
      assert.equal(instance.pingInterval === handle, scenario === 'cancel-throws', 'independent source timer retention');
      assert.equal(failed, scenario === 'cancel-throws', 'independent source cancellation failure');
      steps.push({ calls: [...calls], retained: instance.pingInterval === handle, failed });
    }
    cases.push({ venue, scenario, steps });
  }
}
assert.equal(cases.length, 6, 'two venues by three scenarios');
assert.equal(cases[5].venue, 'private');
assert.equal(cases[5].scenario, 'cancel-throws', 'last timer fixture');
process.stdout.write(JSON.stringify(cases));
