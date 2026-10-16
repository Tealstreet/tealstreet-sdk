const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, capture, load} = require('./types-oracle.cjs');
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const venue = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const units = s => Array.from({length: s.length}, (_, i) => s.charCodeAt(i));
const text = u => String.fromCharCode(...u);
const bits = n => capture(n).bits;
function ast(file) {
  const full = path.join(root, 'packages/safe-cex/src/exchanges/weex', file);
  return ts.createSourceFile(full, fs.readFileSync(full, 'utf8'), ts.ScriptTarget.Latest, true);
}
function fields(file, cls, names) {
  const tree = ast(file), declaration = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.getText(tree) === cls);
  const members = declaration.members.filter(n => ts.isPropertyDeclaration(n) && names.includes(n.name.getText(tree)));
  assert.equal(members.length, names.length, 'ws_control::source_' + cls);
  return members.map(n => n.getText(tree)).join('\n');
}
const privateFields = fields('weex.ws-private.ts', 'WeexPrivateWebsocket', ['subscribe', 'createAuthHeaders', 'logUnsupportedTransport', 'onOpen', 'ping']);
const publicFields = fields('weex.ws-public.ts', 'WeexPublicWebsocket', ['onOpen', 'ping', 'getDesiredTickerTopics', 'subscribeTickerTopics', 'resubscribeTopics', 'subscribeTopics', 'sendSubscriptionOperation']);
const publicTree = ast('weex.ws-public.ts');
const constants = publicTree.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d =>
  ['MAX_WS_CHANNELS', 'MAX_TICKER_CHANNELS', 'MAX_SUBSCRIPTION_OPERATIONS_PER_HOUR', 'SUBSCRIPTION_OPERATION_WINDOW_MS'].includes(d.name.getText(publicTree))));
assert.equal(constants.length, 4, 'ws_control::source_constants');
const apiTree = ast('weex.api.ts');
const signing = apiTree.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(apiTree) === 'signWeexMessage'));
assert.ok(signing, 'ws_control::source_signing');
function compile(source, context) {
  const output = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
  assert.equal(output.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  const scope = {exports: {}, ...context}; vm.runInNewContext(output.outputText, scope); return scope.exports;
}
const sign = compile(signing.getText(apiTree), {createHmac: require('create-hmac')}).signWeexMessage;
const str = s => ({kind: 'string', units: units(s)}), undef = {kind: 'undefined'}, nil = {kind: 'null'};
function value(v) {
  if (v === undefined) return undef; if (v === null) return nil;
  if (typeof v === 'string') return str(v); return {kind: typeof v, value: v};
}
function materialize(v) {
  if (v.kind === 'undefined') return undefined; if (v.kind === 'null') return null;
  return v.kind === 'string' ? text(v.units) : v.value;
}
const auth = [];
function authCase(id, patch = {}) {
  const input = {time: 1700000000123, credentials: {key: str('KEY'), passphrase: str('PASS'), secret: str('SECRET')}, failures: {}, mutateTime: false, ...patch};
  const credentials = {...input.credentials}, events = [], signInputs = [];
  const instance = new (compile('export class Slice {' + privateFields + '}', {
    PRIVATE_WS_REQUEST_PATH: venue.PRIVATE_WS_REQUEST_PATH,
    virtualClock: {getCurrentTimeMs() {events.push('time'); if (input.failures.time) throw new TypeError('clock');
      if (input.mutateTime) credentials.key = str('CHANGED'); return input.time;}},
    signWeexMessage(secret, message) {events.push('sign'); signInputs.push({secret: value(secret), message: units(message)}); return sign(secret, message);},
  }).Slice)();
  const options = {};
  for (const name of ['key', 'passphrase', 'secret']) Object.defineProperty(options, name, {get() {
    events.push('credential:' + name); if (input.failures[name]) throw new RangeError('getter'); return materialize(credentials[name]);
  }});
  instance.parent = {options}; let result;
  try {result = {headers: Object.entries(instance.createAuthHeaders()).map(([name, v]) => ({name, value: value(v)}))};}
  catch (e) {result = {error: e.name};}
  auth.push({id, input: {...input, time: bits(input.time)}, result, events, signInputs});
}
authCase('basic');
for (const [id, time] of [['zero', 0], ['negative_zero', -0], ['nan', NaN], ['infinity', Infinity], ['large_exponent', 1e21], ['small_exponent', 1e-7]]) authCase(id, {time});
authCase('unicode', {credentials: {key: str('🔑'), passphrase: str('\ud800P'), secret: str('\udc00秘密')}});
authCase('nul', {credentials: {key: str('K\0'), passphrase: str('P\0'), secret: str('S\0')}});
authCase('missing_key', {credentials: {key: undef, passphrase: str('PASS'), secret: str('SECRET')}});
authCase('missing_passphrase', {credentials: {key: str('KEY'), passphrase: undef, secret: str('SECRET')}});
authCase('missing_secret', {credentials: {key: str('KEY'), passphrase: str('PASS'), secret: undef}});
authCase('clock_error', {failures: {time: true}});
authCase('key_error', {failures: {key: true}});
authCase('secret_error', {failures: {secret: true}});
authCase('mutate_time', {mutateTime: true});
const control = [];
function controlCase(id, name, publicClass = false, patch = {}, repeat = false) {
  const input = {disposed: false, subscriptionId: 7, pingAt: 2, logged: false, performance: 75.25,
    reason: null, failures: {}, disposeDuringPerformance: false, disposeDuringSend: false, ...patch};
  const events = [], packets = [], logs = [], snapshots = [], results = []; let instance;
  const Slice = compile('export class Slice {' + (publicClass ? publicFields : privateFields) + '}', {...enums,
    performance: {now() {events.push('performance'); if (input.failures.performance) throw new TypeError('performance');
      if (input.disposeDuringPerformance) instance.isDisposed = true; return input.performance;}},
    JSON: {stringify(packet) {events.push('packet'); packets.push({method: packet.method,
      params: packet.params === undefined ? null : packet.params, id: bits(packet.id)});
      if (input.failures.json) throw new TypeError('json'); return JSON.stringify(packet);}},
  }).Slice;
  instance = new Slice(); instance.isDisposed = input.disposed; instance.subscriptionId = input.subscriptionId;
  instance.pingAt = input.pingAt; instance.hasLoggedUnsupportedTransport = input.logged;
  instance.sendWithLog = () => {events.push('sendWithLog'); if (input.disposeDuringSend) instance.isDisposed = true;
    if (input.failures.send) throw new RangeError('sender');};
  instance.emitWsReconnected = () => {events.push('emit'); if (input.failures.emit) throw new TypeError('emit');};
  instance.parent = {fetchAndSetPrivateSnapshot(limit) {events.push('snapshot'); snapshots.push(bits(limit));
    if (input.failures.snapshot) throw new RangeError('snapshot'); return Promise.resolve();},
    log(message, severity) {events.push('warning'); logs.push({message: units(message), severity}); if (input.failures.log) throw new RangeError('logger');}};
  for (let i = 0; i < (repeat ? 2 : 1); i++) {
    try {instance[name](input.reason === null ? undefined : text(input.reason)); results.push({unit: true});}
    catch (e) {results.push({error: e.name}); break;}
  }
  control.push({id, name, publicClass, input: {...input, subscriptionId: bits(input.subscriptionId), pingAt: bits(input.pingAt), performance: bits(input.performance)},
    results, events, packets, logs, snapshots, state: {disposed: instance.isDisposed, subscriptionId: bits(instance.subscriptionId), pingAt: bits(instance.pingAt), logged: instance.hasLoggedUnsupportedTransport}});
}
controlCase('subscribe/basic', 'subscribe');
controlCase('subscribe/negative_zero', 'subscribe', false, {subscriptionId: -0});
controlCase('subscribe/infinity', 'subscribe', false, {subscriptionId: Infinity});
controlCase('subscribe/json_error', 'subscribe', false, {failures: {json: true}});
controlCase('subscribe/send_error', 'subscribe', false, {failures: {send: true}});
controlCase('subscribe/disposed', 'subscribe', false, {disposed: true});
controlCase('log/default', 'logUnsupportedTransport');
controlCase('log/empty', 'logUnsupportedTransport', false, {reason: []});
controlCase('log/utf16', 'logUnsupportedTransport', false, {reason: units('\ud800 reason')});
controlCase('log/already', 'logUnsupportedTransport', false, {logged: true, failures: {log: true}});
controlCase('log/error', 'logUnsupportedTransport', false, {failures: {log: true}});
controlCase('log/repeated', 'logUnsupportedTransport', false, {}, true);
controlCase('private_open/basic', 'onOpen');
controlCase('private_open/disposed', 'onOpen', false, {disposed: true});
controlCase('private_open/json_error', 'onOpen', false, {failures: {json: true}});
controlCase('private_open/snapshot_error', 'onOpen', false, {failures: {snapshot: true}});
controlCase('private_open/emit_error', 'onOpen', false, {failures: {emit: true}});
controlCase('private_open/dispose_during_send', 'onOpen', false, {disposeDuringSend: true});
for (const publicClass of [false, true]) {
  const prefix = publicClass ? 'public_ping/' : 'private_ping/';
  controlCase(prefix + 'basic', 'ping', publicClass);
  controlCase(prefix + 'disposed', 'ping', publicClass, {disposed: true});
  controlCase(prefix + 'performance_error', 'ping', publicClass, {failures: {performance: true}});
  controlCase(prefix + 'json_error', 'ping', publicClass, {failures: {json: true}});
  controlCase(prefix + 'send_error', 'ping', publicClass, {failures: {send: true}});
  controlCase(prefix + 'dispose_during_performance', 'ping', publicClass, {disposeDuringPerformance: true});
}
const open = [];
function publicOpen(id, patch = {}) {
  const input = {disposed: false, subscriptionId: 4, topics: ['depth.A', 'trade.A'], tickers: ['OLD'], timestamps: Array(240).fill(999),
    logged: true, available: ['A', 'B'], active: [], failures: {}, disposeDuringSend: false, ...patch};
  const events = [], packets = []; let instance;
  const Slice = compile(constants.map(n => n.getText(publicTree)).join('\n') + '\nexport class Slice {' + publicFields + '}', {...enums,
    Date: {now() {events.push('now'); if (input.failures.clock) throw new TypeError('clock'); return 1000;}},
    JSON: {stringify(packet) {events.push('packet'); packets.push({method: packet.method, params: packet.params, id: bits(packet.id)}); return JSON.stringify(packet);}},
  }).Slice;
  instance = new Slice(); instance.isDisposed = input.disposed; instance.isConnected = true; instance.subscriptionId = input.subscriptionId;
  instance.topics = new Set(input.topics); instance.tickerTopics = new Set(input.tickers); instance.subscriptionOperationTimestamps = input.timestamps.slice();
  instance.hasLoggedSubscriptionLimit = input.logged; instance.store = {markets: input.available.map(symbol => ({symbol}))};
  instance.parent = {getActiveTickerSymbols() {events.push('desired'); if (input.failures.desired) throw new TypeError('desired'); return input.active;}};
  instance.send = () => {events.push('send'); if (input.disposeDuringSend) instance.isDisposed = true; if (input.failures.send) throw new RangeError('send');};
  instance.emitWsReconnected = () => {events.push('emit'); if (input.failures.emit) throw new TypeError('emit');};
  let result; try {instance.onOpen(); result = {unit: true};} catch (e) {result = {error: e.name};}
  open.push({id, input: {...input, subscriptionId: bits(input.subscriptionId), topics: input.topics.map(units), tickers: input.tickers.map(units),
    timestamps: input.timestamps.map(bits), available: input.available.map(units), active: input.active.map(units)}, result, events, packets,
    state: {disposed: instance.isDisposed, subscriptionId: bits(instance.subscriptionId), topics: Array.from(instance.topics, units), tickers: Array.from(instance.tickerTopics, units),
      timestamps: instance.subscriptionOperationTimestamps.map(bits), logged: instance.hasLoggedSubscriptionLimit}});
}
publicOpen('basic'); publicOpen('disposed', {disposed: true}); publicOpen('desired_error', {failures: {desired: true}});
publicOpen('send_error', {failures: {send: true}}); publicOpen('clock_error', {failures: {clock: true}});
publicOpen('emit_error', {failures: {emit: true}}); publicOpen('duplicates', {available: ['A', 'A', 'B']});
publicOpen('dispose_during_send', {disposeDuringSend: true});
assert.equal(auth.length, 16, 'ws_control::auth_inventory'); assert.equal(control.length, 30, 'ws_control::control_inventory');
assert.equal(open.length, 8, 'ws_control::open_inventory');
process.stdout.write(JSON.stringify({auth, control, open}));
