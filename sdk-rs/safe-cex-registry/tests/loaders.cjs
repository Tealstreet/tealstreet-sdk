// Execute the actual registry initializers. Only platform import/require are injected.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const {root, ts} = require('../../safe-cex-types/scripts/accounts-source.cjs');
function registry(native, host) {
  const filename = root + '/packages/safe-cex/src/exchanges/registry' + (native ? '.native' : '') + '.ts';
  const source = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const declarations = source.statements.filter(ts.isVariableStatement).flatMap(s => [...s.declarationList.declarations]);
  const find = name => {
    const matches = declarations.filter(d => d.name.getText(source) === name);
    assert.equal(matches.length, 1, 'source declaration ' + name);
    return matches[0].initializer.getText(source);
  };
  const text = (native ? 'const load = ' + find('load') + ';\n' : '') + 'export const exchangeRegistry = ' + find('exchangeRegistry') + ';';
  const transformer = context => {
    const visit = node => ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword
      ? context.factory.updateCallExpression(node, context.factory.createIdentifier('__import'), node.typeArguments, node.arguments)
      : ts.visitEachChild(node, visit, context);
    return node => ts.visitNode(node, visit);
  };
  const code = ts.transpileModule(text, {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
    transformers: {before: [transformer]},
  }).outputText;
  const captured = {exports: {}};
  new Function('module', 'exports', 'require', '__import', code)(captured, captured.exports, host, host);
  return captured.exports.exchangeRegistry;
}
(async () => {
  const rows = [];
  for (const native of [true, false]) {
    let state;
    const host = path => {
      state.events.push(path);
      if (state.hostError) {
        if (native) throw state.hostError;
        return Promise.reject(state.hostError);
      }
      const namespace = new Proxy({}, {get(_, key) {
        if (key === 'then') return undefined;
        state.events.push(key);
        if (state.exportError) throw state.exportError;
        return state.constructor;
      }});
      return native ? namespace : Promise.resolve(namespace);
    };
    const entries = registry(native, host);
    assert.equal(Object.keys(entries).length, 49, 'independent assigned source loader count');
    for (const [name, loader] of Object.entries(entries)) {
      let calls = 0;
      function Constructor() { calls++; }
      state = {events: [], constructor: Constructor};
      const future = loader();
      assert.equal(state.events.length, native ? 2 : 1, 'lookup timing before await');
      const selected = await future;
      assert.equal(selected, Constructor, 'source constructor identity');
      assert.equal(calls, 0, 'source loader never constructs');
      assert.equal(state.events.length, 2, 'exact source host and export accesses');
      rows.push({native, name, path: state.events[0], export: state.events[1]});
      state = {events: [], constructor: undefined};
      assert.equal(await loader(), undefined, 'missing source export');
      const sentinel = {name, native};
      for (const fault of ['hostError', 'exportError']) {
        state = {events: [], constructor: Constructor, [fault]: sentinel};
        if (native) assert.throws(() => loader(), e => e === sentinel, 'synchronous source failure identity');
        else await assert.rejects(loader(), e => e === sentinel, 'source rejection identity');
      }
      if (!native) {
        state = {events: [], constructor: Constructor};
        const pending = loader();
        function Replacement() {}
        state.constructor = Replacement;
        assert.equal(await pending, Replacement, 'source live export lookup after import settles');
      }
    }
  }
  // All native/source assertions precede the observation serialization.
  process.stdout.write(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
