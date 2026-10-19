'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', {paths: [root]}));
const axios = require('axios');
const {TinyEmitter} = require('tiny-emitter');
const filename = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.api.ts');
const tree = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0);
const selected = ['RELAY_SIGNABLE_ACTION_TYPES', 'addSigningInterceptor'].map(name => {
  const rows = tree.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(tree) === name));
  assert.equal(rows.length, 1, name);
  return rows[0].getText(tree);
}).join('\n');
const baseFile = path.join(root, 'packages/safe-cex/src/exchanges/base.ts');
const baseTree = ts.createSourceFile(baseFile, fs.readFileSync(baseFile, 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(baseTree.parseDiagnostics.length, 0);
const baseClasses = baseTree.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === 'BaseExchange');
assert.equal(baseClasses.length, 1);
const methods = baseClasses[0].members.filter(n => n.name?.getText(baseTree) === 'emitAndLogError');
assert.equal(methods.length, 1);
const scope = {console: {error() {}, trace() {}, warn() {}}, reportException() {}, readHttpFailureContext() {}, ENDPOINTS: {INFO: '/info'}, markSkipRemoteSigning() {},
  signL1Action() { throw Error('unexpected signer reach'); }, createWalletFromPrivateKey() { throw Error('unexpected wallet reach'); }};
vm.runInNewContext(ts.transpileModule(selected + '\nclass ErrorHost {' + methods[0].getText(baseTree) + '}\nglobalThis.ErrorHost = ErrorHost; globalThis.install = addSigningInterceptor;', {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText, scope, {filename});
(async () => {
  const profiles = [];
  for (const kind of ['unknown', 17, ['unknown']]) {
    for (const throws of [false, true]) {
      const emitter = new TinyEmitter(), sentinel = {marker: 'listener-origin', units: '\ud800', zero: -0};
      let calls = 0, adapters = 0, exact = false, message;
      emitter.on('error', text => {calls++; message = text; if (throws) throw sentinel;});
      const xhr = axios.create({adapter: async config => {adapters++; return {data: {status: 'ok'}, status: 200, headers: {}, config};}});
      const host = Object.assign(new scope.ErrorHost(), {emitter, name: 'hyperliquid', options: {}});
      scope.install(xhr, {secret: 'not-a-valid-key'}, host);
      const results = await Promise.all([0, 1].map(async () => {
        try {await xhr.post('/exchange', {action: {type: kind}, nonce: 1}); return {rejected: false, exact: false};}
        catch (error) {return {rejected: true, exact: error === sentinel};}
      }));
      const rejected = results.every(result => result.rejected);
      exact = results.every(result => result.exact);
      assert.equal(calls, 2);
      assert.equal(adapters, throws ? 0 : 2);
      assert.equal(rejected, throws);
      assert.equal(exact, throws);
      profiles.push({kind, throws, calls, adapters, rejected, exact, message});
    }
  }
  assert.equal(profiles.length, 6);
  assert.equal(profiles.filter(p => p.rejected && p.exact && p.adapters === 0).length, 3);
  console.log(JSON.stringify(profiles));
})().catch(error => {console.error(error); process.exitCode = 1;});
