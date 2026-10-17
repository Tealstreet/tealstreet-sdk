// Execute the maintained source helper with actual ethers and compare Node bytes.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const ethers = require('ethers');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket/polymarket.signing.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const functions = source.statements.filter(n => ts.isFunctionDeclaration(n) && n.name?.text === 'generateHmacSignature');
assert.equal(functions.length, 1);
const code = ts.transpileModule(functions[0].getText(source), {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
const exportsValue = {};
vm.runInNewContext(`(function(exports, ethers) { ${code}\n })`, {Uint8Array, Buffer})(exportsValue, ethers);
const cases = ['abd=', 'ab d=', 'ab!d=', 'abdaA', '-_8=', 'c2VjcmV0', 'a', '===='];
const timestamp = '1700000000';
const message = timestamp + 'GET/balance-allowance';
const rows = cases.map(secret => {
  const normalized = secret.replace(/-/g, '+').replace(/_/g, '/');
  const padded = normalized + '='.repeat((4 - normalized.length % 4) % 4);
  const key = Buffer.from(padded, 'base64');
  const expected = crypto.createHmac('sha256', key).update(message).digest('base64').replace(/\+/g, '-').replace(/\//g, '_');
  const signature = exportsValue.generateHmacSignature(secret, timestamp, 'get', '/balance-allowance');
  assert.equal(signature, expected);
  return {secret, bytes: [...key], signature};
});
process.stdout.write(JSON.stringify({capture: {architecture: process.arch, nodeMajor: Number(process.versions.node.split(".")[0])}, cases: rows}));
