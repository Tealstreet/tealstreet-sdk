const assert = require('node:assert/strict');
const fs = require('node:fs');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = require('node:path').resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bingx/bingx.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(s => ts.isClassDeclaration(s) && s.name.text === 'BingxPublicWebsocket');
const member = klass.members.find(m => m.name?.text === 'getDiagnosticSymbols');
assert(member && ts.isArrowFunction(member.initializer), 'BINGX-AUDIT diagnostic shape');
let initializer = member.initializer.getText(ast);
if (process.env.SAFE_CEX_BINGX_AUDIT_DIAGNOSTIC_CONTROL === '1') {
  const from = 'symbols.add(match[1]);';
  assert.equal(initializer.split(from).length, 2, 'BINGX-AUDIT diagnostic control selector');
  initializer = initializer.replace(from, 'symbols.add("x");');
}
const factory = new Function(ts.transpileModule('return function(){return ' + initializer + '}', { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText)();
const lone = JSON.parse('"\\ud800"');
const symbols = [...factory.call({ messageHandlers: { ['market.kline.' + lone + '.1m']: () => {} } })()];
assert.equal(symbols.length, 1, 'BINGX-AUDIT diagnostic independent count');
assert.equal(symbols[0].length, 1, 'BINGX-AUDIT diagnostic string length');
assert.equal(symbols[0].charCodeAt(0), 0xd800, 'BINGX-AUDIT diagnostic returned unit');
console.log(JSON.stringify({ node: process.version, architecture: process.arch, units: [symbols[0].charCodeAt(0)], stoppedAfter: 1 }));
