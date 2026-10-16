const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const cache = new Map();
function load(filename) {
  if (cache.has(filename)) return cache.get(filename).exports;
  const module = { exports: {} };
  cache.set(filename, module);
  const source = fs.readFileSync(filename, 'utf8');
  const compiled = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    fileName: filename,
  }).outputText;
  new Function('require', 'module', 'exports', compiled)((specifier) => {
    if (specifier.startsWith('@tealstreet/')) {
      return load(path.join(root, 'packages', specifier.slice('@tealstreet/'.length) + '.ts'));
    }
    assert.ok(specifier.startsWith('.'), `unexpected dependency ${specifier}`);
    return load(path.resolve(path.dirname(filename), specifier + '.ts'));
  }, module, module.exports);
  return module.exports;
}
module.exports = {load, root, ts};
const actual = load(path.join(root, 'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'));
assert.equal(typeof actual.RECV_WINDOW, 'number', 'RECV_WINDOW native typeof');
const bits = Buffer.alloc(8);
bits.writeDoubleBE(actual.RECV_WINDOW);
const result = {
  RECV_WINDOW: bits.toString('hex'),
  BROKER_ID: actual.BROKER_ID,
  BASE_URL: actual.BASE_URL,
  BASE_WS_URL: Object.entries(actual.BASE_WS_URL),
  ENDPOINTS: Object.entries(actual.ENDPOINTS),
  INTERVAL: Object.entries(actual.INTERVAL),
  ORDER_TYPE: Object.entries(actual.ORDER_TYPE),
  ORDER_STATUS: Object.entries(actual.ORDER_STATUS),
  ORDER_SIDE: Object.entries(actual.ORDER_SIDE),
  GRAFANA_ENDPOINTS_EVERY: actual.GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC: actual.GRAFANA_ENDPOINTS_PERIODIC,
};
if(require.main===module) {
  process.stderr.write(JSON.stringify({capture_header:require('../scripts/capture-header.cjs')()})+'\n');
  process.stdout.write(JSON.stringify(result));
}
