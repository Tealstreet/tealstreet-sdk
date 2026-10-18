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
  { id: 'shared-mismatch-cached', kind: 'mismatch', cancelFirstCaller: false },
  { id: 'shared-no-mismatch-cached-probe-auth-clock-skipped', kind: 'no-mismatch', cancelFirstCaller: false },
  { id: 'shared-probe-failed-cached', kind: 'probe-failed', cancelFirstCaller: false },
  { id: 'caller-abort-after-response-does-not-cancel-shared-probe', kind: 'mismatch', cancelFirstCaller: true },
];
const pinnedIds = ['shared-mismatch-cached', 'shared-no-mismatch-cached-probe-auth-clock-skipped',
  'shared-probe-failed-cached', 'caller-abort-after-response-does-not-cancel-shared-probe'];
assert.equal(inputs.length, 4, 'declared source review case floor');
assert.deepEqual(inputs.map(value => value.id), pinnedIds);
async function run(input) {
  let nextId = 0, authFailures = 0, authSuccesses = 0, clockSyncs = 0, probes = 0;
  let markProbeEntered, markTwoFailures, releaseProbe;
  const probeEntered = new Promise(resolve => { markProbeEntered = resolve; });
  const twoFailures = new Promise(resolve => { markTwoFailures = resolve; });
  const probeRelease = new Promise(resolve => { releaseProbe = resolve; });
  const physical = [], logs = [], signedPayloads = [], probeResponses = [];
  const options = { key: 'key', secret: 'secret' };
  const controller = new AbortController();
  const api = load(apiPath, {
    axios, 'axios-retry': retry, nanoid: { nanoid: () => `id-${++nextId}` }, qs,
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('no market request in fixture'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, sourceOptions, sourceExchange) {
        assert.equal(sourceOptions, options);
        assert.equal(sourceExchange.name, 'bybit');
        assert.equal(config.baseURL, types.BASE_URL.livenet);
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          const probe = !!current[api.BYBIT_ENV_PROBE_CONFIG_KEY];
          physical.push({ id: current.params.requestId, probe, method: current.method, url: current.url,
            baseURL: current.baseURL, hasSignal: current.signal !== undefined });
          assert.equal(current.method, 'get');
          assert.equal(current.headers.get('X-BAPI-SIGN'), 'sig');
          if (probe) {
            probes++;
            assert.equal(probes, 1, 'one physical probe per factory');
            assert.equal(current.url, types.ENDPOINTS.ACCOUNT);
            assert.equal(current.baseURL, types.DEMO_URL.livenet);
            assert.equal(current.signal, undefined, 'source probe does not inherit caller AbortSignal');
            markProbeEntered();
            await probeRelease;
            if (input.kind === 'probe-failed') {
              throw new axios.AxiosError('scripted probe timeout', 'ECONNABORTED', current);
            }
            const response = { status: 200, statusText: 'OK', headers: {}, config: current,
              data: input.kind === 'mismatch' ? { retCode: 0, retMsg: 'OK' }
                : { retCode: 10003, retMsg: 'recv window rejected server_timestamp[1791518966503]' } };
            probeResponses.push(response);
            return response;
          }
          assert.equal(current.url, '/private');
          assert.equal(current.baseURL, types.BASE_URL.livenet);
          return { status: 200, statusText: 'OK', headers: {}, config: current,
            data: { retCode: 10003, retMsg: 'invalid key' } };
        } });
      },
      installTransportAdapter() { assert.fail('fixture must not install a tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload(value) { assert.equal(value, options); return true; },
      async signPayload(value, exchange, payload, encoding) {
        assert.equal(value, options); assert.equal(exchange, 'bybit'); assert.equal(encoding, 'hex');
        signedPayloads.push(payload); return { signature: 'sig' };
      },
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => false },
    '../../utils/virtualClock': { virtualClock: {
      getCurrentTimeMs: () => 1000,
      async syncBybitClientServerTime() { clockSyncs++; assert.fail('probe must skip fallback clock handler'); },
      syncFromServerTime() { clockSyncs++; assert.fail('probe must skip parsed clock handler'); },
    } },
    './bybit.response': response,
    './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types, '../../utils/retryReadsOnly': retryReads, '../../utils/signatureRejection': signatureRejection,
  }, apiSource.text, { error: (...args) => logs.push(args) });
  const client = api.createAPI(options, { name: 'bybit', isDemo: false,
    recordAuthFailure() { authFailures++; if (authFailures === 2) markTwoFailures(); },
    recordAuthSuccess() { authSuccesses++; }, emitAndLogError: (...args) => logs.push(args) });
  assert.equal('dispose' in client, false, 'source factory exposes Axios client, no factory disposal hook');
  let settledFirst = false, settledSecond = false;
  const first = client.request({ method: 'get', url: '/private',
    ...(input.cancelFirstCaller ? { signal: controller.signal } : {}) });
  const second = client.request({ method: 'get', url: '/private' });
  first.then(() => { settledFirst = true; }, () => { settledFirst = true; });
  second.then(() => { settledSecond = true; }, () => { settledSecond = true; });
  await Promise.all([probeEntered, twoFailures]);
  assert.equal(probes, 1, 'concurrent auth failures share one pending probe');
  assert.equal(physical.length, 3, 'two original attempts plus pending shared probe');
  assert.equal(settledFirst, false, 'first caller awaits shared probe');
  assert.equal(settledSecond, false, 'second caller awaits shared probe');
  if (input.cancelFirstCaller) {
    controller.abort();
    assert.equal(controller.signal.aborted, true);
    assert.equal(settledFirst, false, 'abort after physical response does not settle response interceptor');
    assert.equal(settledSecond, false, 'caller abort does not settle sibling');
  }
  releaseProbe();
  const results = await Promise.all([first, second]);
  assert.equal(settledFirst, true, 'first response observer notified after probe release');
  assert.equal(settledSecond, true, 'second response observer notified after probe release');
  const third = await client.request({ method: 'get', url: '/private' });
  results.push(third);
  assert.equal(probes, 1, 'completed outcome cached across later auth failure');
  assert.equal(physical.length, 4, 'three originals plus exactly one probe');
  assert.equal(nextId, 4);
  assert.equal(authFailures, 3, 'probe does not record auth failure');
  assert.equal(authSuccesses, 0, 'successful probe does not record auth success');
  assert.equal(clockSyncs, 0, 'probe auth+recv payload does not invoke clock handlers');
  assert.deepEqual(logs, []);
  assert.deepEqual(physical.map(value => value.id), ['id-1', 'id-2', 'id-3', 'id-4']);
  assert.deepEqual(physical.map(value => value.probe), [false, false, true, false]);
  assert.deepEqual(physical.map(value => value.hasSignal), input.cancelFirstCaller ? [true, false, false, false] : [false, false, false, false]);
  assert.deepEqual(signedPayloads, [1, 2, 3, 4].map(id => `1000key20000requestId=id-${id}`));
  const expectedMessage = input.kind === 'mismatch'
    ? 'Bybit API key environment mismatch: this key is valid for Bybit Demo Trading, but this account is configured for live Bybit. Create a live Bybit API key or switch the account to Bybit Demo Trading. [invalid key]'
    : 'invalid key';
  assert.equal(results.length, 3, 'three independently observed original responses');
  for (const result of results) {
    assert.equal(result.data.retCode, 10003);
    assert.equal(result.data.retMsg, expectedMessage, 'cached probe outcome applies to every caller');
    assert.equal(!!result.config[api.BYBIT_ENV_PROBE_CONFIG_KEY], false);
  }
  if (input.kind !== 'probe-failed') {
    assert.equal(probeResponses.length, 1);
    assert.deepEqual(probeResponses[0].data, input.kind === 'mismatch'
      ? { retCode: 0, retMsg: 'OK' }
      : { retCode: 10003, retMsg: 'recv window rejected server_timestamp[1791518966503]' }, 'probe response bypasses venue response mutation');
  }
  return { probes, physical, authFailures, authSuccesses, clockSyncs,
    responseMessages: results.map(result => result.data.retMsg),
    callerAbortRequested: input.cancelFirstCaller, callerAbortRejected: false,
    sourceFactoryDisposeHook: false, cachedOutcome: input.kind };
}
(async () => {
  const cases = [];
  for (const input of inputs) {
    let timeout;
    try {
      const source = await Promise.race([run(input), new Promise((_, reject) => {
        timeout = setTimeout(() => reject(new Error(`source fixture deadline: ${input.id}`)), 5000);
      })]);
      cases.push({ ...input, source });
    } finally { clearTimeout(timeout); }
  }
  assert.equal(cases.length, 4, 'executed source review case floor');
  assert.deepEqual(cases.map(value => value.id), pinnedIds);
  process.stdout.write(JSON.stringify({
    scope: 'source-only; unchanged maintained API/response/read-retry with actual Axios/axios-retry/qs; scripted common constructor/physical adapter and recording signer; standard Axios signal only, no invented disposal/cancellation hook; no native lifetime or common HTTP/crypto/remote adapter proof',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0,
    declaredCases: 4, cases,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
