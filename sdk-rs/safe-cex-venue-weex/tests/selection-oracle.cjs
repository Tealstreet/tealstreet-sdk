const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts} = require('./types-oracle.cjs');
function source(name, file) {
  const filename = path.join(root, 'packages/safe-cex/src/exchanges/weex', file);
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const statement = ast.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ast) === name));
  assert.ok(statement, `selection::source_${name}`);
  return statement.getText(ast);
}
function compile(text, context = {}) {
  const output = ts.transpileModule(text, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
  assert.equal(output.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  const scope = {exports: {}, ...context};
  vm.runInNewContext(output.outputText, scope);
  return scope.exports;
}
const extract = compile(source('extractArray', 'weex.exchange.ts') + '\nexports.extract = extractArray;').extract;
const units = s => Array.from({length: s.length}, (_, i) => s.charCodeAt(i));
const text = units => String.fromCharCode(...units);
const undef = {kind: 'undefined'}, nil = {kind: 'null'};
const number = value => ({kind: 'number', value});
const ref = id => ({kind: 'ref', id});
const str = value => ({kind: 'string', units: units(value)});
const error = {kind: 'throw', name: 'RangeError'};
const prop = (key, ...values) => ({key: units(key), values});
function objects(properties = []) {
  return [{kind: 'object', properties}, {kind: 'object', properties: []},
    {kind: 'array', elements: [number(101), number(102)], properties: []},
    {kind: 'array', elements: [number(201), number(202)], properties: []}];
}
const arrays = [];
function array(id, payload, keys, nodes) {
  const trace = [], counters = new Map(), values = nodes.map(node => node.kind === 'array' ? [] : {});
  const identities = new Map(values.map((value, id) => [value, id]));
  function materialize(value) {
    switch (value.kind) {
      case 'ref': return values[value.id];
      case 'undefined': return undefined;
      case 'null': return null;
      case 'number': return value.value;
      case 'string': return text(value.units);
      case 'boolean': return value.value;
      case 'throw': throw new RangeError('fixture getter');
      case 'revoked': {const proxy = Proxy.revocable({}, {}); proxy.revoke(); return proxy.proxy;}
      default: throw new Error(value.kind);
    }
  }
  for (let id = 0; id < nodes.length; id++) {
    if (nodes[id].kind === 'array') values[id].push(...nodes[id].elements.map(materialize));
    for (const property of nodes[id].properties) {
      Object.defineProperty(values[id], text(property.key), {get() {
        trace.push([id, property.key]);
        const token = id + ':' + text(property.key), count = counters.get(token) || 0;
        counters.set(token, count + 1);
        return materialize(property.values[Math.min(count, property.values.length - 1)]);
      }});
    }
  }
  function capture(value) {
    if (value === undefined) return undef;
    if (value === null) return nil;
    if (Array.isArray(value)) return {kind: 'array', id: identities.get(value) ?? -1, elements: value.map(capture)};
    if (typeof value === 'object') return {kind: 'object', id: identities.get(value)};
    if (typeof value === 'string') return str(value);
    return {kind: typeof value, value};
  }
  let result;
  try { result = {ok: capture(extract(materialize(payload), keys.map(text)))}; }
  catch (e) { result = {error: e.name}; }
  arrays.push({id, payload, keys, nodes, result, trace});
}
array('payload_array', ref(2), ['data'].map(units), objects());
array('data_before_keys', ref(0), ['rows'].map(units), objects([prop('data', ref(2)), prop('rows', ref(3))]));
array('root_key', ref(0), ['rows'].map(units), objects([prop('rows', ref(2))]));
let nodes = objects([prop('data', ref(1)), prop('later', ref(3))]);
nodes[1].properties = [prop('early', ref(2))];
array('nested_before_later_root', ref(0), ['early', 'later'].map(units), nodes);
array('first_key_order', ref(0), ['b', 'a'].map(units), objects([prop('a', ref(2)), prop('b', ref(3))]));
array('empty_keys', ref(0), [], objects([prop('rows', ref(2))]));
array('missing', ref(0), ['rows'].map(units), objects());
array('undefined', undef, ['rows'].map(units), objects());
array('null', nil, ['rows'].map(units), objects());
array('number', number(7), ['rows'].map(units), objects());
array('string', str('x'), ['rows'].map(units), objects());
array('boolean', {kind: 'boolean', value: false}, ['rows'].map(units), objects());
array('null_data', ref(0), ['rows'].map(units), objects([prop('data', nil)]));
array('changing_data', ref(0), [], objects([prop('data', ref(2), ref(3))]));
array('changing_root', ref(0), ['rows'].map(units), objects([prop('rows', ref(2), str('changed'))]));
nodes = objects([prop('data', ref(1), ref(1), nil)]); nodes[1].properties = [prop('rows', ref(2))];
array('changing_nested_null', ref(0), ['rows'].map(units), nodes);
array('throw_data', ref(0), [], objects([prop('data', error)]));
array('throw_root', ref(0), ['rows'].map(units), objects([prop('rows', error)]));
array('throw_second_data', ref(0), [], objects([prop('data', ref(2), error)]));
array('lone_surrogate_key', ref(0), ['\ud800'].map(units), objects([prop('\ud800', ref(3))]));
array('revoked_payload', {kind: 'revoked'}, [], objects());
array('revoked_data', ref(0), [], objects([prop('data', {kind: 'revoked'})]));
nodes = objects([prop('data', ref(1))]); nodes[1].properties = [prop('rows', {kind: 'revoked'})];
array('revoked_nested', ref(0), ['rows'].map(units), nodes);

const runtimes = [];
const runtimeSource = source('isNodeOrBunRuntime', 'weex.ws-private.ts') + '\nexports.detect = isNodeOrBunRuntime;';
function runtime(id, defined, node, bun, nodeError = null, bunError = null) {
  const trace = [], scope = {};
  function value(item) {
    if (item.kind === 'undefined') return undefined;
    if (item.kind === 'null') return null;
    if (item.kind === 'string') return text(item.units);
    if (item.kind === 'object') return {};
    if (item.kind === 'nan') return NaN;
    return item.value;
  }
  if (defined) scope.process = {get versions() {
    trace.push('node'); if (nodeError) throw new TypeError('fixture process');
    return {node: value(node)};
  }, get isBun() {trace.push('bun'); if (bunError) throw new RangeError('fixture bun'); return value(bun);}};
  let result;
  try {result = {ok: compile(runtimeSource, scope).detect()};} catch (e) {result = {error: e.name};}
  runtimes.push({id, defined, node, bun, nodeError, bunError, result, trace});
}
runtime('absent', false, undef, undef, 'TypeError', 'RangeError');
runtime('node_short_circuit', true, str('22'), undef, null, 'RangeError');
runtime('bun', true, undef, {kind: 'boolean', value: true});
runtime('neither', true, undef, {kind: 'boolean', value: false});
runtime('empty_node', true, str(''), str('bun'));
runtime('zero_node', true, number(0), number(1));
runtime('nan_node', true, {kind: 'nan'}, nil);
runtime('object_node', true, {kind: 'object'}, undef, null, 'RangeError');
runtime('node_error', true, undef, undef, 'TypeError');
runtime('bun_error', true, nil, undef, null, 'RangeError');

const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.getText(ast) === 'WeexPublicWebsocket');
const member = cls.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(ast) === 'getDesiredTickerTopics');
assert.ok(member, 'selection::source_topics');
const topicSource = source('MAX_WS_CHANNELS', 'weex.ws-public.ts') + '\n' + source('MAX_TICKER_CHANNELS', 'weex.ws-public.ts') + '\nexport class Slice {' + member.getText(ast) + '}';
const Slice = compile(topicSource).Slice;
const topics = [];
function topic(id, available, active, count) {
  const instance = new Slice(); instance.store = {markets: available.map(symbol => ({symbol}))};
  instance.parent = {getActiveTickerSymbols: () => active}; instance.topics = new Set(Array.from({length: count}, (_, i) => String(i)));
  topics.push({id, available: available.map(units), active: active.map(units), count, result: instance.getDesiredTickerTopics().map(units)});
}
topic('all', ['BTC', 'ETH'], [], 0);
topic('active_market_order', ['BTC', 'ETH', 'SOL'], ['SOL', 'BTC'], 0);
topic('duplicates', ['BTC', 'BTC', 'ETH'], ['BTC', 'BTC'], 0);
topic('unavailable', ['BTC'], ['OTHER'], 0);
topic('empty_markets', [], ['BTC'], 0);
topic('utf16', ['\ud800', '😀', '', '__proto__'], ['', '\ud800', '__proto__'], 0);
for (const count of [0, 9, 10, 11, 98, 99, 100, 101]) topic('capacity_' + count, Array.from({length: 105}, (_, i) => 'S' + i), [], count);
process.stdout.write(JSON.stringify({arrays, runtimes, topics}));
