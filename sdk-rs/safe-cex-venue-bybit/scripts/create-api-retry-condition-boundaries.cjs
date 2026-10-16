const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const axios = require('axios');
const retry = require('axios-retry');
const qs = require('qs');

const root = path.resolve(process.env.SAFE_CEX_ROOT || path.join(__dirname, '../../..'));
function parse(file, text = fs.readFileSync(path.join(root, file), 'utf8')) {
  const parsed = ts.createSourceFile(file, text, 99, true);
  assert.equal(parsed.parseDiagnostics.length, 0, `source AST diagnostics: ${file}`);
  return parsed;
}
function load(file, imports = {}, text, outputConsole = console) {
  const source = parse(file, text);
  const compiled = ts.transpileModule(source.text, {
    compilerOptions: { target: 7, module: 1, esModuleInterop: true },
    reportDiagnostics: true,
  });
  assert.equal(compiled.diagnostics.length, 0, `transpile diagnostics: ${file}`);
  const emitted = ts.createSourceFile(file + '.js', compiled.outputText, 99, true, ts.ScriptKind.JS);
  assert.equal(emitted.parseDiagnostics.length, 0, `emitted AST diagnostics: ${file}`);
  const context = vm.createContext({
    exports: {},
    console: outputConsole,
    require(name) {
      assert.ok(Object.hasOwn(imports, name), `undeclared import: ${name}`);
      return imports[name];
    },
  });
  vm.runInContext(compiled.outputText, context);
  return context.exports;
}

const apiPath = 'packages/safe-cex/src/exchanges/bybit/bybit.api.ts';
const apiSource = parse(apiPath);
const moduleFunctions = [];
let factory;
for (const statement of apiSource.statements) {
  if (!ts.isVariableStatement(statement)) continue;
  for (const declaration of statement.declarationList.declarations) {
    if (!declaration.initializer || !ts.isArrowFunction(declaration.initializer)) continue;
    const name = declaration.name.getText(apiSource);
    moduleFunctions.push(name);
    if (name === 'createAPI') factory = declaration.initializer;
  }
}
assert.deepEqual(moduleFunctions, [
  'isSignedRequest', 'isPublicEndpoint', 'isInstrumentsInfoRequest', 'isBybitClockSyncRetMsg',
  'parseBybitServerTimestamp', 'hasRetriedClockSync', 'isBybitEnvProbeRequest',
  'shouldRetryBybitRequest', 'createAPI',
], 'independent module function inventory');
assert.ok(factory);
const callbacks = [];
function visit(node) {
  if (node !== factory && (ts.isArrowFunction(node) || ts.isFunctionExpression(node))) {
    const parent = node.parent;
    if (ts.isVariableDeclaration(parent)) callbacks.push(parent.name.getText(apiSource));
    else if (ts.isPropertyAssignment(parent)) callbacks.push('paramsSerializer.' + parent.name.getText(apiSource));
    else if (ts.isCallExpression(parent)) callbacks.push(parent.expression.getText(apiSource));
    else assert.fail('unclassified createAPI callback');
  }
  ts.forEachChild(node, visit);
}
visit(factory);
assert.deepEqual(callbacks, [
  'paramsSerializer.serialize', 'getOppositeBybitEnvironmentProbe', 'probeOppositeBybitEnvironment',
  'getBybitEnvProbeResult', 'maybePrependBybitEnvMismatch',
  'xhr.interceptors.request.use', 'xhr.interceptors.response.use',
], 'independent createAPI callback inventory');

const typesPath = 'packages/safe-cex/src/exchanges/bybit/bybit.types.ts';
const typesSource = parse(typesPath);
const typeNames = ['RECV_WINDOW', 'BROKER_ID', 'BASE_URL', 'DEMO_URL', 'ENDPOINTS'];
const declarations = typesSource.statements.filter(statement =>
  ts.isVariableStatement(statement) && statement.declarationList.declarations.some(declaration =>
    typeNames.includes(declaration.name.getText(typesSource))));
assert.equal(declarations.length, 5);
const typesProgram = declarations.map(declaration => declaration.getText(typesSource)).join('\n');
parse('assembled-bybit-api-constants.ts', typesProgram);
const types = load('assembled-bybit-api-constants.ts', {}, typesProgram);
assert.deepEqual(Object.keys(types).sort(), [...typeNames].sort());
const response = load('packages/safe-cex/src/exchanges/bybit/bybit.response.ts');
const retryReads = load('packages/safe-cex/src/utils/retryReadsOnly.ts', { 'axios-retry': retry });

const inputs = [
  ['lower-string-substring', '{"ret_msg":"prefix error sign! suffix"}', 'retry'],
  ['camel-string-substring', '{"retMsg":"prefix error sign! suffix"}', 'retry'],
  ['lower-array-exact', '{"ret_msg":["other","error sign!"]}', 'retry'],
  ['camel-array-exact', '{"retMsg":["error sign!"]}', 'retry'],
  ['lower-array-substring-does-not-match', '{"ret_msg":["prefix error sign! suffix"]}', 'original'],
  ['lower-empty-array-falls-through', '{"ret_msg":[],"retMsg":["error sign!"]}', 'retry'],
  ['lower-null-falls-through', '{"ret_msg":null,"retMsg":["error sign!"]}', 'retry'],
  ['lower-empty-object', '{"ret_msg":{}}', 'TypeError'],
  ['lower-number', '{"ret_msg":17}', 'TypeError'],
  ['lower-boolean', '{"ret_msg":false}', 'TypeError'],
  ['lower-own-includes-null', '{"ret_msg":{"includes":null}}', 'TypeError'],
  ['lower-own-includes-string', '{"ret_msg":{"includes":"error sign!"}}', 'TypeError'],
  ['lower-array-true-skips-camel-object', '{"ret_msg":["error sign!"],"retMsg":{}}', 'retry'],
  ['lower-string-true-skips-camel-boolean', '{"ret_msg":"error sign!","retMsg":false}', 'retry'],
  ['lower-array-false-reaches-camel-object', '{"ret_msg":[],"retMsg":{}}', 'TypeError'],
  ['lower-object-throws-before-positive-camel', '{"ret_msg":{},"retMsg":"error sign!"}', 'TypeError'],
  ['lower-empty-string-reaches-camel-number', '{"ret_msg":"","retMsg":0}', 'TypeError'],
  ['camel-array-substring-does-not-match', '{"retMsg":["prefix error sign! suffix"]}', 'original'],
  ['both-null-no-signature-retry', '{"ret_msg":null,"retMsg":null}', 'original'],
  ['json-proto-key-does-not-install-includes', '{"ret_msg":{"__proto__":{"includes":"error sign!"}}}', 'TypeError'],
].map(([id, rawResponse, expected]) => ({ id, rawResponse, expected }));
const pinnedIds = [
  'lower-string-substring', 'camel-string-substring', 'lower-array-exact', 'camel-array-exact',
  'lower-array-substring-does-not-match', 'lower-empty-array-falls-through', 'lower-null-falls-through',
  'lower-empty-object', 'lower-number', 'lower-boolean', 'lower-own-includes-null', 'lower-own-includes-string',
  'lower-array-true-skips-camel-object', 'lower-string-true-skips-camel-boolean',
  'lower-array-false-reaches-camel-object', 'lower-object-throws-before-positive-camel',
  'lower-empty-string-reaches-camel-number', 'camel-array-substring-does-not-match',
  'both-null-no-signature-retry', 'json-proto-key-does-not-install-includes',
];
assert.equal(inputs.length, 20, 'declared source review case floor');
assert.deepEqual(inputs.map(value => value.id), pinnedIds);
assert.equal(inputs.filter(value => value.expected === 'retry').length, 8);
assert.equal(inputs.filter(value => value.expected === 'original').length, 3);
assert.equal(inputs.filter(value => value.expected === 'TypeError').length, 9);
async function run(input) {
  let nextId = 0, authSuccesses = 0, authFailures = 0, sourceError;
  const physical = [], payloads = [], logs = [];
  const options = { key: 'key', secret: 'secret' };
  const api = load(apiPath, {
    axios, 'axios-retry': retry, qs, nanoid: { nanoid: () => `id-${++nextId}` },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private request must not skip remote signing'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, sourceOptions, sourceExchange) {
        assert.equal(sourceOptions, options);
        assert.equal(sourceExchange.name, 'bybitinverse');
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          physical.push({ id: current.params.requestId, method: current.method, url: current.url, data: current.data });
          assert.equal(current.method, 'post');
          assert.equal(current.url, '/private');
          assert.equal(current.headers.get('X-BAPI-SIGN'), 'sig');
          if (physical.length === 1) {
            sourceError = new axios.AxiosError('original venue rejection', 'ERR_BAD_RESPONSE', current, undefined,
              { status: 400, statusText: 'Bad Request', headers: {}, config: current, data: input.rawResponse });
            throw sourceError;
          }
          assert.equal(physical.length, 2, 'unexpected extra physical retry');
          return { status: 200, statusText: 'OK', headers: {}, config: current, data: '{"retCode":0,"retMsg":"OK"}' };
        } });
      },
      installTransportAdapter() { assert.fail('fixture must not install a tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload(value) { assert.equal(value, options); return true; },
      async signPayload(value, exchange, payload, encoding) {
        assert.equal(value, options); assert.equal(exchange, 'bybitinverse'); assert.equal(encoding, 'hex');
        payloads.push(payload); return { signature: 'sig' };
      },
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: {
      getCurrentTimeMs: () => 1000,
      syncBybitClientServerTime() { assert.fail('no clock branch in rejection fixture'); },
      syncFromServerTime() { assert.fail('no clock branch in rejection fixture'); },
    } },
    './bybit.response': response, './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types, '../../utils/retryReadsOnly': retryReads,
  }, apiSource.text, { error: (...args) => logs.push(args) });
  const freshPredicateConfig = { method: 'post', url: '/private', data: { request: true } };
  const freshPredicateInput = new axios.AxiosError('original venue rejection', 'ERR_BAD_RESPONSE',
    freshPredicateConfig, undefined, { status: 400, statusText: 'Bad Request', headers: {},
      config: freshPredicateConfig, data: JSON.parse(input.rawResponse) });
  let freshPredicateResult, freshPredicateError;
  try { freshPredicateResult = api.shouldRetryBybitRequest(freshPredicateInput); }
  catch (error) { freshPredicateError = error; }
  const expectedPredicateMessage = input.id === 'lower-array-false-reaches-camel-object'
    || input.id === 'lower-empty-string-reaches-camel-number'
    ? 'error?.response?.data?.retMsg?.includes is not a function'
    : 'error?.response?.data?.ret_msg?.includes is not a function';
  let predicate;
  if (input.expected === 'TypeError') {
    assert.equal(freshPredicateResult, undefined, 'fresh pure helper does not return after throw');
    assert.equal(freshPredicateError.name, 'TypeError', 'fresh pure helper error class');
    assert.equal(freshPredicateError.message, expectedPredicateMessage, 'fresh pure helper exact error message');
    predicate = { kind: 'error', name: freshPredicateError.name, message: freshPredicateError.message };
  } else {
    assert.equal(freshPredicateError, undefined, 'fresh pure helper does not throw');
    assert.equal(typeof freshPredicateResult, 'boolean', 'fresh pure helper boolean result');
    assert.equal(freshPredicateResult, input.expected === 'retry', 'fresh pure helper independent value');
    predicate = { kind: 'boolean', value: freshPredicateResult };
  }
  const client = api.createAPI(options, { name: 'bybitinverse', isDemo: false,
    recordAuthSuccess: () => authSuccesses++, recordAuthFailure: () => authFailures++,
    emitAndLogError: (...args) => logs.push(args) });
  let result, rejection;
  try { result = await client.request({ method: 'post', url: '/private', data: { request: true } }); }
  catch (error) { rejection = error; }
  const retryExpected = input.expected === 'retry';
  assert.equal(physical.length, retryExpected ? 2 : 1, 'actual retry interceptor physical attempts');
  assert.equal(nextId, physical.length);
  assert.equal(!!result, retryExpected);
  assert.equal(authSuccesses, retryExpected ? 2 : 0, 'successful nested retry response unwind');
  assert.equal(authFailures, 0);
  assert.deepEqual(logs, []);
  assert.deepEqual(sourceError.response.data, JSON.parse(input.rawResponse), 'actual Axios parses rejection response JSON');
  assert.deepEqual(physical.map(value => value.data), retryExpected ? ['{"request":true}', '{"request":true}'] : ['{"request":true}']);
  assert.deepEqual(payloads, retryExpected
    ? ['1000key20000{"request":true}', '1000key20000"{\\"request\\":true}"']
    : ['1000key20000{"request":true}']);
  let predicateResult, predicateError;
  try { predicateResult = api.shouldRetryBybitRequest(sourceError); }
  catch (error) { predicateError = error; }
  if (input.expected === 'TypeError') {
    assert.equal(result, undefined);
    assert.notEqual(rejection, sourceError, 'synchronous retry predicate error replaces original Axios rejection');
    assert.equal(rejection.name, 'TypeError');
    assert.match(rejection.message, /includes is not a function/);
    assert.equal(axios.isAxiosError(rejection), false);
    assert.equal(predicateError.name, 'TypeError');
    assert.equal(predicateError.message, rejection.message, 'same pure-predicate failure class reaches actual Axios caller');
  } else {
    assert.equal(predicateError, undefined);
    assert.equal(predicateResult, retryExpected);
    if (retryExpected) {
      assert.equal(rejection, undefined);
      assert.equal(result.data.retCode, 0);
      assert.equal(result.config.params.requestId, 'id-2');
    } else {
      assert.equal(rejection, sourceError, 'false predicate preserves original Axios rejection identity');
      assert.equal(rejection.code, 'ERR_BAD_RESPONSE');
    }
  }
  if (input.id === 'json-proto-key-does-not-install-includes') {
    assert.equal(Object.hasOwn(sourceError.response.data.ret_msg, '__proto__'), true);
    assert.equal(Object.hasOwn(sourceError.response.data.ret_msg, 'includes'), false);
    assert.equal(sourceError.response.data.ret_msg.includes, undefined);
  }
  return { attempts: physical.length, physical, payloads, authSuccesses, authFailures,
    outcome: retryExpected ? 'resolved-after-retry' : rejection === sourceError ? 'original-rejection' : rejection.name,
    originalRejection: rejection === sourceError, predicate,
    ...(rejection && rejection !== sourceError ? { errorMessage: rejection.message } : {}) };
}
(async () => {
  const cases = [];
  for (const input of inputs) cases.push({ ...input, source: await run(input) });
  assert.equal(cases.length, 20, 'executed source review case floor');
  assert.deepEqual(cases.map(value => value.id), pinnedIds);
  assert.equal(cases.filter(value => value.source.outcome === 'resolved-after-retry').length, 8);
  assert.equal(cases.filter(value => value.source.outcome === 'original-rejection').length, 3);
  assert.equal(cases.filter(value => value.source.outcome === 'TypeError').length, 9);
  assert.equal(cases.filter(value => value.source.predicate.kind === 'boolean' && value.source.predicate.value === true).length, 8);
  assert.equal(cases.filter(value => value.source.predicate.kind === 'boolean' && value.source.predicate.value === false).length, 3);
  assert.equal(cases.filter(value => value.source.predicate.kind === 'error' && value.source.predicate.name === 'TypeError').length, 9);
  process.stdout.write(JSON.stringify({
    scope: 'source-only; raw JSON rejection responses transformed by actual Axios; unchanged maintained API/response/read-retry with actual axios-retry/qs; standard inherited String/Array includes and JSON own noncallable properties, no custom prototype/functions; scripted common constructor/physical adapter and recording signer; no native proof',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0,
    declaredCases: 20, outcomes: { retried: 8, originalRejection: 3, TypeError: 9 }, cases,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
