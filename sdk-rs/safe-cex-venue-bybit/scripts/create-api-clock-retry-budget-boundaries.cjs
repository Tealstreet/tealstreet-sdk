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
  'parseBybitServerTimestamp', 'hasRetriedClockSync', 'isBybitSignRejection', 'isBybitEnvProbeRequest',
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
const signatureRejection = load('packages/safe-cex/src/utils/signatureRejection.ts');

const inputs = [
  { id: 'clock-only', before: 0, after: 0, resolved: true },
  { id: 'clock-with-three-following-network-retries', before: 0, after: 3, resolved: true },
  { id: 'one-used-before-clock-only-two-left', before: 1, after: 3, resolved: false },
  { id: 'one-before-clock-two-after-clock-succeed', before: 1, after: 2, resolved: true },
  { id: 'three-used-before-clock-no-retries-left', before: 3, after: 1, resolved: false },
];
const ids = ['clock-only', 'clock-with-three-following-network-retries', 'one-used-before-clock-only-two-left',
  'one-before-clock-two-after-clock-succeed', 'three-used-before-clock-no-retries-left'];
const counts = [[0, 0], [0, 0, 1, 2, 3], [0, 1, 1, 2, 3], [0, 1, 1, 2, 3], [0, 1, 2, 3, 3]];
const flags = [[false, true], [false, true, true, true, true], [false, false, true, true, true],
  [false, false, true, true, true], [false, false, false, false, true]];
const successCalls = [1, 4, 0, 4, 0];
assert.equal(inputs.length, 5, 'declared source case floor');
assert.equal(counts.length, 5);
assert.equal(flags.length, 5);
assert.equal(successCalls.length, 5);
assert.deepEqual(inputs.map(input => input.id), ids);
async function run(input, index) {
  let sequence = 0, preFailures = 0, postFailures = 0, syncs = 0, clockSeen = false, authSuccesses = 0;
  let originalLastError, originalLastResponse, originalLastConfig;
  const physical = [], emitted = [];
  const options = { key: 'key', secret: 'secret' };
  const api = load(apiPath, {
    axios, 'axios-retry': retry, qs, nanoid: { nanoid: () => `id-${++sequence}` },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private request'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, caller, exchange) {
        assert.equal(caller, options);
        assert.equal(exchange.name, 'bybitinverse');
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          assert.equal(current.url, '/private');
          assert.equal(current.method, 'get');
          assert.equal(current.headers.get('X-BAPI-SIGN'), 'signature');
          physical.push({ id: current.params.requestId, clockRetry: !!current.safeCexBybitClockSyncRetry,
            retryCount: current['axios-retry'].retryCount });
          if ((!clockSeen && preFailures++ < input.before) || (clockSeen && postFailures++ < input.after)) {
            originalLastError = new axios.AxiosError('network', 'ECONNRESET', current);
            throw originalLastError;
          }
          const wasClockSeen = clockSeen;
          clockSeen = true;
          originalLastConfig = current;
          originalLastResponse = { status: 200, statusText: 'OK', headers: {}, config: current,
            data: wasClockSeen ? { retCode: 0, retMsg: 'OK' } : { retCode: 10002, retMsg: 'recv window rejected' } };
          return originalLastResponse;
        } });
      },
      installTransportAdapter() { assert.fail('no tunnel'); },
    },
    '../../utils/payloadSigner': { canSignPayload: () => true,
      signPayload: async () => ({ signature: 'signature' }) },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: { getCurrentTimeMs: () => syncs ? 2000 : 1000,
      async syncBybitClientServerTime() { syncs++; }, syncFromServerTime() { assert.fail('no server timestamp'); } } },
    './bybit.response': response, './bybit.site': { getBybitSiteHeaders: () => ({}) }, './bybit.types': types,
    '../../utils/retryReadsOnly': retryReads, '../../utils/signatureRejection': signatureRejection,
  }, apiSource.text, { error: (...args) => emitted.push(args) });
  const client = api.createAPI(options, { name: 'bybitinverse', isDemo: false,
    recordAuthSuccess() { authSuccesses++; }, recordAuthFailure() { assert.fail('no auth failure'); },
    emitAndLogError(...args) { emitted.push(args); } });
  let value, error;
  try { value = await client.get('/private'); } catch (cause) { error = cause; }
  assert.equal(value !== undefined, input.resolved, 'source completion');
  assert.equal(physical.length, counts[index].length, 'independent physical attempts');
  assert.deepEqual(physical.map(attempt => attempt.retryCount), counts[index], 'carried retry counts');
  assert.deepEqual(physical.map(attempt => attempt.clockRetry), flags[index], 'clock-reentry flags');
  assert.deepEqual(physical.map(attempt => attempt.id), physical.map((_, index) => `id-${index + 1}`), 'fresh source IDs');
  assert.equal(syncs, 1, 'clock synchronized once');
  assert.equal(authSuccesses, successCalls[index], 'successful response unwind multiplicity');
  assert.deepEqual(emitted, []);
  if (input.resolved) {
    assert.equal(value, originalLastResponse, 'original final response identity');
    assert.equal(value.config, originalLastConfig, 'original transformed config identity');
    assert.equal(value.data.retCode, 0);
    assert.equal(error, undefined);
  } else {
    assert.equal(error, originalLastError, 'original terminal rejection identity');
    assert.equal(error.config['axios-retry'].retryCount, 3, 'terminal inherited budget exhausted');
  }
  return { resolved: value !== undefined, attempts: physical.length, physical, syncs, authSuccesses,
    originalFinalResponse: input.resolved ? value === originalLastResponse : null,
    originalFinalConfig: input.resolved ? value.config === originalLastConfig : null,
    originalError: input.resolved ? null : error === originalLastError };
}
(async () => {
  const cases = [];
  for (let index = 0; index < inputs.length; index++) cases.push({ ...inputs[index], source: await run(inputs[index], index) });
  assert.equal(cases.length, 5, 'executed source floor');
  assert.deepEqual(cases.map(value => value.id), ids);
  process.stdout.write(JSON.stringify({ scope: 'source-only; whole maintained Bybit API/response/read-retry with genuine Axios/retry/qs; scripted common constructor/adapter/signer/clock; no native execution',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0, declaredCases: 5, cases }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
