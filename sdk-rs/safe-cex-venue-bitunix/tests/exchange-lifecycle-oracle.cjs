const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts'),
    ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration);
const names = ['dispose', 'configurePublicWebsocketShards', 'listenOrderBook', 'fetchIncomeHistoryRange'];
const functions = Object.fromEntries(
    names.map((name) => {
        const m = cls.members.filter((n) => n.name?.getText(ast) === name);
        assert.equal(m.length, 1);
        return [name, m[0].initializer.getText(ast)];
    }),
);
const publicFile = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.ws-public.ts'),
    publicAst = ts.createSourceFile(publicFile, fs.readFileSync(publicFile, 'utf8'), ts.ScriptTarget.Latest, true);
const cap = publicAst.statements
    .filter(ts.isVariableStatement)
    .flatMap((n) => [...n.declarationList.declarations])
    .find((n) => n.name.getText(publicAst) === 'BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION');
assert(cap);
const chunk = require('lodash/chunk');
const limit = vm.runInNewContext(cap.initializer.getText(publicAst));
function obs(v) {
    if (v === undefined) return { undefined: true };
    if (typeof v === 'number') {
        const b = Buffer.alloc(8);
        b.writeDoubleBE(v);
        return { bits: Number.isNaN(v) ? '7ff8000000000000' : b.toString('hex') };
    }
    if (typeof v === 'string') return { units: Array.from({ length: v.length }, (_, i) => v.charCodeAt(i)) };
    if (Array.isArray(v)) return v.map(obs);
    if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, v]) => [k, obs(v)]));
    return v;
}
const rows = [];
async function run(name, f) {
    const events = [],
        buffers = [],
        sockets = [];
    let owner;
    const note = (name, ...args) => {
        events.push([name, ...args.map(obs)]);
        if (f.fail === name) throw new Error(name + ' failed');
    };
    function socket(id) {
        const s = {
            id,
            dispose: () => {
                note('dispose', id);
                if (f.edit && id === 'old0') {
                    delete old[1];
                    old.push(socket('extra'));
                }
            },
            setMarketSymbols: (symbols) => {
                note('set', id, symbols);
                buffers.push(symbols);
                if (f.editSymbols && id === 'primary') symbols.pop();
            },
        };
        sockets.push(s);
        return s;
    }
    const old = [socket('old0'), socket('old1')];
    if (f.hole) delete old[0];
    let shards = old;
    const primary = socket('primary'),
        secondary = socket('secondary'),
        futures = socket('futures'),
        privateSocket = socket('private');
    class Base {
        dispose() {
            note('base');
        }
    }
    class Create {
        constructor(exchange, secondary, symbols) {
            assert.equal(exchange, owner);
            note(secondary ? 'createSecondary' : 'createPrimary', symbols);
            buffers.push(symbols);
            if (f.editCreateSymbols && !secondary) symbols.push('extra');
            const s = socket('new' + sockets.length);
            s.symbols = symbols;
            return s;
        }
    }
    const context = { Base, chunk, BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION: limit, BitunixPublicWebsocket: Create };
    const Class = vm.runInNewContext(
        ts.transpileModule('(class extends Base {run=' + functions[name] + ';})', {
            compilerOptions: { target: ts.ScriptTarget.ES2022 },
        }).outputText,
        context,
    );
    owner = new Class();
    Object.assign(owner, {
        publicWebsocket: primary,
        publicWebsocketSecondary: secondary,
        futuresWebsocket: futures,
        privateWebsocket: privateSocket,
    });
    Object.defineProperty(owner, 'publicWebsocketShards', {
        get: () => shards,
        set: (v) => {
            note('replace');
            shards = v;
            shards.push = (...s) => {
                note(
                    'push',
                    s.map((v) => v.id),
                );
                return Array.prototype.push.apply(shards, s);
            };
        },
    });
    const callback = { callback: true },
        resultValue = { result: true },
        errorValue = { error: true };
    let result, error, identity;
    try {
        if (name === 'dispose') {
            result = owner.run();
            identity = result === undefined;
        }
        if (name === 'configurePublicWebsocketShards') {
            const markets = Array.from({ length: f.count ?? 0 }, (_, i) => ({
                get symbol() {
                    note('symbol', i);
                    return i === 0 ? '\ud800' : 'S' + i;
                },
            }));
            result = owner.run(markets);
            identity =
                result[0] === primary && result[1] === secondary && result.slice(2).every((s, i) => s === shards[i]);
            assert(buffers.length >= 2);
            assert.equal(buffers[0], buffers[1]);
            for (let i = 2; i < buffers.length; i += 2) assert.equal(buffers[i], buffers[i + 1]);
        }
        if (name === 'listenOrderBook') {
            futures.listenOrderBook = (symbol, c) => {
                note('listen', symbol);
                assert.equal(c, callback);
                if (f.reject) throw errorValue;
                return resultValue;
            };
            result = owner.run(String.fromCharCode(...(f.symbol ?? [0xd800, 65, 0xdc00])), callback);
            identity = result === resultValue;
        }
        if (name === 'fetchIncomeHistoryRange') {
            const a = Buffer.from(f.start ?? '8000000000000000', 'hex').readDoubleBE(),
                b = Buffer.from(f.end ?? '7ff8000000000000', 'hex').readDoubleBE();
            owner.fetchIncomeHistory = async (opts) => {
                note('fetch', opts);
                assert.deepEqual(Object.keys(opts), ['startTime', 'endTime', 'incomeType']);
                if (f.reject) throw errorValue;
                if (f.deferred) {
                    note('pending');
                    await Promise.resolve();
                    note('settled');
                }
                return resultValue;
            };
            result = await owner.run(a, b, Object.hasOwn(f, 'income') ? f.income : undefined);
            identity = result === resultValue;
        }
    } catch (e) {
        error = e === errorValue ? { opaque: true } : { message: e.message };
    }
    rows.push({
        name,
        fixture: f,
        events,
        error: error ?? null,
        identity: identity ?? false,
        shards: shards.map((s) => s.id),
        disposed: old.map((s) => s?.id),
    });
}
(async () => {
    for (const f of [{}, { hole: true }, { edit: true }, { fail: 'base' }, { fail: 'dispose' }])
        await run('dispose', f);
    for (const count of [0, 1, 300, 301, 601]) await run('configurePublicWebsocketShards', { count });
    for (const fail of ['dispose', 'replace', 'symbol', 'set', 'createPrimary', 'createSecondary', 'push'])
        await run('configurePublicWebsocketShards', { count: 301, fail });
    await run('configurePublicWebsocketShards', { count: 301, hole: true, edit: true });
    await run('configurePublicWebsocketShards', { count: 301, editSymbols: true });
    await run('configurePublicWebsocketShards', { count: 301, editCreateSymbols: true });
    for (const f of [{}, { symbol: [] }, { reject: true }, { fail: 'listen' }]) await run('listenOrderBook', f);
    for (const f of [
        {},
        { income: null },
        { income: 'COMMISSION', start: '7ff0000000000000', end: '0000000000000000' },
        { reject: true },
        { fail: 'fetch' },
        { deferred: true },
    ])
        await run('fetchIncomeHistoryRange', f);
    assert.equal(rows.length, 30);
    process.stdout.write(JSON.stringify({ rows }));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
