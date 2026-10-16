const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts} = require('./types-oracle.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.getText(ast) === 'WeexPublicWebsocket');
const members = cls.members.filter(n => ts.isPropertyDeclaration(n) && ['addHandler', 'removeHandler', 'handleRoutedMessage'].includes(n.name.getText(ast)));
assert.equal(members.length, 3, 'handlers::source_inventory');
const output = ts.transpileModule('export class Slice {\n' + members.map(n => n.getText(ast)).join('\n') + '\n}',
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(output.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
const scope = {exports: {}}; vm.runInNewContext(output.outputText, scope, {filename: file});
const units = s => Array.from({length: s.length}, (_, i) => s.charCodeAt(i));
const text = value => String.fromCharCode(...value);
const fn = id => ({kind: 'function', id});
const arr = id => ({kind: 'array', id});
const nil = {kind: 'null'}, undef = {kind: 'undefined'};
const property = (key, value) => ({key: units(key), value});
const cases = [];
function run(id, name, patch = {}) {
  const input = {key: units('topic'), handler: fn(1), arrays: [], registries: [[]], registryReads: [0],
    actions: {}, failures: {}, customIterator: false, message: {number: 37, text: units('\ud800😀')}, ...patch};
  const instance = new scope.exports.Slice(), events = [], calls = [], cleaned = [];
  let reads = 0;
  const message = {number: input.message.number, text: text(input.message.text)};
  let arrays, registries;
  const funcs = new Map();
  function handler(number) {
    if (!funcs.has(number)) funcs.set(number, function(json) {
      calls.push({handler: number, number: json.number, text: units(json.text), sameMessage: json === message});
      events.push('invoke:' + number);
      const action = input.actions[number];
      if (action === 'append') arrays[0].push(handler(3));
      if (action === 'replace') registries[0][text(input.key)] = [];
      if (action === 'remove_second') instance.removeHandler(text(input.key), handler(2));
      if (action === 'message_mutation') {json.number = 99; json.text = 'changed';}
      if (action === 'throw') throw new RangeError('fixture handler');
    });
    return funcs.get(number);
  }
  function materialize(value) {
    switch (value.kind) {
      case 'function': return handler(value.id);
      case 'array': return arrays[value.id];
      case 'undefined': return undefined;
      case 'null': return null;
      case 'boolean': return value.value;
      case 'number': return value.value;
      case 'string': return text(value.units);
      case 'hole': return undefined;
      default: throw new Error(value.kind);
    }
  }
  arrays = input.arrays.map(() => []);
  input.arrays.forEach((items, id) => {
    arrays[id].length = items.length;
    items.forEach((item, index) => {if (item.kind !== 'hole') arrays[id][index] = materialize(item);});
  });
  if (input.customIterator) arrays[0][Symbol.iterator] = function() {
    let index = 0;
    return {next() {if (input.failures.next && index === input.failures.next - 1) throw new TypeError('fixture next');
      return index < arrays[0].length ? {done: false, value: arrays[0][index++]} : {done: true};},
      return() {events.push('close'); if (input.failures.close) throw new TypeError('fixture close'); return {done: true};}};
  };
  registries = input.registries.map((entries, id) => {
    const target = {};
    for (const entry of entries) Object.defineProperty(target, text(entry.key), {value: materialize(entry.value), writable: true, enumerable: true, configurable: true});
    return new Proxy(target, {get(object, key, receiver) {
      events.push('get:' + id + ':' + JSON.stringify(units(String(key))));
      if (input.failures.get) throw new TypeError('fixture get');
      return Reflect.get(object, key, receiver);
    }, set(object, key, value, receiver) {
      events.push('set:' + id + ':' + JSON.stringify(units(String(key))));
      if (input.failures.set) throw new TypeError('fixture set');
      return Reflect.set(object, key, value, receiver);
    }});
  });
  Object.defineProperty(instance, 'messageHandlers', {get() {
    events.push('registry'); reads++;
    if (input.failures.registry === reads) throw new TypeError('fixture registry');
    return registries[input.registryReads[Math.min(reads - 1, input.registryReads.length - 1)]];
  }});
  instance.cleanupMessageHandlerStats = key => {
    events.push('cleanup'); cleaned.push(units(key)); if (input.failures.cleanup) throw new RangeError('fixture cleanup');
  };
  let result;
  try {
    if (name === 'handleRoutedMessage') instance[name](text(input.key), message);
    else instance[name](text(input.key), materialize(input.handler));
    result = {unit: true};
  } catch (e) {result = {error: e.name};}
  function capture(value) {
    if (value === undefined) return undef;
    if (value === null) return nil;
    if (Array.isArray(value)) return {kind: 'array', values: Array.from({length: value.length}, (_, i) => capture(value[i])), present: Array.from({length: value.length}, (_, i) => Object.hasOwn(value, i))};
    if (typeof value === 'function') {
      for (const [id, func] of funcs) if (func === value) return fn(id);
      return {kind: 'inherited_function'};
    }
    if (typeof value === 'string') return {kind: 'string', units: units(value)};
    return {kind: typeof value, value};
  }
  const states = registries.map(registry => Object.getOwnPropertyNames(registry).map(key => ({key: units(key), value: capture(Object.getOwnPropertyDescriptor(registry, key).value)})));
  const retainsOriginal = input.arrays.length && input.registries[0].some(p => text(p.key) === text(input.key) && p.value.kind === 'array') ?
    Object.getOwnPropertyDescriptor(registries[0], text(input.key))?.value === arrays[0] : null;
  cases.push({id, name, input, result, states, arrays: arrays.map(capture), retainsOriginal, calls, cleaned, events, messageAfter: {number: message.number, text: units(message.text)}});
}
run('add/missing', 'addHandler');
run('add/append', 'addHandler', {arrays: [[fn(2)]], registries: [[property('topic', arr(0))]]});
run('add/duplicate', 'addHandler', {arrays: [[fn(1)]], registries: [[property('topic', arr(0))]]});
run('add/null', 'addHandler', {registries: [[property('topic', nil)]]});
run('add/false', 'addHandler', {registries: [[property('topic', {kind: 'boolean', value: false})]]});
run('add/holes', 'addHandler', {arrays: [[{kind: 'hole'}, fn(2)]], registries: [[property('topic', arr(0))]]});
run('add/utf16_key', 'addHandler', {key: units('\ud800')});
run('add/proto', 'addHandler', {key: units('__proto__')});
run('add/constructor', 'addHandler', {key: units('constructor')});
run('add/changed_registry', 'addHandler', {arrays: [[fn(2)]], registries: [[], [property('topic', arr(0))]], registryReads: [0, 1]});
run('add/get_error', 'addHandler', {failures: {get: true}});
run('add/set_error', 'addHandler', {failures: {set: true}});
run('remove/all_duplicates', 'removeHandler', {arrays: [[fn(1), fn(2), fn(1)]], registries: [[property('topic', arr(0))]]});
run('remove/last', 'removeHandler', {arrays: [[fn(1)]], registries: [[property('topic', arr(0))]]});
run('remove/missing', 'removeHandler');
run('remove/absent_handler', 'removeHandler', {arrays: [[fn(2)]], registries: [[property('topic', arr(0))]]});
run('remove/holes', 'removeHandler', {arrays: [[{kind: 'hole'}, fn(2)]], registries: [[property('topic', arr(0))]]});
run('remove/cleanup_error', 'removeHandler', {arrays: [[fn(1)]], registries: [[property('topic', arr(0))]], failures: {cleanup: true}});
run('remove/proto', 'removeHandler', {key: units('__proto__')});
run('remove/changed_registry', 'removeHandler', {arrays: [[fn(1)]], registries: [[], [property('topic', arr(0))]], registryReads: [0, 1, 0]});
run('route/missing', 'handleRoutedMessage');
run('route/null', 'handleRoutedMessage', {registries: [[property('topic', nil)]]});
run('route/empty', 'handleRoutedMessage', {arrays: [[]], registries: [[property('topic', arr(0))]]});
run('route/ordered', 'handleRoutedMessage', {arrays: [[fn(2), fn(1), fn(2)]], registries: [[property('topic', arr(0))]]});
run('route/live_append', 'handleRoutedMessage', {arrays: [[fn(1), fn(2)]], registries: [[property('topic', arr(0))]], actions: {1: 'append'}});
run('route/replace', 'handleRoutedMessage', {arrays: [[fn(1), fn(2)]], registries: [[property('topic', arr(0))]], actions: {1: 'replace'}});
run('route/reenter_remove', 'handleRoutedMessage', {arrays: [[fn(1), fn(2)]], registries: [[property('topic', arr(0))]], actions: {1: 'remove_second'}});
run('route/message_mutation', 'handleRoutedMessage', {arrays: [[fn(1), fn(2)]], registries: [[property('topic', arr(0))]], actions: {1: 'message_mutation'}});
run('route/throw_first', 'handleRoutedMessage', {arrays: [[fn(1), fn(2)]], registries: [[property('topic', arr(0))]], actions: {1: 'throw'}});
run('route/custom_close', 'handleRoutedMessage', {arrays: [[fn(1), fn(2)]], registries: [[property('topic', arr(0))]], actions: {1: 'throw'}, customIterator: true});
run('route/close_throws', 'handleRoutedMessage', {arrays: [[fn(1)]], registries: [[property('topic', arr(0))]], actions: {1: 'throw'}, customIterator: true, failures: {close: true}});
run('route/next_throws', 'handleRoutedMessage', {arrays: [[fn(1), fn(2)]], registries: [[property('topic', arr(0))]], customIterator: true, failures: {next: 2}});
run('route/constructor', 'handleRoutedMessage', {key: units('constructor')});
run('route/toString', 'handleRoutedMessage', {key: units('toString')});
run('route/registry_error', 'handleRoutedMessage', {failures: {registry: 1}});
assert.equal(cases.length, 35, 'handlers::oracle_inventory');
process.stdout.write(JSON.stringify({cases}));
