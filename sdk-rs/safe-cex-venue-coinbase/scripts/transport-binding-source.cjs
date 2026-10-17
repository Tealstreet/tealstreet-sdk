// Maintained Coinbase API + shared factory/installer; only I/O, telemetry and signing are supplied.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const axios = require('axios');
const axiosRetry = require('axios-retry');
function load(file, names, bindings = {}) {
  const ast = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
  const nodes = ast.statements.filter(n => ts.isVariableStatement(n) &&
    n.declarationList.declarations.some(d => names.includes(d.name.text)));
  assert.equal(nodes.length, names.length, 'each actual source declaration must exist');
  const code = ts.transpileModule(nodes.map(n => n.getText(ast)).join('\n'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const exports = {};
  new Function('exports', ...Object.keys(bindings), code)(exports, ...Object.values(bindings));
  return exports;
}
(async () => {
  const tables = load('packages/safe-cex/src/exchanges/coinbase/coinbase.types.ts', ['RECV_WINDOW', 'BASE_URL', 'ENDPOINTS', 'PUBLIC_ENDPOINTS']);
  const reads = load('packages/safe-cex/src/utils/retryReadsOnly.ts', ['isReadOnlyMethod', 'onlyReads', 'networkErrorOnRead'], { isNetworkError: axiosRetry.isNetworkError });
  const url = load('packages/safe-cex/src/utils/replaceUrlParams.ts', ['replaceUrlParams']);
  const rows = [];
  for (const name of ['coinbase', 'coinbasespot']) {
    for (const tunnel of [false, true]) {
      for (const available of [false, true]) {
        let routes = 0, selections = 0, terminal = 0, adapted = 0;
        const sent = [];
        const options = { key: 'synthetic-key', secret: 'synthetic-secret', tealstreetTunnel: tunnel, extra: { recvWindow: 5000 } };
        const exchange = { name, grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [] };
        const adapter = which => async config => {
          if (which === 'terminal') terminal++; else adapted++;
          sent.push(config);
          return { config, status: 200, statusText: 'OK', headers: {}, data: { portfolios: [] } };
        };
        const transport = {
          rest(context) {
            routes++;
            assert.equal(context.exchange, name);
            assert.equal(context.options, options);
            return { baseURL: 'https://scripted-route.invalid', proxied: false };
          },
          restAdapter(context) {
            selections++;
            assert.equal(context.exchange, name);
            assert.equal(context.options, options);
            assert.equal(context.variant, 'default');
            return available ? adapter('adapted') : undefined;
          },
        };
        const shared = load('packages/safe-cex/src/safeCexAxiosCreate.ts', [
          'innerAdapterSlots', 'setInnerAdapterForRemoteSigning', 'installTransportAdapter',
          'maybeInstallRemoteSigningAdapter', 'safeCexAxiosCreate',
        ], {
          axios: { create: config => axios.create({ ...config, adapter: adapter('terminal') }) },
          getTransport: actual => { assert.equal(actual, options); return transport; },
          httpStats: { recordRequest() {} }, markRestRequestStart() {}, isDevelopment: false,
          recordRequestOutcome() {}, isSigningCaptureRequest: () => false, noteRestSuccess() {},
          createRemoteSigningAdapter() { throw Error('no remote signer in this corpus'); },
        });
        const api = load('packages/safe-cex/src/exchanges/coinbase/coinbase.api.ts', ['createAPI'], {
          ...tables, ...reads, ...url, ...shared,
          AxiosHeaders: axios.AxiosHeaders, retry: axiosRetry.default, qs: require('qs'),
          sign: async () => 'synthetic-signed-token',
        });
        await api.createAPI(options, exchange).get('/api/v3/brokerage/portfolios');
        assert.equal(routes, 1);
        assert.equal(selections, Number(tunnel));
        assert.equal(adapted, Number(tunnel && available));
        assert.equal(terminal, Number(!(tunnel && available)));
        assert.equal(sent.length, 1);
        assert.equal(sent[0].baseURL, 'https://scripted-route.invalid');
        assert.equal(sent[0].timeout, 20000);
        const header = tunnel ? 'Authorization' : 'Coinbase-Authorization';
        assert.equal(sent[0].headers[header], 'Bearer synthetic-signed-token');
        rows.push({ name, tunnel, available, routes, selections, terminal, adapted, timeout: sent[0].timeout, header });
      }
    }
  }
  assert.equal(rows.length, 8);
  console.log(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
