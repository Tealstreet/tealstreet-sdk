const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const root = path.resolve(__dirname, '../../..');
const intrinsics = new Set(['Object', 'Function', 'Array', 'String', 'Number', 'Boolean', 'Symbol', 'BigInt', 'Promise', 'Map', 'Set', 'WeakMap', 'WeakSet', 'Error', 'TypeError', 'RangeError', 'RegExp', 'Date', 'Math', 'JSON', 'Reflect', 'ArrayBuffer', 'DataView']);
function intrinsicDeletes(text, filename) {
  const source = ts.createSourceFile(filename, text, ts.ScriptTarget.Latest, true);
  const aliases = new Set(intrinsics);
  const unwrap = node => ts.isParenthesizedExpression(node) || ts.isAsExpression(node) || ts.isTypeAssertionExpression(node) || ts.isNonNullExpression(node) ? unwrap(node.expression) : node;
  const name = node => ts.isPropertyAccessExpression(node) ? node.name.text : ts.isElementAccessExpression(node) && ts.isStringLiteralLike(node.argumentExpression) ? node.argumentExpression.text : undefined;
  function intrinsic(node) {
    node = unwrap(node);
    if (ts.isIdentifier(node)) return aliases.has(node.text);
    if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
      if (['globalThis', 'window', 'global'].includes(node.expression.getText(source)) && intrinsics.has(name(node))) return true;
      return intrinsic(node.expression);
    }
    return false;
  }
  const walk = callback => { function visit(node) { callback(node); ts.forEachChild(node, visit); } visit(source); };
  let changed;
  do {
    changed = false;
    walk(node => {
      if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer && intrinsic(node.initializer) && !aliases.has(node.name.text)) {
        aliases.add(node.name.text); changed = true;
      }
    });
  } while (changed);
  const sites = [], findings = [];
  walk(node => {
    let target;
    if (ts.isDeleteExpression(node)) {
      const property = unwrap(node.expression);
      if (ts.isPropertyAccessExpression(property) || ts.isElementAccessExpression(property)) target = property.expression;
    } else if (ts.isCallExpression(node) && node.expression.getText(source) === 'Reflect.deleteProperty') target = node.arguments[0];
    if (!target) return;
    const row = { file: filename, line: source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1, expression: node.getText(source), target: target.getText(source) };
    sites.push(row);
    if (intrinsic(target) || /(?:^|[.$_])(?:proto(?:type)?|intrinsic)(?:$|[.$_])/i.test(row.target)) findings.push(row);
  });
  return { sites, findings };
}
const controls = intrinsicDeletes('const p=Object.prototype; delete p.x; Reflect.deleteProperty(Function.prototype,"name"); delete Object["prototype"].x; const row={}; delete row.x;', 'audit-control.js');
assert.equal(controls.findings.length, 3, 'intrinsic deletion controls must be detected');
assert.equal(controls.sites.length, 4);
const files = new Set();
function collect(directory) {
  for (const item of fs.readdirSync(directory, { withFileTypes: true })) {
    const file = path.join(directory, item.name);
    if (item.isDirectory()) collect(file);
    else if (/\.(?:[cm]?js|tsx?)$/.test(item.name) && !item.name.endsWith('.d.ts')) files.add(file);
  }
}
for (const pkg of ['safe-cex', 'ccxt-compat', 'math', 'data-structures']) collect(path.join(root, 'packages', pkg, 'src'));
const modules = new Set(['lodash', 'dayjs', 'qs', 'p-iteration', 'tiny-emitter', 'bignumber.js', 'axios', 'axios-rate-limit', 'axios-retry', 'ethers']);
for (const file of files) {
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  for (const node of source.statements) {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier) && /^(?:lodash|dayjs|qs|p-iteration|tiny-emitter|bignumber\.js|axios|axios-rate-limit|axios-retry|ethers)(?:\/|$)/.test(node.moduleSpecifier.text)) modules.add(node.moduleSpecifier.text);
  }
}
for (const name of modules) require(name);
const markerIntrinsics = new Set([Object, Object.prototype, Function, Function.prototype]);
for (const root of [...markerIntrinsics]) for (const descriptor of Object.values(Object.getOwnPropertyDescriptors(root))) {
  for (const value of [descriptor.value, descriptor.get, descriptor.set]) if (typeof value === 'function') markerIntrinsics.add(value);
}
for (const value of markerIntrinsics) assert.equal(Symbol.toStringTag in value, false, 'Lodash getRawTag must not enter temporary tag deletion for represented intrinsics');
for (const file of Object.keys(require.cache)) if (file.includes(`${path.sep}node_modules${path.sep}`) && /\.[cm]?js$/.test(file)) files.add(file);
const sites = [], findings = [];
for (const file of [...files].sort()) {
  const result = intrinsicDeletes(fs.readFileSync(file, 'utf8'), path.relative(root, file));
  sites.push(...result.sites); findings.push(...result.findings);
}
console.log(JSON.stringify({ modules: [...modules].sort(), markerIntrinsics: markerIntrinsics.size, files: files.size, deleteSites: sites.length, findings, sites }, null, 2));
assert.deepEqual(findings, [], 'maintained source/dependencies delete intrinsic properties; explicit refusal needs review');
