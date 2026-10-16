const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, capture} = require('./types-oracle.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.api.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['WEEX_RATE_LIMIT_FALLBACK_MS', 'WEEX_RATE_LIMIT_MAX_DELAY_MS', 'WEEX_RATE_LIMIT_INTERVAL_MS',
  'getHeader', 'getHeaderEntries', 'clampWeexRateLimitDelay', 'parseRetryAfterMs', 'parseRateLimitIntervalMs',
  'getWeexRateLimitRetryDelayMs'];
const nodes = ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => names.includes(d.name.getText(ast))));
assert.equal(nodes.length, 9, 'headers::source_inventory');
const units = text => Array.from({length: text.length}, (_, i) => text.charCodeAt(i));
const text = value => String.fromCharCode(...value);
const specs = [], values = [], identities = new WeakMap();
let trace = [], suppress = false;
function add(spec, value) {
  const id = specs.length; specs.push(spec); values.push(value);
  if ((typeof value === 'object' && value !== null) || typeof value === 'function') identities.set(value, id);
  return id;
}
const undef = add({kind: 'undefined'}, undefined), nil = add({kind: 'null'}, null);
const num = value => add({kind: 'number', bits: capture(value).bits}, value);
const str = value => add({kind: 'string', units: units(value)}, value);
const bool = value => add({kind: 'boolean', value}, value);
const zero = num(0), negativeZero = num(-0), one = num(1), negative = num(-1), three = num(3.25), eight = num(8.5);
const nan = num(NaN), infinity = num(Infinity), empty = str(''), bad = str('not a date');
const seconds = str('2.5'), future = str('2024-01-02T03:04:05Z'), no = bool(false), yes = bool(true);
const symbol = add({kind: 'symbol'}, Symbol('weight')), bigint = add({kind: 'bigint', text: '0'}, 0n);
function object(props = [], extra = {}) {
  const value = extra.array ? [] : Object.create(null);
  const id = add({kind: 'object', props: props.map(([key, item, enumerable = true]) => [units(key), item, enumerable]), ...extra}, value);
  for (const [key, item, enumerable = true] of props) Object.defineProperty(value, key, {value: values[item], enumerable, configurable: true, writable: true});
  if (extra.proto !== undefined) Object.setPrototypeOf(value, values[extra.proto]);
  value[Symbol.toPrimitive] = () => {
    trace.push(['number', id]);
    if (extra.numberError) throw new Error(extra.numberError);
    return extra.numberResult === undefined ? NaN : values[extra.numberResult];
  };
  specs[id].entries = Object.keys(value).map(key => [units(key), props.find(([name]) => name === key)[1]]);
  return id;
}
function method(result, extra = {}) {
  let id;
  const fn = function(name) {
    trace.push(['call', id, identities.get(this), name === undefined ? null : units(name)]);
    if (extra.error) throw new Error(extra.error);
    if (extra.receiverKey) return this[extra.receiverKey];
    return values[result];
  };
  id = add({kind: 'function', result, ...extra}, fn); return id;
}
function proxy(id) {
  if (specs[id].kind !== 'object') return values[id];
  const value = new Proxy(values[id], {
    has(target, key) {if (!suppress) trace.push(['has', id, units(key)]); return Reflect.has(target, key);},
    get(target, key, receiver) {
      if (!suppress) trace.push(['get', id, units(key)]);
      if (specs[id].getError === key) throw new Error('getter sentinel');
      return Reflect.get(target, key, receiver);
    },
  });
  identities.set(value, id); return value;
}
function native(value) {
  if (value === undefined) return {kind: 'undefined'};
  if (value === null) return {kind: 'null'};
  const kind = typeof value;
  if (kind === 'number') return {kind, bits: capture(value).bits};
  if (kind === 'string') return {kind, units: units(value)};
  if (kind === 'boolean') return {kind, value};
  if (kind === 'bigint') return {kind, text: String(value)};
  if (kind === 'symbol') return {kind, id: values.indexOf(value)};
  return {kind, id: identities.get(value)};
}
const serialized = object([['2', eight], ['1', three], ['hidden', one, false]]);
const toJSON = method(serialized), get = method(three), getUndefined = method(undef), getThrows = method(undef, {error: 'method sentinel'});
const receiverGet = method(undef, {receiverKey: 'token'}), functionValue = method(one);
const plain = object([['retry-after', three], ['RETRY-AFTER', eight]]);
const exactFalse = object([['ReTrY-AfTeR', no], ['retry-after', eight]]);
const exactZero = object([['ReTrY-AfTeR', zero], ['retry-after', eight]]);
const upper = object([['ReTrY-AfTeR', nil], ['retry-after', nil], ['RETRY-AFTER', eight]]);
const hasGet = object([['get', get], ['retry-after', eight]]);
const getReturnsUndefined = object([['get', getUndefined], ['retry-after', eight]]);
const proto = object([['get', receiverGet]]), inherited = object([['token', eight]], {proto});
const nonFunctionGet = object([['get', yes], ['retry-after', three]]);
const getterError = object([['get', get]], {getError: 'get'}), methodError = object([['get', getThrows]]);
const loneName = '\ud800X', loneHeader = object([[loneName, three]]);
const jsonHeader = object([['raw', one], ['toJSON', toJSON]]);
const jsonNull = object([['toJSON', method(nil)]]), jsonString = object([['toJSON', method(seconds)]]);
const jsonFunction = object([['toJSON', method(functionValue)]]);
const jsonError = object([['toJSON', getThrows]]), nonFunctionJSON = object([['toJSON', yes], ['value', one]]);
const array = object([['1', eight], ['0', three]], {array: true});
const exhausted = object([['x-remaining-weight-1S', zero], ['X-USED-WEIGHT-1M', one]]);
const observed = object([['X-USED-WEIGHT-1S', no], ['X-ORDER-COUNT-2M', symbol]]);
const mixed = object([['X-REMAINING-WEIGHT-2S', one], ['X-ORDER-REMAINING-3S', negative], ['X-USED-WEIGHT-1M', one]]);
const exhaustedBigint = object([['X-ORDER-REMAINING-2S', bigint]]);
const numericObject = object([], {numberResult: negative}), exhaustedObject = object([['X-REMAINING-WEIGHT-3S', numericObject]]);
const throwingObject = object([], {numberError: 'coercion sentinel'}), coercionError = object([['X-REMAINING-WEIGHT-3S', throwingObject]]);
const symbolError = object([['X-REMAINING-WEIGHT-3S', symbol]]);
const ignoredThrow = object([['X-OTHER-3S', throwingObject], ['X-REMAINING-WEIGHT-0S', throwingObject]]);
const retryFirst = object([['retry-after', seconds], ['toJSON', getThrows]]);
const retryCap = object([['retry-after', infinity], ['X-ORDER-COUNT-2M', one]]);
const retryDate = object([['retry-after', future]]), retryZero = object([['retry-after', negativeZero], ['X-ORDER-COUNT-2S', one]]);
const retryBad = object([['retry-after', bad]]), retryObject = object([['retry-after', throwingObject], ['X-ORDER-COUNT-2S', one]]);
const hugeInterval = object([['X-USED-WEIGHT-' + '9'.repeat(308) + 'D', one]]);
const unicode = object([['x-uſed-weight-2s', one], ['X-ORDER-COUNT-\ud800-3S', one]]);
const remainingNaN = object([['X-REMAINING-WEIGHT-1M', nan], ['X-ORDER-COUNT-2S', one]]);
const queries = [
  ['lookup_mixed', 'header', plain, 'ReTrY-AfTeR'], ['exact_false', 'header', exactFalse, 'ReTrY-AfTeR'],
  ['exact_zero', 'header', exactZero, 'ReTrY-AfTeR'], ['upper_fallback', 'header', upper, 'ReTrY-AfTeR'],
  ['method', 'header', hasGet, 'ReTrY-AfTeR'], ['method_undefined', 'header', getReturnsUndefined, 'retry-after'],
  ['inherited_receiver', 'header', inherited, 'retry-after'], ['nonfunction_get', 'header', nonFunctionGet, 'retry-after'],
  ['getter_error', 'header', getterError, 'retry-after'], ['method_error', 'header', methodError, 'retry-after'],
  ['lone_name', 'header', loneHeader, loneName], ['null', 'header', nil, 'retry-after'],
  ['function', 'header', functionValue, 'retry-after'], ['primitive', 'header', seconds, 'retry-after'],
  ['plain', 'entries', plain], ['serialized', 'entries', jsonHeader], ['json_null', 'entries', jsonNull],
  ['json_string', 'entries', jsonString], ['json_function', 'entries', jsonFunction], ['json_error', 'entries', jsonError],
  ['nonfunction_json', 'entries', nonFunctionJSON], ['array_order', 'entries', array], ['null', 'entries', nil],
  ['fallback', 'delay', undef], ['exhausted_priority', 'delay', exhausted], ['observed_cap', 'delay', observed],
  ['mixed_remaining', 'delay', mixed], ['bigint_weight', 'delay', exhaustedBigint], ['object_weight', 'delay', exhaustedObject],
  ['coercion_error', 'delay', coercionError], ['symbol_error', 'delay', symbolError], ['ignored_coercion', 'delay', ignoredThrow],
  ['retry_first', 'delay', retryFirst], ['retry_nonfinite', 'delay', retryCap], ['retry_date', 'delay', retryDate],
  ['retry_zero', 'delay', retryZero], ['retry_invalid', 'delay', retryBad], ['retry_object', 'delay', retryObject],
  ['huge_interval', 'delay', hugeInterval], ['unicode_names', 'delay', unicode], ['nan_remaining', 'delay', remainingNaN],
];
const caseStrings = new Set(['retry-after', ...queries.filter(q => q[3] !== undefined).map(q => q[3]),
  ...specs.flatMap(s => (s.props || []).map(([key]) => text(key)))]);
const cases = [...caseStrings].map(value => ({input: units(value), lower: units(value.toLowerCase()), upper: units(value.toUpperCase())}));
const dateStrings = new Set([...caseStrings, ...values.filter(v => typeof v === 'number' || typeof v === 'string').map(String)]);
const dates = [...dateStrings].map(value => ({input: units(value), bits: capture(Date.parse(value)).bits}));
const clock = 1700000000000;
const context = {exports: {}, Date: {now: () => {trace.push(['clock']); return clock;},
  parse: value => {trace.push(['date', units(value)]); return Date.parse(value);}},
  Object: new Proxy(Object, {get(target, key) {
    if (key !== 'entries') return Reflect.get(target, key);
    return value => {trace.push(['entries', identities.get(value)]); suppress = true;
      try {return Object.entries(value);} finally {suppress = false;}};
  }}),
  Math: new Proxy(Math, {get(target, key) {
    if (key !== 'max') return Reflect.get(target, key);
    return (...args) => {if (args.length !== 2 || args[1] !== 0) trace.push(['maximum', args.map(value => capture(value).bits)]);
      return Math.max(...args);};
  }}),
};
vm.runInNewContext(ts.transpileModule(nodes.map(n => n.getText(ast)).join('\n') + '\nexport {getHeader,getHeaderEntries};',
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText, context, {filename: file});
const results = queries.map(([id, op, input, name]) => {
  trace = [];
  let result;
  try {
    const headers = proxy(input);
    const value = op === 'header' ? context.exports.getHeader(headers, name) : op === 'entries'
      ? context.exports.getHeaderEntries(headers) : context.exports.getWeexRateLimitRetryDelayMs({response: {headers}});
    result = op === 'entries' ? {entries: value.map(([key, item]) => [units(key), native(item)])} : {value: native(value)};
  } catch (error) {result = {error: {name: error.name, message: error.message}};}
  return {id, op, input, ...(name === undefined ? {} : {name: units(name)}), result, trace};
});
process.stdout.write(JSON.stringify({specs, cases, dates, clock: capture(clock).bits, results}));
