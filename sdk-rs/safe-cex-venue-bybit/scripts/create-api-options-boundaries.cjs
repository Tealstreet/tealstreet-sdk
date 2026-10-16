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

const crypto = require('node:crypto');
const payloadSigner = load('packages/safe-cex/src/utils/payloadSigner.ts', { 'create-hmac': require('create-hmac') });
const site = load('packages/safe-cex/src/exchanges/bybit/bybit.site.ts');
const inputs = [
  { id: 'between-requests-credentials-live-factory-options-captured', requests: 2, mutation: 'after-first-response' },
  { id: 'nanoid-replaces-key-and-secret', requests: 1, mutation: 'inside-nanoid' },
  { id: 'nanoid-enables-remote-signer', requests: 1, mutation: 'inside-nanoid' },
  { id: 'nanoid-disables-remote-signer', requests: 1, mutation: 'inside-nanoid' },
  { id: 'payload-signer-mutates-key-after-payload-selection', requests: 1, mutation: 'inside-payload-signer' },
  { id: 'pending-payload-signer-replaced-before-resume', requests: 2, mutation: 'while-payload-signer-pending' },
];
const pinnedIds = [
  'between-requests-credentials-live-factory-options-captured', 'nanoid-replaces-key-and-secret',
  'nanoid-enables-remote-signer', 'nanoid-disables-remote-signer',
  'payload-signer-mutates-key-after-payload-selection', 'pending-payload-signer-replaced-before-resume',
];
assert.equal(inputs.length, 6, 'declared source review case floor');
assert.deepEqual(inputs.map(value => value.id), pinnedIds);
const expectations = [
  { payloadKeys: ['key-A', 'key-B'], headerKeys: ['key-A', 'key-B'], secrets: ['secret-A', 'secret-B'] },
  { payloadKeys: ['key-B'], headerKeys: ['key-B'], secrets: ['secret-B'] },
  { payloadKeys: [], headerKeys: [null], secrets: [] },
  { payloadKeys: ['key-A'], headerKeys: ['key-A'], secrets: ['secret-A'] },
  { payloadKeys: ['key-A'], headerKeys: ['key-B'], signatures: ['custom-signature'] },
  { payloadKeys: ['key-A', 'key-B'], headerKeys: ['key-B', 'key-B'], signatures: ['custom-signature', null], secrets: [null, 'secret-B'] },
];
assert.equal(expectations.length, 6, 'independent expectation floor');
function hmac(secret, payload) { return crypto.createHmac('sha256', secret).update(payload).digest('hex'); }
async function run(input, expected) {
  const options = { key: 'key-A', secret: 'secret-A', testnet: false,
    bybitSiteId: 'BRA_BTL', extra: { recvWindow: 5000 } };
  const remoteFunction = async () => { assert.fail('scripted transport must not invoke a remote signer'); };
  if (input.id === 'nanoid-disables-remote-signer') options.remoteSigner = remoteFunction;
  let nextId = 0, successes = 0, failures = 0, constructorCalls = 0;
  let signalEntered, releaseSigner;
  const signerEntered = new Promise(resolve => { signalEntered = resolve; });
  const pendingSignature = new Promise(resolve => { releaseSigner = resolve; });
  const signs = [], physical = [], callbacksSeen = [], canSigns = [], logs = [];
  if (input.id === 'payload-signer-mutates-key-after-payload-selection' || input.id === 'pending-payload-signer-replaced-before-resume') {
    options.payloadSigner = request => {
      assert.equal(request.exchangeName, 'bybitinverse');
      assert.equal(request.scheme, 'hmac-sha256');
      assert.equal(request.encoding, 'hex');
      assert.equal(request.key, 'key-A');
      assert.equal(request.payload, '1000key-A5000requestId=id-1');
      callbacksSeen.push({ key: request.key, payload: request.payload });
      if (input.id === 'payload-signer-mutates-key-after-payload-selection') {
        options.key = 'key-B';
        options.secret = 'secret-B';
        options.remoteSigner = remoteFunction;
        return { signature: 'custom-signature' };
      }
      signalEntered();
      return pendingSignature;
    };
  }
  let factorySnapshot;
  const api = load(apiPath, {
    axios, 'axios-retry': retry, qs,
    nanoid: { nanoid() {
      nextId++;
      if (input.id === 'nanoid-replaces-key-and-secret') { options.key = 'key-B'; options.secret = 'secret-B'; }
      if (input.id === 'nanoid-enables-remote-signer') options.remoteSigner = remoteFunction;
      if (input.id === 'nanoid-disables-remote-signer') options.remoteSigner = undefined;
      return `id-${nextId}`;
    } },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private fixture must not tag public data'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, sourceOptions, sourceExchange) {
        constructorCalls++;
        assert.equal(sourceOptions, options, 'source factory passes caller-owned options identity');
        assert.equal(sourceExchange.name, 'bybitinverse');
        assert.equal(config.timeout, 5000, 'factory recvWindow at construction');
        factorySnapshot = { baseURL: config.baseURL, timeout: config.timeout, site: config.headers['x-site-id'], remote: !!options.remoteSigner };
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          assert.equal(current.method, 'get');
          assert.equal(current.url, '/private');
          assert.equal(current.baseURL, types.BASE_URL.livenet, 'base URL stays factory-captured');
          assert.equal(current.timeout, 20000, 'scripted common constructor timeout');
          assert.equal(current.headers.get('x-site-id'), 'BRA_BTL', 'site header stays factory-captured');
          assert.equal(String(current.headers.get('X-BAPI-RECV-WINDOW')), '5000', 'recvWindow header stays factory-captured');
          physical.push({ id: current.params.requestId, baseURL: current.baseURL, site: current.headers.get('x-site-id'),
            recvWindow: String(current.headers.get('X-BAPI-RECV-WINDOW')),
            key: current.headers.get('X-BAPI-API-KEY') ?? null, signature: current.headers.get('X-BAPI-SIGN') ?? null });
          return { status: 200, statusText: 'OK', headers: {}, config: current, data: { retCode: 0, retMsg: 'OK' } };
        } });
      },
      installTransportAdapter() { assert.fail('fixture must not install a tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload(value) {
        assert.equal(value, options, 'source eligibility sees original options identity');
        canSigns.push({ key: value.key, secret: value.secret, custom: !!value.payloadSigner });
        return payloadSigner.canSignPayload(value);
      },
      async signPayload(value, exchange, payload, encoding) {
        assert.equal(value, options, 'source signer sees original options identity');
        signs.push({ payload, key: value.key, custom: !!value.payloadSigner });
        return payloadSigner.signPayload(value, exchange, payload, encoding);
      },
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: {
      getCurrentTimeMs: () => 1000,
      syncBybitClientServerTime() { assert.fail('no clock sync in options fixture'); },
      syncFromServerTime() { assert.fail('no parsed clock sync in options fixture'); },
    } },
    './bybit.response': response, './bybit.site': site, './bybit.types': types,
    '../../utils/retryReadsOnly': retryReads,
  }, apiSource.text, { error: (...args) => logs.push(args) });
  const client = api.createAPI(options, { name: 'bybitinverse', isDemo: false,
    recordAuthSuccess: () => successes++, recordAuthFailure: () => failures++,
    emitAndLogError: (...args) => logs.push(args) });
  const first = client.request({ method: 'get', url: '/private' });
  if (input.id === 'pending-payload-signer-replaced-before-resume') {
    await signerEntered;
    assert.equal(physical.length, 0, 'pending signer precedes physical dispatch');
    options.key = 'key-B';
    options.secret = 'secret-B';
    options.payloadSigner = undefined;
    releaseSigner({ signature: 'custom-signature' });
  }
  await first;
  if (input.requests === 2) {
    if (input.id === 'between-requests-credentials-live-factory-options-captured') {
      options.key = 'key-B'; options.secret = 'secret-B'; options.testnet = true;
      options.bybitSiteId = undefined; options.extra.recvWindow = 9000;
    }
    await client.request({ method: 'get', url: '/private' });
  }
  assert.equal(constructorCalls, 1);
  assert.equal(nextId, input.requests);
  assert.equal(physical.length, input.requests, 'physical request floor');
  assert.deepEqual(physical.map(value => value.id), input.requests === 2 ? ['id-1', 'id-2'] : ['id-1']);
  assert.deepEqual(physical.map(value => value.key), expected.headerKeys, 'request-time header credentials');
  assert.equal(signs.length, expected.payloadKeys.length, 'actual source signer call count');
  assert.equal(canSigns.length, signs.length, 'live source canSign call count');
  assert.deepEqual(signs.map(value => value.key), expected.payloadKeys, 'credentials selected before await');
  const expectedPayloads = expected.payloadKeys.map((key, index) => `1000${key}5000requestId=id-${index + 1}`);
  assert.deepEqual(signs.map(value => value.payload), expectedPayloads, 'factory recvWindow versus live key');
  const expectedSignatures = expected.payloadKeys.map((_, index) =>
    expected.signatures?.[index] ?? hmac(expected.secrets[index], expectedPayloads[index]));
  assert.deepEqual(physical.map(value => value.signature), expectedSignatures.length ? expectedSignatures : [null], 'actual helper HMAC/custom signatures');
  assert.equal(successes, expectedSignatures.length);
  assert.equal(failures, 0);
  assert.deepEqual(logs, []);
  assert.equal(callbacksSeen.length, input.id.includes('payload-signer') ? 1 : 0);
  assert.deepEqual(factorySnapshot, { baseURL: types.BASE_URL.livenet, timeout: 5000, site: 'BRA_BTL', remote: input.id === 'nanoid-disables-remote-signer' });
  return { factorySnapshot, signs, signerCallbacks: callbacksSeen, physical, authSuccesses: successes, authFailures: failures };
}
(async () => {
  const cases = [];
  for (let i = 0; i < 6; i++) cases.push({ ...inputs[i], source: await run(inputs[i], expectations[i]) });
  assert.equal(cases.length, 6, 'executed source review case floor');
  assert.deepEqual(cases.map(value => value.id), pinnedIds);
  process.stdout.write(JSON.stringify({
    scope: 'source-only; unchanged maintained API/response/read-retry/payload-signer/site bodies and actual Axios/axios-retry/qs/create-hmac; scripted common constructor/physical adapter; caller-owned options and callbacks intentionally supplied across realms; no common HTTP/remote adapter, native factory/crypto or network proof',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0,
    declaredCases: 6, cases,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
