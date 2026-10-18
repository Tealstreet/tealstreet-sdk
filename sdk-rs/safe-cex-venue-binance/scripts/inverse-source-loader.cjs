// Load maintained modules with their tracked workspace/thirdparty dependencies.
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');
const {pathToFileURL} = require('node:url');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const external = process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH), 'inverse-oracle.cjs'))
  : createRequire(path.join(root, 'package.json'));
const cache = new Map();
function resolve(base) {
  for (const file of [base + '.ts', base + '.tsx', base + '.js', path.join(base, 'index.ts')]) {
    if (fs.existsSync(file)) return file;
  }
  throw new Error('Missing maintained source module: ' + base);
}
function load(file) {
  if (cache.has(file)) return cache.get(file).exports;
  const module = {exports:{}};
  cache.set(file, module);
  const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    compilerOptions:{target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS, esModuleInterop:true},
    // The maintained base socket resolves Node's ws relative to its source URL.
    // Supply that exact URL when evaluating this ESM environment expression in CJS.
    transformers:{before:[context => source => {
      function visit(node) {
        if (ts.isPropertyAccessExpression(node) && ts.isMetaProperty(node.expression) && node.name.text === 'url') {
          return ts.factory.createStringLiteral(pathToFileURL(file).href);
        }
        return ts.visitEachChild(node, visit, context);
      }
      return ts.visitNode(source, visit);
    }]},
  }).outputText;
  function requireSource(name) {
    if (name.startsWith('.')) return load(resolve(path.resolve(path.dirname(file), name)));
    if (name.startsWith('@tealstreet/')) {
      const source = name.slice('@tealstreet/'.length);
      const owner = source.split('/')[0];
      const thirdparty = ['reconnecting-websocket'].includes(owner);
      return load(resolve(path.join(root, thirdparty ? 'thirdparty' : 'packages', source)));
    }
    return external(name);
  }
  new Function('require', 'module', 'exports', code)(requireSource, module, module.exports);
  return module.exports;
}
module.exports = {root, load, resolve, external};
