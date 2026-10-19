// Execute maintained subscription members; only socket IO and timers are terminal fixtures.
const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const symbols = ['BTCUSDT', 'ETHUSDT', 'BNBUSDT', ...Array.from({length: 40}, (_, i) => `M${39-i}USDT`)];
const names = ['onOpen', 'subscribeMarketsToChannel', 'subscribe', 'enqueueSubscriptions', 'drainSubscriptionQueue'];
const methods = Object.fromEntries(names.map(name => {
  const members = cls.members.filter(n => n.name?.getText(ast) === name);
  assert.equal(members.length, 1, name);
  return [name, ts.transpileModule('(function(){return (' + members[0].initializer.getText(ast) + ');})', {
    compilerOptions: { target: ts.ScriptTarget.ES2022 }
  }).outputText];
}));
const constants = Object.fromEntries(ast.statements.filter(ts.isVariableStatement).flatMap(statement =>
  statement.declarationList.declarations.filter(declaration => ['BITUNIX_SUBSCRIBE_BATCH_SIZE', 'BITUNIX_SUBSCRIBE_INTERVAL_MS', 'BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION'].includes(declaration.name.getText(ast)))
    .map(declaration => [declaration.name.getText(ast), vm.runInNewContext(declaration.initializer.getText(ast))])));
assert.equal(Object.keys(constants).length, 3);
const frames = [false, true].map(isSecondary => {
  const sent = [], timers = [];
  const context = { ...constants,
    setTimeout: (fn, delay) => { const timer = {fn, delay}; timers.push(timer); return timer; }, clearTimeout: () => {} };
  const owner = { isSecondary, isDisposed: false, topics: {}, subscriptionQueue: [],
    parent: { store: { markets: symbols.map(symbol => ({symbol})) }, emitter: {on() {}, off() {}} },
    resetOrderbook() {}, ping() {}, send: frame => sent.push(frame), onMarketsChanged() {} };
  for (const [name, code] of Object.entries(methods)) owner[name] = vm.runInNewContext(code, context).call(owner);
  owner.onOpen();
  while (timers.some(timer => timer.delay === 250)) {
    const index = timers.findIndex(timer => timer.delay === 250);
    timers.splice(index, 1)[0].fn();
  }
  assert.equal(sent.length, 3);
  return sent;
});
console.log(JSON.stringify({symbols, frames}));
