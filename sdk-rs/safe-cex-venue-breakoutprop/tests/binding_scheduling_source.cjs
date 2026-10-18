// Execute maintained TypeScript bodies; timers are the sole scheduling leaf.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function ast(file) {
  const p = path.join(root, file);
  return ts.createSourceFile(p, fs.readFileSync(p, 'utf8'), ts.ScriptTarget.Latest, true);
}
function body(file) {
  const source = ast(file);
  return source.statements.filter(n => !ts.isImportDeclaration(n))
    .map(n => n.getText(source)).join('\n').replace(/\bexport /g, '');
}
function run(code, context) {
  vm.runInContext(ts.transpileModule(code, {
    compilerOptions: {target: ts.ScriptTarget.ES2022}
  }).outputText, context);
}
(async () => {
  const names = ['fetchSymbolMaxLeverage', 'setLeverage', 'changePositionMode',
    'changeMarginType', 'fetchPublicTrades'];
  const source = ast('packages/safe-cex/src/exchanges/base.ts');
  const base = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BaseExchange');
  assert.ok(base);
  const selected = names.map(name => {
    const matches = base.members.filter(n => n.name?.getText(source) === name);
    assert.equal(matches.length, 1, name);
    return matches[0].getText(source);
  });
  let timers = 0;
  const context = vm.createContext({setTimeout() { timers++; throw Error('unexpected timer'); }});
  run('class Defaults {' + selected.join('\n') + '} globalThis.defaults = new Defaults();', context);
  const defaults = {};
  for (const name of names) {
    for (const [symbol, flag, leverage, limit] of [['BTCUSD', true, 2.25, 7], ['雪/ETH', false, 17.5, null]]) {
      const args = name === 'setLeverage' ? [symbol, leverage]
        : name === 'changePositionMode' || name === 'changeMarginType' ? [flag]
        : name === 'fetchPublicTrades' ? [symbol, limit] : [symbol];
      const before = timers;
      await assert.rejects(context.defaults[name](...args), error => {
        defaults[name] = {error: error.message, error_name: error.name, timers: timers - before};
        return error.name === 'Error' && error.message === 'Not implemented';
      });
      assert.equal(timers, before);
    }
  }
  const loops = [];
  for (const delay of [undefined, 0, -1, NaN, 125, Infinity]) {
    let release;
    let calls = 0;
    const sleepDelays = [], callbackDelays = [];
    const clock = vm.createContext({
      console: {warn() {}},
      setTimeout(callback, ms) { sleepDelays.push(ms); release = callback; }
    });
    run(body('packages/sleep/src/sleep.ts'), clock);
    // Keep the actual sleep closure's timer seam, but execute the loop's own
    // zero-delay timer in Node to distinguish enqueue from complete draining.
    // const bindings are accessed through the context, not global properties.
    clock.sourceSleep = vm.runInContext('sleep', clock);
    const loopContext = vm.createContext({
      sleep: clock.sourceSleep,
      console: {warn() {}},
      setTimeout(callback, ms) { callbackDelays.push(ms); return setTimeout(callback, ms); }
    });
    run(body('packages/safe-cex/src/utils/loop.ts'), loopContext);
    const loop = vm.runInContext('loop', loopContext);
    const pending = loop(() => calls++, delay);
    assert.equal(calls, 0);
    if (delay === Infinity) {
      await pending;
      assert.deepEqual(sleepDelays, []);
      assert.deepEqual(callbackDelays, []);
      loops.push({disabled: true, calls});
      continue;
    }
    assert.deepEqual(sleepDelays, [delay === 125 ? 125 : 5000]);
    release();
    await pending;
    assert.equal(calls, 0);
    assert.deepEqual(callbackDelays, [0]);
    await new Promise(resolve => setTimeout(resolve, 0));
    assert.equal(calls, 1);
    loops.push({sleep_ms: sleepDelays[0], queued_calls: 0, drained_calls: calls, callback_ms: callbackDelays[0]});
  }
  process.stdout.write(JSON.stringify({defaults, loops}));
})().catch(error => { console.error(error); process.exitCode = 1; });
