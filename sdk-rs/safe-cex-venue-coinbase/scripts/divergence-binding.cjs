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
(async () => {
  const pem = [];
  for (const payload of payloads) {
    const secret = `-----BEGIN PRIVATE KEY-----\n${payload}\n-----END PRIVATE KEY-----`;
    try { await exportsObject.sign('organizations/synthetic-org/apiKeys/synthetic-key', secret); assert.fail('malformed PEM signed'); }
    catch (error) { assert.equal(error.name, 'InvalidCharacterError'); pem.push({ payload, secret, name: error.name, message: error.message }); }
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
