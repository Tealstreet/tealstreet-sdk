'use strict';
const {ts, fs, path, vm, root, assert, load} = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const members = cls.members.filter(member => member.name?.getText(ast) === 'fetchTickers');
assert.equal(members.length, 1);
const code = ts.transpileModule('(function(){return (' + members[0].initializer.getText(ast) + ');})', {
  compilerOptions: {target: ts.ScriptTarget.ES2022},
}).outputText;
const {ENDPOINTS} = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts'));
const requests = [];
const receiver = {
  xhr: {get: async endpoint => {requests.push(endpoint); return {data: {data: []}};}},
  store: {tickers: [], marketsBySymbol: {}},
};
const method = vm.runInNewContext(code, {ENDPOINTS, console}).call(receiver);
void method();
assert.deepEqual(requests, [ENDPOINTS.TICKERS], 'actual source call prefix survives discarded result');
queueMicrotask(() => process.stdout.write(JSON.stringify({requests})));
