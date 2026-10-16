const { ts, fs, path, vm, root, assert, load } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts'),
    ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration);
const types = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const { ENDPOINTS } = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts'));
const { createPositionId } = load(path.join(root, 'packages/safe-cex/src/utils/createPositionId.ts'));
const context = {
    ...types,
    ENDPOINTS,
    createPositionId,
    uniqBy: require('lodash/uniqBy'),
};
function install(owner, name, extra = {}) {
    const member = cls.members.filter((n) => n.name?.getText(ast) === name);
    assert.equal(member.length, 1);
    owner[name] = vm
        .runInNewContext(
            ts.transpileModule('(function(){return (' + member[0].initializer.getText(ast) + ');})', {
                compilerOptions: { target: ts.ScriptTarget.ES2022 },
            }).outputText,
            { ...context, ...extra },
        )
        .call(owner);
}
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
const base = {
    symbol: 'BTCUSDT',
    qty: '-2.75',
    side: 'BOTH',
    positionMode: 'HEDGE',
    marginMode: 'CROSS',
    entryPrice: '12.25',
    entryValue: '31.5',
    leverage: '8.25',
    unrealizedPNL: '-1.125',
    realizedPNL: '2.625',
    liqPrice: '7.375',
    positionId: 'REST-ID',
};
const specs = [
    {},
    { body: [base] },
    { body: { positionList: [base] } },
    { body: { data: { data: [base] } } },
    { body: { positions: [] } },
    { body: null },
    { body: [base, base] },
    {
        body: [
            {
                instId: 'BTCUSDT',
                size: '3.125',
                positionSide: 'LONG',
                posMode: 'ONE_WAY',
                marginType: 'ISOLATED',
                avgPrice: '14.625',
                upl: '0.25',
                liqPx: '4.375',
            },
        ],
    },
    { body: [{ symbol: '', qty: 2 }] },
    { body: [base], mode: types.PositionMode.OneWay },
    { body: [base], settings: { BTCUSDT: { marginType: 'isolated', leverage: 4.25 } } },
    {
        body: [{ ...base, marginMode: 'unknown', leverage: 'bad' }],
        settings: { BTCUSDT: { marginType: 'isolated', leverage: 4.25 } },
    },
    { fail: true },
    { body: [{ ...base, qty: { toString: null } }] },
    ...['BUY', 'SELL', 'LONG', 'SHORT', 'BOTH', 'unknown'].map((side) => ({ body: [{ ...base, side }] })),
    ...['0', '-0', 'NaN', 'Infinity', '12.5tail', null].map((qty) => ({ body: [{ ...base, qty }] })),
    ...['CROSS', 'ISOLATION', 'ISOLATED', 'unknown'].map((marginMode) => ({ body: [{ ...base, marginMode }] })),
    { body: [base], markets: [] },
    { body: [base], settings: { BTCUSDT: { marginType: 'cross', leverage: 0 } } },
];
const rows = [];
(async () => {
    for (const fixture of specs) {
        const events = [];
        const note = (...a) => events.push(obs(a));
        const body = fixture.body === undefined ? { data: [base] } : fixture.body;
        const markets = fixture.markets ?? [{ symbol: 'BTCUSDT' }, { symbol: 'ETHUSDT' }];
        const cached = [
            {
                id: 'cached',
                symbol: 'CACHED',
                side: 'long',
                entryPrice: 0,
                notional: 0,
                leverage: 10,
                unrealizedPnl: 0,
                contracts: 0,
                liquidationPrice: 0,
            },
        ];
        const store = { positions: cached };
        Object.defineProperty(store, 'markets', {
            get: () => {
                note('markets');
                return markets;
            },
        });
        const owner = {
            store,
            symbolSettings: {
                get: (s) => {
                    const settings = fixture.settings?.[s];
                    note('settings', s, settings ?? null);
                    return settings;
                },
            },
            xhr: {
                get: async (e, c = {}) => {
                    note('read', e, c);
                    if (fixture.fail) throw new Error('read failed');
                    return { data: body };
                },
            },
        };
        Object.defineProperty(owner, 'accountPositionMode', {
            get: () => {
                note('mode');
                return fixture.mode ?? types.PositionMode.Hedged;
            },
        });
        for (const name of [
            'isRecord',
            'getResponseCandidates',
            'getFirstValue',
            'parseFiniteNumber',
            'extractPositionRows',
            'fetchPositions',
        ])
            install(owner, name, { console: { error: (_prefix, e) => note('report', e) } });
        const result = await owner.fetchPositions();
        rows.push({ key: 'positions', fixture: JSON.stringify({ ...fixture, body }), events, result: obs(result) });
    }
    for (const fixture of [
        {},
        { regularFail: true },
        { tpslFail: true },
        { regularFail: true, tpslFail: true },
        { deferred: true },
    ]) {
        const events = [];
        let release;
        const deferred = new Promise((r) => (release = r));
        const regular = { id: 'R' },
            tpsl = { id: 'T' };
        const owner = {
            fetchRegularOrders: async () => {
                events.push(['regular']);
                if (fixture.deferred) await deferred;
                if (fixture.regularFail) throw new Error('regular');
                return [regular];
            },
            fetchTpslOrders: async () => {
                events.push(['tpsl']);
                if (fixture.deferred) release();
                if (fixture.tpslFail) throw new Error('tpsl');
                return [tpsl];
            },
        };
        install(owner, 'fetchOrders');
        const result = await owner.fetchOrders();
        rows.push({
            key: 'orders',
            fixture: JSON.stringify(fixture),
            events,
            result: obs(result),
            identity: result.every((r) => r === regular || r === tpsl),
        });
    }
    assert.equal(specs.length, 32, 'POSITION independent fixture floor');
    assert.equal(rows.length, 37, 'POSITION read source floor');
    process.stdout.write(JSON.stringify(rows));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
