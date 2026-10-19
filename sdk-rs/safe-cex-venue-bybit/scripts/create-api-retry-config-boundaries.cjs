const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const axios = require('axios');
const retry = require('axios-retry');
const qs = require('qs');
const crypto = require('node:crypto');

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

const remotePath = 'packages/safe-cex/src/remoteSigningAdapter.ts';
const remoteSource = parse(remotePath);
const remoteNames = ['SKIP_REMOTE_SIGNING', 'markSkipRemoteSigning'];
const remoteDeclarations = remoteSource.statements.filter(statement =>
  ts.isVariableStatement(statement) && statement.declarationList.declarations.some(declaration =>
    remoteNames.includes(declaration.name.getText(remoteSource))));
assert.equal(remoteDeclarations.length, 2, 'source remote-tag declaration inventory');
const remoteProgram = remoteDeclarations.map(statement => statement.getText(remoteSource)).join('\n');
parse('assembled-remote-tag.ts', remoteProgram);
const remoteTag = load('assembled-remote-tag.ts', {}, remoteProgram);

const inputs = [
  { id: 'private-get-error-url-markets', method: 'get', url: '/private', failureUrl: types.ENDPOINTS.MARKETS },
  { id: 'markets-get-error-url-private', method: 'get', url: types.ENDPOINTS.MARKETS, failureUrl: '/private' },
  { id: 'private-post-error-method-get', method: 'post', url: '/private', failureMethod: 'get', data: { x: 1 } },
  { id: 'private-get-error-method-post', method: 'get', url: '/private', failureMethod: 'post' },
  { id: 'private-get-error-method-uppercase-get', method: 'get', url: '/private', failureMethod: 'GET' },
  { id: 'signature-error-empty-method', method: 'get', url: '/private', failureMethod: '', signatureError: true },
];
const pinnedIds = [
  'private-get-error-url-markets', 'markets-get-error-url-private',
  'private-post-error-method-get', 'private-get-error-method-post',
  'private-get-error-method-uppercase-get', 'signature-error-empty-method',
];
assert.equal(inputs.length, 6, 'declared review case floor');
assert.deepEqual(inputs.map(input => input.id), pinnedIds, 'independent input IDs');
const expectations = [
  { attempts: 1, resolved: false, retryCondition: false, methods: ['get'], urls: ['/private'], successes: 0 },
  { attempts: 2, resolved: true, retryCondition: true, methods: ['get', 'get'], urls: [types.ENDPOINTS.MARKETS, '/private'], successes: 2 },
  { attempts: 2, resolved: true, retryCondition: true, methods: ['post', 'get'], urls: ['/private', '/private'], successes: 2 },
  { attempts: 1, resolved: false, retryCondition: false, methods: ['get'], urls: ['/private'], successes: 0 },
  { attempts: 2, resolved: true, retryCondition: true, methods: ['get', 'get'], urls: ['/private', '/private'], successes: 2 },
  { attempts: 2, resolved: true, retryCondition: true, methods: ['get', 'get'], urls: ['/private', '/private'], successes: 2 },
];
assert.equal(expectations.length, 6, 'independent expected outcome floor');

let optionalConfigHelpers;
function observeOptionalConfigHelpers(api) {
  const signatureResponse = { status: 400, statusText: 'Bad Request', headers: {},
    config: { method: 'post', url: '/private', headers: new axios.AxiosHeaders() },
    data: { ret_msg: 'error sign!' } };
  const helperInputs = [
    ['network-absent-config', new axios.AxiosError('network', 'ERR_NETWORK'), false],
    ['signature-absent-config', new axios.AxiosError('signature', 'ERR_BAD_RESPONSE', undefined, undefined, signatureResponse), false],
    ['network-omitted-method', new axios.AxiosError('network', 'ERR_NETWORK', { url: '/private', headers: new axios.AxiosHeaders() }), false],
    ['network-empty-method', new axios.AxiosError('network', 'ERR_NETWORK', { method: '', url: '/private', headers: new axios.AxiosHeaders() }), false],
  ];
  assert.equal(helperInputs.length, 4, 'independent optional-config helper floor');
  for (const [, error] of helperInputs) assert.ok(error instanceof axios.AxiosError);
  assert.equal(helperInputs[0][1].config, undefined, 'genuine network error has no config');
  assert.equal(helperInputs[1][1].config, undefined, 'signature result does not require error config');
  assert.equal(Object.hasOwn(helperInputs[2][1].config, 'method'), false, 'method genuinely omitted');
  assert.equal(helperInputs[3][1].config.method, '', 'method explicitly empty');
  const result = helperInputs.map(([id, error, expected]) => {
    const predicate = api.shouldRetryBybitRequest(error);
    assert.equal(typeof predicate, 'boolean', `optional-config helper Boolean ${id}`);
    assert.equal(predicate, expected, `optional-config helper expected predicate ${id}`);
    return { id, predicate };
  });
  assert.deepEqual(result.map(value => value.predicate), [false, false, false, false]);
  return result;
}

async function run(input, expected) {
  let nextId = 0, authSuccesses = 0, authFailures = 0, constructorCalls = 0;
  let firstPhysicalConfig, failureConfig, sourceError, lastPhysicalConfig, lastPhysicalResponse;
  const physical = [], payloads = [], logs = [], tagCalls = [];
  const options = { key: 'key', secret: 'secret' };
  const api = load(apiPath, {
    axios, 'axios-retry': retry, nanoid: { nanoid: () => `id-${++nextId}` }, qs,
    '../../remoteSigningAdapter': { markSkipRemoteSigning(config) {
      tagCalls.push(config.url);
      assert.equal(remoteTag.markSkipRemoteSigning(config), config, 'source remote-tag return identity');
    } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, sourceOptions, sourceExchange) {
        constructorCalls++;
        assert.equal(sourceOptions, options);
        assert.equal(sourceExchange.name, 'bybitinverse');
        assert.equal(config.timeout, 20000);
        assert.equal(config.paramsSerializer.serialize({ requestId: 'id-1' }), 'requestId=id-1');
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          assert.equal(current.baseURL, types.BASE_URL.livenet);
          assert.equal(current.headers.get('X-BAPI-SIGN'), 'sig');
          assert.equal(typeof current.timeout, 'number');
          assert.ok(current.timeout > 0 && current.timeout <= 20000, 'source retry budget remains positive and bounded');
          physical.push({
            method: current.method, url: current.url, id: current.params.requestId,
            bodyPresent: current.data !== undefined,
            ...(current.data === undefined ? {} : { data: current.data }),
            skipRemoteSigning: !!current.safeCexSkipRemoteSigning,
          });
          if (physical.length === 1) {
            firstPhysicalConfig = current;
            failureConfig = { ...current,
              ...(Object.hasOwn(input, 'failureUrl') ? { url: input.failureUrl } : {}),
              ...(Object.hasOwn(input, 'failureMethod') ? { method: input.failureMethod } : {}),
            };
            assert.notEqual(failureConfig, current, 'rewrite changes only error config, not dispatched input');
            sourceError = input.signatureError
              ? new axios.AxiosError('scripted signature rejection', 'ERR_BAD_RESPONSE', failureConfig, undefined,
                { status: 400, statusText: 'Bad Request', headers: {}, config: failureConfig, data: { ret_msg: 'error sign!' } })
              : new axios.AxiosError('scripted network rejection', 'ERR_NETWORK', failureConfig);
            if (input.signatureError) {
              assert.equal(sourceError.code, 'ERR_BAD_RESPONSE');
              assert.equal(sourceError.response.config, failureConfig);
              assert.deepEqual(sourceError.response.data, { ret_msg: 'error sign!' });
              assert.equal(retry.isNetworkError(sourceError), false, 'signature rejection is not a network failure');
            }
            assert.equal(sourceError.config, failureConfig);
            throw sourceError;
          }
          assert.equal(physical.length, 2, 'unexpected extra physical attempt');
          lastPhysicalConfig = current;
          lastPhysicalResponse = { status: 200, statusText: 'OK', headers: {}, config: current,
            data: { retCode: 0, retMsg: 'OK' } };
          return lastPhysicalResponse;
        } });
      },
      installTransportAdapter() { assert.fail('fixture must not install a tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload(value) { assert.equal(value, options); return true; },
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
      getCurrentTimeMs: () => 1000,
      syncBybitClientServerTime() { assert.fail('no clock retry in network fixture'); },
      syncFromServerTime() { assert.fail('no parsed clock sync in fixture'); },
    } },
    './bybit.response': response,
    './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types,
    '../../utils/retryReadsOnly': retryReads, '../../utils/signatureRejection': signatureRejection,
  }, apiSource.text, { error: (...args) => logs.push(args) });
  if (!optionalConfigHelpers) optionalConfigHelpers = observeOptionalConfigHelpers(api);
  const freshPredicateConfig = {
    method: input.failureMethod ?? input.method,
    url: input.failureUrl ?? input.url,
    ...(Object.hasOwn(input, 'data') ? { data: JSON.stringify(input.data) } : {}),
  };
  const freshPredicateInput = input.signatureError
    ? new axios.AxiosError('scripted signature rejection', 'ERR_BAD_RESPONSE', freshPredicateConfig, undefined,
      { status: 400, statusText: 'Bad Request', headers: {}, config: freshPredicateConfig, data: { ret_msg: 'error sign!' } })
    : new axios.AxiosError('scripted network rejection', 'ERR_NETWORK', freshPredicateConfig);
  const predicate = api.shouldRetryBybitRequest(freshPredicateInput);
  assert.equal(typeof predicate, 'boolean', 'fresh network helper returns a Boolean');
  assert.equal(predicate, expected.retryCondition, 'fresh network helper reads the effective failure config');
  const exchange = {
    name: 'bybitinverse', isDemo: false,
    recordAuthSuccess: () => authSuccesses++, recordAuthFailure: () => authFailures++,
    emitAndLogError: (...args) => logs.push(args),
  };
  const client = api.createAPI(options, exchange);
  let result, rejection;
  try {
    result = await client.request({ method: input.method, url: input.url,
      ...(Object.hasOwn(input, 'data') ? { data: input.data } : {}) });
  } catch (error) { rejection = error; }
  assert.equal(constructorCalls, 1);
  assert.equal(physical.length, expected.attempts, 'source physical attempt count');
  assert.equal(nextId, expected.attempts, 'source fresh request IDs');
  assert.equal(!!result, expected.resolved, 'source resolution');
  // The actual retry stamped this config; the pure condition is read on an unstamped copy.
  const unstamped = { ...sourceError.config };
  delete unstamped.safeCexSignatureRetry;
  const fresh = Object.assign(Object.create(Object.getPrototypeOf(sourceError)), sourceError, { config: unstamped });
  assert.equal(api.shouldRetryBybitRequest(fresh), expected.retryCondition, 'source condition reads failure config');
  assert.equal(authSuccesses, expected.successes, 'source retry-chain success unwind');
  assert.equal(authFailures, 0);
  assert.deepEqual(logs, []);
  assert.deepEqual(physical.map(value => value.method), expected.methods, 'independent physical methods');
  assert.deepEqual(physical.map(value => value.url), expected.urls, 'independent physical URLs');
  assert.deepEqual(physical.map(value => value.id), expected.attempts === 2 ? ['id-1', 'id-2'] : ['id-1']);
  assert.equal(firstPhysicalConfig.method, input.method, 'original method was not rewritten');
  assert.equal(firstPhysicalConfig.url, input.url, 'original URL was not rewritten');
  assert.equal(failureConfig.method, input.failureMethod ?? input.method, 'failure method rewrite');
  assert.equal(failureConfig.url, input.failureUrl ?? input.url, 'failure URL rewrite');
  assert.equal(failureConfig.params.requestId, 'id-1', 'failure keeps first attempt ID');
  if (expected.resolved) {
    assert.equal(rejection, undefined);
    assert.equal(result, lastPhysicalResponse, 'source final response identity');
    assert.equal(result.config, lastPhysicalConfig, 'source final config identity');
    assert.notEqual(result.config, failureConfig, 'Axios rebuilds the retried config');
    assert.equal(result.config.params.requestId, 'id-2');
  } else {
    assert.equal(rejection, sourceError, 'source original network rejection identity');
    assert.equal(rejection.config, failureConfig, 'source original failure config identity');
    assert.equal(rejection.code, 'ERR_NETWORK');
  }
  if (input.method === 'post') {
    assert.deepEqual(physical.map(value => value.data), Array(expected.attempts).fill('{"x":1}'), 'transformed POST body survives GET retry');
    assert.deepEqual(payloads, expected.attempts === 2 ? ['1000key20000{"x":1}', '1000key20000requestId=id-2'] : ['1000key20000{"x":1}']);
  } else {
    assert.ok(physical.every(value => !value.bodyPresent));
    assert.deepEqual(payloads, expected.attempts === 2
      ? ['1000key20000requestId=id-1', '1000key20000requestId=id-2']
      : ['1000key20000requestId=id-1']);
  }
  if (input.id === 'private-get-error-method-uppercase-get' || input.id === 'signature-error-empty-method') {
    assert.deepEqual(payloads.map(payload => crypto.createHmac('sha256', 'secret').update(payload).digest('hex')), [
      'd7679158b705d7843f97a42f20daf61e69e0a0508c27764103c814be66e10d24',
      'ba3a0f7ba95d8b58f6fded292ee626c757f5827ff24f3335c0a3a96c5a35b7a6',
    ], 'independent HMACs of actual source signing payloads across uppercase error-config reentry');
  }
  assert.deepEqual(tagCalls, input.id === 'markets-get-error-url-private' ? [types.ENDPOINTS.MARKETS] : []);
  assert.deepEqual(physical.map(value => value.skipRemoteSigning),
    input.id === 'markets-get-error-url-private' ? [true, true] : Array(expected.attempts).fill(false));
  return {
    resolved: !!result, attempts: physical.length, predicate,
    retryCondition: api.shouldRetryBybitRequest(sourceError),
    authSuccesses, authFailures, physical, payloads, tagCalls,
    failureConfig: { method: failureConfig.method, url: failureConfig.url, id: failureConfig.params.requestId,
      ...(failureConfig.data === undefined ? {} : { data: failureConfig.data }) },
    originalRejection: !expected.resolved && rejection === sourceError,
    originalFinalResponse: expected.resolved && result === lastPhysicalResponse,
    originalFinalConfig: expected.resolved && result.config === lastPhysicalConfig,
  };
}
(async () => {
  const cases = [];
  for (let i = 0; i < 6; i++) cases.push({ ...inputs[i], source: await run(inputs[i], expectations[i]) });
  assert.equal(cases.length, 6, 'executed review case floor');
  assert.deepEqual(cases.map(value => value.id), pinnedIds);
  assert.deepEqual(cases.map(value => value.source.predicate), [false, true, true, false, true, true], 'fresh helper independent profile inventory');
  assert.equal(cases.filter(value => value.source.predicate === true).length, 4, 'fresh helper true floor');
  assert.equal(cases.filter(value => value.source.predicate === false).length, 2, 'fresh helper false floor');
  const safetyCases = [];
  for (const action of ['create', 'cancel', 'amend']) {
    const url = `/v5/order/${action}`;
    for (const rewritten of [true, false]) {
      const input = { id: `mutation-${action}-${rewritten ? 'rewritten-get' : 'unchanged-post'}`,
        method: 'post', url, data: { x: 1 }, ...(rewritten ? { failureMethod: 'get' } : {}) };
      const expected = { attempts: rewritten ? 2 : 1, resolved: rewritten, retryCondition: rewritten,
        methods: rewritten ? ['post', 'get'] : ['post'], urls: Array(rewritten ? 2 : 1).fill(url),
        successes: rewritten ? 2 : 0 };
      safetyCases.push({ ...input, source: await run(input, expected) });
    }
  }
  assert.equal(safetyCases.length, 6);
  process.stdout.write(JSON.stringify({
    scope: 'source-only; actual Axios/axios-retry/qs and unchanged maintained API/response/read-retry/remote-tag bodies; scripted constructor/physical adapter and recording signer; no common HTTP wrapper, native factory/crypto or network execution',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0,
    declaredCases: 6, cases, declaredSafetyCases: 6, safetyCases, declaredHelperCases: 4, helperCases: optionalConfigHelpers,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
