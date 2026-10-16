import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
export const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const dependencies = process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH), 'oracle.cjs'))
  : require;
const cache = new Map();
// Evaluate maintained relative TS dependencies, rather than copying classifiers.
// External clock transport is an injected failing host: no test makes network calls.
export function loadSource(url) {
  const filename = url instanceof URL ? fileURLToPath(url) : url;
  if (cache.has(filename)) return cache.get(filename).exports;
  const module = {exports: {}};
  cache.set(filename, module);
  const source = fs.readFileSync(filename, 'utf8');
  const code = ts.transpileModule(source, {compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
    esModuleInterop: true,
  }}).outputText;
  const localRequire = specifier => {
    if (specifier === 'axios') return {get: async () => {throw Error('oracle network disabled');}};
    if (specifier === '../transport/registry') return {withPublicCorsProxy: () => {throw Error('oracle network disabled');}};
    if (specifier.startsWith('.')) {
      const resolved = path.resolve(path.dirname(filename), specifier);
      return loadSource(resolved.endsWith('.ts') ? resolved : resolved + '.ts');
    }
    return dependencies(specifier);
  };
  vm.runInThisContext('(function(require,module,exports){' + code + '\n})', {filename})(localRequire,module,module.exports);
  return module.exports;
}
