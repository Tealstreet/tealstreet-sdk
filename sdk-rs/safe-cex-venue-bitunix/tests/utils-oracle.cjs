const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const { createRequire } = require('node:module');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.utils.ts');
const actualRequire = createRequire(file);
const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  fileName: file,
}).outputText;
const target = { exports: {} };
vm.runInThisContext(`(function(require,module,exports){${code}\n})`, {filename:file})(
  id => {
    assert(['@noble/hashes/sha2.js', '@noble/hashes/utils.js'].includes(id), id);
    return actualRequire(id);
  }, target, target.exports,
);
const values = ['', 'abc', 'ABC', 'a\0b', 'nonce1700000000000apiKeysymbolBTCUSDT',
  'é', 'e\u0301', '中文', '😀', '\ufeff', ' \t\r\n', '\ufffd', '\ud800', '\udc00', 'a\ud800b', '\ud800\ud800'];
for (const size of [55,56,63,64,65,127,128,129,1024]) values.push('x'.repeat(size));
process.stdout.write(JSON.stringify(values.map((value,index) => ({
  name: `sha256 case ${index}`,
  units: Array.from({length:value.length}, (_,i)=>value.charCodeAt(i)),
  digest: target.exports.sha256Hex(value),
}))));
