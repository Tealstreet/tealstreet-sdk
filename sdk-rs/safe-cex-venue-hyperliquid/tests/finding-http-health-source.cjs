'use strict';
// Source-only review observation: one case, two actual HealthMonitor getter observations.
// Run from the repository root, or set SAFE_CEX_ROOT. This is not a native parity gate.
// Supplied bindings: scalar-body response extractor, direct error forwarding,
// empty error-pattern list and manual timers. Full BaseExchange dispatch is not exercised.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = process.env.SAFE_CEX_ROOT || path.resolve(__dirname, "../../..");
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', { paths: [root] }));
const read = relative => fs.readFileSync(path.join(root, relative), 'utf8');
const transpile = source => ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
function initializer(relative, name) {
  const sf = ts.createSourceFile(relative, read(relative), ts.ScriptTarget.Latest, true);
  const declaration = sf.statements.filter(ts.isVariableStatement)
    .flatMap(statement => [...statement.declarationList.declarations])
    .find(candidate => candidate.name.getText(sf) === name);
  assert(declaration, `actual source initializer ${name}`);
  return declaration.initializer.getText(sf);
}
const healthFile = 'packages/safe-cex/src/health/ExchangeHealthMonitor.ts';
const sf = ts.createSourceFile(healthFile, read(healthFile), ts.ScriptTarget.Latest, true);
const declaration = sf.statements.find(candidate => ts.isClassDeclaration(candidate) && candidate.name.text === 'ExchangeHealthMonitor');
assert(declaration, 'actual HealthMonitor class');
const out = {};
const context = {
  exports: out, DEFAULT_ERROR_PATTERNS: [], EXCHANGE_SPECIFIC_PATTERNS: {}, mergePatterns: () => [],
  setTimeout: () => 1, clearTimeout: () => {}, ENDPOINTS: { EXCHANGE: '/exchange' },
  extractHyperliquidResponseError: () => undefined,
};
vm.runInNewContext(transpile(read('packages/safe-cex/src/utils/httpFailure.ts')), context);
context.describeHttpFailure = out.describeHttpFailure;
vm.runInNewContext(transpile(declaration.getText(sf) + ';exports.Health=ExchangeHealthMonitor;exports.add=(' +
  initializer('packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.api.ts', 'addResponseInterceptor') + ');'), context);
(async () => {
  let interceptor;
  const emitted = [];
  const xhr = { interceptors: { response: { use(callback) { interceptor = callback; } } } };
  const exchange = { emitAndLogError(message) { emitted.push(message); } };
  out.add(xhr, exchange);
  let error;
  try {
    await interceptor({ config: { url: '/exchange' }, status: 422, data: '😀'.repeat(200) });
  } catch (caught) { error = caught; }
  const expected = 'Hyperliquid API error (HTTP 422): "' + '😀'.repeat(149) + '\ud83d';
  assert.equal(error.message, expected, 'actual intercepted Error.message');
  assert.equal(emitted.length, 1, 'independent emission count');
  assert.equal(emitted[0], expected, 'actual intercepted emitted message');
  const monitor = new out.Health('hyperliquid');
  assert.equal(monitor.processMessage(emitted[0], true), 'unknown');
  let observations = 0;
  for (const message of [monitor.getLastError().message, monitor.getState().lastError.message]) {
    assert.equal(message, expected, 'actual HealthMonitor returned message');
    assert.equal(message.charCodeAt(message.length - 1), 0xd83d, 'materialized returned surrogate half');
    observations++;
  }
  assert.equal(observations, 2, 'independent getter observation floor');
  monitor.dispose();
  process.stdout.write(JSON.stringify({
    cases: 1, last: 'actual response interceptor/httpFailure to actual HealthMonitor getters',
    source_last_unit: 'D83D', getter_observations: observations,
    pattern_timer_bindings: 'supplied empty patterns/manual timer',
  }) + '\n');
})().catch(error => { process.stderr.write(error.stack + '\n'); process.exitCode = 1; });
