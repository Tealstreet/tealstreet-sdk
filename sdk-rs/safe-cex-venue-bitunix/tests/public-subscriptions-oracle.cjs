const { ts, fs, path, vm, root, assert } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.ws-public.ts'),
    ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration);
const names = [
    'setMarketSymbols',
    'enqueueSubscriptions',
    'subscribe',
    'subscribeMarketsToChannel',
    'subscribeToBook',
    'onMarketsChanged',
];
const functions = Object.fromEntries(
    names.map((name) => {
        const members = cls.members.filter((n) => n.name?.getText(ast) === name);
        assert.equal(members.length, 1);
        return [
            name,
            ts.transpileModule('(function(){return (' + members[0].initializer.getText(ast) + ');})', {
                compilerOptions: { target: ts.ScriptTarget.ES2022 },
            }).outputText,
        ];
    }),
);
const rows = [];
function run(name, fixture) {
    const events = [];
    const note = (...v) => {
        events.push(v);
        if (fixture.fail === v[0]) throw new Error(v[0] + ' failed');
    };
    let symbols = fixture.symbols,
        queue = [];
    const args = fixture.args ?? [],
        markets = fixture.markets ?? [{ symbol: 'A' }, { symbol: 'B' }, { symbol: 'C' }];
    const topics = fixture.topics ?? {
        '{"symbol":"X","ch":"ticker"}': 0,
        '{"symbol":"Y","ch":"depth_book1"}': 0,
        '{"symbol":"Z","ch":"price"}': -1,
    };
    const owner = {
        subscriptionQueue: {
            push: (a) => {
                note('push', a);
                queue.push(a);
            },
        },
        subscriptionTimeoutId: fixture.timer,
        isDisposed: fixture.disposed ?? false,
        ws: fixture.noSocket ? undefined : {},
        isSecondary: fixture.secondary ?? false,
        marketSymbols: fixture.filter === undefined ? undefined : new Set(fixture.filter),
        topics,
        parent: { store: { markets } },
        drainSubscriptionQueue: () => note('drain'),
        enqueueSubscriptions: (a, b) => note('enqueue', a, b === undefined ? null : b),
    };
    if (name === 'setMarketSymbols')
        Object.defineProperty(owner, 'marketSymbols', {
            set: (s) => {
                note('assign', s === undefined ? null : [...s]);
                symbols = s === undefined ? null : [...s];
            },
        });
    let error = null;
    try {
        const fn = vm
            .runInNewContext(functions[name], {
                BITUNIX_SUBSCRIBE_BATCH_SIZE: 20,
                BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION: 300,
                JSON: {
                    parse: JSON.parse,
                    stringify: (a) => {
                        note('frame', a);
                        return JSON.stringify(a);
                    },
                },
            })
            .call(owner);
        if (name === 'setMarketSymbols') fn(fixture.symbols ?? undefined);
        else if (name === 'enqueueSubscriptions') fn(args, fixture.batch);
        else if (name === 'subscribe') fn();
        else if (name === 'subscribeMarketsToChannel') fn(fixture.channel ?? 'ticker');
        else if (name === 'subscribeToBook') fn('BOOK', 'depth_book5');
        else fn({ added: markets });
    } catch (e) {
        error = { name: e.name, message: e.message };
    }
    rows.push({ name, fixture, events, error, queue, symbols: symbols ?? null, topics: Object.entries(topics) });
}
for (const symbols of [undefined, [], ['B', 'A', 'B', 'C'], ['😀', 'é', '😀']]) run('setMarketSymbols', { symbols });
run('setMarketSymbols', { symbols: ['B'], fail: 'assign' });
for (const n of [0, 1, 19, 20, 21, 45])
    for (const timer of [undefined, 0, 7])
        run('enqueueSubscriptions', {
            args: Array.from({ length: n }, (_, i) => ({ symbol: String(i), ch: 'ticker' })),
            timer,
        });
for (const batch of [1, 2.5, Infinity, NaN]) run('enqueueSubscriptions', { args: [0, 1, 2, 3, 4], batch });
for (const fail of ['push', 'drain']) run('enqueueSubscriptions', { args: [0, 1], fail });
for (const fixture of [
    {},
    { topics: {} },
    { topics: { '{"symbol":"X","ch":"depth_book5"}': 0 } },
    { topics: { bad: 1 } },
    { fail: 'enqueue' },
])
    run('subscribe', fixture);
for (const fixture of [
    {},
    { filter: [] },
    { filter: ['C', 'A'] },
    { markets: Array.from({ length: 305 }, (_, i) => ({ symbol: String(i) })) },
    { channel: 'price' },
])
    run('subscribeMarketsToChannel', fixture);
run('subscribeToBook', {});
run('subscribeToBook', { fail: 'enqueue' });
for (const fixture of [
    {},
    { markets: [] },
    { noSocket: true },
    { disposed: true },
    { secondary: true },
    { filter: ['B'] },
    { filter: [] },
    { fail: 'enqueue' },
])
    run('onMarketsChanged', fixture);
assert.equal(rows.length, 49, 'SUB independent source floor');
function obs(v) {
    if (v === undefined) return { undefined: true };
    if (typeof v === 'number') {
        const b = Buffer.alloc(8);
        b.writeDoubleBE(v);
        return { bits: Number.isNaN(v) ? '7ff8000000000000' : b.toString('hex') };
    }
    if (Array.isArray(v)) return v.map(obs);
    if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, v]) => [k, obs(v)]));
    return v;
}
const utf16sets = [
    [[0xd800], [0xdfff], [0xd800], [65]],
    [[0xd83d, 0xde00], [0xdfff], [0xd83d, 0xde00]],
].map((input) => {
    let output;
    const owner = {};
    Object.defineProperty(owner, 'marketSymbols', {
        set: (set) => {
            output = [...set].map((s) => Array.from({ length: s.length }, (_, i) => s.charCodeAt(i)));
        },
    });
    vm.runInNewContext(functions.setMarketSymbols).call(owner)(input.map((units) => String.fromCharCode(...units)));
    return { input, output };
});
process.stdout.write(
    JSON.stringify({
        utf16sets,
        rows: rows.map((row) => ({
            ...row,
            fixture: JSON.stringify(row.fixture, (_k, v) =>
                typeof v === 'number' && !Number.isFinite(v) ? { $number: Number.isNaN(v) ? 'NaN' : 'Infinity' } : v,
            ),
            events: obs(row.events),
            queue: obs(row.queue),
            topics: obs(row.topics),
        })),
    }),
);
