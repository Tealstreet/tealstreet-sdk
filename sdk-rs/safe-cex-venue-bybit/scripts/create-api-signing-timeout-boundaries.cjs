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

const payloadSigner = load('packages/safe-cex/src/utils/payloadSigner.ts', { 'create-hmac': require('create-hmac') });
const inputs = [
  { id: 'async-sign-80-wire-10', signerMs: 80, physicalMs: 10, timeout: 100 },
  { id: 'async-sign-120-wire-10', signerMs: 120, physicalMs: 10, timeout: 100 },
  { id: 'async-sign-0-wire-10', signerMs: 0, physicalMs: 10, timeout: 100 },
  { id: 'async-sign-80-wire-100', signerMs: 80, physicalMs: 100, timeout: 100 },
];
const ids = ['async-sign-80-wire-10', 'async-sign-120-wire-10', 'async-sign-0-wire-10', 'async-sign-80-wire-100'];
const expected = [
  { attempts: 2, resolved: true, timeouts: [100, 90], starts: [1080, 1170], authSuccesses: 2 },
  { attempts: 2, resolved: true, timeouts: [100, 90], starts: [1120, 1250], authSuccesses: 2 },
  { attempts: 2, resolved: true, timeouts: [100, 90], starts: [1000, 1010], authSuccesses: 2 },
  { attempts: 1, resolved: false, timeouts: [100], starts: [1080], authSuccesses: 0 },
];
assert.equal(inputs.length, 4);
assert.equal(expected.length, 4);
assert.deepEqual(inputs.map(input => input.id), ids);
async function run(input, expect) {
  let now = 1000, nextId = 0, authSuccesses = 0, authFailures = 0;
  let firstError, lastPhysicalConfig, lastPhysicalResponse;
  const physical = [], signs = [], events = [];
  const options = { key: 'key', secret: 'secret', payloadSigner: async request => {
    signs.push({ time: now, payload: request.payload });
    assert.equal(request.exchangeName, 'bybitinverse');
    assert.equal(request.key, 'key');
    assert.equal(request.scheme, 'hmac-sha256');
    assert.equal(request.encoding, 'hex');
    await Promise.resolve();
    now += input.signerMs;
    return { signature: 'signature' };
  } };
  const api = load(apiPath, {
    axios, 'axios-retry': retry, qs, nanoid: { nanoid: () => `id-${++nextId}` },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private request'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, caller, exchange) {
        assert.equal(caller, options);
        assert.equal(exchange.name, 'bybitinverse');
        assert.equal(config.timeout, 20000);
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          const retryState = current['axios-retry'];
          assert.equal(current.method, 'get');
          assert.equal(current.url, '/private');
          assert.equal(current.baseURL, types.BASE_URL.livenet);
          assert.equal(current.headers.get('X-BAPI-SIGN'), 'signature');
          assert.equal(retryState.lastRequestTime, now, 'retry timer starts after venue signing');
          physical.push({ time: now, timeout: current.timeout, lastRequestTime: retryState.lastRequestTime,
            id: current.params.requestId, retryCount: retryState.retryCount });
          if (physical.length === 1) {
            now += input.physicalMs;
            firstError = new axios.AxiosError('network', 'ECONNRESET', current);
            throw firstError;
          }
          lastPhysicalConfig = current;
          lastPhysicalResponse = { status: 200, statusText: 'OK', headers: {}, config: current,
            data: { retCode: 0, retMsg: 'OK' } };
          return lastPhysicalResponse;
        } });
      },
      installTransportAdapter() { assert.fail('no tunnel in this profile'); },
    },
    '../../utils/payloadSigner': payloadSigner,
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: {
      getCurrentTimeMs: () => now,
      syncBybitClientServerTime() { assert.fail('not a clock response'); },
      syncFromServerTime() { assert.fail('not a clock response'); },
    } },
    './bybit.response': response,
    './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types,
    '../../utils/retryReadsOnly': retryReads,
  }, apiSource.text, { error: (...args) => events.push(args) });
  const exchange = { name: 'bybitinverse', isDemo: false,
    recordAuthSuccess() { authSuccesses++; }, recordAuthFailure() { authFailures++; },
    emitAndLogError(...args) { events.push(args); } };
  const client = api.createAPI(options, exchange);
  const previousDateNow = Date.now;
  Date.now = () => now;
  let value, rejection;
  try {
    try { value = await client.request({ url: '/private', method: 'get', timeout: input.timeout }); }
    catch (error) { rejection = error; }
  } finally { Date.now = previousDateNow; }
  assert.equal(physical.length, expect.attempts, 'physical attempt floor');
  assert.equal(value !== undefined, expect.resolved, 'source completion');
  assert.equal(signs.length, expect.attempts, 'one awaited signer per attempt');
  assert.equal(nextId, expect.attempts, 'fresh request IDs');
  assert.deepEqual(physical.map(call => call.timeout), expect.timeouts, 'source physical retry budgets');
  assert.deepEqual(physical.map(call => call.time), expect.starts, 'source attempt clock');
  assert.deepEqual(physical.map(call => call.lastRequestTime), expect.starts, 'retry timer after signer');
  assert.equal(authSuccesses, expect.authSuccesses);
  assert.equal(authFailures, 0);
  assert.deepEqual(events, []);
  if (expect.resolved) {
    assert.equal(value, lastPhysicalResponse, 'original source final response identity');
    assert.equal(value.config, lastPhysicalConfig, 'original source transformed config identity');
    assert.equal(value.config.params.requestId, 'id-2');
    assert.equal(rejection, undefined);
  } else {
    assert.equal(rejection, firstError, 'original source rejection identity');
    assert.equal(rejection.config.params.requestId, 'id-1');
  }
  return { resolved: value !== undefined, physical, signs, authSuccesses, authFailures,
    originalFinalResponse: expect.resolved ? value === lastPhysicalResponse : null,
    originalFinalConfig: expect.resolved ? value.config === lastPhysicalConfig : null,
    originalError: expect.resolved ? null : rejection === firstError };
}
(async () => {
  const cases = [];
  for (let index = 0; index < inputs.length; index++) {
    cases.push({ ...inputs[index], source: await run(inputs[index], expected[index]) });
  }
  assert.equal(cases.length, 4, 'executed source witness floor');
  assert.deepEqual(cases.map(test => test.id), ids);
  process.stdout.write(JSON.stringify({
    scope: 'source-only; whole maintained Bybit API, response, payloadSigner and read-retry modules; genuine installed Axios/retry/qs; scripted common constructor/physical adapter and async custom signer; no native execution',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0,
    declaredCases: 4, cases,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
