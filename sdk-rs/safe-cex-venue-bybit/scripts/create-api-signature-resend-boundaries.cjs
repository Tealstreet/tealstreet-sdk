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

// Resolved (2xx) sign rejections: resendOnSignatureRejection re-signs once through the
// whole pipeline, sharing its stamp with the rejected-response retry condition.
let inputs = [
  { id: 'resolved-sign-then-ok', replies: [[200, '{"retCode":10004,"retMsg":"error sign!"}'], [200, '{"retCode":0,"retMsg":"OK"}']] },
  { id: 'resolved-sign-twice', replies: [[200, '{"retCode":10004,"retMsg":"error sign!"}'], [200, '{"retCode":10004,"retMsg":"error sign!"}']] },
  { id: 'rejected-then-resolved-sign', replies: [[400, '{"retCode":10004,"retMsg":"error sign!"}'], [200, '{"retCode":10004,"retMsg":"error sign!"}']] },
  { id: 'resolved-other-error', replies: [[200, '{"retCode":10001,"retMsg":"params error"}']] },
];
const pinnedIds = ['resolved-sign-then-ok', 'resolved-sign-twice', 'rejected-then-resolved-sign', 'resolved-other-error'];
assert.deepEqual(inputs.map(value => value.id), pinnedIds, 'independent input IDs');
let expectations = {
  'resolved-sign-then-ok': { attempts: 2, retCode: 0, authFailures: 0, authSuccesses: 1, rejected: false },
  'resolved-sign-twice': { attempts: 2, retCode: 10004, authFailures: 1, authSuccesses: 0, rejected: false },
  // axios-retry's resend runs the chain, then the original chain's interceptor runs again.
  'rejected-then-resolved-sign': { attempts: 2, retCode: 10004, authFailures: 2, authSuccesses: 0, rejected: false },
  'resolved-other-error': { attempts: 1, retCode: 10001, authFailures: 0, authSuccesses: 0, rejected: false },
};
const nativeBudgets = process.argv.includes('--native-retry-budgets');
const retryBudgets = nativeBudgets || process.argv.includes('--retry-budgets');
if (retryBudgets) {
  inputs = [
    { id: 'rejected-sign-zero', retries: 0, replies: [[400, '{"retCode":10004,"retMsg":"error sign!"}'], [200, '{"retCode":0,"retMsg":"OK"}']] },
    { id: 'resolved-sign-zero', retries: 0, replies: [[200, '{"retCode":10004,"retMsg":"error sign!"}'], [200, '{"retCode":0,"retMsg":"OK"}']] },
    ...[0, 1, 1.5, 5].map((retries, i) => ({ id: ['network-zero', 'network-one', 'network-fraction', 'network-five'][i],
      retries, network: true, replies: Array.from({ length: 7 }, () => [0, '']) })),
  ];
  assert.deepEqual(inputs.map(value => value.id), ['rejected-sign-zero', 'resolved-sign-zero', 'network-zero', 'network-one', 'network-fraction', 'network-five']);
  expectations = Object.fromEntries([
    ['rejected-sign-zero', 1, true, 10004], ['resolved-sign-zero', 2, false, 0],
    ['network-zero', 1, true, null], ['network-one', 2, true, null],
    ['network-fraction', 3, true, null], ['network-five', 6, true, null],
  ].map(([id, attempts, rejected, retCode]) => [id, { attempts, rejected, retCode,
    authFailures: 0, authSuccesses: id === 'resolved-sign-zero' ? 1 : 0 }]));
}
if (nativeBudgets) {
  inputs = [{ id: 'network-own-undefined', retries: undefined }, { id: 'network-nan', retries: NaN }]
    .map(value => ({ ...value, network: true, replies: Array.from({ length: 7 }, () => [0, '']) }));
  expectations = Object.fromEntries(inputs.map(value => [value.id, { attempts: 1, rejected: true, retCode: null, authFailures: 0, authSuccesses: 0 }]));
}
async function run(input) {
  let nextId = 0, authSuccesses = 0, authFailures = 0;
  const physical = [], payloads = [], logs = [];
  const options = { key: 'key', secret: 'secret' };
  const api = load(apiPath, {
    axios, 'axios-retry': retry, qs, nanoid: { nanoid: () => `id-${++nextId}` },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('private request must not skip remote signing'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config) {
        return axios.create({ ...config, timeout: 20000, adapter: async current => {
          physical.push({ id: current.params.requestId, data: current.data });
          if (nativeBudgets) {
            assert.ok(Object.hasOwn(current['axios-retry'], 'retries'), 'actual retry config retains own key');
            assert.ok(Object.is(current['axios-retry'].retries, input.retries), 'actual retry config retains native undefined/NaN');
          }
          const reply = input.replies[physical.length - 1];
          assert.ok(reply, 'unscripted extra physical attempt');
          const [status, data] = reply;
          const response = { status, statusText: String(status), headers: {}, config: current, data };
          if (input.network) throw new axios.AxiosError('scripted network', 'ERR_NETWORK', current);
          if (status >= 400) throw new axios.AxiosError('venue rejection', 'ERR_BAD_REQUEST', current, undefined, response);
          return response;
        } });
      },
      installTransportAdapter() { assert.fail('fixture must not install a tunnel'); },
    },
    '../../utils/payloadSigner': {
      canSignPayload: () => true,
      async signPayload(_value, _exchange, payload) { payloads.push(payload); return { signature: 'sig' }; },
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: {
      getCurrentTimeMs: () => 1000,
      syncBybitClientServerTime() { assert.fail('no clock branch'); },
      syncFromServerTime() { assert.fail('no clock branch'); },
    } },
    './bybit.response': response, './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types, '../../utils/retryReadsOnly': retryReads, '../../utils/signatureRejection': signatureRejection,
  }, apiSource.text, { error: (...args) => logs.push(args) });
  const exchange = { name: retryBudgets ? 'bybit' : 'bybitinverse', isDemo: false,
    recordAuthSuccess: () => authSuccesses++, recordAuthFailure: () => authFailures++, emitAndLogError() {} };
  const client = api.createAPI(options, exchange);
  let result, rejection;
  try { result = await client.request({ method: 'post', url: '/private', data: { request: true }, ...(retryBudgets ? { method: input.network ? 'get' : 'post', 'axios-retry': { retries: input.retries } } : {}) }); }
  catch (error) { rejection = error; }
  const expected = expectations[input.id];
  assert.equal(physical.length, expected.attempts, 'source physical attempts');
  assert.equal(!!rejection, expected.rejected, 'source rejection');
  const retCode = (result?.data ?? rejection?.response?.data)?.retCode ?? null;
  assert.equal(retCode, expected.retCode, 'source final retCode');
  assert.equal(authFailures, expected.authFailures, 'source auth failures');
  assert.equal(authSuccesses, expected.authSuccesses, 'source auth successes');
  if (!input.network) assert.deepEqual(payloads, physical.map(() => '1000key20000{"request":true}'), 'every attempt re-signs the same body');
  return { attempts: physical.length, rejected: !!rejection, retCode, authFailures, authSuccesses };
}
(async () => {
  const cases = [];
  for (const input of inputs) cases.push({ id: input.id, source: await run(input) });
  assert.equal(cases.length, nativeBudgets ? 2 : retryBudgets ? 6 : 4, 'executed review case floor');
  process.stdout.write(JSON.stringify({
    scope: 'source-only; unchanged maintained API/response/read-retry/signatureRejection with actual Axios/axios-retry/qs; scripted common constructor/physical adapter and recording signer; no native proof',
    declaredCases: nativeBudgets ? 2 : retryBudgets ? 6 : 4, cases,
  }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
