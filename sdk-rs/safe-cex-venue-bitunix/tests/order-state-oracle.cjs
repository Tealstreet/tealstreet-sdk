const { ts, fs, path, vm, root, assert, load } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts'),
    ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration);
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const funcs = {};
for (const name of [
    'getTpslRawOrderId',
    'getVirtualTpslLeg',
    'getVirtualTpslParentOrderId',
    'getAttachedVirtualTpslLeg',
    'isStandaloneTpslOrder',
    'parseFiniteNumber',
    'getBitunixTriggerType',
    'getTpslLegSnapshot',
    'getPositionId',
    'hydratePositionIdBeforeFormatting',
]) {
    const m = cls.members.filter((n) => n.name?.getText(ast) === name);
    assert.equal(m.length, 1);
    funcs[name] = ts.transpileModule('(function(){return (' + m[0].initializer.getText(ast) + ');})', {
        compilerOptions: { target: ts.ScriptTarget.ES2022 },
    }).outputText;
}
function install(owner) {
    for (const [name, code] of Object.entries(funcs)) owner[name] = vm.runInNewContext(code, enums).call(owner);
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
const order = (
    type = 'limit',
    info = {
        orderId: 'raw',
        positionId: 'P',
        tpPrice: '14.5',
        tpQty: '2.5',
        tpStopType: 'LAST',
        tpOrderType: 'limit',
        tpOrderPrice: '14.25',
        slPrice: '8.5',
        slQty: '1.75',
        slStopType: 'MARK',
        slOrderType: 'MARKET',
    },
    id = 'R',
) => ({
    id,
    status: 'open',
    symbol: 'BTCUSDT',
    type,
    side: 'buy',
    price: 11.125,
    amount: 4.25,
    filled: 0,
    remaining: 4.25,
    reduceOnly: false,
    lastUpdate: 17,
    info,
});
const snapshots = [
    { order: order() },
    { order: order(enums.OrderType.TakeProfit) },
    { order: order(enums.OrderType.StopLoss) },
    { order: order(), orders: [order(enums.OrderType.TakeProfit, { id: 'raw' }, 'other')] },
    { order: order(), orders: [order(enums.OrderType.StopLoss, { id: 'distinct', positionId: 'P' }, 'other')] },
    { order: order(enums.OrderType.TakeProfit, {}, 'R__tp'), orders: [order('limit', {}, 'R')] },
    ...['tpPrice', 'tpQty', 'tpStopType', 'tpOrderType', 'tpOrderPrice'].flatMap((k) =>
        [null, '0', 'bad'].map((v) => ({ order: order('limit', { ...order().info, [k]: v }) })),
    ),
];
const positions = [
    {
        id: 'BTCUSDT:short',
        symbol: 'BTCUSDT',
        side: 'short',
        entryPrice: 0,
        notional: 0,
        leverage: 10,
        unrealizedPnl: 0,
        contracts: 2.5,
        liquidationPrice: 0,
        info: { positionId: 'WS-ID' },
    },
];
const idSpecs = [
    {},
    { positions: [] },
    { positions: [{ ...positions[0], contracts: 0 }] },
    { positions: [{ ...positions[0], contracts: -1 }] },
    { positions: [{ ...positions[0], side: 'long' }] },
    ...['', null, 0, false, [], {}].map((positionId) => ({ positions: [{ ...positions[0], info: { positionId } }] })),
    { positions: [], cache: { 'BTCUSDT:short': false } },
];
const hydration = [
    {},
    { type: 'limit', reduceOnly: false },
    { type: 'limit', reduceOnly: true },
    { type: enums.OrderType.StopLoss, amount: 0 },
    { type: enums.OrderType.TakeProfit, amount: 0 },
    { positions: [] },
    { positions: [{ ...positions[0], info: {} }] },
    { positions: [{ ...positions[0], info: {} }], cache: { 'BTCUSDT:short': 'CACHE-ID' } },
    { positions: [{ ...positions[0], info: {} }], type: enums.OrderType.TakeProfit, amount: 0 },
    { positions: [{ ...positions[0], info: {} }], fail: 'positions' },
    { type: enums.OrderType.TakeProfit, amount: 0, fail: 'orders' },
    { type: enums.OrderType.TakeProfit, amount: 0, fail: 'update' },
    { type: enums.OrderType.TakeProfit, amount: 0, amountBits: '7ff8000000000000' },
];
const rows = [];
(async () => {
    for (const fixture of snapshots)
        for (const leg of ['tp', 'sl']) {
            const owner = { store: { orders: fixture.orders ?? [] } };
            install(owner);
            let result;
            try {
                result = { ok: obs(owner.getTpslLegSnapshot(fixture.order, leg)) };
            } catch (e) {
                result = { error: e.name, message: e.message };
            }
            rows.push({ key: 'snapshot', fixture: JSON.stringify({ ...fixture, leg }), result });
        }
    for (const fixture of idSpecs) {
        const p = fixture.positions ?? positions;
        const cache = new Map(Object.entries(fixture.cache ?? { 'BTCUSDT:short': 'CACHE-ID' }));
        const owner = { store: { positions: p }, positionIdCache: cache };
        install(owner);
        const result = owner.getPositionId('BTCUSDT', 'short');
        const origin = p.find((p) => p.symbol === 'BTCUSDT' && p.side === 'short' && p.contracts > 0);
        const expected = origin?.info?.positionId || cache.get('BTCUSDT:short');
        rows.push({ key: 'id', fixture: JSON.stringify(fixture), result: obs(result), identity: result === expected });
    }
    for (const fixture of hydration) {
        const events = [];
        const payload = { positions: [{ id: 'fresh' }], orders: [{ id: 'new' }] };
        const owner = {
            store: {
                positions: fixture.positions ?? positions,
                update: (value) => {
                    events.push(obs(['update', value]));
                    assert(Object.values(value).every((v) => v === payload.positions || v === payload.orders));
                    if (fixture.fail === 'update') throw new Error('update failed');
                },
            },
            positionIdCache: new Map(Object.entries(fixture.cache ?? {})),
            fetchPositions: async () => {
                events.push(['fetch', 'positions']);
                if (fixture.fail === 'positions') throw new Error('positions failed');
                return payload.positions;
            },
            fetchOrders: async () => {
                events.push(['fetch', 'orders']);
                if (fixture.fail === 'orders') throw new Error('orders failed');
                return payload.orders;
            },
        };
        install(owner);
        let result;
        try {
            const opts = {
                symbol: 'BTCUSDT',
                type: fixture.type ?? enums.OrderType.StopLoss,
                side: 'buy',
                amount: fixture.amount ?? 1,
                reduceOnly: fixture.reduceOnly,
            };
            if (fixture.amountBits) opts.amount = Buffer.from(fixture.amountBits, 'hex').readDoubleBE();
            await owner.hydratePositionIdBeforeFormatting(opts);
            result = { ok: obs(undefined) };
        } catch (e) {
            result = { error: e.name, message: e.message };
        }
        rows.push({ key: 'hydrate', fixture: JSON.stringify(fixture), events, result });
    }
    assert.equal(rows.length, 67, 'ORDER STATE source floor');
    process.stdout.write(JSON.stringify(rows));
})().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
