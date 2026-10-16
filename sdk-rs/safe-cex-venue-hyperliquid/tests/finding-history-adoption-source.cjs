'use strict';
// Source-only composition observation, not a native Rust mutation gate.
// Run from the repository root or set SAFE_CEX_ROOT. Source is pinned by git.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = process.env.SAFE_CEX_ROOT || path.resolve(__dirname, "../../..");
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', { paths: [root] }));
const sourcePath = 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts';
const source = fs.readFileSync(path.join(root,sourcePath),'utf8');
const ast = ts.createSourceFile(sourcePath, source, ts.ScriptTarget.Latest, true);
const classes = ast.statements.filter(node => ts.isClassDeclaration(node) && node.name.text === 'HyperliquidExchange');
assert.equal(classes.length, 1, 'independent actual exchange class selection');
const members = classes[0].members.filter(node => ts.isPropertyDeclaration(node) && node.name.getText(ast) === 'fetchHistoricalPositions');
assert.equal(members.length, 1, 'independent actual historical positions initializer selection');
const initializer = members[0].initializer.getText(ast);
assert(ts.isArrowFunction(members[0].initializer), 'actual source arrow initializer');
const generated = ts.transpileModule('exports.make=function(){return (' + initializer + ');};', {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const context = { exports: {}, assert };
// All awaits involving the terminal value run in this same VM realm. Awaiting a
// VM promise from Node's outer realm would itself introduce another assimilation.
vm.runInNewContext(generated + `
exports.done = (async () => {
  let reads = 0;
  const original = {}, marker = {}, opts = {};
  const value = Object.defineProperty(original, 'then', {
    get() {
      reads++;
      if (reads > 1) throw marker;
      return undefined;
    },
  });
  let folds = 0;
  const receiver = {
    foldClosedPositionRecords(actualOpts, scope) {
      folds++;
      assert.equal(actualOpts, opts, 'original opts identity');
      assert.deepEqual(Object.keys(scope), ['symbolScoped'], 'independent scope keys');
      assert.equal(scope.symbolScoped, true, 'independent source scope field');
      return value;
    },
  };
  const actual = exports.make.call(receiver);
  const promise = actual(opts);
  assert.equal(folds, 1, 'independent source fold invocation floor');
  assert.equal(reads, 1, 'source eager async return reads then once');
  assert.equal(await promise, original, 'source original terminal value identity');
  assert.equal(reads, 1, 'same-realm await does not read terminal then again');
  let extraError;
  try {
    await (async () => {
      const adopted = await promise;
      return adopted;
    })();
  } catch (error) { extraError = error; }
  assert.equal(extraError, marker, 'extra wrapper preserves original second-getter rejection');
  assert.equal(reads, 2, 'extra return adoption observes then a second time');
  exports.proof = {
    source_cases: 1,
    last_index: 0,
    last_name: 'direct return versus additional wrapper',
    source_then_reads: 1,
    wrong_bridge_then_reads: 2,
    original_terminal_identity: true,
    second_getter_error_identity: true,
  };
})();
`, context);
// The completion promise fulfills with undefined, so this outer observer cannot
// re-assimilate the instrumented terminal object.
context.exports.done.then(
  () => process.stdout.write(JSON.stringify(context.exports.proof) + '\n'),
  error => { process.stderr.write(String(error.stack || error) + '\n'); process.exitCode = 1; },
);
