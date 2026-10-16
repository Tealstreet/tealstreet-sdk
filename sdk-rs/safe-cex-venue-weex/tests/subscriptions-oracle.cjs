const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, capture, load} = require('./types-oracle.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.getText(ast) === 'WeexPublicWebsocket');
const names = ['sendSubscriptionOperation', 'subscribeTopics', 'reconcileTickerTopics', 'subscribeTickerTopics',
  'resubscribeTopics', 'subscribeTopic', 'unsubscribeTopic', 'handleActiveTickerSymbolsChanged', 'getDesiredTickerTopics'];
const members = cls.members.filter(n => ts.isPropertyDeclaration(n) && names.includes(n.name.getText(ast)));
assert.equal(members.length, 9, 'subscriptions::source_members');
const constants = ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d =>
  ['MAX_WS_CHANNELS', 'MAX_TICKER_CHANNELS', 'MAX_SUBSCRIPTION_OPERATIONS_PER_HOUR', 'SUBSCRIPTION_OPERATION_WINDOW_MS'].includes(d.name.getText(ast))));
assert.equal(constants.length, 4, 'subscriptions::source_constants');
const code = ts.transpileModule(constants.map(n => n.getText(ast)).join('\n') + '\nexport class Slice {\n' + members.map(n => n.getText(ast)).join('\n') + '\n}',
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(code.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
const {LogSeverity} = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const units = value => Array.from({length: value.length}, (_, i) => value.charCodeAt(i));
const bits = value => capture(value).bits;
const many = n => Array.from({length: n}, (_, i) => 'S' + i);
const topics = n => Array.from({length: n}, (_, i) => 'depth.' + i);
const tickers = symbols => symbols.map(s => s + '@ticker');
const now = 10_000_000;
const recent = n => Array(n).fill(now - 1);
const expired = n => Array(n).fill(now - 3_600_000);
const cases = [];
function run(id, action, patch = {}, actions = null) {
  const input = {topics: [], tickers: [], connected: true, disposed: false, timestamps: [], logged: false, subscriptionId: 1,
    available: ['A', 'B', 'C'], active: [], clock: [now], failures: {}, disconnectAfterSend: false, ...patch};
  const events = [], packets = [], logs = [], results = [];
  let instance, clockIndex = 0, packetIndex = 0;
  const context = {exports: {}, LogSeverity,
    Date: {now() {events.push('now'); if (input.failures.clock) throw new TypeError('fixture clock'); return input.clock[Math.min(clockIndex++, input.clock.length - 1)];}},
    JSON: {stringify(packet) {events.push('packet'); packetIndex++;
      packets.push({method: packet.method, topics: packet.params.map(units), id: bits(packet.id)});
      if (input.failures.json === packetIndex) throw new TypeError('fixture serializer');
      return JSON.stringify(packet);
    }}};
  vm.runInNewContext(code.outputText, context, {filename: file});
  instance = new context.exports.Slice();
  instance.topics = new Set(input.topics); instance.tickerTopics = new Set(input.tickers);
  instance.isConnected = input.connected; instance.isDisposed = input.disposed;
  instance.subscriptionOperationTimestamps = input.timestamps.slice(); instance.hasLoggedSubscriptionLimit = input.logged;
  instance.subscriptionId = input.subscriptionId; instance.store = {markets: input.available.map(symbol => ({symbol}))};
  instance.parent = {getActiveTickerSymbols() {events.push('desired'); if (input.failures.desired) throw new TypeError('fixture active symbols'); return input.active;},
    log(message, severity) {events.push('log'); logs.push({message, severity}); if (input.failures.log) throw new RangeError('fixture logger');}};
  instance.send = function(wire) {
    events.push('send');
    if (input.disconnectAfterSend) instance.isConnected = false;
    if (input.failures.send === packetIndex) throw new RangeError('fixture sender');
  };
  const steps = actions || [action];
  for (const step of steps) {
    try {
      const args = step.name === 'sendSubscriptionOperation' ? [step.method || 'SUBSCRIBE', step.topics || []] :
        step.name === 'subscribeTopics' ? [step.topics || [], step.track ?? true] :
        ['subscribeTopic', 'unsubscribeTopic'].includes(step.name) ? [step.topic] : [];
      const value = instance[step.name](...args);
      results.push(value === undefined ? {unit: true} : {boolean: value});
    } catch (e) {results.push({error: e.name}); break;}
  }
  function actionInput(step) {return {...step, topics: (step.topics || []).map(units), topic: step.topic === undefined ? [] : units(step.topic)};}
  cases.push({id, input: {...input, topics: input.topics.map(units), tickers: input.tickers.map(units),
    timestamps: input.timestamps.map(bits), subscriptionId: bits(input.subscriptionId), available: input.available.map(units),
    active: input.active.map(units), clock: input.clock.map(bits)}, actions: steps.map(actionInput), results, packets, logs, events,
    state: {topics: Array.from(instance.topics, units), tickers: Array.from(instance.tickerTopics, units), connected: instance.isConnected,
      disposed: instance.isDisposed, timestamps: instance.subscriptionOperationTimestamps.map(bits), logged: instance.hasLoggedSubscriptionLimit,
      subscriptionId: bits(instance.subscriptionId)}});
}
const operation = {name: 'sendSubscriptionOperation', topics: ['A@ticker']};
run('operation/basic', operation);
run('operation/unsubscribe', {...operation, method: 'UNSUBSCRIBE'});
run('operation/expired_boundary', operation, {timestamps: [now - 3_600_000, now - 3_600_000 + 1, now + 1, NaN, Infinity, -Infinity]});
run('operation/budget239', operation, {timestamps: recent(239)});
run('operation/budget240', operation, {timestamps: recent(240)});
run('operation/logged240', operation, {timestamps: recent(240), logged: true});
run('operation/pruning_reopens', operation, {timestamps: expired(240)});
run('operation/repeated_cap', operation, {timestamps: recent(240)}, [operation, operation]);
run('operation/nan_id', operation, {subscriptionId: NaN});
run('operation/infinite_id', operation, {subscriptionId: Infinity});
run('operation/signed_zero_id', operation, {subscriptionId: -0});
run('operation/throw_clock', operation, {failures: {clock: true}});
run('operation/throw_log', operation, {timestamps: recent(240), failures: {log: true}});
run('operation/throw_json', operation, {failures: {json: 1}});
run('operation/throw_send', operation, {failures: {send: 1}});
run('batch/disconnected_track', {name: 'subscribeTopics', topics: ['x', 'x', '\ud800']}, {connected: false});
run('batch/empty', {name: 'subscribeTopics'});
for (const n of [1, 50, 51, 101]) run('batch/size' + n, {name: 'subscribeTopics', topics: topics(n)});
run('batch/no_track', {name: 'subscribeTopics', topics: ['x', 'x'], track: false}, {topics: ['old']});
run('batch/exhausted', {name: 'subscribeTopics', topics: topics(101)}, {timestamps: recent(239)});
run('batch/throw_send', {name: 'subscribeTopics', topics: topics(101)}, {failures: {send: 1}});
run('batch/disconnect_during_first', {name: 'subscribeTopics', topics: topics(101)}, {disconnectAfterSend: true});
const reconcile = {name: 'reconcileTickerTopics'};
run('reconcile/connected_replace', reconcile, {tickers: tickers(['A', 'B']), available: ['B', 'A', 'C'], active: ['C', 'B']});
run('reconcile/disconnected_replace', reconcile, {tickers: ['OLD'], connected: false, available: ['B', 'A', 'B']});
run('reconcile/unchanged', reconcile, {tickers: tickers(['B', 'A']), available: ['A', 'B']});
run('reconcile/blocked_both', reconcile, {tickers: tickers(['OLD']), timestamps: recent(240)});
run('reconcile/only_unsub_allowed', reconcile, {tickers: tickers(['OLD']), timestamps: recent(239)});
run('reconcile/empty_desired', reconcile, {tickers: tickers(['A']), active: ['missing']});
run('reconcile/empty_old', reconcile);
run('reconcile/throw_desired', reconcile, {tickers: ['OLD'], failures: {desired: true}});
run('reconcile/throw_unsub', reconcile, {tickers: ['OLD'], failures: {send: 1}});
run('reconcile/throw_sub', reconcile, {tickers: ['OLD'], failures: {send: 2}});
const ticker = {name: 'subscribeTickerTopics'};
run('ticker/basic', ticker, {available: ['A', 'A', 'B']});
run('ticker/disconnected', ticker, {connected: false, tickers: ['OLD']});
run('ticker/blocked', ticker, {timestamps: recent(240)});
run('ticker/throw_desired', ticker, {tickers: ['OLD'], failures: {desired: true}});
run('ticker/throw_send', ticker, {tickers: ['OLD'], failures: {send: 1}});
run('resubscribe/basic', {name: 'resubscribeTopics'}, {topics: topics(51)});
run('resubscribe/empty', {name: 'resubscribeTopics'});
run('resubscribe/disconnected', {name: 'resubscribeTopics'}, {topics: ['a'], connected: false});
run('resubscribe/throw_send', {name: 'resubscribeTopics'}, {topics: ['a'], failures: {send: 1}});
const subscribe = {name: 'subscribeTopic', topic: 'new'};
run('subscribe/basic', subscribe, {topics: topics(10), available: many(105)});
run('subscribe/duplicate', {name: 'subscribeTopic', topic: 'old'}, {topics: ['old']});
run('subscribe/disconnected', subscribe, {connected: false});
run('subscribe/blocked', subscribe, {timestamps: recent(240)});
run('subscribe/throw_desired', subscribe, {failures: {desired: true}});
run('subscribe/throw_send', subscribe, {failures: {send: 1}});
const unsubscribe = {name: 'unsubscribeTopic', topic: 'depth.10'};
run('unsubscribe/basic', unsubscribe, {topics: topics(11), available: many(105), tickers: tickers(many(89))});
run('unsubscribe/disconnected', unsubscribe, {topics: topics(11), connected: false, tickers: ['OLD']});
run('unsubscribe/missing', unsubscribe, {topics: ['other']});
run('unsubscribe/blocked', unsubscribe, {topics: topics(11), timestamps: recent(240), tickers: ['OLD']});
run('unsubscribe/throw_send', unsubscribe, {topics: topics(11), failures: {send: 1}, tickers: ['OLD']});
const changed = {name: 'handleActiveTickerSymbolsChanged'};
run('changed/disposed', changed, {disposed: true, tickers: ['OLD']});
run('changed/connected', changed, {tickers: ['OLD']});
run('changed/disconnected', changed, {connected: false, tickers: ['OLD']});
run('changed/throw_desired', changed, {failures: {desired: true}});
assert.equal(cases.length, 59, 'subscriptions::oracle_inventory');
process.stdout.write(JSON.stringify({cases}));
