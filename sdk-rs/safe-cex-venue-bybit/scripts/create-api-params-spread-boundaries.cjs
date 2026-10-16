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

const payloadSigner = load('packages/safe-cex/src/utils/payloadSigner.ts', { 'create-hmac': require('create-hmac') });
const site = load('packages/safe-cex/src/exchanges/bybit/bybit.site.ts');
const inputs = [
  { id: 'array', params: [17, 23] },
  { id: 'string', params: 'ab' },
  { id: 'number', params: 7 },
  { id: 'boolean', params: false },
  { id: 'null', params: null },
  { id: 'absent' },
];
const ids = ['array', 'string', 'number', 'boolean', 'null', 'absent'];
const expectedParams = [
  { '0': 17, '1': 23, requestId: 'id-1' },
  { '0': 'a', '1': 'b', requestId: 'id-1' },
  { requestId: 'id-1' }, { requestId: 'id-1' }, { requestId: 'id-1' }, { requestId: 'id-1' },
];
const expectedQueries = ['0=17&1=23&requestId=id-1', '0=a&1=b&requestId=id-1',
  'requestId=id-1', 'requestId=id-1', 'requestId=id-1', 'requestId=id-1'];
assert.equal(inputs.length, 6);
assert.equal(expectedParams.length, 6);
assert.equal(expectedQueries.length, 6);
assert.deepEqual(inputs.map(input => input.id), ids);
async function run(input, index, signed) {
  let client, captured, originalResponse, authSuccesses = 0, canSigns = 0, nextId = 0;
  const options = { key: signed ? 'key' : '', secret: signed ? 'secret' : '' };
  const payloads = [];
  const api = load(apiPath, {
    axios, 'axios-retry': retry, qs, nanoid: { nanoid: () => `id-${++nextId}` },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private input'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, caller, exchange) {
        assert.equal(caller, options);
        assert.equal(exchange.name, 'bybitinverse');
        assert.equal(config.paramsSerializer.serialize({ a: ['x y'] }), 'a%5B0%5D=x%20y');
        client = axios.create({ ...config, timeout: 20000, adapter: async current => {
          assert.equal(captured, undefined, 'one physical dispatch');
          captured = current;
          originalResponse = { status: 200, statusText: 'OK', headers: {}, config: current, data: { retCode: 0, retMsg: 'OK' } };
          return originalResponse;
        } });
        return client;
      },
      installTransportAdapter() { assert.fail('no tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload(options) { canSigns++; return payloadSigner.canSignPayload(options); },
      signPayload(options, exchange, payload, encoding) {
        payloads.push(payload);
        return payloadSigner.signPayload(options, exchange, payload, encoding);
      },
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: { getCurrentTimeMs: () => 1000 } },
    './bybit.response': response, './bybit.site': site, './bybit.types': types, '../../utils/retryReadsOnly': retryReads, '../../utils/signatureRejection': signatureRejection,
  }, apiSource.text, { error() { assert.fail('successful input'); } });
  const xhr = api.createAPI(options, { name: 'bybitinverse', isDemo: false,
    recordAuthSuccess() { authSuccesses++; }, recordAuthFailure() { assert.fail('no auth failure'); },
    emitAndLogError() { assert.fail('no exchange error'); } });
  const config = { method: 'get', url: '/private' };
  if (Object.hasOwn(input, 'params')) config.params = input.params;
  const result = await xhr.request(config);
  assert.equal(result, originalResponse, 'original response identity');
  assert.equal(result.config, captured, 'original transformed config identity');
  assert.equal(JSON.stringify(captured.params), JSON.stringify(expectedParams[index]), 'source own-params spread');
  assert.equal(client.getUri(captured), types.BASE_URL.livenet + '/private?' + expectedQueries[index], 'actual factory serializer wire query');
  assert.equal(nextId, 1);
  assert.equal(canSigns, Number(signed), 'key guard before signability');
  assert.deepEqual(payloads, signed ? ['1000key20000' + expectedQueries[index]] : [], 'actual maintained signer payload');
  assert.equal(authSuccesses, Number(signed));
  assert.equal(Boolean(captured.headers.get('X-BAPI-SIGN')), signed);
  return { params: captured.params, wireQuery: client.getUri(captured).split('?')[1], payloads, authSuccesses,
    signature: captured.headers.get('X-BAPI-SIGN') ?? null, originalFinalResponse: result === originalResponse,
    originalFinalConfig: result.config === captured };
}
(async () => {
  const cases = [];
  for (const signed of [false, true]) for (let index = 0; index < inputs.length; index++) {
    cases.push({ id: `${signed ? 'signed' : 'unsigned'}-${inputs[index].id}`, signed, input: inputs[index],
      source: await run(inputs[index], index, signed) });
  }
  assert.equal(cases.length, 12, 'independent executed floor');
  assert.deepEqual(cases.map(value => value.id), [
    'unsigned-array', 'unsigned-string', 'unsigned-number', 'unsigned-boolean', 'unsigned-null', 'unsigned-absent',
    'signed-array', 'signed-string', 'signed-number', 'signed-boolean', 'signed-null', 'signed-absent',
  ]);
  process.stdout.write(JSON.stringify({ scope: 'source-only; whole maintained Bybit API/response/site/payloadSigner/read-retry modules; actual Axios/retry/qs/create-hmac; scripted common constructor/adapter/clock; no native execution',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0, declaredCases: 12, cases }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
