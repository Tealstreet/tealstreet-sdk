import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const cache = new Map();
export function loadSource(url, expose = [], select = []) {
  const filename = url instanceof URL ? fileURLToPath(url) : url;
  const key = filename + ':' + expose.join(',') + ':' + select.join(',');
  if (cache.has(key)) return cache.get(key).exports;
  const module = {exports: {}};
  cache.set(key, module);
  let source = fs.readFileSync(filename, 'utf8');
  if (select.length) {
    const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
    const selected = ast.statements.filter(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(declaration => select.includes(declaration.name.getText(ast))));
    if (selected.length !== select.length) throw Error('Missing maintained policy declaration');
    const domainImports = ast.statements.filter(node => ts.isImportDeclaration(node) && node.moduleSpecifier.text === '../../types');
    source = [...domainImports,...selected].map(node => node.getText(ast)).join('\n');
  }
  if (filename.endsWith('/safe-cex/src/types.ts')) {
    // Domain enums are the only runtime imports in these pure mappers.
    const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
    source = ast.statements.filter(ts.isEnumDeclaration).map(node => node.getText(ast)).join('\n');
  }
  if (expose.length) source += '\nexport {'+expose.join(',')+'};';
  const code = ts.transpileModule(source, {compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
  }}).outputText;
  const localRequire = specifier => {
    if (!specifier.startsWith('.')) throw Error('Uninjected oracle host dependency: '+specifier);
    const resolved = path.resolve(path.dirname(filename), specifier);
    return loadSource(resolved.endsWith('.ts') ? resolved : resolved+'.ts');
  };
  vm.runInThisContext('(function(require,module,exports){'+code+'\n})', {filename})(localRequire, module, module.exports);
  return module.exports;
}
