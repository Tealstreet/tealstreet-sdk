'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const TinyEmitter = require('tiny-emitter');
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/lighter/lighter.ws-private.ts');
const source = fs.readFileSync(filename, 'utf8');
const tree = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
const klass = tree.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'LighterPrivateWebsocket');
assert.ok(klass);
const members = ['isRecentStreamEvent', 'handleFillsTopic'].map(name => {
  const member = klass.members.find(node => node.name?.getText(tree) === name);
  assert.ok(member && ts.isPropertyDeclaration(member) && ts.isArrowFunction(member.initializer), name);
  return member.getText(tree);
});
const exported = {};
const now = Number(process.argv[2] ?? 1700000000000);
vm.runInNewContext(ts.transpileModule(`class Subject { ${members.join('\n')} } exports.subject = new Subject();`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText, {exports: exported, Date: {now: () => now}, virtualClock: {getCurrentTimeMs: () => 7000}});
const subject = exported.subject;
subject.lastConnectTime = 1000;
subject.useReadonlyFallback = false;
subject.emitter = new TinyEmitter();
const values = {
  recent: now, boundary: now - 5000, stale: now - 5001, future: now + 1, zero: 0, negative: -1,
  string: String(now), padded: ` ${now} `, exponential: '1.7e12', empty: '', null: null,
  false: false, true: true, array: [now], object: {},
};
const rows = [];
for (const [label, timestamp] of Object.entries(values)) {
  const expected = [];
  const listener = fill => expected.push(fill);
  subject.emitter.on('fill', listener);
  subject.handleFillsTopic([{orderId: label, side: 'BUY', symbol: 'BTC', price: '49000', size: '0.1', timestamp}]);
  subject.emitter.off('fill', listener);
  rows.push({label, timestamp, expected});
}
process.stdout.write(JSON.stringify(rows));
