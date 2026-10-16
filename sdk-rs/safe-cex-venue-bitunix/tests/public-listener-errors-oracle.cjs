const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(ts.isClassDeclaration),
    rows = [];
for (const name of ['listenOHLCV', 'listenOrderBook']) {
    const members = owner.members.filter((n) => n.name?.getText(ast) === name);
    assert.equal(members.length, 1);
    const fn = vm.runInNewContext(
        ts.transpileModule('(' + members[0].initializer.getText(ast) + ')', {
            compilerOptions: { target: ts.ScriptTarget.ES2022 },
        }).outputText,
    );
    for (const args of [
        [],
        [null, null],
        [{ symbol: 'BTCUSDT', interval: '1m' }, () => assert.fail('Unexpected callback')],
    ]) {
        let error;
        try {
            fn(...args);
            assert.fail('Expected source rejection');
        } catch (e) {
            error = { name: e.name, message: e.message };
        }
        rows.push({ name, error });
    }
}
assert.equal(rows.length, 6);
process.stdout.write(JSON.stringify(rows));
