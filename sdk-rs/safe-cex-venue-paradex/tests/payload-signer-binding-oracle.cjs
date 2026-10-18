const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', { paths: [root] }));
const starknet = require(require.resolve('starknet', { paths: [root] }));
const source = ts.createSourceFile('paradex.sdk.ts', fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(source.parseDiagnostics.length, 0);
const names = ['signParadexTypedData', 'ParadexRequestSigner', 'ParadexJWTManager'];
const nodes = names.map(name => {
  const found = source.statements.filter(node => node.name?.text === name);
  assert.equal(found.length, 1); return found[0].getText(source);
});
assert.equal(nodes[0].split("import('starknet')").length, 2);
nodes[0] = nodes[0].replace("import('starknet')", 'loadStarknet()');
const compiled = ts.transpileModule(nodes.join('\n'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true });
assert.equal(compiled.diagnostics.length, 0);
(async () => {
  const input = JSON.parse(fs.readFileSync(0, 'utf8')), clocks = [...input.clocks];
  const module = { exports: {} }, requests = [];
  vm.runInNewContext(compiled.outputText, {
    module, exports: module.exports, BigNumber: require('bignumber.js'),
    Date: { now() { assert.ok(clocks.length); return clocks.shift(); } },
    loadStarknet: async () => starknet, fetch, TextEncoder, console,
  });
  const options = { key: 'hosted-key-reference', payloadSigner(request) {
    assert.equal(this, options); assert.equal(request.scheme, 'stark'); assert.equal(request.encoding, 'felt-pair');
    requests.push(request);
    const signature = starknet.ec.starkCurve.sign(request.payload, '0x123');
    return Promise.resolve({ signature: JSON.stringify([signature.r.toString(), signature.s.toString()]), headers: { 'x-provider-route': 'retained' } });
  } };
  const sdkAccount = { account: { address: '0x1234' }, config: { paradexChainId: '0x534e5f5345504f4c4941' } };
  const signer = new module.exports.ParadexRequestSigner(sdkAccount, options);
  const outputs = [await signer.signOrder(input.parameters), await signer.signModifyOrder(input.parameters)];
  const manager = new module.exports.ParadexJWTManager(sdkAccount, false, '0x456', options);
  manager.apiBaseUrl = input.url + "/v1";
  const token = await manager.generateNewToken();
  assert.equal(requests.length, 3); assert.equal(clocks.length, 0);
  process.stdout.write(JSON.stringify({ requests, outputs, token }));
})().catch(error => { console.error(error); process.exitCode = 1; });
