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

const { Readable, Writable } = require('node:stream');
const traffic = 'There might be too much traffic or a configuration error';
const inputs = [
  ['native-json-object', 'native', '{"retCode":0}', 'object'],
  ['native-quoted-scalar', 'native', '"ready"', 'string'],
  ['native-quoted-json-object', 'native', '"{\\"retCode\\":0}"', 'string'],
  ['native-prior-transformed-string', 'prior-transformed', '{"retCode":0}', 'object'],
  ['physical-json-object', 'physical', '{"retCode":0}', 'object'],
  ['physical-bom-json-object', 'physical', '\uFEFF{"retCode":0}', 'object'],
  ['native-bom-remains-string', 'native', '\uFEFF{"retCode":0}', 'string'],
  ['native-invalid-json-remains-string', 'native', '{broken', 'string'],
  ['native-empty-string', 'native', '', 'string'],
  ['native-quoted-nul', 'native', '"\\u0000"', 'string'],
  ['native-overflow-number', 'native', '1e400', 'number'],
  ['native-lone-surrogate', 'native', '"\\ud800"', 'string'],
  ['raw-null-skips-call', 'raw', null, 'object'],
  ['raw-empty-object-skips-missing-call', 'raw', {}, 'object'],
  ['raw-own-null-includes-skips-call', 'raw', { includes: null }, 'object'],
  ['raw-own-number-includes-throws', 'raw', { includes: 4 }, 'TypeError'],
  ['raw-number-skips-missing-call', 'raw', 7, 'object'],
  ['raw-array-exact-includes', 'raw', [traffic], 'object'],
  ['raw-string-substring-includes', 'raw', 'prefix ' + traffic + ' suffix', 'object'],
  ['raw-boolean-skips-missing-call', 'raw', false, 'object'],
].map(([id, kind, data, expected]) => ({ id, kind, data, expected }));
const pinnedIds = ['native-json-object', 'native-quoted-scalar', 'native-quoted-json-object',
  'native-prior-transformed-string', 'physical-json-object', 'physical-bom-json-object',
  'native-bom-remains-string', 'native-invalid-json-remains-string', 'native-empty-string',
  'native-quoted-nul', 'native-overflow-number', 'native-lone-surrogate', 'raw-null-skips-call',
  'raw-empty-object-skips-missing-call', 'raw-own-null-includes-skips-call',
  'raw-own-number-includes-throws', 'raw-number-skips-missing-call', 'raw-array-exact-includes',
  'raw-string-substring-includes', 'raw-boolean-skips-missing-call'];
assert.equal(inputs.length, 20, 'declared source review case floor');
assert.deepEqual(inputs.map(value => value.id), pinnedIds);
function observation(value) {
  const kind = value === null ? 'null' : typeof value;
  if (kind === 'string') return { kind, units: Array.from({ length: value.length }, (_, i) => value.charCodeAt(i)) };
  if (kind === 'number') return { kind, value: Number.isFinite(value) ? value : String(value), negativeZero: Object.is(value, -0) };
  return { kind, value };
}
async function run(input) {
  let attempts = 0, nodeTransportCalls = 0, authSuccesses = 0, authFailures = 0;
  let commonData, transportConfig;
  const logs = [], emitted = [], common = [];
  const options = { key: 'key', secret: 'secret' };
  const api = load(apiPath, {
    axios, 'axios-retry': retry, qs, nanoid: { nanoid: () => 'id-1' },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private input'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config) {
        let adapter;
        const transport = {
          request(requestOptions, respond) {
            nodeTransportCalls++;
            assert.equal(requestOptions.hostname, 'source-fixture.invalid');
            const request = new Writable({ autoDestroy: false,
              write(_chunk, _encoding, complete) { complete(); },
              final(complete) {
                queueMicrotask(() => {
                  const response = Readable.from([Buffer.from(input.data, 'utf8')]);
                  response.statusCode = 200; response.statusMessage = 'OK';
                  response.headers = { 'content-type': 'application/json' }; response.req = request;
                  respond(response);
                });
                complete();
              },
            });
            request.setTimeout = () => request;
            return request;
          },
        };
        if (input.kind === 'physical') {
          const http = axios.getAdapter('http');
          adapter = current => {
            attempts++; transportConfig = current;
            return http({ ...current, transport, proxy: false, baseURL: 'http://source-fixture.invalid' });
          };
        } else {
          adapter = async current => {
            attempts++; transportConfig = current;
            return { status: 200, statusText: 'OK', headers: {}, config: current,
              data: input.kind === 'raw'
                ? JSON.stringify({ retCode: 10001, retMsg: 'request rejected', raw_response: input.data })
                : input.data };
          };
        }
        const client = axios.create({ ...config, timeout: 20000, adapter });
        client.interceptors.response.use(response => {
          commonData = response.data;
          common.push(observation(response.data));
          return response;
        });
        return client;
      },
      installTransportAdapter() { assert.fail('no tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload: () => true,
      signPayload: async () => ({ signature: 'sig' }),
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: {
      getCurrentTimeMs: () => 1000,
      syncBybitClientServerTime() { assert.fail('no clock branch'); },
      syncFromServerTime() { assert.fail('no clock branch'); },
    } },
    './bybit.response': response, './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types, '../../utils/retryReadsOnly': retryReads,
  }, apiSource.text, { error: (...args) => logs.push(args) });
  const client = api.createAPI(options, { name: 'bybitinverse', isDemo: false,
    recordAuthSuccess: () => authSuccesses++, recordAuthFailure: () => authFailures++,
    emitAndLogError: (...args) => emitted.push(args) });
  let result, error;
  try { result = await client.request({ method: 'get', url: '/private' }); }
  catch (rejection) { error = rejection; }
  assert.equal(attempts, 1, 'physical source attempt count');
  assert.equal(nodeTransportCalls, input.kind === 'physical' ? 1 : 0, 'actual Node adapter used only for physical bytes');
  assert.equal(common.length, 1, 'response hook sees transformed data once');
  assert.equal(authFailures, 0);
  if (input.expected === 'TypeError') {
    assert.equal(error.name, 'TypeError');
    assert.match(error.message, /raw_response.*includes.*not a function/);
    assert.equal(result, undefined);
    assert.equal(common[0].kind, 'object', 'parsing precedes retry/venue predicate error');
    assert.deepEqual(logs, []);
    assert.deepEqual(emitted, []);
  } else {
    assert.equal(error, undefined);
    assert.equal(typeof result.data, input.expected);
    assert.equal(result.data, commonData, 'source venue retains original transformed data identity');
    assert.deepEqual(observation(result.data), common[0], 'common hook already sees final parse result');
    if (input.kind === 'raw') {
      assert.equal(logs.length, 1);
      assert.equal(logs[0][0], 'bybitinverse error');
      assert.equal(logs[0][1], result.data, 'console receives original data object');
      assert.equal(emitted.length, ['raw-array-exact-includes', 'raw-string-substring-includes'].includes(input.id) ? 1 : 0);
      if (emitted.length) assert.deepEqual(emitted[0], ['Server was unavailable for a request, retrying...']);
    } else {
      assert.deepEqual(logs, []); assert.deepEqual(emitted, []);
    }
  }
  if (['native-json-object', 'native-prior-transformed-string', 'physical-json-object', 'physical-bom-json-object'].includes(input.id)) {
    assert.deepEqual(result.data, { retCode: 0 }); assert.equal(authSuccesses, 1);
  } else {
    assert.equal(authSuccesses, 0);
  }
  if (input.id === 'native-quoted-json-object') assert.equal(result.data, '{"retCode":0}', 'one default parse, no recursive parse');
  if (input.id === 'native-quoted-scalar') assert.equal(result.data, 'ready');
  if (input.id === 'native-bom-remains-string') assert.equal(result.data, '\uFEFF{"retCode":0}');
  if (input.id === 'native-invalid-json-remains-string') assert.equal(result.data, '{broken');
  if (input.id === 'native-empty-string') assert.equal(result.data, '');
  if (input.id === 'native-quoted-nul') assert.deepEqual(observation(result.data).units, [0]);
  if (input.id === 'native-lone-surrogate') assert.deepEqual(observation(result.data).units, [0xd800]);
  if (input.id === 'native-overflow-number') assert.equal(result.data, Infinity);
  assert.equal(transportConfig.params.requestId, 'id-1');
  return { attempts, nodeTransportCalls, authSuccesses, authFailures,
    common: common[0], result: result ? observation(result.data) : { kind: error.name, message: error.message },
    consoleCalls: logs.length, emittedCalls: emitted.length };
}
(async () => {
  const cases = [];
  for (const input of inputs) cases.push({ id: input.id, kind: input.kind, source: await run(input) });
  assert.equal(cases.length, 20, 'executed source review case floor');
  assert.deepEqual(cases.map(value => value.id), pinnedIds);
  process.stdout.write(JSON.stringify({
    scope: 'source-only; maintained API/response/read-retry and actual Axios/axios-retry/qs; physical-byte cases use actual Node HTTP adapter with scripted Writable/Readable transport and no network; other cases supply current native data directly; common observer is registered on actual Axios before venue hooks; prior-transformed label describes current string value, not a source phase flag; UTF16 units observed without serializing decoded lone surrogates; no native proof',
    sourceCallbacks: callbacks, sourceModuleFunctions: moduleFunctions, assembledAstDiagnostics: 0,
    declaredCases: 20, cases,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
