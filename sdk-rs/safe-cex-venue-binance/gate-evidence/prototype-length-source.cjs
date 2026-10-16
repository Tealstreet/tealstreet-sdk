// Run from the repository root with SAFE_CEX_TYPESCRIPT_PATH already configured.
// Actual source only: no Rust prediction is executed or reported as measured.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {createRequire} = require('node:module');
const path = require('node:path');
const projectRequire = createRequire(path.resolve('package.json'));
const ts = projectRequire(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = 'packages/safe-cex/src/exchanges/base.ws.ts';
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'BaseWebSocket');
assert.ok(klass, 'actual BaseWebSocket class exists');
const members = ['messageHandlers', 'messageHandlersStats', 'lastStatsReconcileAt', 'recordMessageHandlerStats', 'reconcileMessageHandlerStats', 'cleanupMessageHandlerStats'].map(name => {
  const member = klass.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, `actual source member ${name}`);
  return member.getText(ast);
}).join('\n');
const constants = ['STATS_RECONCILE_INTERVAL_MS', 'STATS_RECONCILE_FORCE_MIN_MS'].map(name => {
  const declaration = ast.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(value => value.name.getText(ast) === name));
  assert.ok(declaration, `actual source constant ${name}`);
  return declaration.getText(ast).replace(/^export /, '');
}).join('\n');
const context = vm.createContext({assert});
const program = ts.transpileModule(`${constants}\nclass Fixture { ${members} }`, {
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
}).outputText;
vm.runInContext(program, context);
const result = vm.runInContext(`(() => {
  let time = 1000;
  Date.now = () => time;
  Object.prototype.length = -0.5;
  const a = new Fixture(), b = new Fixture();
  a.recordMessageHandlerStats('__proto__');
  assert.equal(Object.prototype.lastCallbacksCount, -0.5, 'source mutable prototype length retains negative fractional callback count');
  assert.equal(typeof Object.prototype.lastCallbacksCount, 'number', 'source callback count remains a native number before serialization');
  assert.equal(Object.prototype.hadHandlers, true, 'source negative fractional handler length remains truthy');
  assert.equal(Object.prototype.lastMessageTimestamp, 1000, 'source prototype timestamp commits');
  assert.equal(a.messageHandlersStats.__proto__, b.messageHandlersStats.__proto__, 'source sockets retain the same mutated referent');
  const record = {count: Object.prototype.lastCallbacksCount, has: Object.prototype.hadHandlers, timestamp: Object.prototype.lastMessageTimestamp};
  const caught = operation => { try { operation(); return null; } catch (error) { return {name: error.name, message: error.message}; } };
  time = 6000;
  const reconcileError = caught(() => b.recordMessageHandlerStats('book.BTC'));
  assert.equal(reconcileError?.name, 'TypeError', 'source later reconciliation observes prototype pollution');
  assert.equal(reconcileError.message, "Cannot create property 'lastCallbacksCount' on number '-0.5'", 'source enumerable length is the first primitive reconciliation error');
  assert.equal(b.messageHandlersStats['book.BTC'].lastMessageTimestamp, 6000, 'source own entry commits before reconciliation failure');
  time = 6250;
  const cleanupError = caught(() => b.cleanupMessageHandlerStats('book.BTC'));
  assert.equal(cleanupError?.name, 'TypeError', 'source forced cleanup observes prototype pollution');
  assert.equal(Object.hasOwn(b.messageHandlersStats, 'book.BTC'), false, 'source cleanup deletes own entry before failure');
  assert.equal(Object.prototype.lastCallbacksCount, -0.5, 'source cleanup preserves shared negative fractional count');
  return {case: 0, length: -0.5, record, reconcileError, cleanupError};
})()`, context);
process.stdout.write(JSON.stringify(result) + '\n');
