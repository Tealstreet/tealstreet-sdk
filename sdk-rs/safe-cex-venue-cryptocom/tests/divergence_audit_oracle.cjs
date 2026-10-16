// Audit observations execute complete maintained modules; only imports are supplied.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'AUDIT-COMPILER-REQUIRED');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const packageRequire = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../package.json'));
const source = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom');
function execute(file, suffix, dependency) {
  const filename = path.join(source, file);
  const compiled = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
    fileName: filename, reportDiagnostics: true,
    compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true},
  });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0, 'AUDIT-SOURCE-TRANSPILE');
  const context = vm.createContext({exports: {}, require: dependency});
  vm.runInContext(compiled.outputText + suffix, context, {filename});
  return context.exports;
}
const types = execute('cryptocom.types.ts', '', name => {throw Error('unexpected type import: ' + name);});
const api = execute('cryptocom.api.ts', '\nexports.__write = isNonRetryablePrivateWrite; exports.__public = isPublicEndpoint;', name => {
  if (name === './cryptocom.types') return types;
  if (['axios-retry', '../../safeCexAxiosCreate', './cryptocom.signing', '../../utils/signatureRejection'].includes(name)) return {};
  throw Error('unexpected API import: ' + name);
});
let placements = 0;
let booleanObservations = 0;
for (const code of [0xd800, 0xdfff]) {
  const half = String.fromCharCode(code);
  for (const endpoint of ['/public/get-book', '/private/create-order']) {
    for (const url of [half + endpoint, endpoint + half, endpoint + '?x=' + half, endpoint.replace('-', '-' + half)]) {
      const replacement = url.toWellFormed();
      for (const classify of [api.__public, api.__write]) {
        const original = classify(url), repaired = classify(replacement);
        assert.equal(typeof original, 'boolean'); assert.equal(typeof repaired, 'boolean');
        assert.equal(original, repaired, 'AUDIT-API-NO-SURROGATE-COST');
        booleanObservations += 2;
      }
      placements++;
    }
  }
}
assert.equal(placements, 16, 'AUDIT-API-PLACEMENT-FLOOR');
assert.equal(booleanObservations, 64, 'AUDIT-API-BOOLEAN-FLOOR');
const mutable = {customBefore: api.__public('/audit/custom'), bookBefore: api.__public('/public/get-book')};
assert.equal(mutable.customBefore, false); assert.equal(mutable.bookBefore, true);
if (process.env.SAFE_CEX_CRYPTOCOM_AUDIT_SOURCE_CONTROL !== 'public-table-noop') types.PUBLIC_ENDPOINTS.push('/audit/custom');
mutable.customAfter = api.__public('/audit/custom');
assert.equal(mutable.customAfter, true, 'AUDIT-SOURCE-MUTABLE-PUBLIC');
const bookIndex = types.PUBLIC_ENDPOINTS.indexOf(types.ENDPOINTS.BOOK); assert.notEqual(bookIndex, -1);
types.PUBLIC_ENDPOINTS.splice(bookIndex, 1);
mutable.bookAfter = api.__public('/public/get-book');
assert.equal(mutable.bookAfter, false, 'AUDIT-SOURCE-MUTABLE-REMOVE');
types.ENDPOINTS.CREATE_ORDER = '/audit/rewritten';
assert.equal(api.__write('/private/create-order'), true, 'AUDIT-PRIVATE-CAPTURED-OLD');
assert.equal(api.__write('/audit/rewritten'), false, 'AUDIT-PRIVATE-CAPTURED-NEW');
let clockCalls = 0;
const signing = execute('cryptocom.signing.ts', '\nexports.__params = paramsToString;', name => {
  if (name === 'create-hmac') return packageRequire('create-hmac');
  if (name === '../../utils/virtualClock') return {virtualClock: {getCurrentTimeMs() { clockCalls++; return 1000; }}};
  throw Error('unexpected signing import: ' + name);
});
const original = JSON.parse('{"\\ud800":1,"\\ue000":2}');
const replacement = JSON.parse('{"\\ufffd":1,"\\ue000":2}');
const units = value => Array.from({length: value.length}, (_, index) => value.charCodeAt(index));
assert.deepEqual(units(signing.__params(original)), [55296,49,57344,50], 'AUDIT-PARAM-KEY-UNITS');
const digest = params => signing.generateSignature('private/create-order', 17, 'api', 'secret', params, 1700123456789);
const originalDigest = digest(original), replacementDigest = digest(replacement);
assert.equal(typeof originalDigest, 'string'); assert.equal(originalDigest.length, 64);
assert.notEqual(originalDigest, replacementDigest, 'AUDIT-KEY-ORDER-HMAC-COST');
assert.equal(signing.generateSignature('\ud800',17,'api','secret',{},1), signing.generateSignature('\ufffd',17,'api','secret',{},1), 'AUDIT-METHOD-ENCODING-CONVERGES');
assert.equal(signing.generateSignature('method',17,'api','\ud800',{},1), signing.generateSignature('method',17,'api','\ufffd',{},1), 'AUDIT-SECRET-ENCODING-CONVERGES');
const params = {counter: 1};
const request = signing.generateAuthenticatedRequest('\ud800', params, '\ud800', 'secret', 97);
assert.equal(clockCalls, 1, 'AUDIT-SOURCE-EAGER-REQUEST-CLOCK');
assert.deepEqual(units(request.method), [55296]); assert.deepEqual(units(request.api_key), [55296]);
assert.equal(request.params, params, 'AUDIT-SOURCE-PARAM-ALIAS');
params.counter = 7;
assert.equal(request.params.counter, 7, 'AUDIT-SOURCE-LIVE-PARAM-ALIAS');
const message = signing.generateWsAuthMessage('\ud800', 'secret');
assert.equal(clockCalls, 3, 'AUDIT-SOURCE-EAGER-WS-CLOCK');
assert.deepEqual(units(message.api_key), [55296]);
console.log(JSON.stringify({header: {node: process.version, placements, booleanObservations, stop: 'index15 DFFF inside private/create-order'}, mutable, originalDigest, replacementDigest, requestMethodUnits: units(request.method), requestKeyUnits: units(request.api_key), wsKeyUnits: units(message.api_key), clockCalls, paramsAlias: request.params === params, mutatedCounter: request.params.counter}));
