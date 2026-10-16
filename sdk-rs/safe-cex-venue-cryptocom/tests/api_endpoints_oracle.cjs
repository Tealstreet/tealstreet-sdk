// Execute both complete source modules; expose private helpers without changing their bodies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'compiler_path_required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const sourceDirectory = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom');
function execute(name, suffix, requireModule) {
  const file = path.join(sourceDirectory, name);
  const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    fileName: file, reportDiagnostics: true,
    compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true},
  });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0, 'endpoint_transpile');
  const context = vm.createContext({exports: {}, require: requireModule});
  vm.runInContext(compiled.outputText + suffix, context, {filename: file});
  return context.exports;
}
const types = execute('cryptocom.types.ts', '', name => {throw Error(`unexpected_types_import:${name}`);});
const api = execute('cryptocom.api.ts', '\nexports.__public = isPublicEndpoint; exports.__write = isNonRetryablePrivateWrite;', name => {
  if (name === './cryptocom.types') return types;
  if (['axios-retry', '../../safeCexAxiosCreate', './cryptocom.signing'].includes(name)) return {};
  throw Error(`unexpected_api_import:${name}`);
});
const inputs = [
  // null is capture metadata for an omitted URL, not a tested JavaScript null argument.
  null, '', '/public/get-instruments',
  'https://api.crypto.com/exchange/v1/public/get-instruments?x=1',
  'prefix/public/get-book-suffix', '/PUBLIC/get-instruments',
  '/public/get-tickers', '/public/get-ticker', '/public/get-trades',
  '/public/get-candlestick', '/public/get-valuations',
  '/public/get-expired-settlement-price', '/public/get-insurance',
  '/private/create-order', '/private/advanced/create-order',
  '/private/amend-order', '/private/advanced/amend-order',
  '/private/cancel-order', '/private/advanced/cancel-order',
  '/private/cancel-all-orders', '/private/close-position', '/private/change-account-leverage',
  '/private/get-open-orders', '/private/user-balance', '/private/create-order-extra',
  '/public/get-book?next=/private/cancel-order', '🙂/private/close-position',
  '/private/create-withdrawal', '/public/not-real',
];
assert.equal(inputs.length, 29, 'endpoint_source_count');
const cases = inputs.map(input => ({
  input, public: api.__public(input ?? ''), write: api.__write(input ?? undefined),
}));
console.log(JSON.stringify({cases}));
