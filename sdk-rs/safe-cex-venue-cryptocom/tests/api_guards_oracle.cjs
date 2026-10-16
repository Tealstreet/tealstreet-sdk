const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'API-GUARDS compiler path required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom/cryptocom.api.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
function expression(name) {
  const found = source.statements.filter(ts.isVariableStatement).flatMap(s => s.declarationList.declarations).filter(d => d.name.getText(source) === name);
  assert.equal(found.length, 1, 'API-GUARDS actual source declaration ' + name);
  return found[0].initializer;
}
const constant = expression('CRYPTOCOM_AUTHENTICATION_ERROR_CODE');
assert(ts.isStringLiteral(constant), 'API-GUARDS actual authentication constant');
const units = value => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
function build(name, deps) {
  const arrow = expression(name);
  assert(ts.isArrowFunction(arrow), 'API-GUARDS complete actual arrow ' + name);
  const compiled = ts.transpileModule('return (' + arrow.getText(source) + ');', { reportDiagnostics: true, compilerOptions: { target: ts.ScriptTarget.ES2022 } });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  return new Function(...Object.keys(deps), '"use strict";\n' + compiled.outputText)(...Object.values(deps));
}
function context(input) {
  const events = [];
  const capture = { events, converted: null, stringError: false, array: null, arrayError: false, parsedObject: false, parsedArray: null, parsedArrayError: false, parsed: undefined, thrown: undefined };
  const deps = {
    CRYPTOCOM_AUTHENTICATION_ERROR_CODE: constant.text,
    String(value) {
      assert(Object.is(value, input), 'API-GUARDS source String input identity');
      events.push({ kind: 'string' });
      try { const text = String(value); capture.converted = units(text); return text; }
      catch (e) { capture.stringError = true; capture.thrown = e; throw e; }
    },
    Array: { isArray(value) {
      const parsing = capture.parsed !== undefined;
      assert(Object.is(value, parsing ? capture.parsed : input), 'API-GUARDS source Array input identity');
      events.push({ kind: 'array' });
      try { const result = Array.isArray(value); if (parsing) capture.parsedArray = result; else capture.array = result; return result; }
      catch (e) { if (parsing) capture.parsedArrayError = true; else capture.arrayError = true; capture.thrown = e; throw e; }
    } },
  };
  capture.record = build('isRecord', deps);
  capture.auth = build('isCryptocomAuthenticationErrorCode', deps);
  capture.message = build('isCryptocomAuthenticationFailureMessage', deps);
  capture.suppress = build('shouldSuppressValidationAuthErrorEmission', { ...deps, isCryptocomAuthenticationErrorCode: capture.auth });
  return { ...capture, capture, deps };
}
function observe(c, call, parsed = false) {
  try {
    const result = call();
    if (parsed) {
      if (result === undefined) return { kind: 'undefined' };
      assert.strictEqual(result, c.parsed, 'API-GUARDS returned parsed native identity');
      return { kind: 'parsed' };
    }
    assert.equal(typeof result, 'boolean', 'API-GUARDS native result boolean');
    return { kind: 'boolean', value: result };
  } catch (e) {
    assert.strictEqual(e, c.thrown, 'API-GUARDS original thrown value identity');
    return { kind: 'original' };
  }
}
const rows = [];
const revoked = target => { const proxy = Proxy.revocable(target, {}); proxy.revoke(); return proxy.proxy; };
const records = [
 ['undefined', undefined], ['null', null], ['false', false], ['true', true], ['number', NaN], ['string', ''], ['symbol', Symbol()], ['bigint', 1n], ['function', () => {}],
 ['object', {}], ['array', []], ['date', new Date(0)], ['map', new Map()], ['boxed', new String('')], ['null-prototype', Object.create(null)],
 ['proxy-object', new Proxy({}, { get() { throw Error('Array.isArray must not get'); } })], ['proxy-array', new Proxy([], {})],
 ['revoked-object', revoked({})], ['revoked-array', revoked([])], ['revoked-function', revoked(() => {})],
];
assert.equal(records.length, 20, 'API-GUARDS independent record floor');
for (const [id, input] of records) {
 const { capture: c, record } = context(input);
 const result = observe(c, () => record(input));
 rows.push({ group: 'record', id, object: typeof input === 'object' && input !== null, array: c.array, arrayError: c.arrayError, events: c.events, result });
}
const parses = [
 ['object', '{}'], ['member', '{"a":1}'], ['array', '[]'], ['null', 'null'], ['boolean', 'true'], ['zero', '0'], ['negative-zero', '-0'], ['infinity', '1e400'],
 ['string', '""'], ['surrogate-string', '"\\ud800"'], ['surrogate-object', '{"\\ud800":"\\udfff"}'], ['proto-key', '{"__proto__":{"polluted":true}}'],
 ['bad', 'bad'], ['empty', ''], ['whitespace', '  {"a":1}\n'], ['duplicate', '{"a":1,"a":2}'], ['nested-array', '[{}]'],
 ['parser-throw', '{}'], ['record-throw', '{}'],
];
assert.equal(parses.length, 19, 'API-GUARDS independent parse floor');
for (const [id, input] of parses) {
 const { capture: c, deps } = context(input);
 let parseError = false;
 const parse = build('parseJsonObject', { ...deps,
  JSON: { parse(value) {
   c.events.push({ kind: 'parse', units: units(value) });
   assert.strictEqual(value, input, 'API-GUARDS complete parse input');
   try { if (id === 'parser-throw') throw Error('parser'); c.parsed = id === 'record-throw' ? revoked({}) : JSON.parse(value); c.parsedObject = typeof c.parsed === 'object' && c.parsed !== null; return c.parsed; }
   catch (e) { parseError = true; c.thrown = e; throw e; }
  } },
  isRecord(value) { assert.strictEqual(value, c.parsed); c.events.push({ kind: 'record' }); return c.record(value); },
 });
 const result = observe(c, () => parse(input), true);
 rows.push({ group: 'parse', id, units: units(input), parseError, object: c.parsedObject, array: c.parsedArray, arrayError: c.parsedArrayError, events: c.events, result });
}
const codes = [
 ['undefined', undefined], ['null', null], ['false', false], ['true', true], ['zero', 0], ['negative-zero', -0], ['nan', NaN], ['infinity', Infinity],
 ['number-code', 40101], ['bigint-code', 40101n], ['string-code', '40101'], ['padded', ' 40101'], ['surrogate', '40101\ud800'], ['symbol', Symbol('40101')],
 ['array', [40101]], ['boxed', new Number(40101)], ['object', { toString() { return '40101'; } }],
 ['coercion-throw', { [Symbol.toPrimitive]() { throw Error('coercion'); } }], ['proxy-throw', new Proxy({}, { get() { throw Error('proxy'); } })],
];
assert.equal(codes.length, 19, 'API-GUARDS independent code floor');
for (const [id, input] of codes) {
 const { capture: c, auth } = context(input);
 const result = observe(c, () => auth(input));
 rows.push({ group: 'code', id, converted: c.converted, stringError: c.stringError, events: c.events, result });
}
const needle = 'Crypto.com API Error 40101';
const messages = [
 ['undefined', undefined], ['null', null], ['number', 40101], ['boxed', new String(needle)], ['object', { toString() { throw Error('message coercion'); } }],
 ['empty', ''], ['exact', needle], ['prefix', 'prefix: ' + needle], ['suffix', needle + ': denied'], ['case', needle.toLowerCase()],
 ['other-code', 'Crypto.com API Error 40102'], ['longer-code', needle + '0'], ['gap', 'Crypto.com API Error  40101'],
 ['surrogate-prefix', '\ud800' + needle], ['surrogate-suffix', needle + '\udfff'], ['nul', 'Crypto.com API Error\0 40101'],
];
assert.equal(messages.length, 16, 'API-GUARDS independent message floor');
for (const [id, input] of messages) {
 const { capture: c, message } = context(input);
 const result = observe(c, () => message(input));
 rows.push({ group: 'message', id, units: typeof input === 'string' ? units(input) : null, events: c.events, result });
}
const suppressions = [
 ['true', 40101, true], ['false', '40101', false], ['undefined', '40101', undefined], ['null', '40101', null], ['one', '40101', 1], ['string', '40101', 'true'],
 ['boxed', '40101', new Boolean(true)], ['identity', '40101', {}], ['symbol', '40101', Symbol()], ['wrong-code', 40102, true], ['missing-code', undefined, true],
 ['code-throw', codes[17][1], true], ['flag-throw', '40101', undefined],
];
assert.equal(suppressions.length, 13, 'API-GUARDS independent suppression floor');
for (const [id, input, flag] of suppressions) {
 const { capture: c, suppress } = context(input);
 const exchange = Object.defineProperty({}, 'suppressCryptocomValidationErrorEmission', { get() {
  c.events.push({ kind: 'flag' }); if (id === 'flag-throw') { c.thrown = Error('flag'); throw c.thrown; } return flag;
 } });
 const result = observe(c, () => suppress(exchange, input));
 rows.push({ group: 'suppress', id, converted: c.converted, stringError: c.stringError, flag: typeof flag === 'boolean' ? flag : null, flagError: id === 'flag-throw', events: c.events, result });
}
assert.equal(rows.length, 87, 'API-GUARDS independent total floor');
process.stdout.write(JSON.stringify(rows));
