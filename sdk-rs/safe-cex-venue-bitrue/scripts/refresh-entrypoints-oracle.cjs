const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'TypeScript compiler path required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function initializer(relative, className, name, receiver, ErrorConstructor = Error) {
  const file = path.join(root, relative);
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const classes = source.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === className);
  assert.equal(classes.length, 1, 'REFRESH actual source class');
  const members = classes[0].members.filter(n => n.name?.getText(source) === name);
  assert.equal(members.length, 1, 'REFRESH actual source field ' + name);
  assert(ts.isArrowFunction(members[0].initializer), 'REFRESH actual complete arrow initializer');
  const compiled = ts.transpileModule('return (' + members[0].initializer.getText(source) + ');', {
    compilerOptions: { target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true,
  });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  return new Function('Error', '"use strict";\n' + compiled.outputText).call(receiver, ErrorConstructor);
}
const exchange = 'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts';
const privateWs = 'packages/safe-cex/src/exchanges/bitrue/bitrue.ws-private.ts';
const units = text => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
const token = Object.freeze({ token: 'native-identity' });
const thrown = Object.freeze({ token: 'original-exception' });
function value(v) {
  if (v === token) return { kind: 'identity' };
  if (v === undefined) return { kind: 'undefined' };
  if (v === null) return { kind: 'null' };
  if (typeof v === 'string') return { kind: 'string', units: units(v) };
  if (typeof v === 'number') { const b = Buffer.alloc(8); b.writeDoubleBE(v); return { kind: 'number', bits: b.toString('hex') }; }
  if (typeof v === 'boolean') return { kind: 'boolean', boolean: v };
  throw Error('unexpected capture value');
}
function outcome(v) { assert.equal(v, undefined, 'REFRESH source native void'); return { kind: 'void' }; }
function failure(e) {
  if (e === thrown) return { kind: 'original' };
  assert(e instanceof Error, 'REFRESH newly constructed native Error');
  return { kind: 'error', name: e.name, units: units(e.message) };
}
(async () => {
  const rows = [];
  const fillSpecs = [
    ['null', null], ['undefined', undefined], ['missing', {}], ['empty', { symbol: '' }],
    ['string', { symbol: 'E-BTC-USDT' }], ['surrogate', { symbol: '\ud800X\udfff' }],
    ['zero', { symbol: -0 }], ['nan', { symbol: NaN }], ['false', { symbol: false }],
    ['null-symbol', { symbol: null }], ['identity', { symbol: token }],
    ['primitive', 7], ['resolve-throw', null], ['getter', null], ['getter-throw', null], ['queue-throw', { symbol: token }],
  ];
  assert.equal(fillSpecs.length, 16, 'REFRESH independent fill floor');
  for (const [id, raw] of fillSpecs) {
    const events = [];
    const queue = function(...args) { assert.equal(this, receiver, 'REFRESH queue receiver'); events.push({ kind: 'queue', args: args.map(value) }); if (id === 'queue-throw') throw thrown; return token; };
    const receiver = Object.defineProperty({}, 'queueFastPrivateRefresh', { get() { events.push({ kind: 'resolve' }); if (id === 'resolve-throw') throw thrown; return queue; } });
    let fill = raw;
    if (id === 'getter' || id === 'getter-throw' || id === 'resolve-throw') fill = Object.defineProperty({}, 'symbol', { get() { events.push({ kind: 'read' }); if (id === 'getter-throw') throw thrown; return token; } });
    let result;
    try { result = outcome(initializer(exchange, 'BitrueExchange', 'handleFillForFastRefresh', receiver)(fill)); }
    catch (e) { result = failure(e); }
    rows.push({ method: 'fill', id, events, result });
  }
  const accountSpecs = [['undefined', undefined], ['null', null], ['identity', token], ['proxy', new Proxy({}, { get() { throw Error('ignored account payload read'); } })], ['queue-throw', token], ['resolve-throw', token]];
  assert.equal(accountSpecs.length, 6, 'REFRESH independent account floor');
  for (const [id, json] of accountSpecs) {
    const events = [];
    const parent = { queueFastPrivateRefresh(...args) { assert.equal(this, parent); events.push({ kind: 'queue', args: args.map(value) }); if (id === 'queue-throw') throw thrown; return token; } };
    const queue = parent.queueFastPrivateRefresh;
    Object.defineProperty(parent, 'queueFastPrivateRefresh', { get() { events.push({ kind: 'resolve' }); if (id === 'resolve-throw') throw thrown; return queue; } });
    const receiver = Object.defineProperty({}, 'parent', { get() { events.push({ kind: 'parent' }); return parent; } });
    let result;
    try { result = outcome(initializer(privateWs, 'BitruePrivateWebsocket', 'handleAccountUpdate', receiver)(json)); }
    catch (e) { result = failure(e); }
    rows.push({ method: 'account', id, events, result });
  }
  const marginSpecs = [['false', false, undefined], ['true', true, 'E-ETH-USDT'], ['ignored-proxies', token, new Proxy({}, { get() { throw Error('ignored margin symbol read'); } })], ['emit-throw', false, 'BTC'], ['constructor-throw', true, undefined]];
  assert.equal(marginSpecs.length, 5, 'REFRESH independent margin floor');
  for (const [id, isolated, symbol] of marginSpecs) {
    const events = [];
    const receiver = { emitAndLogError(message) { assert.equal(this, receiver); events.push({ kind: 'emit', message: units(message) }); if (id === 'emit-throw') throw thrown; return token; } };
    let constructed;
    function ErrorConstructor(message) {
      events.push({ kind: 'construct', message: units(message) });
      if (id === 'constructor-throw') throw thrown;
      constructed = new Error(message);
      assert.equal(typeof constructed.stack, 'string', 'REFRESH actual Error carries stack');
      assert.equal(Object.getPrototypeOf(constructed), Error.prototype, 'REFRESH actual Error prototype');
      return constructed;
    }
    const pending = initializer(exchange, 'BitrueExchange', 'changeMarginType', receiver, ErrorConstructor)(isolated, symbol);
    const immediate = events.slice();
    let result;
    try { result = outcome(await pending); }
    catch (e) {
      if (e === constructed) { assert(e instanceof Error); result = { kind: 'constructed' }; }
      else result = failure(e);
    }
    rows.push({ method: 'margin', id, immediate, events, result });
  }
  assert.equal(rows.length, 27, 'REFRESH independent total floor');
  process.stdout.write(JSON.stringify(rows));
})().catch(e => { console.error(e); process.exitCode = 1; });
