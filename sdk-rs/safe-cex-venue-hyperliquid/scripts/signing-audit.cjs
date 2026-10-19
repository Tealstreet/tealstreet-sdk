'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { createRequire } = require('node:module');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', {paths: [root]}));
const filename = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.signing.ts');
const localRequire = createRequire(filename);
const source = fs.readFileSync(filename, 'utf8');
const tree = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0);
assert.equal(tree.statements.filter(n => ts.isFunctionDeclaration(n) && n.name?.text === 'signL1Action').length, 1);
const context = vm.createContext({exports: {}, require: localRequire, Uint8Array, DataView});
vm.runInContext(ts.transpileModule(source, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText, context, {filename});
const errorProfile = vm.runInContext('(error) => ({name: error.name, message: error.message, display: String(error), isError: error instanceof Error, isRangeError: error instanceof RangeError})', context);
const {ethers} = localRequire('ethers');
const wallet = new ethers.Wallet('0x' + '11'.repeat(32));
const action = {type: 'cancel', cancels: [{a: 1, o: 1234}]};
(async () => {
  const rows = [];
  for (const mainnet of [true, false]) {
    for (const [id, nonce] of [['zero', 0], ['negative-zero', -0], ['negative', -1], ['safe-max', Number.MAX_SAFE_INTEGER], ['wrap64', 2 ** 64], ['fraction-positive', 1.25], ['fraction-negative', -0.25]]) {
      let syncThrow = false, pending;
      try { pending = context.exports.signL1Action(wallet, action, null, nonce, mainnet); }
      catch (error) { syncThrow = true; pending = Promise.reject(error); }
      const row = {id, mainnet, nonceBits: Buffer.from(new Float64Array([nonce]).buffer).toString('hex'), syncThrow, promise: typeof pending.then === 'function'};
      try { row.signature = await pending; row.ok = true; }
      catch (error) { Object.assign(row, {ok: false, error: errorProfile(error)}); }
      rows.push(row);
    }
  }
  assert.equal(rows.length, 14);
  assert.equal(rows.filter(r => r.ok).length, 10);
  assert.equal(rows.filter(r => !r.ok).length, 4);
  process.stdout.write(JSON.stringify(rows) + '\n');
})().catch(error => {process.stderr.write(error.stack + '\n'); process.exitCode = 1;});
