// Separate FIX-audit scenes; existing source corpora and identity pins stay intact.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase');
const file = path.join(root, 'coinbase.utils.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['COINBASE_KEY_NAME_PATTERN', 'COINBASE_PRIVATE_KEY_PATTERN', 'COINBASE_ED25519_PREFIX', 'getCoinbaseJsonSecret', 'parseCoinbaseCredentialJson', 'base64ToBytes', 'normalizeCoinbaseApiCredentials', 'cachedCryptoKey', 'cachedKeySecret', 'base64UrlEncode', 'base64UrlEncodeString', 'asn1Wrap', 'pemToPkcs8Der', 'getSigningKey', 'sign'];
const selected = ast.statements.filter(n => (ts.isFunctionDeclaration(n) && names.includes(n.name?.text)) || (ts.isVariableStatement(n) && n.declarationList.declarations.some(d => names.includes(d.name.getText(ast)))));
assert.equal(selected.length, names.length, 'every signing dependency comes from maintained source');
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(selected.map(n => n.getText(ast)).join('\n'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText, {
  exports: exportsObject, atob, btoa, TextEncoder, crypto: require('node:crypto').webcrypto,
  virtualClock: { getCurrentTimeMs: () => 1700000000000 }, uuid: () => 'fixture-nonce',
  ed25519Sign: () => { throw Error('PEM profiles must use actual ES256 path'); },
});
const payloads = ['A', '!', 'A!', 'Zg=', 'Zg===', '-___', 'AQID\u0085', 'AAAAA', '!AAAA', 'AAAA!', 'A==='];
function loadDeclarations(relative, names, bindings = {}) {
  const file = path.resolve(root, '../..', relative);
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const nodes = ast.statements.filter(node =>
    (ts.isFunctionDeclaration(node) && names.includes(node.name?.text)) ||
    (ts.isVariableStatement(node) && node.declarationList.declarations.some(d => names.includes(d.name.getText(ast)))));
  assert.equal(nodes.length, names.length, `maintained declarations: ${relative}`);
  const code = ts.transpileModule(nodes.map(node => node.getText(ast)).join('\n'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function('exports', ...Object.keys(bindings), code)(exports, ...Object.values(bindings));
  return exports;
}
async function signingRequest(options, name) {
  const axios = require('axios'), retry = require('axios-retry');
  let dispatched = 0, original;
  const transport = { rest: ({ config }) => ({ baseURL: config.baseURL, proxied: false }) };
  const enhancer = loadDeclarations('utils/axiosErrorEnhancer.ts', ['enhanceAxiosError']);
  const shared = loadDeclarations('safeCexAxiosCreate.ts', [
    'innerAdapterSlots', 'setInnerAdapterForRemoteSigning', 'maybeInstallRemoteSigningAdapter', 'safeCexAxiosCreate',
  ], {
    axios: { create: config => axios.create({ ...config, adapter: async () => { dispatched++; throw Error('PEM must fail before I/O'); } }) },
    getTransport: () => transport, ...enhancer, isDevelopment: false,
    httpStats: { recordRequest() {} }, markRestRequestStart() {}, recordRequestOutcome() {},
    isSigningCaptureRequest: () => false, noteRestFailure() {}, noteRestSuccess() {},
  });
  const tables = loadDeclarations('exchanges/coinbase/coinbase.types.ts', ['RECV_WINDOW', 'BASE_URL', 'ENDPOINTS', 'PUBLIC_ENDPOINTS']);
  const reads = loadDeclarations('utils/retryReadsOnly.ts', ['isReadOnlyMethod', 'onlyReads', 'networkErrorOnRead'], { isNetworkError: retry.isNetworkError });
  const urls = loadDeclarations('utils/replaceUrlParams.ts', ['replaceUrlParams']);
  const api = loadDeclarations('exchanges/coinbase/coinbase.api.ts', ['createAPI'], {
    ...shared, ...tables, ...reads, ...urls, AxiosHeaders: axios.AxiosHeaders, retry: retry.default, qs: require('qs'),
    sign: async (...args) => {
      try { return await exportsObject.sign(...args); }
      catch (error) { original = error; throw error; }
    },
  });
  const exchange = { name, grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [] };
  const xhr = api.createAPI(options, exchange);
  let factory;
  xhr.interceptors.response.use(undefined, error => {
    assert.equal(error, original, 'maintained factory enhances the original signing error');
    assert.equal(error.name, 'InvalidCharacterError');
    assert.equal(error.isAxiosError, true);
    assert.equal(error.originalError, original);
    assert.equal(error.response, undefined);
    factory = { name: error.name, message: error.message };
    throw error;
  });
  const file = path.join(root, 'coinbase.exchange.ts');
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const declaration = ast.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'CoinbaseExchange');
  const member = declaration.members.filter(node => node.name?.getText(ast) === 'placeOrder');
  assert.equal(member.length, 1, 'actual placement catch must execute');
  const code = ts.transpileModule(`class Placement { ${member[0].getText(ast)} }`, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const Placement = new Function('ENDPOINTS', code + '; return Placement;')(tables.ENDPOINTS);
  const reports = [];
  const placement = Object.assign(new Placement(), {
    xhr, resolveCallerClientOrderId() {}, formatCreateOrder: () => ({ product_id: 'BTC-USDC' }),
    emitAndLogError: message => reports.push(message),
  });
  try {
    await placement.placeOrder({});
    assert.fail('malformed PEM placement resolved');
  } catch (error) {
    assert.equal(dispatched, 0, 'actual request signer rejects before terminal adapter');
    assert.equal(error.name, 'Error', 'actual placement catch creates a new Error');
    assert.equal(error.message, factory.message);
    assert.notEqual(error, original);
    assert.deepEqual(reports, [factory.message]);
    return { name: error.name, message: error.message, factory };
  }
}

(async () => {
  const pem = [];
  for (const payload of payloads) {
    const secret = `-----BEGIN PRIVATE KEY-----\n${payload}\n-----END PRIVATE KEY-----`;
    try { await exportsObject.sign('organizations/synthetic-org/apiKeys/synthetic-key', secret); assert.fail('malformed PEM signed'); }
    catch (error) {
      assert.equal(error.name, 'InvalidCharacterError');
      const rest = {};
      for (const name of ['coinbase', 'coinbasespot']) {
        rest[name] = await signingRequest({ key: 'organizations/synthetic-org/apiKeys/synthetic-key', secret }, name);
        assert.equal(rest[name].message, `[${name}] Request setup error: ${error.message}`);
      }
      pem.push({ payload, secret, name: error.name, message: error.message, rest });
    }
  }
  const { write } = require('./load-starbase.cjs');
  const intent = { unmodeledRuntimeField: { retained: true }, savedSymbol: 'BTC-PERP-INTX', side: 'buy', size: { totalContracts: 2, source: 'gateway-contract-count', unit: 'contracts' }, type: 'limit', price: 10 };
  const blocked = write.buildCoinbaseStarbasePlaceOrder(intent);
  assert.equal(blocked.intent, intent, 'source blocked result retains exact caller root');
  assert.equal(blocked.intent.unmodeledRuntimeField, intent.unmodeledRuntimeField);
  const exchangeFile = path.join(root, 'coinbase.exchange.ts');
  const exchange = ts.createSourceFile(exchangeFile, fs.readFileSync(exchangeFile, 'utf8'), ts.ScriptTarget.Latest, true);
  const classes = exchange.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === 'CoinbaseExchange');
  assert.equal(classes.length, 1);
  const methods = classes[0].members.filter(n => n.name?.getText(exchange) === 'placeOrder');
  assert.equal(methods.length, 1);
  const member = methods[0].getText(exchange);
  assert(member.includes('this.xhr.post') && member.includes('ENDPOINTS.ORDER'));
  assert(!member.toLowerCase().includes('starbase'));
  process.stdout.write(JSON.stringify({ pem, starbase: { blocker: blocked.blocker, keys: Object.keys(blocked.intent), retained: blocked.intent.unmodeledRuntimeField, sourcePlacement: 'legacy-rest' } }));
})().catch(error => { console.error(error); process.exitCode = 1; });
