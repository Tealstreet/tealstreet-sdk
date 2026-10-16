const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const cases = [];
for (const [venue, filename, name] of [
  ['exchange', 'polymarket.exchange.ts', 'PolymarketExchange'],
  ['private', 'polymarket.ws-private.ts', 'PolymarketPrivateWebSocket'],
  ['public', 'polymarket.ws-public.ts', 'PolymarketPublicWebSocket'],
]) {
  const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket', filename);
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === name);
  assert(klass, name);
  const method = name => {
    const member = klass.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(ast) === name);
    assert(member?.initializer && ts.isArrowFunction(member.initializer), `actual ${name}`);
    return `${name}=${member.initializer.getText(ast)};`;
  };
  // Preserve super dispatch in the original arrow body. Only the shared parent
  // and cancellation host boundaries are injected; no hand-rewritten dispose.
  const code = ts.transpileModule(`module.exports=class extends Parent { ${venue === 'exchange' ? '' : method('clearPingInterval')} ${method('dispose')} };`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const scenarios = venue === 'exchange' ? ['base-ok', 'base-fails'] : ['empty', 'live', 'base-fails', 'cancel-fails'];
  for (const scenario of scenarios) {
    const trace = [];
    const failure = Object.assign(new Error('parent failure'), { code: 'PARENT', marker: 73 });
    const cancelFailure = new Error('cancel failure');
    const handle = { id: 17 };
    let instance;
    class Parent {
      dispose() {
        trace.push('base');
        if (venue !== 'exchange') {
          assert.equal(instance.pingInterval, undefined, 'timer cleared before parent disposal');
          if (venue === 'public') {
            assert.equal(instance.subscribedAssets.size, 0, 'assets cleared before parent disposal');
            assert.equal(instance.orderbookCallbacks.size, 0, 'callbacks cleared before parent disposal');
          }
        }
        if (scenario === 'base-fails') throw failure;
      }
    }
    const module = { exports: {} };
    vm.runInNewContext(code, { module, Parent, clearTimeout(timer) {
      assert.equal(timer, handle, 'cancel original timer');
      assert.equal(instance.pingInterval, handle, 'cancel before clearing');
      trace.push('cancel');
      if (scenario === 'cancel-fails') throw cancelFailure;
    } });
    instance = new module.exports();
    if (venue !== 'exchange') {
      instance.pingInterval = scenario === 'empty' ? undefined : handle;
      instance.subscribedAssets = new Set(scenario === 'empty' ? [] : ['asset-A', 'asset-B']);
      instance.orderbookCallbacks = new Map(scenario === 'empty' ? [] : [['asset-A', new Set([() => {}, () => {}])], ['asset-B', new Set([() => {}])]]);
    }
    const steps = [];
    for (let iteration = 0; iteration < 2; iteration++) {
      let error = null;
      try { assert.equal(instance.dispose(), undefined, 'source void return'); }
      catch (caught) {
        assert.equal(caught, scenario === 'cancel-fails' ? cancelFailure : failure, 'source propagated error identity');
        error = { name: caught.name, message: caught.message, code: caught.code ?? null, marker: caught.marker ?? null };
      }
      const expectedTrace = venue === 'exchange' || scenario === 'empty'
        ? Array(iteration + 1).fill('base')
        : scenario === 'cancel-fails' ? Array(iteration + 1).fill('cancel')
        : ['cancel', ...Array(iteration + 1).fill('base')];
      assert.deepEqual(trace, expectedTrace, 'independent disposal call order');
      assert.equal(error !== null, ['base-fails', 'cancel-fails'].includes(scenario), 'independent disposal failure floor');
      steps.push({ trace: [...trace], error, ...(venue === 'exchange' ? {} : {
        timerPresent: instance.pingInterval !== undefined,
        assets: [...instance.subscribedAssets],
        callbacks: [...instance.orderbookCallbacks].map(([id, rows]) => [id, rows.size]),
      }) });
    }
    cases.push({ venue, scenario, steps });
  }
}
assert.equal(cases.length, 10, 'independent disposal fixture floor');
assert.equal(cases[9].venue, 'public');
assert.equal(cases[9].scenario, 'cancel-fails', 'last disposal fixture');
process.stdout.write(JSON.stringify(cases));
