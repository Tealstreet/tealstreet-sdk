const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const text = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const owner = ast.statements.filter(node => ts.isClassDeclaration(node) && node.name.text === 'BitunixExchange');
assert.equal(owner.length, 1, 'actual Bitunix class census');
const instance = {};
for (const name of ['getVirtualTpslLeg', 'getVirtualTpslParentOrderId']) {
  const rows = owner[0].members.filter(node => node.name?.getText(ast) === name);
  assert.equal(rows.length, 1, `actual helper census ${name}`);
  assert(ts.isArrowFunction(rows[0].initializer), `actual arrow ${name}`);
  const source = `const helper = ${rows[0].initializer.getText(ast)}; helper;`;
  const js = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
  // Evaluated as a function body so arrow lexical this is the actual helper owner.
  instance[name] = vm.runInThisContext(`(function(){${js.replace(/helper;\s*$/, 'return helper;')}})`).call(instance);
}
const values = ['', 'plain', '__tp', '__sl', 'id__tp', 'id__sl', 'id__TP', 'id__SL',
 'id__tp ', 'id__sl\n', 'id__tp__sl', 'id__sl__tp', 'id__tp__tp', 'a\0b__sl', '😀__tp',
 'e\u0301__sl', '中文__tp', 'é__sl', '__t', '__s', 'tp', 'sl', 'a___tp', '\ud800__tp', '\udc00__sl'];
const units = value => Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
process.stdout.write(JSON.stringify(values.map(value=>({
 units:units(value), legType:typeof instance.getVirtualTpslLeg(value), leg:instance.getVirtualTpslLeg(value)??null,
 parentUnits:units(instance.getVirtualTpslParentOrderId(value)),
}))));
