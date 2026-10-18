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
  { id: 'network-auth-unwind', method: 'get', first: 'ERR_NETWORK', final: 'HTTP200/10003' },
  { id: 'clock-post-body-phase', method: 'post', data: { x: 1 }, first: 'HTTP200/10002/recv window rejected', final: 'HTTP200/0' },
];
const pinnedIds = ['network-auth-unwind', 'clock-post-body-phase'];
assert.equal(inputs.length, 2, 'declared review case floor');
assert.deepEqual(inputs.map(input => input.id), pinnedIds, 'independent input IDs');

async function run(input) {
  let attempts = 0, authFailures = 0, authSuccesses = 0, clock = 1000, nextId = 0;
  let fallbackSyncs = 0, parsedSyncs = 0, signabilityChecks = 0;
  let lastPhysicalConfig, lastPhysicalResponse;
  const payloads = [], configs = [], logs = [];
  const options = { key: 'key', secret: 'secret' };
  const listenerError = Object.assign(new Error('subscriber refused diagnostic'), {
    name: 'DiagnosticSubscriberError', code: { retry: false, sentinel: NaN, ownUndefined: undefined },
  });
  let diagnosticCalls = 0;
  const api = load(apiPath, {
    axios,
    'axios-retry': retry,
    nanoid: { nanoid: () => `id-${++nextId}` },
    qs,
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private fixture must not skip remote signing'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, sourceOptions, sourceExchange) {
        assert.equal(sourceOptions, options);
        assert.equal(sourceExchange.name, 'bybitinverse');
        assert.equal(config.timeout, 20000);
        assert.equal(config.paramsSerializer.serialize({ requestId: 'id-1' }), 'requestId=id-1');
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          attempts++;
          assert.equal(current.url, '/private');
          assert.equal(current.baseURL, types.BASE_URL.livenet);
          assert.equal(current.method, input.method);
          assert.equal(current.headers.get('X-BAPI-SIGN'), 'sig');
          configs.push({
            id: current.params.requestId,
            ...(current.data === undefined ? {} : { data: current.data }),
            clockRetry: !!current.safeCexBybitClockSyncRetry,
          });
          if (input.id === 'network-auth-unwind' && attempts === 1) {
            throw new axios.AxiosError('network', 'ERR_NETWORK', current);
          }
          lastPhysicalConfig = current;
          lastPhysicalResponse = {
            status: 200, statusText: 'OK', headers: {}, config: current,
            data: input.id === 'diagnostic-listener-refusal'
              ? { retCode: 10016, retMsg: 'service unavailable', raw_response: 'There might be too much traffic or a configuration error' }
              : input.id === 'clock-post-body-phase' && attempts === 1
              ? { retCode: 10002, retMsg: 'recv window rejected' }
              : input.id === 'network-auth-unwind'
                ? { retCode: 10003, retMsg: 'invalid key' }
                : { retCode: 0, retMsg: 'OK' },
          };
          return lastPhysicalResponse;
        } });
      },
      installTransportAdapter() { assert.fail('fixture must not install a tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload(value) { assert.equal(value, options); signabilityChecks++; return true; },
      async signPayload(value, exchange, payload, encoding) {
        assert.equal(value, options);
        assert.equal(exchange, 'bybitinverse');
        assert.equal(encoding, 'hex');
        payloads.push(payload);
        return { signature: 'sig' };
      },
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: {
      getCurrentTimeMs: () => clock,
      async syncBybitClientServerTime(...args) { assert.equal(args.length, 0); fallbackSyncs++; clock = 2000; },
      syncFromServerTime() { parsedSyncs++; assert.fail('no parsed timestamp in fixture'); },
    } },
    './bybit.response': response,
    './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types,
    '../../utils/retryReadsOnly': retryReads, '../../utils/signatureRejection': signatureRejection,
  }, apiSource.text, { error: (...args) => logs.push(args) });
  const exchange = {
    name: 'bybitinverse', isDemo: false,
    recordAuthFailure: () => authFailures++, recordAuthSuccess: () => authSuccesses++,
    emitAndLogError: (...args) => {
      if (input.id === 'diagnostic-listener-refusal') {
        assert.equal(args[0], 'Server was unavailable for a request, retrying...');
        diagnosticCalls++;
        throw listenerError;
      }
      logs.push(args);
    },
  };
  const client = api.createAPI(options, exchange);
  if (input.id === 'diagnostic-listener-refusal') {
    await assert.rejects(client.request({ method: 'get', url: '/private' }), error => error === listenerError);
    assert.equal(diagnosticCalls, 1);
    assert.equal(attempts, 1, 'diagnostic rejection does not retry');
    assert.ok(Number.isNaN(listenerError.code.sentinel));
    assert.ok(Object.hasOwn(listenerError.code, 'ownUndefined'));
    assert.equal(listenerError.code.ownUndefined, undefined);
    return { attempts, diagnosticCalls, name: listenerError.name, code: listenerError.code };
  }
  const result = await client.request({
    method: input.method, url: '/private',
    ...(Object.hasOwn(input, 'data') ? { data: input.data } : {}),
  });
  assert.equal(attempts, 2, 'source physical attempt count');
  assert.equal(nextId, 2, 'source fresh IDs on recursive request');
  assert.equal(signabilityChecks, 2);
  assert.equal(parsedSyncs, 0);
  assert.deepEqual(logs, []);
  if (input.id === 'network-auth-unwind') {
    assert.equal(authFailures, 2, 'source auth failures include outer retry-chain unwind');
    assert.equal(authSuccesses, 0);
    assert.equal(fallbackSyncs, 0);
    assert.equal(result.data.retCode, 10003);
    assert.deepEqual(payloads, ['1000key20000requestId=id-1', '1000key20000requestId=id-2']);
    assert.deepEqual(configs, [{ id: 'id-1', clockRetry: false }, { id: 'id-2', clockRetry: false }]);
  } else {
    assert.equal(authFailures, 0);
    assert.equal(authSuccesses, 1);
    assert.equal(fallbackSyncs, 1);
    assert.equal(result.data.retCode, 0);
    assert.deepEqual(payloads, ['1000key20000{"x":1}', '2000key20000{"x":1}']);
    assert.deepEqual(configs, [
      { id: 'id-1', data: '{"x":1}', clockRetry: false },
      { id: 'id-2', data: '{"x":1}', clockRetry: true },
    ]);
  }
  assert.equal(result.config.params.requestId, 'id-2', 'source final request ID');
  assert.equal(result, lastPhysicalResponse, 'source original final response identity');
  assert.equal(result.config, lastPhysicalConfig, 'source original final config identity');
  return {
    attempts, authFailures, authSuccesses, fallbackSyncs, parsedSyncs, payloads, configs,
    originalFinalResponse: result === lastPhysicalResponse, originalFinalConfig: result.config === lastPhysicalConfig,
  };
}

(async () => {
  const cases = [];
  for (const input of inputs) cases.push({ ...input, source: await run(input) });
  assert.equal(cases.length, 2, 'executed review case floor');
  assert.deepEqual(cases.map(value => value.id), pinnedIds);
  const diagnosticListenerRefusal = await run({ id: 'diagnostic-listener-refusal', method: 'get' });
  process.stdout.write(JSON.stringify({
    scope: 'source-only; actual Axios/retry/qs and maintained API/response/read-retry bodies; scripted constructor/adapter and recording signer; common HTTP wrapper, native crypto and native factory not executed',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0,
    declaredCases: 2, cases, diagnosticListenerRefusal,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
