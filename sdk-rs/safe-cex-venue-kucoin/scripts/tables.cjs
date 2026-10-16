// Execute live source, resolving only local imports and the isolated CI compiler.
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file).exports;
  const module = { exports: {} };
  cache.set(file, module);
  const source = fs.readFileSync(file, 'utf8');
  const code = ts.transpileModule(source, { compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
  }}).outputText;
  new Function('require', 'module', 'exports', code)(specifier => {
    if (!specifier.startsWith('.') && !specifier.startsWith('@tealstreet/')) throw new Error(`unexpected dependency: ${specifier}`);
    const base = specifier.startsWith('@tealstreet/') ? path.join(root, 'packages', specifier.slice('@tealstreet/'.length)) : path.resolve(path.dirname(file), specifier);
    return load(fs.existsSync(base + '.ts') ? base + '.ts' : path.join(base, 'index.ts'));
  }, module, module.exports);
  return module.exports;
}
if (require.main === module) {
const result = {};
for (const name of ['kucoin', 'kucoinspot']) {
  result[name] = load(path.join(root, `packages/safe-cex/src/exchanges/kucoin/${name}.types.ts`));
}
process.stdout.write(JSON.stringify(result));
}
module.exports = { load, root };
