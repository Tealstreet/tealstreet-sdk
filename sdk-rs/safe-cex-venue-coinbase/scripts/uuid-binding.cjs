// Execute actual uuid, mint initializer and JWT header expressions. Only the
// imported nanoid entropy result is supplied; production Rust is never replaced.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function source(relative) {
  const p = path.join(root, 'packages/safe-cex/src', relative);
  return ts.createSourceFile(p, fs.readFileSync(p, 'utf8'), ts.ScriptTarget.Latest, true);
}
function one(nodes, predicate) {
  const found = nodes.filter(predicate); assert.equal(found.length, 1); return found[0];
}
function variable(ast, name) {
  return one(ast.statements.flatMap(s => ts.isVariableStatement(s) ? [...s.declarationList.declarations] : []), d => ts.isIdentifier(d.name) && d.name.text === name);
}
function expression(node, ast, scope) {
  const code = ts.transpileModule(`(${node.getText(ast)})`, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
  return vm.runInNewContext(code, scope);
}
const ids = source('utils/uuid.ts');
let calls = 0;
const uuid = expression(variable(ids, 'uuid').initializer, ids, { nanoid: () => { calls++; return '-_' + 'M'.repeat(19); } });
const exchange = source('exchanges/coinbase/coinbase.exchange.ts');
const cls = one([...exchange.statements], n => ts.isClassDeclaration(n) && n.name.text === 'CoinbaseExchange');
const mint = one([...cls.members], n => n.name?.getText(exchange) === 'clientOrderIdRule');
const rule = expression(mint.initializer, exchange, { uuid, patternClientOrderIdRule: (_, __, options) => options });
const utils = source('exchanges/coinbase/coinbase.utils.ts');
const sign = variable(utils, 'sign').initializer;
const header = variable(sign.body, 'header');
const actualHeader = expression(header.initializer, utils, { algorithm: 'EdDSA', key_name: 'fixture', uuid });
const crypto = require('node:crypto');
const key = crypto.createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32)]),format:'der',type:'pkcs8'});
const publicKey = [...crypto.createPublicKey(key).export({format:'der', type:'spki'}).subarray(-32)];
const result = { uuid: uuid(), minted: rule.mint(), nonce: actualHeader.nonce, publicKey };
assert.equal(calls, 3);
process.stdout.write(JSON.stringify(result));
