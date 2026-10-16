const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert.ok(compiler, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(compiler);
const root = path.resolve(__dirname, '../../..');
const external = require('node:module').createRequire(path.join(compiler, 'package.json'));
const cache = new Map();
function load(file) {
  file = path.resolve(file);
  if (cache.has(file)) return cache.get(file).exports;
  const module = { exports: {} };
  cache.set(file, module);
  const source = fs.readFileSync(file, 'utf8');
  const result = ts.transpileModule(source, { compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true,
  }});
  new vm.Script(`(function(require,module,exports){${result.outputText}\n})`, { filename: file })
    .runInThisContext()((name) => {
      if (name === 'lodash/memoize') return external(name);
      if (name === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
      if (!name.startsWith('.')) throw new Error(`Unexpected oracle dependency: ${name}`);
      return load(path.resolve(path.dirname(file), name + '.ts'));
    }, module, module.exports);
  return module.exports;
}
const directory = path.join(root, 'packages/safe-cex/src/exchanges/orderly');
const constants = load(path.join(directory, 'orderly.types.ts'));
const utils = load(path.join(directory, 'orderly.utils.ts'));
const names = ['RECV_WINDOW', 'BROKER_ID', 'TESTNET_BROKER_ID', 'BASE_URL', 'BASE_WS_URL',
  'ENDPOINTS', 'GRAFANA_ENDPOINTS_EVERY', 'GRAFANA_ENDPOINTS_PERIODIC', 'ORDER_TYPE', 'ORDER_SIDE'];
assert.deepEqual(Object.keys(constants).sort(), [...names].sort(), 'runtime export census');
const output = { runtime: { node: process.version, arch: process.arch } };
for (const name of names) {
  const value = constants[name];
  output[name] = name === 'BASE_WS_URL'
    ? Object.entries(value).map(([key, inner]) => [key, Object.entries(inner)])
    : value && typeof value === 'object' && !Array.isArray(value) ? Object.entries(value) : value;
}
const bits = Buffer.alloc(8);
assert.equal(typeof constants.RECV_WINDOW, 'number', 'RECV_WINDOW pre-serialization typeof');
bits.writeDoubleBE(constants.RECV_WINDOW);
output.recvBits = bits.toString('hex');
const inputs = ['', 'BTCUSDC', 'PERP_BTC_USDC', 'PERP_ETH_USDT_extra', 'X', 'X_BTC', 'X__USDC',
  '___', 'USDCBTCUSDC', 'BTCusdc', 'PERP_😀_USDC', 'PERP_é_€', 'constructor', '__proto__', ' PERP_BTC_USDC ', '\tUSDCBTC\n', '\ufeffPERP_BTC_USDC'];
// Generated strings include missing, empty and excess segments; no reciprocal-only assertions.
for (const a of ['', 'BTC', 'USDC', '😀']) for (const b of ['', 'ETH', 'USDC']) {
  inputs.push(a + '_' + b, 'PERP_' + a + '_' + b, a + 'USDC' + b + 'USDC');
}
output.symbols = inputs.map(input => ({ input, normalize: utils.normalizeSymbol(input), reverse: utils.reverseSymbol(input) }));
// Record the deliberate representation refusal without ever putting a lone surrogate in Rust String.
output.surrogates = ['PERP_\ud800_USDC', '\udfffUSDC'].map(input => {
  return [input, utils.normalizeSymbol(input), utils.reverseSymbol(input)].map(s =>
    Array.from({ length: s.length }, (_, i) => s.charCodeAt(i)));
});
// Public lodash cache manipulation is observable and deliberately omitted in Rust.
utils.normalizeSymbol.cache.set('cache-control', 'injected-normalize');
utils.reverseSymbol.cache.set('cache-control', 'injected-reverse');
output.cache = [utils.normalizeSymbol('cache-control'), utils.reverseSymbol('cache-control')];
console.log(JSON.stringify(output));
