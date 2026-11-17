// Execute the maintained TypeScript risk policy and its real safe-math helper.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../..');
const cache = new Map();
const enums = Object.fromEntries(['OrderSide', 'OrderStatus', 'OrderType', 'PositionMode', 'PositionSide'].map(name =>
  [name, load(path.join(root, 'packages/safe-cex/src/generated', `${name}.ts`))[name]]));
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const output = ts.transpileModule(fs.readFileSync(file, 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } }).outputText;
  const module = { exports: {} }; cache.set(file, module.exports);
  const requireModule = (name) => {
    if (name === '../types') return enums;
    if (name === '@tealstreet/math/src/isNotNumber') return load(path.join(root, 'packages/math/src/isNotNumber.ts'));
    if (name.startsWith('.')) return load(path.resolve(path.dirname(file), `${name}.ts`));
    throw Error(`Unexpected oracle dependency: ${name}`);
  };
  vm.runInNewContext(`(function(require,module,exports){${output}\n})`, {}, { filename: file })(requireModule,module,module.exports);
  return module.exports;
}
const { calculateOrdersRisk } = load(path.join(root, 'packages/safe-cex/src/utils/risk.ts'));
const fixtures = JSON.parse(fs.readFileSync(0, 'utf8'));
process.stdout.write(JSON.stringify(fixtures.map(s => calculateOrdersRisk(s.markets,s.tickers,s.positions,s.orders))));
