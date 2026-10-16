const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function code(text) {
  return ts.transpileModule(text, { compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
  } }).outputText;
}
const configText = fs.readFileSync(path.join(root, 'packages/safe-cex/src/config/config.ts'), 'utf8');
const config = [];
for (const [random, simulate, debug] of [
  [0.25, '0x10', 'NaN'], [0.75, 'Infinity', '\uFEFF-0\uFEFF'], [0, undefined, undefined],
  [0.5, undefined, '0x10'],
]) {
  let calls = 0;
  const math = Object.create(Math);
  math.random = () => { calls++; return random; };
  const exports = {};
  new Function('exports', 'process', 'Math', code(configText))(exports, { env: {
    NEXT_PUBLIC_SIMULATE_HTTP_OVER_WEBSOCKET: simulate,
    NEXT_PUBLIC_WS_LIFECYCLE_DEBUG: debug,
  } }, math);
  assert.equal(calls, 1, 'ROOT-UTILS source random call count');
  config.push(exports);
}
assert.equal(config.length, 4, 'ROOT-UTILS independent source config count');

const accountFile = path.join(root, 'packages/safe-cex/src/createExchangeFromAccount.ts');
const accountText = fs.readFileSync(accountFile, 'utf8');
const ast = ts.createSourceFile(accountFile, accountText, ts.ScriptTarget.Latest, true);
const names = ['getPublicEnv', 'parseKucoinBrokerVerify', 'isLighterExchange',
  'getKucoinBrokerEnv', 'resolvePublicKucoinBrokerConfig', 'KUCOIN_BROKER_ENV_PREFIX_BY_EXCHANGE'];
const selected = ast.statements.filter(node => ts.isVariableStatement(node)
  && node.declarationList.declarations.some(decl => names.includes(decl.name.getText(ast))));
assert.equal(selected.length, 6, 'ROOT-UTILS unique helper declaration count');
const text = selected.map(node => node.getText(ast)).join('\n')
  + '\nreturn {getPublicEnv,parseKucoinBrokerVerify,isLighterExchange,getKucoinBrokerEnv,resolvePublicKucoinBrokerConfig};';
const env = {
  NEXT_PUBLIC_KUCOIN_BROKER_PARTNER: ' global partner ',
  NEXT_PUBLIC_KUCOIN_BROKER_KEY: ' global key ',
  NEXT_PUBLIC_KUCOIN_BROKER_NAME: ' global name ',
  NEXT_PUBLIC_KUCOIN_BROKER_VERIFY: 'yes',
  NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_PARTNER: '\uFEFF futures partner \uFEFF',
  NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_KEY: ' ',
  NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_NAME: 'futures name',
  NEXT_PUBLIC_KUCOIN_SPOT_BROKER_PARTNER: 'spot partner',
  NEXT_PUBLIC_KUCOIN_SPOT_BROKER_VERIFY: 'unexpected',
};
const helpers = new Function('process', code(text))({env});
const brokers = ['kucoin', 'kucoinspot', 'lighter'].map(name => helpers.resolvePublicKucoinBrokerConfig(name) ?? null);
const verifyInputs = [undefined, '', ' TRUE ', '1', 'Yes', 'false', ' 0 ', 'NO', 'unknown', '\uFEFFtrue\uFEFF'];
const verifies = verifyInputs.map(value => helpers.parseKucoinBrokerVerify(value) ?? null);
const lighter = ['lighter', 'lighter-robinhood', 'kucoin', 'LIGHTER'].map(helpers.isLighterExchange);
const reads = [];
const empty = new Function('process', code(text))({env: new Proxy({}, {
  get(_object, key) { reads.push(key); return undefined; },
})});
assert.equal(empty.resolvePublicKucoinBrokerConfig('kucoin'), undefined);
assert.equal(reads.length, 6, 'ROOT-UTILS source required reads before refusal');

const telemetryText = fs.readFileSync(path.join(root, 'packages/safe-cex/src/telemetry.ts'), 'utf8');
const telemetry = {};
new Function('exports', code(telemetryText))(telemetry);
const trace = [];
telemetry.reportEvent('empty', {});
assert.equal(trace.length, 0, 'ROOT-UTILS source default no-op');
const data = {value: 41}, context = {tags: {venue: 'example'}}, error = {message: 'opaque'};
telemetry.setTelemetrySink({
  event(name, value) {
    assert.equal(value, data, 'ROOT-UTILS source event object identity');
    trace.push(name);
    telemetry.setTelemetrySink({event: next => trace.push(next)});
  },
});
telemetry.reportEvent('first', data);
telemetry.reportEvent('next', data);
telemetry.setTelemetrySink({captureException(value, supplied) {
  assert.equal(value, error, 'ROOT-UTILS source error identity');
  assert.equal(supplied, context, 'ROOT-UTILS source context identity');
  trace.push('exception');
}});
telemetry.reportException(error, context);
telemetry.setTelemetrySink({event() { throw error; }});
assert.throws(() => telemetry.reportEvent('throws', data), value => value === error,
  'ROOT-UTILS source exception propagation');
telemetry.setTelemetrySink({captureException() { throw error; }});
assert.throws(() => telemetry.reportException(error, context), value => value === error,
  'ROOT-UTILS source exception callback propagation');
let nameUnits;
telemetry.setTelemetrySink({event(name) { nameUnits = [name.charCodeAt(0)]; }});
telemetry.reportEvent('\uD800', data);
// The callback reads the native source unit before any JSON projection. Rust's
// &str entrypoint cannot carry it; this witnesses the declared FORCED boundary.
assert.deepEqual(nameUnits, [0xD800], 'ROOT-UTILS source event name observable lone unit');
console.log(JSON.stringify({config, brokers, verifies, lighter, reads, trace, nameUnits}));
