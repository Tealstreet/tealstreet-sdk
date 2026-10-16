const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const axios = require('axios');
const retry = require('axios-retry');
const qs = require('qs');
const root = path.resolve(process.env.SAFE_CEX_ROOT || path.join(__dirname, '../../..'));
const parse = file => {
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), 99, true);
  assert.equal(source.parseDiagnostics.length, 0);
  return source;
};
function load(file, text, imports = {}, extra = {}) {
  const output = ts.transpileModule(text, { compilerOptions: { target: 7, module: 1, esModuleInterop: true }, reportDiagnostics: true });
  assert.equal(output.diagnostics.length, 0);
  const context = vm.createContext({ exports: {}, console, ...extra, require(name) {
    assert.ok(Object.hasOwn(imports, name), `undeclared import ${name}`);
    return imports[name];
  } });
  vm.runInContext(output.outputText, context);
  return context.exports;
}
const apiPath = 'packages/safe-cex/src/exchanges/bybit/bybit.api.ts';
const api = parse(apiPath);
const typesPath = 'packages/safe-cex/src/exchanges/bybit/bybit.types.ts';
const typesSource = parse(typesPath);
const names = ['RECV_WINDOW', 'BROKER_ID', 'BASE_URL', 'DEMO_URL', 'ENDPOINTS'];
const constants = typesSource.statements.filter(node => ts.isVariableStatement(node)
  && node.declarationList.declarations.some(d => names.includes(d.name.getText(typesSource))));
assert.equal(constants.length, 5);
const types = load(typesPath, constants.map(node => node.getText(typesSource)).join('\n'));
const httpPath = 'packages/safe-cex/src/safeCexAxiosCreate.ts';
const http = parse(httpPath);
const httpNames = ['innerAdapterSlots', 'setInnerAdapterForRemoteSigning', 'installTransportAdapter'];
const httpStatements = http.statements.filter(node => ts.isVariableStatement(node)
  && node.declarationList.declarations.some(d => httpNames.includes(d.name.getText(http))));
assert.equal(httpStatements.length, 3, 'actual maintained transport installation dependency inventory');
const responsePath = 'packages/safe-cex/src/exchanges/bybit/bybit.response.ts';
const response = load(responsePath, parse(responsePath).text);
const readsPath = 'packages/safe-cex/src/utils/retryReadsOnly.ts';
const reads = load(readsPath, parse(readsPath).text, { 'axios-retry': retry });
const rejectionPath = 'packages/safe-cex/src/utils/signatureRejection.ts';
const rejection = load(rejectionPath, parse(rejectionPath).text);
const cases = [];
for (const region of [undefined, 'JA', 'SG', '', 'US']) {
  const options = { key: 'key', secret: 'secret', tealstreetTunnel: true,
    ...(region === undefined ? {} : { proxyRegion: region }) };
  let constructorCalls = 0, providerCalls = 0;
  let client, forwarded;
  const install = load(httpPath, httpStatements.map(node => node.getText(http)).join('\n'), {}, {
    getTransport(received) {
      assert.equal(received, options, 'actual install helper retains options');
      return { restAdapter(context) {
        providerCalls++;
        assert.equal(context.options, options);
        assert.equal(context.exchange, 'bybitinverse');
        assert.equal(context.variant.region, region, 'factory forwards valid string unchanged');
        forwarded = context.variant;
        return undefined;
      } };
    },
  });
  const module = load(apiPath, api.text, {
    axios, 'axios-retry': retry, qs,
    nanoid: { nanoid() { assert.fail('construction must not dispatch'); } },
    '../../remoteSigningAdapter': { markSkipRemoteSigning() { assert.fail('no request'); } },
    '../../safeCexAxiosCreate': {
      safeCexAxiosCreate(config, received) {
        constructorCalls++;
        assert.equal(received, options);
        client = axios.create(config);
        return client;
      },
      installTransportAdapter: install.installTransportAdapter,
    },
    '../../utils/payloadSigner': {
      canSignPayload() { assert.fail('no request'); },
      signPayload() { assert.fail('no request'); },
    },
    '../../utils/runtime': { isNodeOrBunRuntime: () => true },
    '../../utils/virtualClock': { virtualClock: {} },
    './bybit.response': response,
    './bybit.site': { getBybitSiteHeaders: () => ({}) },
    './bybit.types': types,
    '../../utils/retryReadsOnly': reads,
    '../../utils/signatureRejection': rejection,
  });
  const result = module.createAPI(options, { name: 'bybitinverse', isDemo: false });
  assert.equal(result, client);
  assert.equal(constructorCalls, 1);
  assert.equal(providerCalls, 1);
  assert.deepEqual(Object.keys(forwarded), ['region']);
  cases.push({ region: region === undefined ? { undefined: true } : region, constructed: true, providerCalls });
}
assert.equal(cases.length, 5);
process.stdout.write(JSON.stringify({ declaredCases: 5, scope: 'whole maintained Bybit API construction plus actual maintained install helpers; real Axios and retry; provider returns no replacement; no physical request or native execution', cases }) + '\n');
